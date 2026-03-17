#!/usr/bin/env bash
set -euo pipefail

# ============================================================================
# AetherAgent Vision — Fully Automated Training Bootstrap
# ============================================================================
#
# Ett enda kommando. Skapar venv, installerar CUDA PyTorch + ultralytics,
# laddar ner YOLOv8n basmodell, genererar dataset, tränar, exporterar ONNX.
#
# Kör i WSL (Ubuntu):
#   chmod +x tools/train.sh && ./tools/train.sh
#
# Med eget dataset:
#   ./tools/train.sh --dataset /mnt/c/Users/robin/labels/my-dataset
#
# Bara exportera befintlig modell:
#   ./tools/train.sh --export-only runs/detect/aether-ui-v1/weights/best.pt
#
# ============================================================================

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
VENV_DIR="$PROJECT_DIR/.venv-vision"
PYTHON=""
DATASET_DIR=""
EPOCHS=150
BATCH=32
IMGSZ=640
VERSION="v1"
MODEL_BASE="yolov8n.pt"
EXPORT_ONLY=""
VERIFY_ONLY=""
SKIP_VERIFY=false
INTERACTIVE=false
DOWNLOAD_STARTER=false
SERVER_URL="http://localhost:3000"
DEPLOY_DIR="$PROJECT_DIR/models"

# ---------------------------------------------------------------------------
# Färger
# ---------------------------------------------------------------------------
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
CYAN='\033[0;36m'
MAGENTA='\033[0;35m'
NC='\033[0m'

log()  { echo -e "${CYAN}[INFO]${NC} $1"; }
ok()   { echo -e "${GREEN}[OK]${NC} $1"; }
warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
err()  { echo -e "${RED}[ERR]${NC} $1"; }
step() { echo -e "${MAGENTA}[STEP]${NC} $1"; }

banner() {
    echo ""
    echo -e "${MAGENTA}╔═══════════════════════════════════════════════════════════════╗${NC}"
    echo -e "${MAGENTA}║     AetherAgent Vision — Fully Automated Training Pipeline   ║${NC}"
    echo -e "${MAGENTA}║     YOLOv8-nano · CUDA · RTX 5090 · WSL Ready                ║${NC}"
    echo -e "${MAGENTA}╚═══════════════════════════════════════════════════════════════╝${NC}"
    echo ""
}

# ---------------------------------------------------------------------------
# CLI argument parsing
# ---------------------------------------------------------------------------
usage() {
    echo "Usage: $0 [OPTIONS]"
    echo ""
    echo "Options:"
    echo "  --dataset PATH         Path to labeled dataset"
    echo "  --download-starter     Generate synthetic starter dataset"
    echo "  --epochs N             Training epochs (default: 150)"
    echo "  --batch N              Batch size (default: 32)"
    echo "  --imgsz N              Image size (default: 640)"
    echo "  --version TAG          Model version tag (default: v1)"
    echo "  --export-only PATH     Only export .pt -> ONNX"
    echo "  --verify-only PATH     Only verify ONNX against API"
    echo "  --server URL           AetherAgent server URL (default: http://localhost:3000)"
    echo "  --skip-verify          Skip API verification"
    echo "  --interactive          Interactive wizard mode"
    echo "  --help                 Show this help"
    echo ""
    echo "Examples:"
    echo "  $0                          # Starter dataset + full pipeline"
    echo "  $0 --dataset ./my-data      # Your dataset + full pipeline"
    echo "  $0 --epochs 300 --batch 64  # Custom training config"
}

while [[ $# -gt 0 ]]; do
    case $1 in
        --dataset)       DATASET_DIR="$2"; shift 2 ;;
        --download-starter) DOWNLOAD_STARTER=true; shift ;;
        --epochs)        EPOCHS="$2"; shift 2 ;;
        --batch)         BATCH="$2"; shift 2 ;;
        --imgsz)         IMGSZ="$2"; shift 2 ;;
        --version)       VERSION="$2"; shift 2 ;;
        --export-only)   EXPORT_ONLY="$2"; shift 2 ;;
        --verify-only)   VERIFY_ONLY="$2"; shift 2 ;;
        --server)        SERVER_URL="$2"; shift 2 ;;
        --skip-verify)   SKIP_VERIFY=true; shift ;;
        --interactive)   INTERACTIVE=true; shift ;;
        --help)          usage; exit 0 ;;
        *)               err "Unknown option: $1"; usage; exit 1 ;;
    esac
done

# Default: download starter if no dataset specified
if [[ -z "$DATASET_DIR" && -z "$EXPORT_ONLY" && -z "$VERIFY_ONLY" && "$INTERACTIVE" == false ]]; then
    DOWNLOAD_STARTER=true
fi

# ---------------------------------------------------------------------------
# Steg 0: System-beroenden (apt-paket som WSL kan sakna)
# ---------------------------------------------------------------------------
install_system_deps() {
    step "Steg 0/8: Kontrollerar systemberoenden..."

    local need_install=false
    for pkg in python3 python3-venv python3-pip git wget curl; do
        if ! dpkg -s "$pkg" &>/dev/null; then
            warn "$pkg saknas"
            need_install=true
        fi
    done

    if $need_install; then
        log "Installerar systemberoenden via apt..."
        sudo apt-get update -qq
        sudo apt-get install -y -qq python3 python3-venv python3-pip python3-dev git wget curl libgl1-mesa-glx libglib2.0-0
        ok "Systemberoenden installerade"
    else
        ok "Alla systemberoenden finns redan"
    fi

    # Kolla om NVIDIA-driver finns i WSL
    if command -v nvidia-smi &>/dev/null; then
        log "GPU hittad:"
        nvidia-smi --query-gpu=name,memory.total,driver_version --format=csv,noheader 2>/dev/null || true
    else
        warn "nvidia-smi hittades inte — träning körs på CPU (långsamt)"
        warn "Installera NVIDIA-drivrutiner i Windows och starta om WSL"
    fi
}

# ---------------------------------------------------------------------------
# Steg 1: Skapa venv
# ---------------------------------------------------------------------------
setup_venv() {
    step "Steg 1/8: Skapar Python virtual environment..."

    if [[ -d "$VENV_DIR" && -f "$VENV_DIR/bin/python" ]]; then
        ok "Venv finns redan: $VENV_DIR"
    else
        log "Skapar venv i $VENV_DIR..."
        python3 -m venv "$VENV_DIR"
        ok "Venv skapad"
    fi

    PYTHON="$VENV_DIR/bin/python"

    # Uppgradera pip
    log "Uppgraderar pip..."
    "$PYTHON" -m pip install --upgrade pip --quiet
    ok "pip uppdaterad: $("$PYTHON" -m pip --version)"
}

# ---------------------------------------------------------------------------
# Steg 2: Installera PyTorch + CUDA + Ultralytics
# ---------------------------------------------------------------------------
install_python_deps() {
    step "Steg 2/8: Installerar PyTorch + CUDA + Ultralytics..."

    # Kolla om torch redan finns med CUDA
    if "$PYTHON" -c "import torch; assert torch.cuda.is_available()" 2>/dev/null; then
        local torch_ver
        torch_ver=$("$PYTHON" -c "import torch; print(torch.__version__)")
        ok "PyTorch $torch_ver med CUDA redan installerat"
    else
        log "Installerar PyTorch med CUDA 12.4 (Blackwell/Ada-kompatibel)..."
        "$PYTHON" -m pip install --quiet \
            torch torchvision torchaudio \
            --index-url https://download.pytorch.org/whl/cu124

        # Verifiera
        if "$PYTHON" -c "import torch; print(f'PyTorch {torch.__version__}, CUDA: {torch.cuda.is_available()}')" 2>/dev/null; then
            ok "PyTorch installerat"
        else
            warn "PyTorch installerat men CUDA inte tillgängligt — kör CPU"
        fi
    fi

    # Ultralytics + dependencies
    log "Installerar ultralytics + verktyg..."
    "$PYTHON" -m pip install --quiet \
        ultralytics \
        pillow \
        requests \
        tqdm \
        pyyaml \
        onnx \
        onnxsim \
        opencv-python-headless \
        matplotlib \
        seaborn \
        pandas

    local ultra_ver
    ultra_ver=$("$PYTHON" -c "import ultralytics; print(ultralytics.__version__)")
    ok "Ultralytics $ultra_ver installerat"

    # Valfritt: rten-convert
    log "Försöker installera rten-convert (valfritt)..."
    "$PYTHON" -m pip install --quiet rten-convert 2>/dev/null || warn "rten-convert ej tillgängligt (OK — ONNX fungerar direkt)"
}

# ---------------------------------------------------------------------------
# Steg 3: Ladda ner YOLOv8n basmodell
# ---------------------------------------------------------------------------
download_base_model() {
    step "Steg 3/8: Laddar ner YOLOv8n basmodell..."

    local model_path="$PROJECT_DIR/$MODEL_BASE"

    if [[ -f "$model_path" ]]; then
        ok "Basmodell finns redan: $model_path ($(du -h "$model_path" | cut -f1))"
        return
    fi

    log "Laddar ner $MODEL_BASE från Ultralytics..."
    # Ultralytics laddar ner automatiskt vid första användning, men vi förladdar
    "$PYTHON" -c "
from ultralytics import YOLO
import os
os.chdir('$PROJECT_DIR')
model = YOLO('$MODEL_BASE')
print(f'Modell laddad: {model.model_name}')
"
    ok "Basmodell nedladdad: $model_path"
}

# ---------------------------------------------------------------------------
# Steg 4: Skapa/validera dataset
# ---------------------------------------------------------------------------
prepare_dataset() {
    step "Steg 4/8: Förbereder dataset..."

    if [[ -n "$DATASET_DIR" ]]; then
        if [[ ! -d "$DATASET_DIR" ]]; then
            err "Dataset-sökväg finns inte: $DATASET_DIR"
            exit 1
        fi
        ok "Använder dataset: $DATASET_DIR"
        return
    fi

    if $DOWNLOAD_STARTER; then
        DATASET_DIR="$PROJECT_DIR/dataset"

        if [[ -d "$DATASET_DIR/images/train" ]]; then
            local n_train
            n_train=$(find "$DATASET_DIR/images/train" -name "*.png" -o -name "*.jpg" 2>/dev/null | wc -l)
            if [[ $n_train -gt 0 ]]; then
                ok "Starter-dataset finns redan: $n_train träningsbilder"
                return
            fi
        fi

        log "Genererar syntetiskt UI-dataset (200 train + 40 val)..."
        "$PYTHON" "$SCRIPT_DIR/train_vision.py" --download-starter --skip-verify 2>/dev/null || {
            # Fallback: kör dataset-generering direkt
            "$PYTHON" -c "
import sys
sys.path.insert(0, '$SCRIPT_DIR')
from train_vision import download_starter_dataset
from pathlib import Path
download_starter_dataset(Path('$DATASET_DIR'))
"
        }
        ok "Dataset genererat i $DATASET_DIR"
    fi
}

# ---------------------------------------------------------------------------
# Steg 5: Träna modellen
# ---------------------------------------------------------------------------
train() {
    step "Steg 5/8: Tränar YOLOv8-nano..."

    local run_name="${DEFAULT_NAME:-aether-ui}-$VERSION"

    cd "$PROJECT_DIR"

    log "Parametrar: epochs=$EPOCHS  batch=$BATCH  imgsz=$IMGSZ  version=$VERSION"
    log "Output: runs/detect/$run_name/"

    # Auto-detektera GPU och justera batch
    local effective_batch=$BATCH
    local gpu_mem
    gpu_mem=$("$PYTHON" -c "
import torch
if torch.cuda.is_available():
    props = torch.cuda.get_device_properties(0)
    print(int(props.total_mem / (1024**3)))
else:
    print(0)
" 2>/dev/null || echo "0")

    if [[ "$gpu_mem" -eq 0 ]]; then
        warn "Ingen GPU — sänker batch till 8"
        effective_batch=8
    elif [[ "$gpu_mem" -lt 8 ]]; then
        warn "GPU har bara ${gpu_mem}GB VRAM — sänker batch till 16"
        effective_batch=16
    elif [[ "$gpu_mem" -ge 20 ]]; then
        log "GPU har ${gpu_mem}GB VRAM — kör batch=$effective_batch (RTX 5090 mode)"
    fi

    "$PYTHON" -c "
import os
os.chdir('$PROJECT_DIR')

from ultralytics import YOLO

model = YOLO('$MODEL_BASE')

results = model.train(
    data='$DATASET_DIR/data.yaml',
    epochs=$EPOCHS,
    imgsz=$IMGSZ,
    batch=$effective_batch,
    project='runs/detect',
    name='$run_name',
    exist_ok=True,
    # RTX 5090 / Blackwell optimeringar
    workers=8,
    amp=True,
    cache='ram',
    # UI-anpassad augmentering
    mosaic=0.5,
    mixup=0.0,
    degrees=0.0,
    flipud=0.0,
    fliplr=0.3,
    hsv_h=0.01,
    hsv_s=0.3,
    hsv_v=0.3,
    scale=0.3,
    translate=0.1,
    verbose=True,
    plots=True,
)

print('TRAINING_DONE')
"

    local best_pt="$PROJECT_DIR/runs/detect/$run_name/weights/best.pt"
    if [[ ! -f "$best_pt" ]]; then
        err "best.pt hittades inte efter träning!"
        exit 1
    fi

    ok "Träning klar: $best_pt ($(du -h "$best_pt" | cut -f1))"
}

# ---------------------------------------------------------------------------
# Steg 6: Validera
# ---------------------------------------------------------------------------
validate() {
    step "Steg 6/8: Validerar modellen..."

    local run_name="${DEFAULT_NAME:-aether-ui}-$VERSION"
    local best_pt="$PROJECT_DIR/runs/detect/$run_name/weights/best.pt"

    "$PYTHON" -c "
import os
os.chdir('$PROJECT_DIR')
from ultralytics import YOLO

model = YOLO('$best_pt')
metrics = model.val(data='$DATASET_DIR/data.yaml', imgsz=$IMGSZ, verbose=False)

print(f'mAP@50:    {metrics.box.map50:.4f}')
print(f'mAP@50-95: {metrics.box.map:.4f}')
print(f'Precision:  {metrics.box.mp:.4f}')
print(f'Recall:     {metrics.box.mr:.4f}')

if metrics.box.map50 < 0.3:
    print('WARN: mAP@50 < 0.30 — behöver mer data eller fler epochs')
"
}

# ---------------------------------------------------------------------------
# Steg 7: Exportera ONNX
# ---------------------------------------------------------------------------
export_onnx() {
    step "Steg 7/8: Exporterar till ONNX..."

    local run_name="${DEFAULT_NAME:-aether-ui}-$VERSION"
    local best_pt="${EXPORT_ONLY:-$PROJECT_DIR/runs/detect/$run_name/weights/best.pt}"

    if [[ ! -f "$best_pt" ]]; then
        err "Modell finns inte: $best_pt"
        exit 1
    fi

    "$PYTHON" -c "
import os
os.chdir('$PROJECT_DIR')
from ultralytics import YOLO

model = YOLO('$best_pt')
path = model.export(format='onnx', imgsz=$IMGSZ, opset=17, simplify=True, dynamic=False)
print(f'ONNX_PATH={path}')

import os
size_mb = os.path.getsize(path) / (1024*1024)
print(f'ONNX storlek: {size_mb:.1f} MB')
if size_mb > 6:
    print(f'VARNING: Modellen är {size_mb:.1f} MB (mål < 6 MB)')
"

    # Kopiera till deploy
    local onnx_src="$PROJECT_DIR/runs/detect/$run_name/weights/best.onnx"
    if [[ -n "$EXPORT_ONLY" ]]; then
        onnx_src="${EXPORT_ONLY%.pt}.onnx"
    fi

    mkdir -p "$DEPLOY_DIR"
    cp "$onnx_src" "$DEPLOY_DIR/aether-ui-$VERSION.onnx"
    cp "$onnx_src" "$DEPLOY_DIR/aether-ui-latest.onnx"

    ok "ONNX deployad: $DEPLOY_DIR/aether-ui-$VERSION.onnx"

    # Försök rten-konvertering
    if "$PYTHON" -c "import rten_convert" 2>/dev/null; then
        log "Konverterar till rten-format..."
        "$PYTHON" -m rten_convert "$onnx_src" "$DEPLOY_DIR/aether-ui-$VERSION.rten" 2>/dev/null && \
            ok "rten-format: $DEPLOY_DIR/aether-ui-$VERSION.rten" || \
            warn "rten-konvertering misslyckades (ONNX fungerar ändå)"
    fi
}

# ---------------------------------------------------------------------------
# Steg 8: Verifiera mot AetherAgent
# ---------------------------------------------------------------------------
verify() {
    if $SKIP_VERIFY; then
        log "Hoppar över API-verifiering (--skip-verify)"
        return
    fi

    step "Steg 8/8: Verifierar mot AetherAgent API..."

    local model_path="${VERIFY_ONLY:-$DEPLOY_DIR/aether-ui-$VERSION.onnx}"

    "$PYTHON" -c "
import base64, json, sys
try:
    import requests
except ImportError:
    print('requests ej installerat, hoppar över verifiering')
    sys.exit(0)

# Skapa testbild
from PIL import Image, ImageDraw
img = Image.new('RGB', (640, 640), (255, 255, 255))
draw = ImageDraw.Draw(img)
draw.rounded_rectangle([100, 200, 250, 240], radius=5, fill=(59, 130, 246))
draw.text((120, 210), 'Sign In', fill='white')
draw.rectangle([100, 150, 350, 180], outline=(200, 200, 200), width=2)

import io
buf = io.BytesIO()
img.save(buf, format='PNG')
png_b64 = base64.b64encode(buf.getvalue()).decode()

with open('$model_path', 'rb') as f:
    model_b64 = base64.b64encode(f.read()).decode()

try:
    resp = requests.post('$SERVER_URL/api/parse-screenshot', json={
        'png_base64': png_b64,
        'model_base64': model_b64,
        'goal': 'find the sign in button',
        'config': {
            'confidence_threshold': 0.25,
            'nms_threshold': 0.45,
            'input_size': 640,
            'model_version': 'aether-ui-$VERSION',
        },
    }, timeout=30)

    if resp.status_code == 200:
        result = resp.json()
        n = len(result.get('detections', []))
        ms = result.get('inference_time_ms', '?')
        print(f'API OK: {n} detektioner, {ms}ms inference')
        for det in result.get('detections', [])[:5]:
            print(f\"  {det['class']} (conf={det['confidence']:.2f})\")
    else:
        print(f'API svarade {resp.status_code}: {resp.text[:200]}')
except requests.ConnectionError:
    print(f'Kunde inte ansluta till $SERVER_URL')
    print('Starta servern: cargo run --features server,vision --bin aether-server')
    print('Modellen exporterades korrekt — verifiera manuellt')
except Exception as e:
    print(f'Verifieringsfel: {e}')
" || true
}

# ---------------------------------------------------------------------------
# Slutrapport
# ---------------------------------------------------------------------------
print_summary() {
    local onnx_path="$DEPLOY_DIR/aether-ui-$VERSION.onnx"
    local onnx_size
    onnx_size=$(du -h "$onnx_path" 2>/dev/null | cut -f1 || echo "?")

    echo ""
    echo -e "${GREEN}════════════════════════════════════════════════════════════════${NC}"
    echo -e "${GREEN}  TRÄNING KLAR!${NC}"
    echo -e "${GREEN}════════════════════════════════════════════════════════════════${NC}"
    echo ""
    echo "  Version:        $VERSION"
    echo "  ONNX-modell:    $onnx_path ($onnx_size)"
    echo "  Dataset:        $DATASET_DIR"
    echo "  Epochs:         $EPOCHS"
    echo "  Batch:          $BATCH"
    echo "  Träningsloggar: $PROJECT_DIR/runs/detect/aether-ui-$VERSION/"
    echo ""
    echo "  Använd i Python:"
    echo "    model = open('$onnx_path', 'rb').read()"
    echo "    # Skicka till /api/parse-screenshot"
    echo ""
    echo "  Använd med curl:"
    echo "    MODEL=\$(base64 -w0 $onnx_path)"
    echo "    PNG=\$(base64 -w0 screenshot.png)"
    echo '    curl -X POST http://localhost:3000/api/parse-screenshot \'
    echo '      -H "Content-Type: application/json" \'
    echo '      -d "{\"png_base64\": \"$PNG\", \"model_base64\": \"$MODEL\", \"goal\": \"find buttons\"}"'
    echo ""
    echo -e "${GREEN}════════════════════════════════════════════════════════════════${NC}"
}

# ---------------------------------------------------------------------------
# Huvudflöde
# ---------------------------------------------------------------------------
main() {
    banner

    # --- Export-only mode ---
    if [[ -n "$EXPORT_ONLY" ]]; then
        setup_venv
        install_python_deps
        export_onnx
        verify
        ok "Export klar!"
        exit 0
    fi

    # --- Verify-only mode ---
    if [[ -n "$VERIFY_ONLY" ]]; then
        setup_venv
        install_python_deps
        verify
        exit 0
    fi

    # --- Full pipeline ---
    install_system_deps
    setup_venv
    install_python_deps
    download_base_model
    prepare_dataset

    # Skapa data.yaml om den inte finns
    if [[ ! -f "$DATASET_DIR/data.yaml" ]]; then
        "$PYTHON" -c "
import sys
sys.path.insert(0, '$SCRIPT_DIR')
from train_vision import create_data_yaml
from pathlib import Path
create_data_yaml(Path('$DATASET_DIR'), Path('$DATASET_DIR/data.yaml'))
"
    fi

    train
    validate
    export_onnx
    verify
    print_summary
}

main
