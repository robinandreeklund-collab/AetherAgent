#!/usr/bin/env python3
"""
AetherAgent Vision Training Pipeline
=====================================

Automated end-to-end pipeline: dataset → train → export → deploy → verify.
Designed for RTX 5090 (24 GB VRAM). Single command, zero manual steps.

Usage:
    # Full pipeline with existing labeled dataset:
    python tools/train_vision.py --dataset ./my-dataset

    # Full pipeline — download WebUI-7K starter dataset:
    python tools/train_vision.py --download-starter

    # Just export an existing .pt model to ONNX:
    python tools/train_vision.py --export-only runs/detect/aether-ui-v1/weights/best.pt

    # Just verify a model against AetherAgent API:
    python tools/train_vision.py --verify-only model.onnx --server http://localhost:3000

    # Interactive mode (step by step with prompts):
    python tools/train_vision.py --interactive

Requirements:
    pip install ultralytics>=8.3 pillow requests tqdm pyyaml
"""

import argparse
import base64
import json
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path

# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

# Basklasser — namnen matchar Rust-rollerna i parser.rs/types.rs
UI_CLASSES = [
    "button", "textbox", "link", "icon", "text",
    "img", "checkbox", "radio", "combobox", "heading",
]

# Utökade agentsemantiska klasser — aktiveras med --extended-classes
# Dessa ger modellen förmågan att skilja på t.ex. pris vs vanlig text,
# CTA-knapp vs generell knapp, produktbild vs dekorationsbild.
# Kräver om-träning om man byter — kan inte blandas med standard 10-klassmodell.
# Namnen matchar exakt Rust-rollerna i parser.rs/types.rs:
#   nav → "navigation", search → "searchbox", select → "combobox"
UI_CLASSES_EXTENDED = [
    "button",          # 0  - generell knapp
    "textbox",         # 1  - textfält (Rust: "textbox")
    "link",            # 2  - klickbar länk
    "icon",            # 3  - ikon
    "text",            # 4  - generell text
    "img",             # 5  - bild (Rust: "img")
    "checkbox",        # 6  - checkbox
    "radio",           # 7  - radioknapp
    "combobox",        # 8  - dropdown (Rust: "combobox")
    "heading",         # 9  - rubrik
    "price",           # 10 - pristext (valuta, siffror)
    "cta",             # 11 - call-to-action (köp, lägg i kundvagn)
    "product_card",    # 12 - produktkort (bild + text + pris)
    "navigation",      # 13 - navigering (Rust: "navigation")
    "searchbox",       # 14 - sökfält (Rust: "searchbox")
    "form",            # 15 - formulärgrupp
]

# RTX 5090 defaults (24 GB VRAM)
DEFAULT_EPOCHS = 300
DEFAULT_BATCH = 32
DEFAULT_IMGSZ = 640
DEFAULT_MODEL_BASE = "yolo26n.pt"  # YOLO26 nano — NMS-free, edge-optimized, ONNX < 6 MB
DEFAULT_PROJECT = str(Path(__file__).resolve().parent.parent / "runs" / "detect")
DEFAULT_NAME = "aether-ui"
_REPO_ROOT = Path(__file__).resolve().parent.parent

# Early stopping — metric-baserade mål
# Optimerade trösklar för UI-element detection (YOLO26n @ 640px):
#   mAP@50 ≥ 0.65  = stark detection, redo för produktion
#   mAP@50-95 ≥ 0.50 = bra lokalisering, stabil modell
#   Patience 30 = stoppa om ingen förbättring på 30 epochs (platå)
DEFAULT_TARGET_MAP50 = 0.65
DEFAULT_TARGET_MAP5095 = 0.50
DEFAULT_PATIENCE = 30

BANNER = r"""
 ╔═══════════════════════════════════════════════════════════════╗
 ║          AetherAgent Vision Training Pipeline                ║
 ║          YOLO26-nano · Ultralytics · RTX 5090                ║
 ╚═══════════════════════════════════════════════════════════════╝
"""

# Modellval — visas i interaktiv prompt
AVAILABLE_MODELS = [
    ("yolo26n.pt", "YOLO26 nano  — NMS-free, 43% snabbare CPU, edge-optimerad (REKOMMENDERAD)"),
    ("yolo26s.pt", "YOLO26 small — NMS-free, högre precision, lite tyngre"),
    ("yolo11n.pt", "YOLO11 nano  — beprövad, stabil, bra balans"),
    ("yolo11s.pt", "YOLO11 small — mer precision, långsammare"),
    ("yolov8n.pt", "YOLOv8 nano  — legacy, äldre arkitektur"),
]


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def log(msg: str, level: str = "INFO"):
    colors = {"INFO": "\033[36m", "OK": "\033[32m", "WARN": "\033[33m", "ERR": "\033[31m", "STEP": "\033[35m"}
    reset = "\033[0m"
    color = colors.get(level, "")
    print(f"{color}[{level}]{reset} {msg}")


def prompt_model_selection() -> str:
    """Interaktiv modellväljare i terminalen."""
    print("\n  Välj basmodell för träning:\n")
    for i, (name, desc) in enumerate(AVAILABLE_MODELS, 1):
        marker = " *" if name == DEFAULT_MODEL_BASE else "  "
        print(f"  {marker} {i}) {name:16s} {desc}")
    print()

    while True:
        choice = input(f"  Modell [1-{len(AVAILABLE_MODELS)}, Enter={DEFAULT_MODEL_BASE}]: ").strip()
        if not choice:
            return DEFAULT_MODEL_BASE
        try:
            idx = int(choice)
            if 1 <= idx <= len(AVAILABLE_MODELS):
                selected = AVAILABLE_MODELS[idx - 1][0]
                log(f"Vald modell: {selected}", "OK")
                return selected
        except ValueError:
            # Tillåt direkt modellnamn, t.ex. "yolo26n.pt"
            if choice.endswith(".pt"):
                log(f"Custom modell: {choice}", "OK")
                return choice
        print(f"  Ogiltigt val. Ange 1-{len(AVAILABLE_MODELS)} eller ett modellnamn (.pt)")


def _find_latest_model() -> Path | None:
    """Hitta senaste best.pt i runs/detect/ för auto-chaining.

    Söker igenom alla aether-ui-*/weights/best.pt och returnerar
    den med senaste mtime (= senaste avslutade träning).
    """
    project_dir = Path(DEFAULT_PROJECT)
    if not project_dir.exists():
        return None

    candidates = sorted(
        project_dir.glob(f"{DEFAULT_NAME}-*/weights/best.pt"),
        key=lambda p: p.stat().st_mtime,
        reverse=True,
    )

    if candidates:
        return candidates[0]
    return None


def run(cmd: str, check: bool = True, capture: bool = False):
    """Run a shell command, streaming output."""
    log(f"$ {cmd}", "INFO")
    if capture:
        result = subprocess.run(cmd, shell=True, capture_output=True, text=True)
        if check and result.returncode != 0:
            log(f"Command failed: {result.stderr}", "ERR")
            sys.exit(1)
        return result.stdout.strip()
    else:
        result = subprocess.run(cmd, shell=True)
        if check and result.returncode != 0:
            log(f"Command failed with code {result.returncode}", "ERR")
            sys.exit(1)
        return None


def _detect_gpu_arch():
    """Detektera GPU compute capability utan att krascha.

    Returnerar (namn, sm_sträng, vram_gb) eller None.
    """
    try:
        import torch
        if not torch.cuda.is_available():
            return None
        name = torch.cuda.get_device_name(0)
        props = torch.cuda.get_device_properties(0)
        mem = props.total_memory / (1024**3)
        sm = f"sm_{props.major}{props.minor}"
        return name, sm, mem
    except Exception:
        return None


def _install_pytorch_for_blackwell():
    """Installerar PyTorch med Blackwell (sm_120) stöd.

    RTX 5090/5080/5070 kräver CUDA 12.8+ kernels.
    """
    log("Installerar PyTorch med Blackwell/RTX 5090 stöd...", "STEP")
    log("Detta kan ta 2-5 minuter (laddar ner ~2 GB)...", "INFO")

    # Avinstallera gammal torch först för att undvika konflikter
    run(f"{sys.executable} -m pip uninstall -y torch torchvision torchaudio 2>/dev/null",
        check=False, capture=True)

    # Installera PyTorch med CUDA 12.8 stöd (Blackwell-kompatibel)
    # Försök stable först, sedan nightly
    install_cmds = [
        # Stable med cu128 (om tillgänglig)
        f"{sys.executable} -m pip install torch torchvision "
        f"--index-url https://download.pytorch.org/whl/cu128",
        # Nightly med cu128 (alltid tillgänglig)
        f"{sys.executable} -m pip install --pre torch torchvision "
        f"--index-url https://download.pytorch.org/whl/nightly/cu128",
    ]

    for cmd in install_cmds:
        result = subprocess.run(cmd, shell=True, capture_output=True, text=True)
        if result.returncode == 0:
            log("PyTorch med Blackwell-stöd installerat!", "OK")
            # Verifiera
            verify = subprocess.run(
                f'{sys.executable} -c "import torch; print(torch.__version__); '
                f't = torch.zeros(1, device=\\"cuda\\"); print(\\"GPU OK\\")"',
                shell=True, capture_output=True, text=True
            )
            if verify.returncode == 0:
                log(f"Verifierat: {verify.stdout.strip()}", "OK")
                return True
            log("Installation klar men GPU-verifiering misslyckades, provar nästa...", "WARN")
        else:
            log(f"Kunde inte installera med detta index, provar nästa...", "INFO")

    log("Kunde inte installera Blackwell-kompatibel PyTorch automatiskt", "ERR")
    log("Manuell installation:", "INFO")
    log("  pip install --pre torch torchvision --index-url "
        "https://download.pytorch.org/whl/nightly/cu128", "INFO")
    return False


def check_gpu():
    """Check CUDA availability and GPU compatibility.

    Returnerar 'cuda' eller 'cpu'.
    Detekterar RTX 5090 (Blackwell, sm_120) och installerar rätt PyTorch automatiskt.
    """
    try:
        import torch
    except ImportError:
        log("PyTorch inte installerat — installerar...", "WARN")
        run(f"{sys.executable} -m pip install torch torchvision", check=False)
        try:
            import torch
        except ImportError:
            log("Kunde inte installera PyTorch", "ERR")
            return "cpu"

    if not torch.cuda.is_available():
        log("Ingen CUDA GPU detekterad — tränar på CPU (långsamt!)", "WARN")
        return "cpu"

    name = torch.cuda.get_device_name(0)
    props = torch.cuda.get_device_properties(0)
    mem = props.total_memory / (1024**3)
    sm = f"sm_{props.major}{props.minor}"

    log(f"GPU: {name} ({mem:.1f} GB VRAM, {sm})", "OK")

    # Testa om PyTorch faktiskt kan köra på denna GPU
    try:
        test_tensor = torch.zeros(1, device="cuda")
        del test_tensor
        log("GPU-kompatibilitet verifierad", "OK")
        return "cuda"
    except RuntimeError as e:
        err_str = str(e)
        if "no kernel image" not in err_str and "not compatible" not in err_str:
            raise

    # GPU hittad men PyTorch saknar stöd — troligen Blackwell (sm_120)
    log(f"PyTorch {torch.__version__} saknar stöd för {name} ({sm})", "WARN")

    # Automatisk fix: installera rätt version
    if _install_pytorch_for_blackwell():
        # Ladda om torch-modulen efter ny installation
        import importlib
        importlib.reload(torch)
        return "cuda"

    # Om auto-install misslyckades, falla tillbaka till CPU
    log("Faller tillbaka till CPU-träning", "WARN")
    return "cpu"


def ensure_deps():
    """Install required Python packages if missing."""
    required = ["ultralytics", "PIL", "requests", "tqdm", "yaml"]
    pkg_map = {"PIL": "pillow", "yaml": "pyyaml"}
    missing = []
    for pkg in required:
        try:
            __import__(pkg)
        except ImportError:
            missing.append(pkg_map.get(pkg, pkg))

    if missing:
        log(f"Installing missing packages: {', '.join(missing)}", "INFO")
        run(f"{sys.executable} -m pip install {' '.join(missing)}")
        log("Dependencies installed", "OK")
    else:
        log("All dependencies present", "OK")


# ---------------------------------------------------------------------------
# Dataset Downloads
# ---------------------------------------------------------------------------

# Kända dataset-URL:er och metadata
_DATASET_REGISTRY = {
    "rico": {
        "name": "Rico UI Screenshots",
        "url": "https://storage.googleapis.com/crowdstf-rico-uiuc-4540/rico_dataset_v0.1/unique_uis.tar.gz",
        "annotations_url": "https://storage.googleapis.com/crowdstf-rico-uiuc-4540/rico_dataset_v0.1/semantic_annotations.zip",
        "size_hint": "~6 GB (screenshots) + ~300 MB (annotations)",
        "description": "66K Android UI screenshots med semantiska annotations",
    },
    "coco": {
        "name": "Common Objects in Context (COCO) 2017 — Val",
        "url": "http://images.cocodataset.org/zips/val2017.zip",
        "annotations_url": "http://images.cocodataset.org/annotations/annotations_trainval2017.zip",
        "size_hint": "~1 GB (val images) + ~250 MB (annotations)",
        "description": "COCO 2017 validation set — generella objekt, mappas till UI-klasser",
    },
    "webui": {
        "name": "WebUI — web element screenshots",
        "url": "https://huggingface.co/datasets/poni-ai/webui-7k/resolve/main/data.zip",
        "size_hint": "~2 GB",
        "description": "7K web UI screenshots med element-annotations",
    },
    # ── 2025-2026 datasets (HuggingFace) ──────────────────────────────
    "osatlas": {
        "name": "OS-Atlas Web Subset — 270K+ web screenshots, 3M+ element",
        "hf_dataset": "OS-Copilot/OS-Atlas-data",
        "hf_subset": "web",
        "size_hint": "~15 GB",
        "description": "ICLR 2025 Spotlight. Störst cross-platform GUI grounding corpus. "
                       "Vi laddar ner web-subsetet (270K screenshots, 3M+ element).",
    },
    "guiactor": {
        "name": "GUI-Actor-Data — 1M screenshots, 10M element",
        "hf_dataset": "cckevinn/GUI-Actor-Data",
        "size_hint": "~30 GB",
        "description": "Merge av 6 publika datasets (Uground, GUICourse, AMEX, AndroidControl, Wave-UI). "
                       "Bbox supervision, web + mobil + desktop.",
    },
    "showui-web": {
        "name": "ShowUI Web — 22K screenshots, 576K interaktiva element",
        "hf_dataset": "Voxel51/ShowUI_Web",
        "size_hint": "~5 GB",
        "description": "CVPR 2025. Filtrerar bort statisk text → fokus på interaktiva UI-element. "
                       "Perfekt för agent-orienterad detection.",
    },
    "waveui": {
        "name": "WaveUI-25K — curated, dedup, 25K samples",
        "hf_dataset": "agentsea/wave-ui-25k",
        "size_hint": "~10.6 GB",
        "description": "Curated och dedup:at från WebUI + Roboflow + GroundUI-18K. "
                       "LLM-berikade element-beskrivningar. Dec 2025.",
    },
    "yashjain": {
        "name": "UI-Elements-Detection — YOLO-format, webfokus",
        "hf_repo": "YashJain/UI-Elements-Detection-Dataset",
        "size_hint": "~2 GB",
        "description": "Direkt YOLO-format med train/val/test splits. Klasser: buttons, links, "
                       "inputs, checkboxes, radios, dropdowns, sliders, toggles, labels, icons. Okt 2025.",
    },
    # ── Nya datasets (2026) ──────────────────────────────────────────
    "klarna": {
        "name": "Klarna Product Page Dataset — 51.7K e-handelssidor",
        "github_repo": "klarna/product-page-dataset",
        "screenshots_url": "https://klarna-product-page-dataset.s3.eu-west-1.amazonaws.com/screenshots.zip",
        "wtl_url": "https://klarna-product-page-dataset.s3.eu-west-1.amazonaws.com/klarna-product-page-dataset-wtl.zip",
        "size_hint": "~8 GB (screenshots) + ~15 GB (WTL metadata)",
        "description": "51.7K e-handelssidor från 8175 sajter. Annoterade element: Price, Name, "
                       "Main picture, Add to cart, Cart. WTL-metadata med bboxar.",
    },
    "webclick": {
        "name": "Hcompany/WebClick — 1639 web-screenshots, intent-annoterat",
        "hf_dataset": "Hcompany/WebClick",
        "size_hint": "~500 MB",
        "description": "1639 engelska web-screenshots från 100+ sajter. ScreenSpot-format: "
                       "bild + naturligt språk-instruktion + exakt bbox (HTML-gränser). "
                       "Intent-annoterat för agent-navigation.",
    },
    "roboflow-ui": {
        "name": "Roboflow UI Screenshots — 1800 bilder, 8 klasser",
        "roboflow_workspace": "webuiproject",
        "roboflow_project": "ui-screenshots",
        "roboflow_version": 1,
        "size_hint": "~300 MB",
        "description": "1800 web-UI screenshots med 8 klasser: button, field, heading, iframe, "
                       "image, label, link, text. YOLO-format, MIT-licens.",
    },
}


def _download_file(url: str, dest: Path, desc: str = ""):
    """Ladda ner en fil med progress-visning."""
    import urllib.request

    dest.parent.mkdir(parents=True, exist_ok=True)

    if dest.exists():
        log(f"Redan nedladdat: {dest}", "OK")
        return

    log(f"Laddar ner {desc or dest.name}...", "STEP")
    log(f"  URL: {url}", "INFO")

    try:
        from tqdm import tqdm

        class _TqdmHook:
            def __init__(self):
                self.pbar = None

            def __call__(self, block_num, block_size, total_size):
                if self.pbar is None:
                    self.pbar = tqdm(total=total_size, unit="B", unit_scale=True, desc=dest.name)
                downloaded = block_num * block_size
                self.pbar.update(block_size)
                if downloaded >= total_size and self.pbar:
                    self.pbar.close()

        urllib.request.urlretrieve(url, str(dest), _TqdmHook())
    except ImportError:
        # tqdm saknas — enkel nedladdning utan progress
        urllib.request.urlretrieve(url, str(dest))

    log(f"Nedladdat: {dest} ({dest.stat().st_size / (1024**2):.0f} MB)", "OK")


def _extract_archive(archive: Path, dest: Path):
    """Packa upp .tar.gz eller .zip med progress-indikator."""
    import tarfile
    import zipfile

    dest.mkdir(parents=True, exist_ok=True)

    # WSL-varning: /mnt/c/ är extremt långsam för många filer
    if "/mnt/c" in str(dest) or "/mnt/d" in str(dest):
        proc_version = Path("/proc/version")
        if proc_version.exists() and "microsoft" in proc_version.read_text().lower():
            log("WSL detekterat: /mnt/c/ är ~10x långsammare för filoperationer", "WARN")
            log("Rekommendation: kör från ~/AetherAgent istället (native WSL-filsystem)", "WARN")

    log(f"Packar upp {archive.name} → {dest}...", "STEP")

    if archive.name.endswith(".tar.gz") or archive.name.endswith(".tgz"):
        with tarfile.open(archive, "r:gz") as tar:
            members = tar.getmembers()
            total = len(members)
            log(f"Extraherar {total} filer...", "INFO")
            for i, member in enumerate(members, 1):
                tar.extract(member, path=dest, filter="data")
                if i % 1000 == 0 or i == total:
                    pct = i * 100 // total
                    print(f"\r  [{pct:3d}%] {i}/{total} filer extraherade", end="", flush=True)
            print()  # Ny rad efter progress
    elif archive.name.endswith(".zip"):
        with zipfile.ZipFile(archive, "r") as z:
            members = z.namelist()
            total = len(members)
            log(f"Extraherar {total} filer...", "INFO")
            for i, name in enumerate(members, 1):
                z.extract(name, path=dest)
                if i % 1000 == 0 or i == total:
                    pct = i * 100 // total
                    print(f"\r  [{pct:3d}%] {i}/{total} filer extraherade", end="", flush=True)
            print()
    else:
        log(f"Okänt arkivformat: {archive.name}", "ERR")
        sys.exit(1)

    log(f"Uppackat till {dest}", "OK")


def download_dataset(fmt: str, output_dir: Path) -> Path:
    """Ladda ner ett dataset baserat på format.

    Args:
        fmt: "rico", "coco", eller "webui"
        output_dir: Rotkatalog för nedladdning (t.ex. dataset/downloads/)

    Returns:
        Sökväg till det uppackade datasetet (redo för konvertering)
    """
    if fmt not in _DATASET_REGISTRY:
        log(f"Inget känt dataset för format '{fmt}'", "ERR")
        log(f"Kända format: {', '.join(_DATASET_REGISTRY.keys())}", "INFO")
        sys.exit(1)

    info = _DATASET_REGISTRY[fmt]
    dl_dir = output_dir / "downloads"
    extract_dir = output_dir / f"{fmt}_raw"

    log(f"Dataset: {info['name']}", "STEP")
    log(f"  Beskrivning: {info['description']}", "INFO")
    log(f"  Storlek: {info['size_hint']}", "INFO")

    if fmt == "rico":
        return _download_rico(info, dl_dir, extract_dir)
    elif fmt == "coco":
        return _download_coco(info, dl_dir, extract_dir)
    elif fmt == "webui":
        return _download_webui(info, dl_dir, extract_dir)
    elif fmt == "klarna":
        return _download_klarna(info, dl_dir, extract_dir)
    elif fmt == "webclick":
        return _download_webclick(info, extract_dir)
    elif fmt == "roboflow-ui":
        return _download_roboflow_ui(info, extract_dir)
    elif "hf_repo" in info:
        return _download_hf_repo(info, extract_dir)
    elif "hf_dataset" in info:
        return _download_hf_dataset(info, extract_dir)

    log(f"Nedladdning ej implementerad för {fmt}", "ERR")
    sys.exit(1)


def _download_rico(info: dict, dl_dir: Path, extract_dir: Path) -> Path:
    """Ladda ner Rico-dataset (screenshots + semantic annotations)."""
    screenshots_archive = dl_dir / "rico_unique_uis.tar.gz"
    annotations_archive = dl_dir / "rico_semantic_annotations.zip"

    _download_file(info["url"], screenshots_archive, "Rico screenshots")
    _download_file(info["annotations_url"], annotations_archive, "Rico annotations")

    # Packa upp
    screenshots_dir = extract_dir / "screenshots"
    annotations_dir = extract_dir / "semantic_annotations"

    if not screenshots_dir.exists():
        _extract_archive(screenshots_archive, extract_dir)
    if not annotations_dir.exists():
        _extract_archive(annotations_archive, extract_dir)

    # Rico-konverteraren förväntar sig: rico_dir/semantic_annotations/ + rico_dir/screenshots/
    # Kontrollera att strukturen stämmer
    # Ibland packas Rico upp med extra wrapper-mapp
    known_dirs = ("semantic_annotations", "combined", "jsons")

    # Kontrollera direkt i extract_dir
    for name in known_dirs:
        if (extract_dir / name).exists():
            return extract_dir

    # Sök en nivå ner (wrapper-mappar från arkivet)
    for child in extract_dir.iterdir():
        if child.is_dir():
            for name in known_dirs:
                if (child / name).exists():
                    return child
            # Mappen själv kan vara combined/ eller semantic_annotations/
            if child.name in known_dirs:
                return extract_dir

    # Sök två nivåer ner (dubbla wrappers, t.ex. rico_dataset_v0.1/data/combined/)
    for child in extract_dir.iterdir():
        if child.is_dir():
            for grandchild in child.iterdir():
                if grandchild.is_dir():
                    for name in known_dirs:
                        if (grandchild / name).exists():
                            return grandchild
                    if grandchild.name in known_dirs:
                        return child

    # Kontrollera om det finns JSON-filer direkt (platt struktur)
    if any(extract_dir.glob("*.json")):
        return extract_dir

    log(f"Rico-dataset uppackat till {extract_dir} men kunde inte hitta förväntad struktur", "WARN")
    log("Förväntad: semantic_annotations/ + screenshots/ ELLER combined/ + screenshot/", "WARN")
    log(f"Innehåll: {[p.name for p in list(extract_dir.iterdir())[:20]]}", "INFO")
    return extract_dir


def _download_coco(info: dict, dl_dir: Path, extract_dir: Path) -> Path:
    """Ladda ner COCO val2017 (bilder + annotations)."""
    images_archive = dl_dir / "val2017.zip"
    annotations_archive = dl_dir / "annotations_trainval2017.zip"

    _download_file(info["url"], images_archive, "COCO val2017 images")
    _download_file(info["annotations_url"], annotations_archive, "COCO annotations")

    images_dir = extract_dir / "images"
    if not images_dir.exists() and not (extract_dir / "val2017").exists():
        _extract_archive(images_archive, extract_dir)
    if not (extract_dir / "annotations").exists():
        _extract_archive(annotations_archive, extract_dir)

    # COCO packar upp till val2017/ — rename till images/ om det behövs
    val_dir = extract_dir / "val2017"
    if val_dir.exists() and not images_dir.exists():
        val_dir.rename(images_dir)
        log("Döpte om val2017/ → images/", "INFO")

    return extract_dir


def _download_webui(info: dict, dl_dir: Path, extract_dir: Path) -> Path:
    """Ladda ner WebUI-dataset."""
    archive = dl_dir / "webui_data.zip"

    _download_file(info["url"], archive, "WebUI dataset")

    if not extract_dir.exists() or not any(extract_dir.iterdir()):
        _extract_archive(archive, extract_dir)

    # WebUI kan ha en extra wrapper-mapp
    children = [c for c in extract_dir.iterdir() if c.is_dir()]
    if len(children) == 1:
        child = children[0]
        # Om det bara finns en undermapp och den innehåller data, använd den
        if any(child.glob("*.json")) or any(child.glob("*.jsonl")) or (child / "annotations").exists():
            return child

    return extract_dir


def _download_klarna(info: dict, dl_dir: Path, extract_dir: Path) -> Path:
    """Ladda ner Klarna Product Page Dataset (screenshots + WTL metadata med bboxar).

    Datasetet innehåller 51.7K e-handelssidor. WTL-snapshots har elementmetadata
    med bounding boxes och klarna-ai-label attribut (Price, Name, Main picture,
    Add to cart, Cart).
    """
    extract_dir.mkdir(parents=True, exist_ok=True)
    marker = extract_dir / ".download_complete"
    if marker.exists():
        log(f"Klarna-dataset redan nedladdat: {extract_dir}", "OK")
        return extract_dir

    screenshots_archive = dl_dir / "klarna_screenshots.zip"
    wtl_archive = dl_dir / "klarna_wtl.zip"

    _download_file(info["screenshots_url"], screenshots_archive, "Klarna screenshots")
    _download_file(info["wtl_url"], wtl_archive, "Klarna WTL metadata")

    screenshots_dir = extract_dir / "screenshots"
    wtl_dir = extract_dir / "wtl"

    if not screenshots_dir.exists():
        _extract_archive(screenshots_archive, screenshots_dir)
    if not wtl_dir.exists():
        _extract_archive(wtl_archive, wtl_dir)

    marker.touch()
    return extract_dir


def _download_webclick(info: dict, extract_dir: Path) -> Path:
    """Ladda ner Hcompany/WebClick och konvertera till YOLO.

    WebClick är i ScreenSpot-format: bild + instruktion + bbox.
    Vi laddar ner via HuggingFace och konverterar direkt med
    _convert_webclick_to_yolo().
    """
    hf_name = info["hf_dataset"]

    extract_dir.mkdir(parents=True, exist_ok=True)
    marker = extract_dir / ".hf_download_complete"
    if marker.exists():
        log(f"WebClick redan nedladdat: {extract_dir}", "OK")
        return extract_dir

    log(f"Laddar ner {hf_name} från HuggingFace...", "STEP")

    try:
        from datasets import load_dataset
    except ImportError:
        log("Installerar 'datasets' (HuggingFace)...", "INFO")
        run(f"{sys.executable} -m pip install datasets")
        from datasets import load_dataset

    try:
        ds = load_dataset(hf_name, trust_remote_code=True)
    except Exception as e:
        log(f"Kunde inte ladda {hf_name}: {e}", "ERR")
        sys.exit(1)

    _convert_webclick_to_yolo(ds, extract_dir)

    marker.touch()
    log(f"WebClick-dataset klart: {extract_dir}", "OK")
    return extract_dir


def _download_roboflow_ui(info: dict, extract_dir: Path) -> Path:
    """Ladda ner Roboflow UI Screenshots dataset (YOLO-format).

    Försöker via Roboflow API (om API-nyckel finns), annars snapshot_download
    från HuggingFace-mirror.
    """
    extract_dir.mkdir(parents=True, exist_ok=True)
    marker = extract_dir / ".download_complete"
    if marker.exists():
        log(f"Roboflow UI-dataset redan nedladdat: {extract_dir}", "OK")
        return _find_yolo_root(extract_dir)

    workspace = info["roboflow_workspace"]
    project = info["roboflow_project"]
    version = info["roboflow_version"]

    api_key = os.environ.get("ROBOFLOW_API_KEY", "")

    if api_key:
        log(f"Laddar ner {workspace}/{project} v{version} via Roboflow API...", "STEP")
        try:
            from roboflow import Roboflow
        except ImportError:
            log("Installerar 'roboflow'...", "INFO")
            run(f"{sys.executable} -m pip install roboflow")
            from roboflow import Roboflow

        rf = Roboflow(api_key=api_key)
        rf_project = rf.workspace(workspace).project(project)
        rf_dataset = rf_project.version(version).download("yolov8", location=str(extract_dir))
        log(f"Roboflow-dataset nedladdat: {rf_dataset.location if rf_dataset else extract_dir}", "OK")
    else:
        log("Ingen ROBOFLOW_API_KEY — laddar ner via URL-export...", "STEP")
        log("  Sätt ROBOFLOW_API_KEY i miljön för automatisk nedladdning.", "INFO")
        log(f"  Skapa gratis konto på roboflow.com, gå till {workspace}/{project}", "INFO")

        # Roboflow offentliga dataset har en publik export-URL
        export_url = (
            f"https://universe.roboflow.com/ds/"
            f"{workspace}/{project}/{version}/yolov8"
        )
        archive = extract_dir / "roboflow_ui.zip"
        try:
            _download_file(export_url, archive, "Roboflow UI Screenshots (YOLO)")
            _extract_archive(archive, extract_dir)
        except Exception as e:
            log(f"Nedladdning utan API-nyckel misslyckades: {e}", "ERR")
            log("Sätt ROBOFLOW_API_KEY eller ladda ner manuellt:", "INFO")
            log(f"  1. Gå till https://universe.roboflow.com/{workspace}/{project}", "INFO")
            log("  2. Klicka 'Download Dataset' → YOLOv8 format", "INFO")
            log(f"  3. Packa upp i {extract_dir}/", "INFO")
            sys.exit(1)

    marker.touch()
    yolo_root = _find_yolo_root(extract_dir)
    log(f"Roboflow YOLO-dataset klart: {yolo_root}", "OK")
    return yolo_root


def _download_hf_repo(info: dict, extract_dir: Path) -> Path:
    """Ladda ner ett HuggingFace-repo som är YOLO-format (git clone)."""
    repo_id = info["hf_repo"]

    extract_dir.mkdir(parents=True, exist_ok=True)
    marker = extract_dir / ".hf_download_complete"
    if marker.exists():
        log(f"HuggingFace-repo redan klonat: {extract_dir}", "OK")
        return _find_yolo_root(extract_dir)

    log(f"Klonar {repo_id} från HuggingFace...", "STEP")

    # Försök med huggingface_hub först (hanterar LFS bättre)
    try:
        from huggingface_hub import snapshot_download
        snapshot_download(
            repo_id=repo_id,
            repo_type="dataset",
            local_dir=str(extract_dir),
        )
    except ImportError:
        log("Installerar 'huggingface_hub'...", "INFO")
        run(f"{sys.executable} -m pip install huggingface_hub")
        from huggingface_hub import snapshot_download
        snapshot_download(
            repo_id=repo_id,
            repo_type="dataset",
            local_dir=str(extract_dir),
        )

    marker.touch()
    yolo_root = _find_yolo_root(extract_dir)
    log(f"YOLO-dataset klart: {yolo_root}", "OK")
    return yolo_root


def _find_yolo_root(base_dir: Path) -> Path:
    """Hitta roten av ett YOLO-dataset (mappen som innehåller images/ eller train/)."""
    # Direkt YOLO-struktur: images/train/ + labels/train/
    if (base_dir / "images" / "train").exists():
        return base_dir
    # Alternativ: train/images/ (som YashJain)
    if (base_dir / "train" / "images").exists():
        return base_dir
    # dataset.yaml finns?
    if (base_dir / "dataset.yaml").exists() or (base_dir / "data.yaml").exists():
        return base_dir
    # Sök en nivå ner
    for child in base_dir.iterdir():
        if child.is_dir():
            if (child / "images" / "train").exists():
                return child
            if (child / "train" / "images").exists():
                return child
            if (child / "dataset.yaml").exists() or (child / "data.yaml").exists():
                return child
    # Fallback
    return base_dir


def _download_hf_dataset(info: dict, extract_dir: Path) -> Path:
    """Ladda ner ett HuggingFace-dataset via snapshot_download + direkt konvertering.

    Steg 1: snapshot_download() — laddar ner ALLA filer parallellt (8+ trådar).
    Steg 2: Om FiftyOne-format (samples.json) — konverterar direkt utan load_dataset().
             Annars fallback till load_dataset() från lokal kopia.
    """
    hf_name = info["hf_dataset"]
    hf_subset = info.get("hf_subset")

    extract_dir.mkdir(parents=True, exist_ok=True)

    marker = extract_dir / ".hf_download_complete"
    if marker.exists():
        log(f"HuggingFace-dataset redan nedladdat: {extract_dir}", "OK")
        return extract_dir

    # --- Steg 1: Parallell bulk-nedladdning via snapshot_download ---
    log(f"Laddar ner {hf_name} från HuggingFace (parallellt)...", "STEP")
    if hf_subset:
        log(f"  Subset: {hf_subset}", "INFO")

    snapshot_dir = extract_dir / "snapshot"
    try:
        from huggingface_hub import snapshot_download
    except ImportError:
        log("Installerar 'huggingface_hub'...", "INFO")
        run(f"{sys.executable} -m pip install huggingface_hub")
        from huggingface_hub import snapshot_download

    try:
        snapshot_download(
            repo_id=hf_name,
            repo_type="dataset",
            local_dir=str(snapshot_dir),
        )
        log(f"Snapshot-nedladdning klar: {snapshot_dir}", "OK")
    except Exception as e:
        log(f"snapshot_download misslyckades: {e}", "WARN")
        log("Faller tillbaka på load_dataset() (långsammare)...", "WARN")
        snapshot_dir = None

    # --- Steg 2: Konvertera till YOLO ---
    samples_json = snapshot_dir / "samples.json" if snapshot_dir else None

    if samples_json and samples_json.exists():
        # FiftyOne-format: konvertera direkt utan load_dataset() (mycket snabbare)
        _convert_fiftyone_to_yolo(snapshot_dir, extract_dir, info)
    else:
        # Fallback: använd load_dataset() (t.ex. parquet-baserade dataset)
        try:
            from datasets import load_dataset
        except ImportError:
            log("Installerar 'datasets' (HuggingFace)...", "INFO")
            run(f"{sys.executable} -m pip install datasets")
            from datasets import load_dataset

        kwargs = {"trust_remote_code": True}
        if hf_subset:
            kwargs["name"] = hf_subset

        try:
            if snapshot_dir and snapshot_dir.exists():
                log("Laddar dataset från lokal snapshot...", "STEP")
                ds = load_dataset(str(snapshot_dir), **kwargs)
            else:
                ds = load_dataset(hf_name, **kwargs)
        except Exception as e:
            log(f"Kunde inte ladda med subset, provar utan: {e}", "WARN")
            try:
                if snapshot_dir and snapshot_dir.exists():
                    ds = load_dataset(str(snapshot_dir), trust_remote_code=True)
                else:
                    ds = load_dataset(hf_name, trust_remote_code=True)
            except Exception as e2:
                log(f"Kunde inte ladda {hf_name}: {e2}", "ERR")
                sys.exit(1)

        # Extrahera bilder och bboxar till YOLO-struktur
        _convert_hf_to_yolo(ds, extract_dir, info)

    marker.touch()
    log(f"HuggingFace-dataset klart: {extract_dir}", "OK")
    return extract_dir


def _map_fiftyone_label_to_ui_class(label: str) -> int | None:
    """Mappa FiftyOne-labels (ShowUI-web etc.) till AetherAgent UI_CLASSES index.

    ShowUI-web labels: ListItem, Button, Checkbox, Link, Heading, Image, Icon,
                       RadioButton, Textbox, Select, Text, etc.
    UI_CLASSES: button, textbox, link, icon, text, img, checkbox, radio, combobox, heading

    Returns None om labeln ska filtreras bort (t.ex. ej interaktiv).
    """
    _ui_class_map = {name: i for i, name in enumerate(UI_CLASSES)}
    label_lower = label.lower().strip()

    # Direkt matchning
    if label_lower in _ui_class_map:
        return _ui_class_map[label_lower]

    # ShowUI-web → UI_CLASSES mappning
    _fiftyone_to_ui = {
        "button": 0,           # button
        "btn": 0,
        "submitbutton": 0,
        "textbox": 1,          # textbox
        "textarea": 1,
        "input": 1,
        "textfield": 1,
        "searchbox": 1,
        "link": 2,             # link
        "hyperlink": 2,
        "anchor": 2,
        "icon": 3,             # icon
        "svg": 3,
        "text": 4,             # text
        "statictext": 4,
        "label": 4,
        "paragraph": 4,
        "span": 4,
        "img": 5,              # img
        "image": 5,
        "figure": 5,
        "photo": 5,
        "checkbox": 6,         # checkbox
        "radio": 7,            # radio
        "radiobutton": 7,
        "combobox": 8,         # combobox
        "select": 8,
        "dropdown": 8,
        "listbox": 8,
        "heading": 9,          # heading
        "header": 9,
        "title": 9,
        "h1": 9, "h2": 9, "h3": 9, "h4": 9, "h5": 9, "h6": 9,
        # ShowUI-specifika
        "edit": 1,             # Edit → textbox (textinmatningsfält)
        "listitem": 2,         # ListItem → link (interaktivt, klickbart)
        "menuitem": 2,         # MenuItem → link
        "tab": 0,              # Tab → button
        "tabitem": 0,          # TabItem → button
        "switch": 6,           # Switch → checkbox (toggle)
        "slider": 1,           # Slider → textbox (interaktiv input)
        "progressbar": 4,      # ProgressBar → text (visuellt)
        "navigation": 2,       # Navigation → link
        "nav": 2,
    }

    if label_lower in _fiftyone_to_ui:
        return _fiftyone_to_ui[label_lower]

    # Fuzzy: kolla om något UI_CLASS-namn finns i labeln
    for name, cid in _ui_class_map.items():
        if name in label_lower or label_lower in name:
            return cid

    # Fallback: text (klass 4)
    return 4


def _convert_fiftyone_to_yolo(snapshot_dir: Path, output_dir: Path, info: dict):
    """Konverterar ett FiftyOne-format dataset (samples.json + bilder) direkt till YOLO.

    Mycket snabbare än load_dataset() — läser JSON + kopierar/symlänkar bilder.
    FiftyOne bbox-format: [x, y, width, height] (normaliserade 0-1) → matchar YOLO
    (x_center, y_center, width, height).

    Mappar FiftyOne-labels till AetherAgent UI_CLASSES (10 klasser).
    """
    import json
    import shutil

    samples_json = snapshot_dir / "samples.json"
    log(f"Konverterar FiftyOne-dataset direkt (skippar load_dataset)...", "STEP")

    images_dir = output_dir / "images" / "train"
    labels_dir = output_dir / "labels" / "train"
    images_dir.mkdir(parents=True, exist_ok=True)
    labels_dir.mkdir(parents=True, exist_ok=True)

    log("Läser samples.json...", "INFO")
    with open(samples_json, "r") as f:
        data = json.load(f)

    samples = data.get("samples", [])
    log(f"  {len(samples)} samples hittade", "INFO")

    # Samla unika labels för loggning
    all_labels = set()
    for sample in samples:
        detections = sample.get("detections", {})
        det_list = detections.get("detections", []) if isinstance(detections, dict) else []
        for det in det_list:
            all_labels.add(det.get("label", "unknown"))
    log(f"  {len(all_labels)} unika FiftyOne-labels: {sorted(all_labels)[:15]}...", "INFO")
    log(f"  Mappar till {len(UI_CLASSES)} UI_CLASSES: {UI_CLASSES}", "INFO")

    saved = 0
    skipped = 0
    label_stats = {}

    for idx, sample in enumerate(samples):
        if idx % 2000 == 0 and idx > 0:
            log(f"  {idx}/{len(samples)} konverterade...", "INFO")

        # Hitta bildfilen
        filepath = sample.get("filepath", "")
        # FiftyOne lagrar relativa sökvägar som "data/data_X/screenshot-Y.png"
        # eller absoluta sökvägar — hantera båda
        if filepath.startswith("/"):
            # Absolut sökväg — gör relativ till snapshot_dir
            img_path = snapshot_dir / Path(filepath).name
        else:
            img_path = snapshot_dir / filepath

        if not img_path.exists():
            # Prova att leta i data/-mappen
            for data_subdir in (snapshot_dir / "data").iterdir() if (snapshot_dir / "data").exists() else []:
                candidate = data_subdir / Path(filepath).name
                if candidate.exists():
                    img_path = candidate
                    break

        if not img_path.exists():
            skipped += 1
            continue

        # Extrahera detections
        detections = sample.get("detections", {})
        det_list = detections.get("detections", []) if isinstance(detections, dict) else []

        if not det_list:
            skipped += 1
            continue

        # Konvertera bboxar till YOLO-format med UI_CLASSES-mappning
        yolo_lines = []
        for det in det_list:
            label = det.get("label", "unknown")
            bbox = det.get("bounding_box", [])
            if len(bbox) != 4:
                continue
            # FiftyOne: [x, y, w, h] (top-left, normaliserat)
            # YOLO: [x_center, y_center, w, h] (normaliserat)
            x, y, w, h = bbox
            x_center = x + w / 2
            y_center = y + h / 2

            class_id = _map_fiftyone_label_to_ui_class(label)
            if class_id is None:
                continue

            yolo_lines.append(f"{class_id} {x_center:.6f} {y_center:.6f} {w:.6f} {h:.6f}")
            label_stats[label] = label_stats.get(label, 0) + 1

        if not yolo_lines:
            skipped += 1
            continue

        # Kopiera bild (symlänk om möjligt för snabbhet)
        img_dest = images_dir / f"{idx:06d}.png"
        label_dest = labels_dir / f"{idx:06d}.txt"

        try:
            img_dest.symlink_to(img_path.resolve())
        except OSError:
            shutil.copy2(img_path, img_dest)

        with open(label_dest, "w") as f:
            f.write("\n".join(yolo_lines) + "\n")

        saved += 1

    log(f"  Konverterat: {saved} bilder, skippade: {skipped}", "OK")

    # Logga label-distribution
    top_labels = sorted(label_stats.items(), key=lambda x: -x[1])[:15]
    for label, count in top_labels:
        mapped = _map_fiftyone_label_to_ui_class(label)
        ui_name = UI_CLASSES[mapped] if mapped is not None else "SKIP"
        log(f"    {label} → {ui_name} ({count} detections)", "INFO")

    # Auto-split till train/val/test
    auto_split_dataset(output_dir)

    # Spara dataset.yaml med UI_CLASSES (inte FiftyOne-labels)
    import yaml
    yaml_path = output_dir / "dataset.yaml"
    yaml_data = {
        "path": str(output_dir.resolve()),
        "train": "images/train",
        "val": "images/val",
        "test": "images/test",
        "nc": len(UI_CLASSES),
        "names": {i: name for i, name in enumerate(UI_CLASSES)},
    }
    with open(yaml_path, "w") as f:
        yaml.dump(yaml_data, f, default_flow_style=False)
    log(f"  dataset.yaml sparad med {len(UI_CLASSES)} UI_CLASSES", "OK")


def _convert_hf_to_yolo(ds, output_dir: Path, info: dict):
    """Konverterar ett HuggingFace dataset till YOLO-format.

    Hanterar varierande kolumnnamn: bbox, bounding_box, objects, etc.
    Sparar bilder till images/train/ och labels till labels/train/.
    """
    from PIL import Image as PILImage
    import io

    images_dir = output_dir / "images" / "train"
    labels_dir = output_dir / "labels" / "train"
    images_dir.mkdir(parents=True, exist_ok=True)
    labels_dir.mkdir(parents=True, exist_ok=True)

    # Hämta split — föredra "train", annars första tillgängliga
    split_name = "train" if "train" in ds else list(ds.keys())[0]
    split = ds[split_name]
    columns = split.column_names

    log(f"Konverterar {len(split)} samples (split: {split_name})...", "STEP")
    log(f"  Kolumner: {columns}", "INFO")

    # Detektera bildkolumn
    img_col = None
    for candidate in ("image", "screenshot", "img", "png", "frame"):
        if candidate in columns:
            img_col = candidate
            break

    # Detektera bbox-kolumn
    bbox_col = None
    label_col = None
    for candidate in ("objects", "annotations", "bboxes", "bbox", "bounding_boxes"):
        if candidate in columns:
            bbox_col = candidate
            break

    # Detektera element/label-kolumner
    for candidate in ("label", "labels", "class", "classes", "category"):
        if candidate in columns:
            label_col = candidate
            break

    saved = 0
    skipped = 0

    for idx, row in enumerate(split):
        if idx % 1000 == 0:
            log(f"  {idx}/{len(split)} konverterade...", "INFO")

        # Extrahera bild
        img = None
        if img_col and row.get(img_col) is not None:
            val = row[img_col]
            if isinstance(val, PILImage.Image):
                img = val
            elif isinstance(val, dict) and "bytes" in val:
                img = PILImage.open(io.BytesIO(val["bytes"]))
            elif isinstance(val, bytes):
                img = PILImage.open(io.BytesIO(val))
        if img is None:
            skipped += 1
            continue

        w, h = img.size
        if w < 10 or h < 10:
            skipped += 1
            continue

        # Spara bild
        img_name = f"{idx:06d}.png"
        img.save(str(images_dir / img_name))

        # Extrahera bboxar
        yolo_lines = []
        bboxes_data = row.get(bbox_col) if bbox_col else None

        if bboxes_data is not None:
            yolo_lines = _parse_hf_bboxes(bboxes_data, label_col, row, w, h)

        # Skriv YOLO label-fil
        label_name = f"{idx:06d}.txt"
        with open(labels_dir / label_name, "w") as f:
            f.write("\n".join(yolo_lines))

        saved += 1

    log(f"Konverterat {saved} bilder ({skipped} överhoppade)", "OK")

    # Auto-split
    auto_split_dataset(output_dir)


def _parse_hf_bboxes(bboxes_data, label_col, row, img_w, img_h) -> list:
    """Parsa bboxar från varierande HuggingFace-format → YOLO-rader."""
    lines = []

    # UI-klass-mappning (text → class-id)
    _class_map = {name: i for i, name in enumerate(UI_CLASSES)}

    def _map_class(label) -> int:
        if isinstance(label, int):
            return min(label, len(UI_CLASSES) - 1)
        label_lower = str(label).lower().strip()
        if label_lower in _class_map:
            return _class_map[label_lower]
        # Fuzzy-match
        for name, cid in _class_map.items():
            if name in label_lower or label_lower in name:
                return cid
        return _class_map.get("text", 4)  # fallback: text

    # Format A: {"bbox": [...], "category": [...]} dict
    if isinstance(bboxes_data, dict):
        bboxes = bboxes_data.get("bbox", bboxes_data.get("bboxes", []))
        labels = bboxes_data.get("category", bboxes_data.get("label",
                 bboxes_data.get("labels", bboxes_data.get("classes", []))))

        for i, bbox in enumerate(bboxes):
            cls_id = _map_class(labels[i]) if i < len(labels) else 4
            yolo_bbox = _bbox_to_yolo(bbox, img_w, img_h)
            if yolo_bbox:
                lines.append(f"{cls_id} {yolo_bbox}")

    # Format B: lista av dicts [{bbox: [...], label: ...}, ...]
    elif isinstance(bboxes_data, list) and bboxes_data and isinstance(bboxes_data[0], dict):
        for item in bboxes_data:
            bbox = item.get("bbox", item.get("bounding_box", item.get("coordinates")))
            label = item.get("label", item.get("class", item.get("category", "text")))
            if bbox:
                cls_id = _map_class(label)
                yolo_bbox = _bbox_to_yolo(bbox, img_w, img_h)
                if yolo_bbox:
                    lines.append(f"{cls_id} {yolo_bbox}")

    # Format C: lista av listor [[x1, y1, x2, y2], ...]
    elif isinstance(bboxes_data, list) and bboxes_data and isinstance(bboxes_data[0], (list, tuple)):
        labels = row.get(label_col, []) if label_col else []
        for i, bbox in enumerate(bboxes_data):
            cls_id = _map_class(labels[i]) if i < len(labels) else 4
            yolo_bbox = _bbox_to_yolo(bbox, img_w, img_h)
            if yolo_bbox:
                lines.append(f"{cls_id} {yolo_bbox}")

    return lines


def _bbox_to_yolo(bbox, img_w: int, img_h: int) -> str | None:
    """Konvertera bbox → YOLO-format (cx cy w h, normaliserat 0-1).

    Hanterar: [x1,y1,x2,y2], [x,y,w,h], [left,top,right,bottom] (normaliserat eller pixlar).
    """
    if not bbox or len(bbox) < 4:
        return None

    x1, y1, x2, y2 = float(bbox[0]), float(bbox[1]), float(bbox[2]), float(bbox[3])

    # Om alla värden är 0-1 → redan normaliserat, tolka som [left,top,right,bottom]
    if all(0 <= v <= 1.0 for v in [x1, y1, x2, y2]):
        if x2 <= x1 or y2 <= y1:
            # Troligen [x, y, w, h] normaliserat
            cx, cy, bw, bh = x1 + x2 / 2, y1 + y2 / 2, x2, y2
        else:
            cx = (x1 + x2) / 2
            cy = (y1 + y2) / 2
            bw = x2 - x1
            bh = y2 - y1
    else:
        # Pixelkoordinater
        if x2 > img_w or y2 > img_h:
            # Troligen redan [x1,y1,x2,y2] i pixlar
            pass
        if x2 <= x1:
            # Troligen [x, y, w, h] i pixlar
            bw = x2 / img_w
            bh = y2 / img_h
            cx = (x1 + x2 / 2) / img_w
            cy = (y1 + y2 / 2) / img_h
        else:
            cx = (x1 + x2) / 2 / img_w
            cy = (y1 + y2) / 2 / img_h
            bw = (x2 - x1) / img_w
            bh = (y2 - y1) / img_h

    # Validera
    if not (0 < bw <= 1 and 0 < bh <= 1 and 0 <= cx <= 1 and 0 <= cy <= 1):
        return None

    return f"{cx:.6f} {cy:.6f} {bw:.6f} {bh:.6f}"


# ---------------------------------------------------------------------------
# Step 1: Dataset Preparation
# ---------------------------------------------------------------------------

def create_data_yaml(dataset_dir: Path, output_path: Path) -> Path:
    """Create or validate data.yaml for the dataset."""
    import yaml

    yaml_path = dataset_dir / "data.yaml"
    existing_data = None

    if yaml_path.exists():
        with open(yaml_path) as f:
            existing_data = yaml.safe_load(f)
        if existing_data and "names" in existing_data and "train" in existing_data:
            # Validera att train-sökvägen faktiskt innehåller bilder
            train_path = existing_data.get("train", "")
            resolved_base = Path(existing_data.get("path", str(dataset_dir.resolve())))
            train_abs = resolved_base / train_path if train_path else None
            has_images = (train_abs is not None and train_abs.exists()
                          and (any(train_abs.rglob("*.jpg")) or any(train_abs.rglob("*.png"))))

            if not has_images:
                log(f"data.yaml train-sökväg '{train_path}' har inga bilder, regenererar", "WARN")
                # Faller igenom till auto-detektering nedan
            else:
                # Sökvägar OK — kontrollera klassnamn
                existing_names = list(existing_data["names"].values()) if isinstance(existing_data["names"], dict) else list(existing_data["names"])
                nc = len(existing_names)
                if nc == len(UI_CLASSES) and existing_names == list(UI_CLASSES):
                    log(f"Existing data.yaml found: {nc} classes", "OK")
                    return yaml_path
                # Klassnamn matchar inte — uppdatera namn, behåll validerade sökvägar
                log(f"data.yaml har {nc} klasser men namnen matchar inte UI_CLASSES, uppdaterar", "WARN")
                existing_data["nc"] = len(UI_CLASSES)
                existing_data["names"] = {i: name for i, name in enumerate(UI_CLASSES)}
                existing_data["path"] = str(dataset_dir.resolve())
                with open(yaml_path, "w") as f:
                    yaml.dump(existing_data, f, default_flow_style=False)
                log(f"Uppdaterade klassnamn i data.yaml, behöll train/val-sökvägar", "OK")
                return yaml_path
        elif existing_data:
            log("data.yaml exists but is incomplete, regenerating", "WARN")

    # Generera data.yaml — auto-detektera mappstruktur
    # Standard YOLO: images/train/ + images/val/
    # YashJain-stil: train/images/ + val/images/
    train_rel, val_rel, test_rel = None, None, None

    # Ordning: testa vanligaste strukturerna — kräv att mappen har bilder (inte bara existerar)
    candidates = [
        ("images/train", "images/val", "images/test"),    # Standard YOLO
        ("train/images", "val/images", "test/images"),    # YashJain
        ("train", "val", "test"),                         # Flat
    ]

    def _dir_has_images(d: Path) -> bool:
        return d.exists() and (any(d.rglob("*.jpg")) or any(d.rglob("*.png")))

    for train_try, val_try, test_try in candidates:
        if _dir_has_images(dataset_dir / train_try):
            train_rel = train_try
            val_rel = val_try if (dataset_dir / val_try).exists() else train_try
            test_rel = test_try if (dataset_dir / test_try).exists() else ""
            break

    if train_rel is None:
        log(f"Kunde inte hitta träningsbilder i {dataset_dir}", "ERR")
        log("Förväntad struktur: images/train/ eller train/images/", "ERR")
        sys.exit(1)

    train_imgs = dataset_dir / train_rel
    val_imgs = dataset_dir / val_rel

    # Räkna bilder
    def _count_images(d: Path) -> int:
        if not d.exists():
            return 0
        return len(list(d.glob("*.png")) + list(d.glob("*.jpg")) + list(d.glob("*.jpeg")))

    train_count = _count_images(train_imgs)
    val_count = _count_images(val_imgs) if val_rel != train_rel else 0

    data = {
        "path": str(dataset_dir.resolve()),
        "train": train_rel,
        "val": val_rel,
        "test": test_rel,
        "nc": len(UI_CLASSES),
        "names": {i: name for i, name in enumerate(UI_CLASSES)},
    }

    with open(yaml_path, "w") as f:
        yaml.dump(data, f, default_flow_style=False)

    log(f"Created data.yaml: {train_count} train, {val_count} val images ({train_rel})", "OK")
    return yaml_path


def auto_split_dataset(dataset_dir: Path, val_ratio: float = 0.15, test_ratio: float = 0.05):
    """Auto-split dataset into train/val/test.

    Hanterar två strukturer:
    1. Flat: images/*.png → skapar images/train/, images/val/, images/test/
    2. Redan i train: images/train/*.png → flyttar subset till val/test

    Motsvarande labels flyttas med (labels/train/ → labels/val/ etc.).
    """
    import random
    import shutil

    # Kolla om bilder redan ligger i images/train/ (t.ex. från _convert_fiftyone_to_yolo)
    train_imgs_dir = dataset_dir / "images" / "train"
    train_lbls_dir = dataset_dir / "labels" / "train"
    val_imgs_dir = dataset_dir / "images" / "val"

    if train_imgs_dir.exists() and val_imgs_dir.exists() and any(val_imgs_dir.iterdir()):
        log("Dataset already split into train/val/test", "OK")
        return

    if train_imgs_dir.exists():
        # Bilder ligger i images/train/ — split därifrån
        all_images = list(train_imgs_dir.glob("*.png")) + list(train_imgs_dir.glob("*.jpg"))
        labels_source = train_lbls_dir
    else:
        # Flat: bilder direkt i images/
        flat_imgs = dataset_dir / "images"
        if not flat_imgs.exists():
            flat_imgs = dataset_dir
        all_images = list(flat_imgs.glob("*.png")) + list(flat_imgs.glob("*.jpg"))
        labels_source = dataset_dir / "labels"

    if not all_images:
        log("No images found for splitting", "WARN")
        return

    log(f"Auto-splitting {len(all_images)} images ({1 - val_ratio - test_ratio:.0%}/{val_ratio:.0%}/{test_ratio:.0%})", "STEP")

    random.shuffle(all_images)
    n_val = int(len(all_images) * val_ratio)
    n_test = int(len(all_images) * test_ratio)

    # Val och test tas från slutet, resten förblir train
    val_images = all_images[:n_val]
    test_images = all_images[n_val:n_val + n_test]
    # Train = resten (behöver inte flyttas om de redan ligger i images/train/)

    for split_name, images in [("val", val_images), ("test", test_images)]:
        img_dir = dataset_dir / "images" / split_name
        lbl_dir = dataset_dir / "labels" / split_name
        img_dir.mkdir(parents=True, exist_ok=True)
        lbl_dir.mkdir(parents=True, exist_ok=True)

        for img_path in images:
            # Flytta bild till val/test
            shutil.move(str(img_path), str(img_dir / img_path.name))
            # Flytta matchande label
            label_name = img_path.stem + ".txt"
            label_path = labels_source / label_name
            if label_path.exists():
                shutil.move(str(label_path), str(lbl_dir / label_name))
            else:
                (lbl_dir / label_name).touch()

        log(f"  {split_name}: {len(images)} images", "INFO")

    # Om flat-struktur: flytta resterande till train/
    if not (dataset_dir / "images" / "train").exists():
        train_dir = dataset_dir / "images" / "train"
        train_lbl = dataset_dir / "labels" / "train"
        train_dir.mkdir(parents=True, exist_ok=True)
        train_lbl.mkdir(parents=True, exist_ok=True)
        remaining = list((dataset_dir / "images").glob("*.png")) + list((dataset_dir / "images").glob("*.jpg"))
        for img_path in remaining:
            shutil.move(str(img_path), str(train_dir / img_path.name))
            label_path = labels_source / (img_path.stem + ".txt")
            if label_path.exists():
                shutil.move(str(label_path), str(train_lbl / (img_path.stem + ".txt")))

    train_count = len(list((dataset_dir / "images" / "train").glob("*")))
    log(f"  train: {train_count} images (kvar efter split)", "INFO")


def merge_datasets(dataset_dirs: list[Path], output_dir: Path) -> Path:
    """Merge multiple YOLO datasets into one combined dataset (incremental merge).

    Kopierar bilder + labels från alla dataset till en gemensam mapp.
    Hanterar namnkollisioner genom att prefixa med dataset-index.
    Stödjer alla vanliga YOLO-strukturer:
      - images/train/ + labels/train/   (standard YOLO)
      - train/images/ + train/labels/   (YashJain-stil)
    """
    log(f"Merging {len(dataset_dirs)} datasets into {output_dir}", "STEP")

    for split in ["train", "val", "test"]:
        (output_dir / "images" / split).mkdir(parents=True, exist_ok=True)
        (output_dir / "labels" / split).mkdir(parents=True, exist_ok=True)

    total_copied = {"train": 0, "val": 0, "test": 0}

    for ds_idx, ds_dir in enumerate(dataset_dirs):
        ds_dir = Path(ds_dir).resolve()
        if not ds_dir.exists():
            log(f"  Dataset finns inte, hoppar över: {ds_dir}", "WARN")
            continue

        ds_prefix = f"ds{ds_idx}_"
        ds_name = ds_dir.name

        # Detektera mappstruktur
        # Variant 1: images/train/ + labels/train/
        # Variant 2: train/images/ + train/labels/
        for split in ["train", "val", "test"]:
            src_imgs = None
            src_lbls = None

            if (ds_dir / "images" / split).exists():
                src_imgs = ds_dir / "images" / split
                src_lbls = ds_dir / "labels" / split
            elif (ds_dir / split / "images").exists():
                src_imgs = ds_dir / split / "images"
                src_lbls = ds_dir / split / "labels"

            if src_imgs is None or not src_imgs.exists():
                continue

            images = list(src_imgs.glob("*.png")) + list(src_imgs.glob("*.jpg")) + list(src_imgs.glob("*.jpeg"))
            for img_path in images:
                # Prefix med dataset-index för att undvika namnkollisioner
                dst_name = f"{ds_prefix}{img_path.name}"
                dst_img = output_dir / "images" / split / dst_name
                shutil.copy2(str(img_path), str(dst_img))

                # Kopiera matchande label
                label_name = img_path.stem + ".txt"
                dst_label_name = f"{ds_prefix}{img_path.stem}.txt"
                if src_lbls and (src_lbls / label_name).exists():
                    shutil.copy2(str(src_lbls / label_name),
                                 str(output_dir / "labels" / split / dst_label_name))
                else:
                    # Tom label = negativ sample
                    (output_dir / "labels" / split / dst_label_name).touch()

                total_copied[split] += 1

        log(f"  [{ds_idx + 1}/{len(dataset_dirs)}] {ds_name}: kopierad", "OK")

    for split, count in total_copied.items():
        if count > 0:
            log(f"  {split}: {count} bilder totalt", "INFO")

    total = sum(total_copied.values())
    if total == 0:
        log("Inga bilder hittades i dataseten!", "ERR")
        sys.exit(1)

    log(f"Merged dataset: {total} bilder → {output_dir}", "OK")
    return output_dir


def download_starter_dataset(output_dir: Path):
    """Download a small starter dataset for testing the pipeline."""
    log("Creating starter dataset with synthetic examples...", "STEP")

    # Create directory structure
    for split in ["train", "val"]:
        (output_dir / "images" / split).mkdir(parents=True, exist_ok=True)
        (output_dir / "labels" / split).mkdir(parents=True, exist_ok=True)

    try:
        from PIL import Image, ImageDraw, ImageFont
    except ImportError:
        run(f"{sys.executable} -m pip install pillow")
        from PIL import Image, ImageDraw, ImageFont

    import random

    def make_ui_screenshot(idx: int, split: str):
        """Generate a synthetic UI screenshot with labeled elements."""
        w, h = 640, 640
        img = Image.new("RGB", (w, h), color=(245, 245, 245))
        draw = ImageDraw.Draw(img)
        labels = []

        # Draw random UI elements
        elements = random.randint(3, 8)
        used_rects = []

        for _ in range(elements):
            elem_type = random.choice(range(len(UI_CLASSES)))
            # Random position
            for _attempt in range(20):
                ex = random.randint(20, w - 120)
                ey = random.randint(20, h - 50)
                ew = random.randint(60, 150)
                eh = random.randint(25, 45)

                # Check overlap
                overlap = False
                for rx, ry, rw, rh in used_rects:
                    if not (ex + ew < rx or ex > rx + rw or ey + eh < ry or ey > ry + rh):
                        overlap = True
                        break
                if not overlap:
                    break
            else:
                continue

            used_rects.append((ex, ey, ew, eh))

            # Draw based on type
            if elem_type == 0:  # button
                draw.rounded_rectangle([ex, ey, ex + ew, ey + eh], radius=5, fill=(59, 130, 246))
                draw.text((ex + 10, ey + 8), "Button", fill="white")
            elif elem_type == 1:  # input
                draw.rectangle([ex, ey, ex + ew, ey + eh], outline=(200, 200, 200), width=2)
                draw.text((ex + 5, ey + 8), "Type here...", fill=(180, 180, 180))
            elif elem_type == 2:  # link
                draw.text((ex, ey + 8), "Click here", fill=(59, 130, 246))
                draw.line([ex, ey + 25, ex + 70, ey + 25], fill=(59, 130, 246))
            elif elem_type == 4:  # text
                draw.text((ex, ey), "Sample text content", fill=(50, 50, 50))
            elif elem_type == 6:  # checkbox
                draw.rectangle([ex, ey, ex + 20, ey + 20], outline=(100, 100, 100), width=2)
                draw.text((ex + 25, ey + 2), "Option", fill=(50, 50, 50))
            elif elem_type == 8:  # select
                draw.rectangle([ex, ey, ex + ew, ey + eh], outline=(200, 200, 200), width=2)
                draw.text((ex + 5, ey + 8), "Select...", fill=(100, 100, 100))
                draw.polygon([(ex + ew - 20, ey + 12), (ex + ew - 10, ey + 12), (ex + ew - 15, ey + 25)], fill=(100, 100, 100))
            elif elem_type == 9:  # heading
                draw.text((ex, ey), "Page Title", fill=(30, 30, 30))
            else:
                draw.rectangle([ex, ey, ex + ew, ey + eh], fill=(220, 220, 220), outline=(180, 180, 180))

            # YOLO format: class cx cy w h (normalized)
            cx = (ex + ew / 2) / w
            cy = (ey + eh / 2) / h
            nw = ew / w
            nh = eh / h
            labels.append(f"{elem_type} {cx:.6f} {cy:.6f} {nw:.6f} {nh:.6f}")

        img.save(output_dir / "images" / split / f"ui_{idx:04d}.png")
        with open(output_dir / "labels" / split / f"ui_{idx:04d}.txt", "w") as f:
            f.write("\n".join(labels) + "\n")

    # Generate training images
    for i in range(200):
        make_ui_screenshot(i, "train")
    for i in range(40):
        make_ui_screenshot(i, "val")

    log(f"Created starter dataset: 200 train + 40 val synthetic screenshots", "OK")
    log("NOTE: For production, replace with real labeled screenshots!", "WARN")


# ---------------------------------------------------------------------------
# Dataset Format Converters (Rico / COCO / WebUI → YOLO)
# ---------------------------------------------------------------------------

# Mappning från externa klassnamn till våra UI_CLASSES-index
# Klasser som inte matchar ignoreras
_RICO_CLASS_MAP = {
    "Text Button": 0,    # button
    "Icon": 3,           # icon
    "Text": 4,           # text
    "Image": 5,          # img
    "Input": 1,          # textbox
    "Web View": 4,       # text (fallback)
    "List Item": 4,      # text
    "Card": 4,           # text
    "Radio Button": 7,   # radio
    "Checkbox": 6,       # checkbox
    "Switch": 6,         # checkbox (nära nog)
    "Spinner": 8,        # combobox
    "Toolbar": 9,        # heading
    "Multi-Tab": 2,      # link
    "Slider": 1,         # textbox (fallback)
    "Advertisement": 5,  # img
    "Pager Indicator": 3,  # icon
    "Modal": 4,          # text
    "Button Bar": 0,     # button
    "Number Stepper": 1, # textbox
    "Map View": 5,       # img
    "Video": 5,          # img
    "Date Picker": 8,    # combobox
    "On/Off Switch": 6,  # checkbox
    "Drawer": 2,         # link
    "Bottom Navigation": 2,  # link
    "Upper Tab Bar": 2,  # link
}

# Rico viewType-mappning (det andra vanliga formatet med "views" array)
_RICO_VIEWTYPE_MAP = {
    "android.widget.Button": 0,          # button
    "android.widget.ImageButton": 0,     # button
    "android.widget.EditText": 1,        # input
    "android.widget.AutoCompleteTextView": 1,
    "android.widget.TextView": 4,        # text (heuristik avgör vidare)
    "android.widget.ImageView": 5,       # image
    "android.widget.CheckBox": 6,        # checkbox
    "android.widget.RadioButton": 7,     # radio
    "android.widget.Spinner": 8,         # select
    "android.widget.Switch": 6,          # checkbox
    "android.widget.ToggleButton": 6,    # checkbox
    "android.widget.SeekBar": 1,         # input
    "android.widget.ProgressBar": 4,     # text (visuellt)
    "android.widget.RatingBar": 4,       # text
    # Förkortade classnamn (utan full package path)
    "Button": 0,
    "ImageButton": 0,
    "EditText": 1,
    "TextView": 4,
    "ImageView": 5,
    "CheckBox": 6,
    "RadioButton": 7,
    "Spinner": 8,
    "Switch": 6,
    "ToggleButton": 6,
}

# Utökad Rico-mappning för --extended-classes
_RICO_CLASS_MAP_EXTENDED = {
    **_RICO_CLASS_MAP,
    "Bottom Navigation": 13,  # nav
    "Upper Tab Bar": 13,      # nav
    "Multi-Tab": 13,          # nav
    "Drawer": 13,             # nav
    "Card": 12,               # product_card (heuristik kan förbättra)
    "Input": 1,               # input
}

_RICO_VIEWTYPE_MAP_EXTENDED = {
    **_RICO_VIEWTYPE_MAP,
    "android.widget.SearchView": 14,     # search
}

# Valuta- och CTA-heuristik-mönster
_CURRENCY_PATTERNS = [
    "kr", "$", "€", "£", "¥", "sek", "usd", "eur", "gbp",
    "price", "pris", "cost", "total", "sum",
]
_CTA_PATTERNS = [
    "buy", "köp", "add to cart", "lägg i kundvagn", "checkout",
    "sign up", "registrera", "subscribe", "prenumerera", "get started",
    "book", "boka", "order", "beställ", "continue", "next", "submit",
    "download", "ladda ner", "install", "try free", "start",
]

_WEBUI_CLASS_MAP = {
    "button": 0,
    "btn": 0,
    "input": 1,
    "textbox": 1,
    "textarea": 1,
    "link": 2,
    "anchor": 2,
    "a": 2,
    "icon": 3,
    "img": 5,
    "image": 5,
    "text": 4,
    "paragraph": 4,
    "p": 4,
    "span": 4,
    "label": 4,
    "checkbox": 6,
    "radio": 7,
    "select": 8,
    "dropdown": 8,
    "heading": 9,
    "h1": 9,
    "h2": 9,
    "h3": 9,
    "h4": 9,
    "title": 9,
    "nav": 2,            # fallback till link i bas; navigation(13) i extended
    "menu": 2,
    "search": 1,         # fallback till textbox i bas; searchbox(14) i extended
    "searchbox": 1,
    "combobox": 8,
    "navigation": 2,     # fallback till link i bas
}


def convert_rico_to_yolo(rico_dir: Path, output_dir: Path, extended: bool = False) -> Path:
    """Konverterar Rico-dataset (JSON + screenshots) till YOLO-format.

    Stödjer tre Rico-format:

    Format A — combined (componentLabel + bounds):
        rico_dir/combined/0.json  +  rico_dir/screenshot/0.jpg

    Format B — semantic_annotations:
        rico_dir/semantic_annotations/0.json  +  rico_dir/screenshots/0.jpg

    Format C — views[] med viewType (det format Rico-verktygen genererar):
        {"screenshot": "0.png", "screen_width": 1440, "screen_height": 2560,
         "views": [{"viewType": "Button", "bounds": [x,y,w,h], "text": "Buy"}, ...]}
    """
    log("Konverterar Rico-dataset till YOLO-format...", "STEP")
    if extended:
        log("Utökade agentklasser aktiverade (16 klasser)", "INFO")

    # Välj klassmappningar baserat på extended-flagga
    class_map = _RICO_CLASS_MAP_EXTENDED if extended else _RICO_CLASS_MAP
    viewtype_map = _RICO_VIEWTYPE_MAP_EXTENDED if extended else _RICO_VIEWTYPE_MAP
    active_classes = UI_CLASSES_EXTENDED if extended else UI_CLASSES

    # Hitta var bilderna och JSON-filerna ligger
    combined_dir = rico_dir / "combined"
    screenshot_dir = rico_dir / "screenshot"

    # Alternativa sökvägar för JSON-filer
    if not combined_dir.exists():
        combined_dir = rico_dir / "semantic_annotations"
    if not combined_dir.exists() and (rico_dir / "jsons").exists():
        combined_dir = rico_dir / "jsons"
    if not combined_dir.exists():
        # Platt struktur: JSON-filer i root
        combined_dir = rico_dir

    # Alternativa sökvägar för screenshots (separat från JSON-sökning)
    if not screenshot_dir.exists():
        screenshot_dir = rico_dir / "screenshots"
    if not screenshot_dir.exists() and (rico_dir / "images").exists():
        screenshot_dir = rico_dir / "images"
    # unique_uis.tar.gz lägger ofta bilder i combined/ tillsammans med JSON
    if not screenshot_dir.exists() and (rico_dir / "combined").exists():
        screenshot_dir = rico_dir / "combined"
    if not screenshot_dir.exists():
        screenshot_dir = rico_dir

    # Sök JSON-filer — prova platt först, sedan rekursivt
    json_files = sorted(combined_dir.glob("*.json"))
    if not json_files:
        json_files = sorted(combined_dir.glob("**/*.json"))
    if not json_files:
        # Sista utväg: sök rekursivt från rico_dir
        json_files = sorted(rico_dir.glob("**/*.json"))
    if not json_files:
        log(f"Inga JSON-filer hittades i {rico_dir}", "ERR")
        sys.exit(1)

    log(f"Hittade {len(json_files)} Rico JSON-filer", "INFO")

    # Skapa output-struktur
    for split in ["images", "labels"]:
        (output_dir / split).mkdir(parents=True, exist_ok=True)

    converted = 0
    skipped = 0
    skipped_no_img = 0
    heuristic_upgrades = {"price": 0, "cta": 0, "heading": 0}

    # Diagnostik: logga sökvägar
    log(f"JSON-källa: {combined_dir}", "INFO")
    log(f"Bild-källa: {screenshot_dir}", "INFO")

    for json_path in json_files:
        screen_id = json_path.stem

        with open(json_path) as f:
            data = json.load(f)

        # --- Format C: views[] array med viewType ---
        if "views" in data:
            img_name = data.get("screenshot", f"{screen_id}.png")
            if "/" in img_name:
                img_name = img_name.split("/")[-1]
            img_stem = Path(img_name).stem
            img_path = _find_image(screenshot_dir, img_stem)
            if img_path is None and screenshot_dir != rico_dir:
                img_path = _find_image(rico_dir, img_stem)
            if img_path is None:
                img_path = _find_image(json_path.parent, img_stem)
            if img_path is None:
                skipped_no_img += 1
                if skipped_no_img == 1:
                    log(f"Första saknade bild: {img_stem}.png/jpg i {screenshot_dir}", "WARN")
                skipped += 1
                continue

            img_w = data.get("screen_width", 1440)
            img_h = data.get("screen_height", 2560)

            labels = []
            for view in data["views"]:
                view_type = view.get("viewType", view.get("class", ""))
                view_text = view.get("text", "")
                if isinstance(view_text, list):
                    view_text = " ".join(str(t) for t in view_text)
                elif not isinstance(view_text, str):
                    view_text = str(view_text) if view_text else ""
                bounds = view.get("bounds", [])
                if len(bounds) != 4:
                    continue

                # Mappa viewType → klass-index
                class_idx = viewtype_map.get(view_type)
                if class_idx is None:
                    class_idx = class_map.get(view_type)
                if class_idx is None:
                    continue

                # --- Textheuristik: uppgradera klass baserat på textinnehåll ---
                class_idx, upgrade = _apply_text_heuristics(
                    class_idx, view_text, extended
                )
                if upgrade:
                    heuristic_upgrades[upgrade] = heuristic_upgrades.get(upgrade, 0) + 1

                # Rico views: bounds kan vara [x, y, w, h] ELLER [x1, y1, x2, y2]
                # Heuristik: om tredje/fjärde värdet > halva skärmen → troligen x2,y2
                bx, by, bz, bw_val = [float(v) for v in bounds]
                if bz > img_w * 0.5 or bw_val > img_h * 0.5:
                    # [x1, y1, x2, y2] format
                    x1, y1, x2, y2 = bx, by, bz, bw_val
                    w = x2 - x1
                    h = y2 - y1
                else:
                    # [x, y, w, h] format
                    x1, y1, w, h = bx, by, bz, bw_val

                if w <= 0 or h <= 0:
                    continue

                cx = (x1 + w / 2) / img_w
                cy = (y1 + h / 2) / img_h
                nw = w / img_w
                nh = h / img_h

                # Filtrera orimliga bboxar
                if nw > 0.95 and nh > 0.95:
                    continue
                if nw < 0.005 or nh < 0.005:
                    continue

                labels.append(f"{class_idx} {cx:.6f} {cy:.6f} {nw:.6f} {nh:.6f}")

            if not labels:
                skipped += 1
                continue

            shutil.copy2(img_path, output_dir / "images" / img_path.name)
            label_file = output_dir / "labels" / f"{screen_id}.txt"
            with open(label_file, "w") as f:
                f.write("\n".join(labels) + "\n")
            converted += 1
            continue

        # --- Format A/B: combined/semantic med componentLabel + rekursiv tree ---
        img_path = _find_image(screenshot_dir, screen_id)
        # Fallback: sök i rico_dir och JSON-filens egen katalog
        if img_path is None and screenshot_dir != rico_dir:
            img_path = _find_image(rico_dir, screen_id)
        if img_path is None:
            img_path = _find_image(json_path.parent, screen_id)
        if img_path is None:
            skipped_no_img += 1
            if skipped_no_img == 1:
                log(f"Första saknade bild: {screen_id}.png/jpg i {screenshot_dir}", "WARN")
            skipped += 1
            continue

        try:
            from PIL import Image
            img = Image.open(img_path)
            img_w, img_h = img.size
        except Exception:
            skipped += 1
            continue

        labels = []
        nodes = _extract_rico_nodes(data)

        for node in nodes:
            class_name = node.get("componentLabel", node.get("class", ""))
            class_idx = class_map.get(class_name)
            # Fallback: pröva viewtype-mappning (android.widget.* klassnamn)
            if class_idx is None:
                class_idx = viewtype_map.get(class_name)
            # Sista utväg: pröva kort klassnamn (t.ex. "Button" från "android.widget.Button")
            if class_idx is None and "." in class_name:
                short_name = class_name.rsplit(".", 1)[-1]
                class_idx = viewtype_map.get(short_name)
            if class_idx is None:
                continue

            # Textheuristik på combined-noder
            node_text = node.get("text", node.get("content-desc", ""))
            if isinstance(node_text, list):
                node_text = " ".join(str(t) for t in node_text)
            elif not isinstance(node_text, str):
                node_text = str(node_text) if node_text else ""
            class_idx, upgrade = _apply_text_heuristics(
                class_idx, node_text, extended
            )
            if upgrade:
                heuristic_upgrades[upgrade] = heuristic_upgrades.get(upgrade, 0) + 1

            bounds = node.get("bounds", [])
            if len(bounds) != 4:
                continue

            x1, y1, x2, y2 = bounds
            if x2 <= x1 or y2 <= y1:
                continue

            cx = ((x1 + x2) / 2) / img_w
            cy = ((y1 + y2) / 2) / img_h
            bw = (x2 - x1) / img_w
            bh = (y2 - y1) / img_h

            if bw > 0.95 and bh > 0.95:
                continue
            if bw < 0.005 or bh < 0.005:
                continue

            labels.append(f"{class_idx} {cx:.6f} {cy:.6f} {bw:.6f} {bh:.6f}")

        if not labels:
            skipped += 1
            continue

        shutil.copy2(img_path, output_dir / "images" / img_path.name)
        label_file = output_dir / "labels" / f"{screen_id}.txt"
        with open(label_file, "w") as f:
            f.write("\n".join(labels) + "\n")

        converted += 1

    log(f"Rico → YOLO: {converted} bilder konverterade, {skipped} hoppades över", "OK")
    if skipped_no_img > 0:
        log(f"{skipped_no_img} JSON-filer saknade matchande bild", "WARN")
        # Hjälp användaren: lista kataloger med bildfiler
        img_dirs = set()
        for ext in ("*.jpg", "*.png", "*.jpeg"):
            for p in rico_dir.glob(f"**/{ext}"):
                img_dirs.add(str(p.parent))
                if len(img_dirs) >= 5:
                    break
        if img_dirs:
            log(f"Bilder hittades i: {sorted(img_dirs)[:5]}", "INFO")

    # Rapportera heuristikuppgraderingar
    upgrades_total = sum(heuristic_upgrades.values())
    if upgrades_total > 0:
        log(f"Textheuristik: {upgrades_total} uppgraderingar "
            f"(price={heuristic_upgrades.get('price', 0)}, "
            f"cta={heuristic_upgrades.get('cta', 0)}, "
            f"heading={heuristic_upgrades.get('heading', 0)})", "OK")

    # Auto-splitta
    auto_split_dataset(output_dir)
    _create_data_yaml_for_classes(output_dir, active_classes)

    return output_dir


def _apply_text_heuristics(
    class_idx: int, text: str, extended: bool
) -> tuple:
    """Uppgraderar klass baserat på textinnehåll.

    Returnerar (ny_klass_idx, upgrade_typ_eller_None).
    I standard-läge (10 klasser) behåller vi button/text men loggar.
    I extended-läge (16 klasser) mappas till price/cta.
    """
    if not text:
        return class_idx, None

    text_lower = text.lower().strip()

    # Prisdetektering: text som innehåller valutasymboler/ord
    if class_idx == 4:  # text
        import re
        # Matcha mönster som "129 kr", "$19.99", "€ 5,00" etc.
        has_currency = any(p in text_lower for p in _CURRENCY_PATTERNS)
        has_number = bool(re.search(r'\d+[.,]?\d*', text_lower))
        if has_currency and has_number:
            if extended:
                return 10, "price"  # price-klass i extended
            return 4, "price"  # behåll som text men logga

    # CTA-detektering: knappar/text med köp-/action-fraser
    if class_idx in (0, 4):  # button eller text
        if any(p in text_lower for p in _CTA_PATTERNS):
            if extended:
                return 11, "cta"  # cta-klass i extended
            if class_idx == 4:
                return 0, "cta"  # uppgradera text → button i standard

    # Rubrikdetektering: kort text i stora element → heading
    if class_idx == 4 and len(text_lower) < 40:
        # Kort text som ser ut som en rubrik (stor bokstav, inga meningar)
        if text and text[0].isupper() and "." not in text_lower:
            if len(text_lower.split()) <= 5:
                return 9, "heading"

    return class_idx, None


def _create_data_yaml_for_classes(dataset_dir: Path, classes: list):
    """Skapar data.yaml med given klasslista (standard eller extended)."""
    import yaml

    train_imgs = dataset_dir / "images" / "train"
    val_imgs = dataset_dir / "images" / "val"
    test_imgs = dataset_dir / "images" / "test"

    data = {
        "path": str(dataset_dir.resolve()),
        "train": "images/train",
        "val": "images/val" if val_imgs.exists() else "images/train",
        "test": "images/test" if test_imgs.exists() else "",
        "nc": len(classes),
        "names": {i: name for i, name in enumerate(classes)},
    }

    yaml_path = dataset_dir / "data.yaml"
    with open(yaml_path, "w") as f:
        yaml.dump(data, f, default_flow_style=False)

    log(f"data.yaml: {len(classes)} klasser", "OK")


def _extract_rico_nodes(data: dict) -> list:
    """Extraherar alla leaf-noder med bounds från Rico JSON (rekursivt)."""
    nodes = []

    def _walk(node):
        if not isinstance(node, dict):
            return
        # Samla noder med componentLabel eller class + bounds
        if ("componentLabel" in node or "class" in node) and "bounds" in node:
            nodes.append(node)
        for child in node.get("children", []):
            _walk(child)

    # Rico combined format: {"activity": {"root": {...}}}
    if "activity" in data:
        root = data["activity"].get("root", data.get("activity", {}))
        _walk(root)
    else:
        _walk(data)

    return nodes


def convert_coco_to_yolo(coco_json_path: Path, images_dir: Path, output_dir: Path,
                         extended: bool = False) -> Path:
    """Konverterar COCO-format (annotations JSON + bilder) till YOLO-format.

    COCO-format:
        annotations.json  ← {"images": [...], "annotations": [...], "categories": [...]}
        images/
            img001.jpg, ...
    """
    log("Konverterar COCO-dataset till YOLO-format...", "STEP")
    active_classes = UI_CLASSES_EXTENDED if extended else UI_CLASSES

    with open(coco_json_path) as f:
        coco = json.load(f)

    # Bygg kategori-mappning
    coco_categories = {}
    for cat in coco.get("categories", []):
        cat_name = cat["name"].lower().strip()
        yolo_idx = _match_class_name(cat_name, extended)
        if yolo_idx is not None:
            coco_categories[cat["id"]] = yolo_idx

    if not coco_categories:
        log("Inga COCO-kategorier matchade UI_CLASSES. Mappar alla till index 0-N.", "WARN")
        for i, cat in enumerate(coco.get("categories", [])):
            if i < len(active_classes):
                coco_categories[cat["id"]] = i

    log(f"Mappade {len(coco_categories)} av {len(coco.get('categories', []))} COCO-kategorier", "INFO")

    # Bygg bild-ID → filnamn + dimensioner
    img_info = {}
    for img in coco.get("images", []):
        img_info[img["id"]] = {
            "file_name": img["file_name"],
            "width": img["width"],
            "height": img["height"],
        }

    # Gruppera annotationer per bild
    img_annotations = {}
    for ann in coco.get("annotations", []):
        img_id = ann["image_id"]
        cat_id = ann["category_id"]
        if cat_id not in coco_categories:
            continue
        if img_id not in img_annotations:
            img_annotations[img_id] = []
        img_annotations[img_id].append(ann)

    # Skapa output
    for split in ["images", "labels"]:
        (output_dir / split).mkdir(parents=True, exist_ok=True)

    converted = 0
    for img_id, anns in img_annotations.items():
        if img_id not in img_info:
            continue

        info = img_info[img_id]
        img_w = info["width"]
        img_h = info["height"]
        file_name = info["file_name"]

        # Kopiera bild
        src_img = images_dir / file_name
        if not src_img.exists():
            continue

        labels = []
        for ann in anns:
            class_idx = coco_categories[ann["category_id"]]
            bbox = ann.get("bbox", [])
            if len(bbox) != 4:
                continue

            # COCO bbox: [x, y, width, height] (pixel, top-left)
            x, y, w, h = bbox
            if w <= 0 or h <= 0:
                continue

            cx = (x + w / 2) / img_w
            cy = (y + h / 2) / img_h
            nw = w / img_w
            nh = h / img_h

            labels.append(f"{class_idx} {cx:.6f} {cy:.6f} {nw:.6f} {nh:.6f}")

        if not labels:
            continue

        shutil.copy2(src_img, output_dir / "images" / file_name)
        stem = Path(file_name).stem
        with open(output_dir / "labels" / f"{stem}.txt", "w") as f:
            f.write("\n".join(labels) + "\n")

        converted += 1

    log(f"COCO → YOLO: {converted} bilder konverterade", "OK")

    auto_split_dataset(output_dir)
    _create_data_yaml_for_classes(output_dir, active_classes)

    return output_dir


def convert_webui_to_yolo(webui_dir: Path, output_dir: Path, extended: bool = False) -> Path:
    """Konverterar WebUI-dataset till YOLO-format.

    Stödjer flera vanliga WebUI-format:

    Format A (JSON annotations per bild):
        webui_dir/
            images/
                page_001.png, ...
            annotations/
                page_001.json  ← {"elements": [{"type": "button", "bbox": [x1,y1,x2,y2]}, ...]}

    Format B (En stor JSON-fil):
        webui_dir/
            images/
                page_001.png, ...
            annotations.json  ← [{"image": "page_001.png", "elements": [...]}, ...]

    Format C (JSONL per rad):
        webui_dir/
            images/
                page_001.png, ...
            annotations.jsonl  ← en JSON per rad
    """
    log("Konverterar WebUI-dataset till YOLO-format...", "STEP")

    for split in ["images", "labels"]:
        (output_dir / split).mkdir(parents=True, exist_ok=True)

    converted = 0

    # Detektera format
    annotations_dir = webui_dir / "annotations"
    annotations_json = webui_dir / "annotations.json"
    annotations_jsonl = webui_dir / "annotations.jsonl"

    if annotations_dir.exists() and annotations_dir.is_dir():
        converted = _convert_webui_per_file(webui_dir, annotations_dir, output_dir)
    elif annotations_json.exists():
        converted = _convert_webui_single_json(webui_dir, annotations_json, output_dir)
    elif annotations_jsonl.exists():
        converted = _convert_webui_jsonl(webui_dir, annotations_jsonl, output_dir)
    else:
        log("Okänt WebUI-format. Förväntade annotations/ mapp, annotations.json eller annotations.jsonl", "ERR")
        sys.exit(1)

    log(f"WebUI → YOLO: {converted} bilder konverterade", "OK")

    active_classes = UI_CLASSES_EXTENDED if extended else UI_CLASSES
    auto_split_dataset(output_dir)
    _create_data_yaml_for_classes(output_dir, active_classes)

    return output_dir


def _convert_webui_per_file(webui_dir: Path, annotations_dir: Path, output_dir: Path) -> int:
    """Konverterar WebUI med en JSON-fil per bild."""
    images_dir = webui_dir / "images"
    converted = 0

    for ann_file in sorted(annotations_dir.glob("*.json")):
        with open(ann_file) as f:
            data = json.load(f)

        stem = ann_file.stem
        img_path = _find_image(images_dir, stem)
        if img_path is None:
            continue

        try:
            from PIL import Image
            img = Image.open(img_path)
            img_w, img_h = img.size
        except Exception:
            continue

        labels = _extract_webui_labels(data, img_w, img_h)
        if not labels:
            continue

        shutil.copy2(img_path, output_dir / "images" / img_path.name)
        with open(output_dir / "labels" / f"{stem}.txt", "w") as f:
            f.write("\n".join(labels) + "\n")
        converted += 1

    return converted


def _convert_webui_single_json(webui_dir: Path, json_path: Path, output_dir: Path) -> int:
    """Konverterar WebUI med en stor JSON-fil."""
    images_dir = webui_dir / "images"

    with open(json_path) as f:
        data = json.load(f)

    entries = data if isinstance(data, list) else data.get("pages", data.get("images", []))
    converted = 0

    for entry in entries:
        img_name = entry.get("image", entry.get("file_name", entry.get("screenshot", "")))
        if not img_name:
            continue

        img_path = images_dir / img_name
        if not img_path.exists():
            continue

        try:
            from PIL import Image
            img = Image.open(img_path)
            img_w, img_h = img.size
        except Exception:
            continue

        labels = _extract_webui_labels(entry, img_w, img_h)
        if not labels:
            continue

        shutil.copy2(img_path, output_dir / "images" / img_path.name)
        stem = Path(img_name).stem
        with open(output_dir / "labels" / f"{stem}.txt", "w") as f:
            f.write("\n".join(labels) + "\n")
        converted += 1

    return converted


def _convert_webui_jsonl(webui_dir: Path, jsonl_path: Path, output_dir: Path) -> int:
    """Konverterar WebUI med JSONL-format (en JSON per rad)."""
    images_dir = webui_dir / "images"
    converted = 0

    with open(jsonl_path) as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            entry = json.loads(line)

            img_name = entry.get("image", entry.get("file_name", entry.get("screenshot", "")))
            if not img_name:
                continue

            img_path = images_dir / img_name
            if not img_path.exists():
                continue

            try:
                from PIL import Image
                img = Image.open(img_path)
                img_w, img_h = img.size
            except Exception:
                continue

            labels = _extract_webui_labels(entry, img_w, img_h)
            if not labels:
                continue

            shutil.copy2(img_path, output_dir / "images" / img_path.name)
            stem = Path(img_name).stem
            with open(output_dir / "labels" / f"{stem}.txt", "w") as f:
                f.write("\n".join(labels) + "\n")
            converted += 1

    return converted


def _extract_webui_labels(data: dict, img_w: int, img_h: int) -> list:
    """Extraherar YOLO-labels från en WebUI annotation-post."""
    labels = []
    elements = data.get("elements", data.get("annotations", data.get("components", [])))

    for elem in elements:
        # Hämta klassnamn
        class_name = elem.get("type", elem.get("class", elem.get("label", elem.get("category", "")))).lower().strip()
        class_idx = _WEBUI_CLASS_MAP.get(class_name)
        if class_idx is None:
            class_idx = _match_class_name(class_name)
        if class_idx is None:
            continue

        # Hämta bbox — stödjer [x1,y1,x2,y2], {"x","y","width","height"}, och "bounds"
        bbox = elem.get("bbox", elem.get("bounds", elem.get("bounding_box", None)))
        if bbox is None and all(k in elem for k in ["x", "y", "width", "height"]):
            x, y, w, h = elem["x"], elem["y"], elem["width"], elem["height"]
            bbox = [x, y, x + w, y + h]

        if bbox is None or len(bbox) != 4:
            continue

        x1, y1, x2, y2 = [float(v) for v in bbox]

        # Autodetektera om koordinaterna är normaliserade (0-1) eller pixlar
        if max(x1, y1, x2, y2) <= 1.0:
            cx = (x1 + x2) / 2
            cy = (y1 + y2) / 2
            bw = x2 - x1
            bh = y2 - y1
        else:
            if x2 <= x1 or y2 <= y1:
                continue
            cx = ((x1 + x2) / 2) / img_w
            cy = ((y1 + y2) / 2) / img_h
            bw = (x2 - x1) / img_w
            bh = (y2 - y1) / img_h

        if bw <= 0 or bh <= 0 or bw > 1 or bh > 1:
            continue

        labels.append(f"{class_idx} {cx:.6f} {cy:.6f} {bw:.6f} {bh:.6f}")

    return labels


def _match_class_name(name: str, extended: bool = False) -> int:
    """Fuzzy-matchar ett klassnamn mot UI_CLASSES (eller extended)."""
    name = name.lower().strip()
    active_classes = UI_CLASSES_EXTENDED if extended else UI_CLASSES

    # Exakt match
    if name in active_classes:
        return active_classes.index(name)

    # WebUI-map
    if name in _WEBUI_CLASS_MAP:
        return _WEBUI_CLASS_MAP[name]

    # Extended-specifika mappningar (index matchar UI_CLASSES_EXTENDED)
    if extended:
        extended_map = {
            "price": 10, "pris": 10, "cost": 10, "currency": 10,
            "cta": 11, "call_to_action": 11, "buy_button": 11, "add_to_cart": 11,
            "product_card": 12, "product": 12, "card": 12,
            "navigation": 13, "nav": 13, "breadcrumb": 13, "tabs": 13, "menu": 13,
            "searchbox": 14, "search": 14, "searchbar": 14, "search_field": 14,
            "form": 15, "form_group": 15,
        }
        if name in extended_map:
            return extended_map[name]

    # Delsträngs-match
    for i, cls in enumerate(active_classes):
        if cls in name or name in cls:
            return i

    return None


def _find_image(images_dir: Path, stem: str) -> Path:
    """Hittar en bildfil med givet filnamn (utan extension)."""
    for ext in [".png", ".jpg", ".jpeg", ".webp"]:
        candidate = images_dir / f"{stem}{ext}"
        if candidate.exists():
            return candidate
    return None


def _prepare_yolo_repo_yaml(source_path: Path):
    """Förbered data.yaml för ett YOLO-repo (t.ex. YashJain).

    Hanterar varierande mappstrukturer:
    - train/images/ + val/images/ (YashJain-stil)
    - images/train/ + images/val/ (standard YOLO)
    - yolo_dataset/ undermapp
    """
    import yaml

    yaml_dst = source_path / "data.yaml"

    # Sök efter existerande yaml
    for name in ("data.yaml", "dataset.yaml"):
        candidate = source_path / name
        if candidate.exists():
            with open(candidate) as f:
                data = yaml.safe_load(f)
            if data and "names" in data:
                # Sätt absolut path
                data["path"] = str(source_path.resolve())
                with open(yaml_dst, "w") as f:
                    yaml.dump(data, f, default_flow_style=False)
                log(f"data.yaml uppdaterad med absolut path: {source_path.resolve()}", "OK")
                return

    # Ingen yaml hittad — auto-generera baserat på mappstruktur
    log("Ingen data.yaml hittad, autogenererar...", "INFO")

    # Hitta train-bilder
    train_dir = None
    for candidate in [
        source_path / "train" / "images",
        source_path / "images" / "train",
        source_path / "yolo_dataset" / "train" / "images",
        source_path / "yolo_dataset" / "images" / "train",
    ]:
        if candidate.exists():
            train_dir = candidate
            break

    if train_dir is None:
        log(f"Kunde inte hitta train-bilder i {source_path}", "WARN")
        return

    # Bestäm relativa paths
    rel_train = str(train_dir.parent.relative_to(source_path))
    val_dir = train_dir.parent.parent / "val" / "images"
    if not val_dir.exists():
        val_dir = train_dir.parent.parent / "images" / "val"
    rel_val = str(val_dir.parent.relative_to(source_path)) if val_dir.exists() else rel_train

    data = {
        "path": str(source_path.resolve()),
        "train": rel_train,
        "val": rel_val,
        "nc": len(UI_CLASSES),
        "names": {i: name for i, name in enumerate(UI_CLASSES)},
    }
    with open(yaml_dst, "w") as f:
        yaml.dump(data, f, default_flow_style=False)
    log(f"Genererade data.yaml: train={rel_train}, val={rel_val}", "OK")


# Ommappning: YashJain originalklasser → våra UI_CLASSES-index
# YashJain dataset.yaml: 0:link 1:button 2:input 3:select 4:textarea
#   5:label 6:checkbox 7:radio 8:dropdown 9:slider 10:toggle
#   11:menu_item 12:clickable 13:icon 14:image
_YASHJAIN_REMAP = {
    0: 2,     # link → link
    1: 0,     # button → button
    2: 1,     # input → textbox
    3: 8,     # select → combobox
    4: 1,     # textarea → textbox
    5: 4,     # label → text
    6: 6,     # checkbox → checkbox
    7: 7,     # radio → radio
    8: 8,     # dropdown → combobox
    9: None,  # slider → skip (ingen matchande klass)
    10: 6,    # toggle → checkbox
    11: 2,    # menu_item → link
    12: 0,    # clickable → button
    13: 3,    # icon → icon
    14: 5,    # image → img
}


def _remap_yashjain_labels(source_path: Path, extended: bool = False):
    """Ommappa YashJain YOLO-labels till AetherAgent UI_CLASSES.

    YashJain-datasetet har 15 klasser med annan ordning.
    Vi omskriver varje label-fils klass-ID till våra index.
    Rader med omappningsbara klasser (slider) tas bort.
    """
    import yaml

    active_classes = UI_CLASSES_EXTENDED if extended else UI_CLASSES
    nc = len(active_classes)
    remap = _YASHJAIN_REMAP

    # Hitta alla label-kataloger
    label_dirs = []
    for split in ("train", "val", "test"):
        # train/labels/ eller labels/train/
        for candidate in [
            source_path / split / "labels",
            source_path / "labels" / split,
        ]:
            if candidate.exists():
                label_dirs.append((split, candidate))
                break

    total_remapped = 0
    total_dropped = 0

    for split, label_dir in label_dirs:
        txt_files = list(label_dir.glob("*.txt"))
        for txt_file in txt_files:
            lines = txt_file.read_text().strip().split("\n")
            new_lines = []
            for line in lines:
                if not line.strip():
                    continue
                parts = line.strip().split()
                if len(parts) < 5:
                    continue
                old_cls = int(parts[0])
                new_cls = remap.get(old_cls)
                if new_cls is None or new_cls >= nc:
                    total_dropped += 1
                    continue
                parts[0] = str(new_cls)
                new_lines.append(" ".join(parts))
                total_remapped += 1
            txt_file.write_text("\n".join(new_lines) + "\n" if new_lines else "")

        log(f"  {split}: {len(txt_files)} label-filer behandlade", "INFO")

    log(f"Ommappning klar: {total_remapped} annotationer behållna, {total_dropped} borttagna", "OK")

    # Generera korrekt data.yaml
    yaml_dst = source_path / "data.yaml"

    # Detektera mappstruktur
    train_rel = val_rel = test_rel = None
    for t, v, te in [
        ("train/images", "val/images", "test/images"),
        ("images/train", "images/val", "images/test"),
    ]:
        td = source_path / t
        if td.exists() and (any(td.rglob("*.jpg")) or any(td.rglob("*.png"))):
            train_rel = t
            val_rel = v if (source_path / v).exists() else t
            test_rel = te if (source_path / te).exists() else ""
            break

    if train_rel is None:
        log("Kunde inte hitta träningsbilder efter ommappning", "ERR")
        sys.exit(1)

    data = {
        "path": str(source_path.resolve()),
        "train": train_rel,
        "val": val_rel,
        "test": test_rel,
        "nc": nc,
        "names": {i: name for i, name in enumerate(active_classes)},
    }
    with open(yaml_dst, "w") as f:
        yaml.dump(data, f, default_flow_style=False)

    log(f"data.yaml genererad: nc={nc}, train={train_rel}", "OK")


def convert_klarna_to_yolo(klarna_dir: Path, output_dir: Path,
                           extended: bool = False) -> Path:
    """Konverterar Klarna Product Page Dataset till YOLO-format.

    Klarna-datasetet har WTL-snapshots med JSON-metadata som innehåller
    elementkoordinater (bounding boxes) och klarna-ai-label attribut:
    Price, Name, Main picture, Add to cart, Cart.

    Mappning till UI_CLASSES:
      Price       → text (4) / price (10 extended)
      Name        → heading (9)
      Main picture → img (5)
      Add to cart  → button (0) / cta (11 extended)
      Cart         → button (0) / cta (11 extended)
    """
    import json
    import shutil

    active_classes = UI_CLASSES_EXTENDED if extended else UI_CLASSES

    # Klarna-label → class-id
    _klarna_label_map = {
        "Price": 10 if extended else 4,           # price / text
        "Name": 9,                                 # heading
        "Main picture": 5,                         # img
        "Add to cart": 11 if extended else 0,      # cta / button
        "Cart": 11 if extended else 0,             # cta / button
    }

    images_dir = output_dir / "images" / "train"
    labels_dir = output_dir / "labels" / "train"
    images_dir.mkdir(parents=True, exist_ok=True)
    labels_dir.mkdir(parents=True, exist_ok=True)

    # Hitta screenshots och WTL-metadata
    screenshots_dir = klarna_dir / "screenshots"
    wtl_dir = klarna_dir / "wtl"

    # WTL-snapshots har JSON-filer med elementmetadata
    # Struktur: wtl/<id>/snapshot.json eller wtl/<id>.json
    json_files = list(wtl_dir.rglob("*.json")) if wtl_dir.exists() else []
    log(f"Klarna: hittade {len(json_files)} WTL JSON-filer", "INFO")

    # Hitta alla screenshot-bilder
    screenshot_files = {}
    if screenshots_dir.exists():
        for img_path in screenshots_dir.rglob("*.png"):
            screenshot_files[img_path.stem] = img_path
        for img_path in screenshots_dir.rglob("*.jpg"):
            screenshot_files[img_path.stem] = img_path

    log(f"Klarna: hittade {len(screenshot_files)} screenshots", "INFO")

    saved = 0
    skipped = 0

    for json_path in json_files:
        try:
            with open(json_path, "r", encoding="utf-8") as f:
                data = json.load(f)
        except (json.JSONDecodeError, UnicodeDecodeError):
            skipped += 1
            continue

        # Hitta matchande screenshot
        page_id = json_path.stem
        if page_id == "snapshot":
            page_id = json_path.parent.name

        img_path = screenshot_files.get(page_id)
        if img_path is None:
            # Sök med varianter
            for variant in [page_id.replace("-", "_"), page_id.replace("_", "-")]:
                if variant in screenshot_files:
                    img_path = screenshot_files[variant]
                    break
        if img_path is None:
            skipped += 1
            continue

        # Hämta bilddimensioner
        try:
            from PIL import Image as PILImage
            with PILImage.open(img_path) as pil_img:
                img_w, img_h = pil_img.size
        except Exception:
            skipped += 1
            continue

        if img_w < 10 or img_h < 10:
            skipped += 1
            continue

        # Extrahera annoterade element från WTL-metadata
        yolo_lines = []
        elements = data.get("elements", data.get("nodes", []))

        # WTL-format: element har "attributes" dict med "klarna-ai-label"
        # och "location" dict med x, y, width, height
        for elem in elements:
            attrs = elem.get("attributes", {})
            klarna_label = attrs.get("klarna-ai-label")
            if not klarna_label:
                continue

            class_id = _klarna_label_map.get(klarna_label)
            if class_id is None:
                continue

            # Verifiera att class_id finns i aktiva klasser
            if class_id >= len(active_classes):
                # Fallback för extended klasser i standard-läge
                if klarna_label == "Price":
                    class_id = 4   # text
                else:
                    class_id = 0   # button
                if class_id >= len(active_classes):
                    continue

            loc = elem.get("location", elem.get("rect", elem.get("bbox", {})))
            if not loc:
                continue

            x = loc.get("x", loc.get("left", 0))
            y = loc.get("y", loc.get("top", 0))
            w = loc.get("width", loc.get("w", 0))
            h = loc.get("height", loc.get("h", 0))

            if w <= 0 or h <= 0:
                continue

            # Normalisera till YOLO-format
            x_center = (x + w / 2) / img_w
            y_center = (y + h / 2) / img_h
            nw = w / img_w
            nh = h / img_h

            # Klipp till [0, 1]
            x_center = max(0.0, min(1.0, x_center))
            y_center = max(0.0, min(1.0, y_center))
            nw = max(0.001, min(1.0, nw))
            nh = max(0.001, min(1.0, nh))

            yolo_lines.append(
                f"{class_id} {x_center:.6f} {y_center:.6f} {nw:.6f} {nh:.6f}"
            )

        if not yolo_lines:
            skipped += 1
            continue

        # Kopiera bild och spara labels
        dst_img = images_dir / f"klarna_{page_id}.png"
        shutil.copy2(str(img_path), str(dst_img))
        (labels_dir / f"klarna_{page_id}.txt").write_text("\n".join(yolo_lines))
        saved += 1

        if saved % 1000 == 0:
            log(f"  {saved} sidor konverterade...", "INFO")

    log(f"Klarna: konverterat {saved} sidor ({skipped} överhoppade)", "OK")

    # Auto-split
    auto_split_dataset(output_dir)

    return output_dir


def _convert_webclick_to_yolo(ds, output_dir: Path):
    """Konverterar Hcompany/WebClick dataset (ScreenSpot-format) till YOLO.

    Varje rad har: image (screenshot), instruction (text), bbox [x, y, w, h].
    Varje bild har exakt en annoterad element — vi mappar instruktionen till
    en UI-klass baserat på nyckelord.
    """
    from PIL import Image as PILImage

    images_dir = output_dir / "images" / "train"
    labels_dir = output_dir / "labels" / "train"
    images_dir.mkdir(parents=True, exist_ok=True)
    labels_dir.mkdir(parents=True, exist_ok=True)

    _class_map = {name: i for i, name in enumerate(UI_CLASSES)}

    # Instruktions-nyckelord → UI-klass
    _intent_to_class = {
        "click": 0,          # button
        "button": 0,
        "submit": 0,
        "press": 0,
        "tap": 0,
        "type": 1,           # textbox
        "enter": 1,
        "input": 1,
        "search": 1,
        "fill": 1,
        "write": 1,
        "link": 2,           # link
        "navigate": 2,
        "go to": 2,
        "open": 2,
        "select": 8,         # combobox
        "dropdown": 8,
        "choose": 8,
        "check": 6,          # checkbox
        "toggle": 6,
        "image": 5,          # img
        "photo": 5,
        "picture": 5,
        "heading": 9,        # heading
        "title": 9,
        "menu": 2,           # link (navigation)
        "tab": 0,            # button
        "close": 0,
        "icon": 3,           # icon
        "logo": 3,
    }

    # Hämta split
    split_name = "test" if "test" in ds else ("train" if "train" in ds else list(ds.keys())[0])
    split = ds[split_name]
    columns = split.column_names
    log(f"WebClick: {len(split)} samples (split: {split_name})", "STEP")
    log(f"  Kolumner: {columns}", "INFO")

    # Detektera kolumnnamn
    img_col = None
    for c in ("image", "screenshot", "img"):
        if c in columns:
            img_col = c
            break

    bbox_col = None
    for c in ("bbox", "bounding_box", "box", "target"):
        if c in columns:
            bbox_col = c
            break

    instruction_col = None
    for c in ("instruction", "intent", "text", "query", "command"):
        if c in columns:
            instruction_col = c
            break

    saved = 0
    skipped = 0
    import io

    for idx, row in enumerate(split):
        if idx % 500 == 0 and idx > 0:
            log(f"  {idx}/{len(split)} konverterade...", "INFO")

        # Extrahera bild
        img = None
        if img_col and row.get(img_col) is not None:
            val = row[img_col]
            if isinstance(val, PILImage.Image):
                img = val
            elif isinstance(val, dict) and "bytes" in val:
                img = PILImage.open(io.BytesIO(val["bytes"]))
            elif isinstance(val, bytes):
                img = PILImage.open(io.BytesIO(val))
        if img is None:
            skipped += 1
            continue

        img_w, img_h = img.size
        if img_w < 10 or img_h < 10:
            skipped += 1
            continue

        # Extrahera bbox
        bbox = row.get(bbox_col) if bbox_col else None
        if bbox is None or len(bbox) < 4:
            skipped += 1
            continue

        # Bestäm UI-klass från instruktion
        instruction = row.get(instruction_col, "") if instruction_col else ""
        class_id = _classify_webclick_instruction(instruction, _intent_to_class)

        # Tolka bbox — ScreenSpot-format: [x, y, w, h] (pixlar)
        bx, by, bw, bh = float(bbox[0]), float(bbox[1]), float(bbox[2]), float(bbox[3])

        # Kolla om det är [x1, y1, x2, y2] (om w/h > img dimensions)
        if bw > img_w or bh > img_h or (bw > bx and bh > by and bw < img_w * 2):
            # Troligen [x1, y1, x2, y2]
            x1, y1, x2, y2 = bx, by, bw, bh
            bw = x2 - x1
            bh = y2 - y1
            bx = x1

        # Normalisera till YOLO
        x_center = (bx + bw / 2) / img_w
        y_center = (by + bh / 2) / img_h
        nw = bw / img_w
        nh = bh / img_h

        # Klipp till [0, 1]
        x_center = max(0.0, min(1.0, x_center))
        y_center = max(0.0, min(1.0, y_center))
        nw = max(0.001, min(1.0, nw))
        nh = max(0.001, min(1.0, nh))

        # Spara
        img_name = f"webclick_{idx:06d}.png"
        img.save(str(images_dir / img_name))
        label_name = f"webclick_{idx:06d}.txt"
        (labels_dir / label_name).write_text(
            f"{class_id} {x_center:.6f} {y_center:.6f} {nw:.6f} {nh:.6f}"
        )
        saved += 1

    log(f"WebClick: konverterat {saved} bilder ({skipped} överhoppade)", "OK")

    auto_split_dataset(output_dir)


def _classify_webclick_instruction(instruction: str, intent_map: dict) -> int:
    """Klassificera en WebClick-instruktion till UI-klass baserat på nyckelord."""
    text = instruction.lower()
    for keyword, class_id in intent_map.items():
        if keyword in text:
            return class_id
    # Default: button (mest troligt klick-mål)
    return 0


def _remap_roboflow_ui_labels(source_path: Path, extended: bool = False):
    """Mappa Roboflow UI Screenshots klasser till AetherAgent UI_CLASSES.

    Roboflow-klasser: button, field, heading, iframe, image, label, link, text
    → AetherAgent: button(0), textbox(1), link(2), icon(3), text(4),
                   img(5), checkbox(6), radio(7), combobox(8), heading(9)
    """
    active_classes = UI_CLASSES_EXTENDED if extended else UI_CLASSES
    nc = len(active_classes)

    # Roboflow klass-index → AetherAgent klass-index
    # Roboflow ordning: 0=button, 1=field, 2=heading, 3=iframe, 4=image, 5=label, 6=link, 7=text
    _roboflow_to_ui = {
        0: 0,   # button → button
        1: 1,   # field → textbox
        2: 9,   # heading → heading
        3: 5,   # iframe → img (visuellt block)
        4: 5,   # image → img
        5: 4,   # label → text
        6: 2,   # link → link
        7: 4,   # text → text
    }

    log("Roboflow UI: omklassificerar labels till AetherAgent-klasser", "STEP")
    total_remapped = 0
    total_dropped = 0

    for split in ("train", "val", "test"):
        labels_path = None
        for pattern in [
            source_path / "labels" / split,
            source_path / split / "labels",
        ]:
            if pattern.exists():
                labels_path = pattern
                break

        if labels_path is None:
            continue

        txt_files = list(labels_path.glob("*.txt"))
        for txt_file in txt_files:
            if txt_file.name == "classes.txt":
                continue
            lines = txt_file.read_text().strip().splitlines()
            new_lines = []
            for line in lines:
                parts = line.strip().split()
                if len(parts) < 5:
                    continue
                old_cls = int(parts[0])
                new_cls = _roboflow_to_ui.get(old_cls)
                if new_cls is None or new_cls >= nc:
                    total_dropped += 1
                    continue
                parts[0] = str(new_cls)
                new_lines.append(" ".join(parts))
                total_remapped += 1
            txt_file.write_text("\n".join(new_lines) + "\n" if new_lines else "")

        log(f"  {split}: {len(txt_files)} label-filer behandlade", "INFO")

    log(f"Roboflow ommappning klar: {total_remapped} behållna, {total_dropped} borttagna", "OK")

    # Generera korrekt data.yaml
    yaml_dst = source_path / "data.yaml"
    train_rel = val_rel = test_rel = None
    for t, v, te in [
        ("train/images", "val/images", "test/images"),
        ("images/train", "images/val", "images/test"),
    ]:
        td = source_path / t
        if td.exists() and (any(td.rglob("*.jpg")) or any(td.rglob("*.png"))):
            train_rel = t
            val_rel = v if (source_path / v).exists() else t
            test_rel = te if (source_path / te).exists() else ""
            break

    if train_rel:
        import yaml
        data = {
            "path": str(source_path.resolve()),
            "train": train_rel,
            "val": val_rel,
            "test": test_rel,
            "nc": nc,
            "names": {i: name for i, name in enumerate(active_classes)},
        }
        with open(yaml_dst, "w") as f:
            yaml.dump(data, f, default_flow_style=False)
        log(f"data.yaml genererad: nc={nc}, train={train_rel}", "OK")


def convert_dataset(source_path: Path, output_dir: Path, fmt: str,
                    extended: bool = False) -> Path:
    """Huvudfunktion: konverterar dataset från givet format till YOLO.

    Args:
        source_path: Sökväg till källdataset
        output_dir: Sökväg till YOLO-output
        fmt: Format — "rico", "coco", "webui", "yolo" (passthrough)
        extended: Använd utökade agentklasser (16 st)

    Returns:
        Sökväg till YOLO-dataset
    """
    fmt = fmt.lower().strip()

    if fmt == "yolo":
        log("Dataset är redan YOLO-format, ingen konvertering behövs", "OK")
        return source_path

    if fmt == "rico":
        return convert_rico_to_yolo(source_path, output_dir, extended=extended)

    if fmt == "coco":
        # COCO: förväntar sig annotations JSON + images/
        coco_json = source_path / "annotations.json"
        if not coco_json.exists():
            # Sök efter andra vanliga namn
            for name in ["instances_train.json", "instances_default.json", "_annotations.coco.json"]:
                candidate = source_path / name
                if candidate.exists():
                    coco_json = candidate
                    break
            else:
                # Sök i annotations/ mapp
                ann_dir = source_path / "annotations"
                if ann_dir.exists():
                    jsons = list(ann_dir.glob("*.json"))
                    if jsons:
                        coco_json = jsons[0]
                    else:
                        log(f"Hittade ingen COCO JSON i {source_path}", "ERR")
                        sys.exit(1)
                else:
                    log(f"Hittade ingen COCO annotations-fil i {source_path}", "ERR")
                    sys.exit(1)

        images_dir = source_path / "images"
        if not images_dir.exists():
            images_dir = source_path / "train"
        if not images_dir.exists():
            images_dir = source_path  # Bilder i root

        return convert_coco_to_yolo(coco_json, images_dir, output_dir, extended=extended)

    if fmt == "webui":
        return convert_webui_to_yolo(source_path, output_dir, extended=extended)

    # yashjain — YOLO-format men med annan klassindelning, kräver ommappning
    if fmt == "yashjain":
        log("YashJain-dataset: omklassificerar labels till AetherAgent-klasser", "STEP")
        _remap_yashjain_labels(source_path, extended=extended)
        return source_path

    # klarna — WTL-metadata med bboxar + screenshots
    if fmt == "klarna":
        return convert_klarna_to_yolo(source_path, output_dir, extended=extended)

    # webclick — HuggingFace ScreenSpot-format, konverteras vid nedladdning
    if fmt == "webclick":
        log("WebClick-dataset konverterades vid nedladdning", "OK")
        return source_path

    # roboflow-ui — YOLO-format, kräver klassommappning
    if fmt == "roboflow-ui":
        log("Roboflow UI: omklassificerar labels till AetherAgent-klasser", "STEP")
        _remap_roboflow_ui_labels(source_path, extended=extended)
        return source_path

    # HuggingFace-datasets konverteras redan vid nedladdning (_convert_hf_to_yolo)
    hf_formats = ("osatlas", "guiactor", "showui-web", "waveui")
    if fmt in hf_formats:
        log(f"HuggingFace-dataset ({fmt}) konverterades vid nedladdning", "OK")
        return source_path

    log(f"Okänt format: {fmt}. Stödda: rico, coco, webui, osatlas, guiactor, "
        f"showui-web, waveui, yashjain, klarna, webclick, roboflow-ui, yolo", "ERR")
    sys.exit(1)


# ---------------------------------------------------------------------------
# Step 2: Training
# ---------------------------------------------------------------------------

def train_model(
    data_yaml: Path,
    epochs: int,
    batch: int,
    imgsz: int,
    model_base: str,
    project: str,
    name: str,
    resume: bool = False,
    device: str = None,
    early_stop: bool = False,
    target_map50: float = DEFAULT_TARGET_MAP50,
    target_map5095: float = DEFAULT_TARGET_MAP5095,
    patience: int = DEFAULT_PATIENCE,
) -> Path:
    """Train YOLO model with Ultralytics.

    Early stopping: om early_stop=True, träningen avbryts automatiskt när:
      1. mAP@50 ≥ target_map50 OCH mAP@50-95 ≥ target_map5095, eller
      2. Ingen förbättring på `patience` epochs (Ultralytics inbyggd).
    Modellen sparas alltid — best.pt uppdateras löpande av Ultralytics.
    """
    from ultralytics import YOLO

    if early_stop:
        log(f"Starting training: max {epochs} epochs (early-stop ON), batch={batch}, imgsz={imgsz}", "STEP")
        log(f"  Mål: mAP@50 ≥ {target_map50:.2f}, mAP@50-95 ≥ {target_map5095:.2f}, patience={patience}", "INFO")
    else:
        log(f"Starting training: {epochs} epochs, batch={batch}, imgsz={imgsz}, device={device or 'auto'}", "STEP")
    log(f"Base model: {model_base}", "INFO")

    model = YOLO(model_base)

    # Detektera VRAM för auto-tuning av batch/workers
    vram_gb = 0
    num_cpu = os.cpu_count() or 8
    try:
        import torch
        if torch.cuda.is_available():
            vram_gb = torch.cuda.get_device_properties(0).total_memory / (1024**3)
    except Exception:
        pass

    # Auto-tune batch om användaren valt default
    if batch == DEFAULT_BATCH and vram_gb > 0:
        # YOLO26n @ 640px med AMP: ~0.5 GB overhead + ~0.55 GB/batch-item
        # Lämna 3 GB marginal för overhead, augmentation, gradients
        usable_vram = vram_gb - 3.0
        auto_batch = max(8, min(int(usable_vram / 0.55), 64))
        # Avrunda nedåt till närmaste multipel av 8
        auto_batch = (auto_batch // 8) * 8
        if auto_batch != batch:
            log(f"Auto-tuned batch: {batch} → {auto_batch} (baserat på {vram_gb:.0f} GB VRAM)", "OK")
            batch = auto_batch

    # Workers: max 4 med cache=ram (fler → RAM-explosion), annars max 8
    optimal_workers = min(num_cpu, 4)

    # Bygg träningsparametrar
    train_kwargs = dict(
        data=str(data_yaml),
        epochs=epochs,
        imgsz=imgsz,
        batch=batch,
        project=project,
        name=name,
        exist_ok=True,
        resume=resume,
        workers=optimal_workers,
        cache="ram",   # Snabbast — RAM-tryck hanteras via färre workers
        # Augmentation tuned for UI (less aggressive than natural images)
        mosaic=0.5,
        mixup=0.0,         # Mixup hurts UI element detection
        degrees=0.0,       # No rotation — UI is always upright
        flipud=0.0,        # No vertical flip
        fliplr=0.3,        # Slight horizontal flip OK
        hsv_h=0.01,        # Minimal hue shift (buttons are colored)
        hsv_s=0.3,
        hsv_v=0.3,
        scale=0.3,
        translate=0.1,
        verbose=True,
        plots=True,
        # Dataloader-optimeringar
        multi_scale=0.0,   # Avstängt — 0.5 krympte bilder till ~352px, svälte GPU:n
    )

    # Early stopping: Ultralytics patience (platå-detection)
    if early_stop:
        train_kwargs["patience"] = patience
    else:
        # patience=0 = ingen early stop (kör alla epochs)
        train_kwargs["patience"] = 0

    # GPU-specifika optimeringar
    if device == "cpu":
        train_kwargs["device"] = "cpu"
        train_kwargs["amp"] = False  # AMP fungerar inte på CPU
        train_kwargs["multi_scale"] = 0.0  # Multi-scale för dyrt på CPU
        log("Training on CPU — this will be slow but works", "WARN")
    else:
        if device:
            train_kwargs["device"] = device
        train_kwargs["amp"] = True  # FP16 mixed precision — 2x speedup på Ampere+

        # torch.compile (PyTorch 2.0+) — JIT-kompilerar modellen för snabbare kernels
        # Avstängt: Blackwell (RTX 5090) har instabilt Dynamo-stöd, ger pow_by_natural-
        # varningar och extremt lång kompileringstid. Kan återaktiveras när PyTorch
        # har fullt Blackwell-stöd.
        # try:
        #     import torch
        #     if hasattr(torch, "compile") and torch.__version__ >= "2.0":
        #         train_kwargs["compile"] = True
        #         log("torch.compile aktiverat — snabbare GPU-kernels", "OK")
        # except Exception:
        #     pass

    log(f"Training config: batch={batch}, workers={optimal_workers}, "
        f"VRAM={vram_gb:.0f}GB, multi_scale={train_kwargs.get('multi_scale', 0)}", "INFO")

    # Metric-target callback: stoppar träning när mål nåtts
    # Ultralytics patience hanterar platå, men denna callback stoppar vid specifika mål.
    _stop_reason = {}  # Mutable dict för att kommunicera från callback
    if early_stop:
        results_csv_path = Path(project) / name / "results.csv"

        def _check_metric_targets(trainer):
            """Callback som körs efter varje val-epoch. Stoppar om mål nåtts."""
            history = _parse_results_csv(results_csv_path)
            if not history:
                return
            latest = history[-1]
            current_map50 = latest.get("map50", 0)
            current_map5095 = latest.get("map5095", 0)
            epoch_num = int(latest.get("epoch", len(history)))

            # Kolla om båda målen nåtts
            map50_ok = current_map50 >= target_map50
            map5095_ok = current_map5095 >= target_map5095

            if map50_ok and map5095_ok:
                log(f"🎯 Mål nåtts vid epoch {epoch_num}! "
                    f"mAP@50={current_map50:.4f} (≥{target_map50}), "
                    f"mAP@50-95={current_map5095:.4f} (≥{target_map5095})", "OK")
                _stop_reason["met_targets"] = True
                _stop_reason["epoch"] = epoch_num
                _stop_reason["map50"] = current_map50
                _stop_reason["map5095"] = current_map5095
                # Signalera till Ultralytics att stoppa
                trainer.stop = True
            elif epoch_num % 10 == 0:
                # Progress-rapport var 10:e epoch
                status_parts = []
                if map50_ok:
                    status_parts.append(f"mAP@50 ✓ {current_map50:.3f}")
                else:
                    status_parts.append(f"mAP@50 {current_map50:.3f}/{target_map50}")
                if map5095_ok:
                    status_parts.append(f"mAP@50-95 ✓ {current_map5095:.3f}")
                else:
                    status_parts.append(f"mAP@50-95 {current_map5095:.3f}/{target_map5095}")
                log(f"  Epoch {epoch_num}: {', '.join(status_parts)}", "INFO")

        # Registrera callback på Ultralytics trainer
        model.add_callback("on_fit_epoch_end", _check_metric_targets)
        log("Metric-target callback registrerad", "OK")

    results = model.train(**train_kwargs)

    # Logga varför träningen stoppade
    if early_stop:
        if _stop_reason.get("met_targets"):
            log(f"Träningen stoppade: mål nåtts vid epoch {_stop_reason['epoch']} "
                f"(mAP@50={_stop_reason['map50']:.4f}, mAP@50-95={_stop_reason['map5095']:.4f})", "OK")
        else:
            # Kolla om patience triggade
            final_epoch = getattr(results, "epoch", epochs)
            if final_epoch < epochs - 1:
                log(f"Träningen stoppade: patience ({patience} epochs utan förbättring)", "INFO")
            else:
                log(f"Träningen körde alla {epochs} epochs utan att nå målen", "WARN")
                log(f"  Tips: öka --epochs, justera --target-map50/--target-map5095, eller förbättra datasetet", "INFO")

    best_pt = Path(project) / name / "weights" / "best.pt"
    if not best_pt.exists():
        # Fallback: YOLO sparar ibland i results.save_dir
        save_dir = getattr(results, "save_dir", None)
        if save_dir:
            fallback = Path(save_dir) / "weights" / "best.pt"
            if fallback.exists():
                log(f"best.pt hittades via save_dir: {fallback}", "WARN")
                best_pt = fallback
        if not best_pt.exists():
            log(f"best.pt hittades inte efter träning! Förväntad: {best_pt}", "ERR")
            sys.exit(1)

    # Print key metrics
    log(f"Training complete! Best model: {best_pt}", "OK")

    return best_pt


# ---------------------------------------------------------------------------
# Step 3: Validation
# ---------------------------------------------------------------------------

def validate_model(best_pt: Path, data_yaml: Path, imgsz: int) -> dict:
    """Run validation and return metrics."""
    from ultralytics import YOLO

    log("Running validation on best model...", "STEP")

    model = YOLO(str(best_pt))
    metrics = model.val(data=str(data_yaml), imgsz=imgsz, verbose=False)

    map50 = metrics.box.map50
    map5095 = metrics.box.map
    precision = metrics.box.mp
    recall = metrics.box.mr

    log(f"mAP@50:    {map50:.4f}", "OK")
    log(f"mAP@50-95: {map5095:.4f}", "OK")
    log(f"Precision:  {precision:.4f}", "OK")
    log(f"Recall:     {recall:.4f}", "OK")

    if map50 < 0.3:
        log("mAP@50 < 0.30 — model may need more training data or epochs", "WARN")

    return {
        "map50": float(map50),
        "map5095": float(map5095),
        "precision": float(precision),
        "recall": float(recall),
    }


# ---------------------------------------------------------------------------
# Step 4: ONNX Export
# ---------------------------------------------------------------------------

EXPORT_OPTIONS = [
    {
        "key": "fp32",
        "label": "ONNX FP32",
        "desc": "Full precision, ~11-12 MB, 0% kvalitetsförlust",
        "kwargs": {},
    },
    {
        "key": "fp16",
        "label": "ONNX FP16 (half)",
        "desc": "~5.8-6.5 MB, 1.5-2x snabbare, ~0-2% kvalitetsförlust (REKOMMENDERAD)",
        "kwargs": {"half": True},
    },
    {
        "key": "int8",
        "label": "ONNX INT8",
        "desc": "~3.5-5 MB, 2-3x snabbare (CPU/edge), ~3-8% kvalitetsförlust",
        "kwargs": {"int8": True},
    },
]


def prompt_export_format() -> dict:
    """Interaktiv val av exportformat i terminalen."""
    print("\n  Välj exportformat:\n")
    print("  ┌─────────────────────────────────────────────────────────────────────┐")
    print("  │ #  Format          Storlek      Hastighet    Kvalitetsförlust       │")
    print("  ├─────────────────────────────────────────────────────────────────────┤")
    print("  │ 1) ONNX FP32      ~11-12 MB    Baseline     0%                     │")
    print("  │ 2) ONNX FP16 *    ~5.8-6.5 MB  1.5-2x       ~0-2%  (REKOMMENDERAD)│")
    print("  │ 3) ONNX INT8      ~3.5-5 MB    2-3x (CPU)   ~3-8%  (kräver kalib.) │")
    print("  └─────────────────────────────────────────────────────────────────────┘")
    print()

    while True:
        choice = input("  Exportformat [1-3, Enter=2 (FP16)]: ").strip()
        if not choice:
            return EXPORT_OPTIONS[1]  # FP16 default
        try:
            idx = int(choice)
            if 1 <= idx <= len(EXPORT_OPTIONS):
                selected = EXPORT_OPTIONS[idx - 1]
                log(f"Valt exportformat: {selected['label']}", "OK")
                return selected
        except ValueError:
            pass
        print(f"  Ogiltigt val. Ange 1-{len(EXPORT_OPTIONS)}.")


def export_onnx(best_pt: Path, imgsz: int, export_fmt: dict | None = None) -> Path:
    """Export best.pt → ONNX med valt format (FP32/FP16/INT8)."""
    from ultralytics import YOLO

    if export_fmt is None:
        export_fmt = EXPORT_OPTIONS[1]  # FP16 default

    log(f"Exporting to {export_fmt['label']}...", "STEP")

    model = YOLO(str(best_pt))

    export_kwargs = dict(
        format="onnx",
        imgsz=imgsz,
        opset=17,
        simplify=True,
        dynamic=False,
    )
    export_kwargs.update(export_fmt.get("kwargs", {}))

    onnx_path_str = model.export(**export_kwargs)
    onnx_path = Path(onnx_path_str)

    size_mb = onnx_path.stat().st_size / (1024 * 1024)
    log(f"ONNX exported: {onnx_path} ({size_mb:.1f} MB)", "OK")

    pt_size = best_pt.stat().st_size / (1024 * 1024)
    reduction = (1 - size_mb / pt_size) * 100 if pt_size > 0 else 0
    if reduction > 0:
        log(f"  Storlek: {pt_size:.1f} MB (.pt) → {size_mb:.1f} MB (.onnx) "
            f"({reduction:.0f}% reduktion)", "INFO")

    if size_mb > 6:
        log(f"Model is {size_mb:.1f} MB (> 6 MB target). Prova FP16 eller INT8.", "WARN")

    return onnx_path


def convert_rten(onnx_path: Path) -> Path:
    """Optionally convert ONNX → rten format (faster loading in AetherAgent)."""
    rten_path = onnx_path.with_suffix(".rten")

    try:
        run(f"{sys.executable} -m pip install rten-convert 2>/dev/null", check=False, capture=True)
        run(f"rten-convert {onnx_path} {rten_path}")
        size_mb = rten_path.stat().st_size / (1024 * 1024)
        log(f"rten format: {rten_path} ({size_mb:.1f} MB)", "OK")
        return rten_path
    except Exception:
        log("rten-convert not available — using ONNX directly (works fine)", "WARN")
        return onnx_path


# ---------------------------------------------------------------------------
# Step 5: Deploy & Verify
# ---------------------------------------------------------------------------

def copy_to_deploy(onnx_path: Path, deploy_dir: Path, version: str) -> Path:
    """Copy model to deployment directory with versioned name."""
    deploy_dir.mkdir(parents=True, exist_ok=True)
    deploy_name = f"aether-ui-{version}.onnx"
    deploy_path = deploy_dir / deploy_name
    shutil.copy2(onnx_path, deploy_path)
    log(f"Deployed to: {deploy_path}", "OK")

    # Also copy as "latest"
    latest_path = deploy_dir / "aether-ui-latest.onnx"
    shutil.copy2(onnx_path, latest_path)
    log(f"Latest symlink: {latest_path}", "OK")

    # Also deploy .rten version if it exists (preferred by AetherAgent server)
    rten_path = onnx_path.with_suffix(".rten")
    if rten_path.exists():
        rten_deploy = deploy_dir / f"aether-ui-{version}.rten"
        rten_latest = deploy_dir / "aether-ui-latest.rten"
        shutil.copy2(rten_path, rten_deploy)
        shutil.copy2(rten_path, rten_latest)
        log(f"rten deployed: {rten_deploy} + {rten_latest.name}", "OK")

    return deploy_path


def verify_with_server(onnx_path: Path, server_url: str, test_png: Path = None):
    """Verify the model works with AetherAgent's /api/parse-screenshot endpoint."""
    import requests

    log(f"Verifying model against {server_url}...", "STEP")

    # Create a simple test image if none provided
    if test_png is None or not test_png.exists():
        try:
            from PIL import Image, ImageDraw
        except ImportError:
            log("Pillow not installed, skipping verification", "WARN")
            return

        img = Image.new("RGB", (640, 640), (255, 255, 255))
        draw = ImageDraw.Draw(img)
        # Draw a button
        draw.rounded_rectangle([100, 200, 250, 240], radius=5, fill=(59, 130, 246))
        draw.text((120, 210), "Sign In", fill="white")
        # Draw an input
        draw.rectangle([100, 150, 350, 180], outline=(200, 200, 200), width=2)

        test_png = Path("/tmp/aether_test_screenshot.png")
        img.save(test_png)

    # Read files
    with open(test_png, "rb") as f:
        png_b64 = base64.b64encode(f.read()).decode()
    with open(onnx_path, "rb") as f:
        model_b64 = base64.b64encode(f.read()).decode()

    # Call API
    try:
        resp = requests.post(
            f"{server_url}/api/parse-screenshot",
            json={
                "png_base64": png_b64,
                "model_base64": model_b64,
                "goal": "find the sign in button",
                "config": {
                    "confidence_threshold": 0.25,
                    "nms_threshold": 0.45,
                    "input_size": 640,
                    "model_version": onnx_path.stem,
                },
            },
            timeout=30,
        )

        if resp.status_code == 200:
            result = resp.json()
            n_detections = len(result.get("detections", []))
            inference_ms = result.get("inference_time_ms", "?")
            log(f"API verification OK: {n_detections} detections, {inference_ms}ms inference", "OK")

            for det in result.get("detections", [])[:5]:
                log(f"  {det['class']} (conf={det['confidence']:.2f}) @ {det['bbox']}", "INFO")
        else:
            log(f"API returned {resp.status_code}: {resp.text[:200]}", "WARN")
            log("Model exported successfully — verify API server has --features vision", "WARN")

    except requests.ConnectionError:
        log(f"Cannot connect to {server_url} — server not running?", "WARN")
        log("Start server: cargo run --features server,vision --bin aether-server", "INFO")
        log("Model exported successfully — verify manually when server is running", "INFO")
    except Exception as e:
        log(f"Verification error: {e}", "WARN")


def _parse_results_csv(csv_path: Path) -> list[dict]:
    """Parse YOLO results.csv for epoch-by-epoch metrics."""
    history = []
    if not csv_path.exists():
        return history
    try:
        with open(csv_path) as f:
            import csv as csv_mod
            reader = csv_mod.DictReader(f)
            for row in reader:
                # YOLO results.csv har kolumner med whitespace i namnen
                cleaned = {k.strip(): v.strip() for k, v in row.items()}
                entry = {}
                for key, target in [
                    ("epoch", "epoch"),
                    ("metrics/mAP50(B)", "map50"),
                    ("metrics/mAP50-95(B)", "map5095"),
                    ("metrics/precision(B)", "precision"),
                    ("metrics/recall(B)", "recall"),
                    ("train/box_loss", "box_loss"),
                    ("train/cls_loss", "cls_loss"),
                    ("train/dfl_loss", "dfl_loss"),
                ]:
                    if key in cleaned:
                        try:
                            entry[target] = float(cleaned[key])
                        except ValueError:
                            pass
                if entry:
                    history.append(entry)
    except Exception:
        pass
    return history


def _pct_change(baseline: float, final: float) -> str:
    """Format percentage change with arrow."""
    if baseline == 0:
        return "N/A"
    pct = ((final - baseline) / baseline) * 100
    arrow = "▲" if pct > 0 else "▼" if pct < 0 else "="
    # Near-zero baseline ger absurda procent — visa absolut diff istället
    if baseline < 0.01:
        return f"{arrow} {final:.4f} (från ~0)"
    return f"{arrow} {pct:+.1f}%"


# ---------------------------------------------------------------------------
# Benchmarks — automated validation against standard GUI grounding datasets
# ---------------------------------------------------------------------------

_BENCHMARK_REGISTRY = {
    "screenspot-v2": {
        "name": "ScreenSpot-v2",
        "hf_dataset": "Voxel51/ScreenSpot-v2",
        "size_hint": "~1.4 GB",
        "samples": 1272,
        "description": "Standard GUI grounding eval — web, desktop, mobile. "
                       "Normalized [x,y,w,h] bboxar, click accuracy.",
        "metric": "click_accuracy",
        "platforms": ["web", "desktop", "mobile"],
    },
    "screenspot-pro": {
        "name": "ScreenSpot-Pro",
        "hf_dataset": "Voxel51/ScreenSpot-Pro",
        "size_hint": "~2 GB",
        "samples": 1581,
        "description": "Svår benchmark — professionella appar, element = 0.07% av bild. "
                       "23 appar, 5 branscher.",
        "metric": "click_accuracy",
        "platforms": ["desktop"],
    },
    "groundui-18k": {
        "name": "GroundUI-18K",
        "hf_dataset": "Voxel51/GroundUI-18k",
        "size_hint": "~5 GB",
        "samples": 18026,
        "description": "Cross-platform grounding — 18K samples från Mind2Web, OmniACT, "
                       "MoTIF, ScreenSpot, AgentStudio. ICLR 2025.",
        "metric": "click_accuracy",
        "platforms": ["web", "desktop", "mobile"],
    },
    "ui-vision": {
        "name": "ServiceNow UI-Vision",
        "hf_dataset": "ServiceNow/ui-vision",
        "size_hint": "~3 GB",
        "samples": 5000,
        "description": "83 desktop-appar, MIT-licens. Element grounding + layout grounding. "
                       "ICML 2025.",
        "metric": "map_and_click",
        "platforms": ["desktop"],
    },
}


def download_benchmark(name: str, output_dir: Path) -> Path:
    """Ladda ner ett benchmark-dataset från HuggingFace.

    Returns:
        Sökväg till nedladdat dataset
    """
    if name not in _BENCHMARK_REGISTRY:
        log(f"Okänt benchmark: {name}", "ERR")
        log(f"Tillgängliga: {', '.join(_BENCHMARK_REGISTRY.keys())}", "INFO")
        sys.exit(1)

    info = _BENCHMARK_REGISTRY[name]
    hf_name = info["hf_dataset"]
    bench_dir = output_dir / f"benchmark_{name}"
    bench_dir.mkdir(parents=True, exist_ok=True)

    marker = bench_dir / ".benchmark_download_complete"
    if marker.exists():
        log(f"Benchmark {info['name']} redan nedladdat: {bench_dir}", "OK")
        return bench_dir

    log(f"Laddar ner benchmark: {info['name']}", "STEP")
    log(f"  Dataset: {hf_name}", "INFO")
    log(f"  Storlek: {info['size_hint']}", "INFO")
    log(f"  Samples: ~{info['samples']}", "INFO")

    try:
        from huggingface_hub import snapshot_download
    except ImportError:
        log("Installerar 'huggingface_hub'...", "INFO")
        run(f"{sys.executable} -m pip install huggingface_hub")
        from huggingface_hub import snapshot_download

    snapshot_dir = bench_dir / "snapshot"
    try:
        snapshot_download(
            repo_id=hf_name,
            repo_type="dataset",
            local_dir=str(snapshot_dir),
        )
    except Exception as e:
        log(f"snapshot_download misslyckades: {e}", "WARN")
        log("Faller tillbaka på load_dataset()...", "WARN")

    marker.touch()
    log(f"Benchmark nedladdat: {bench_dir}", "OK")
    return bench_dir


def _load_benchmark_samples(bench_dir: Path, bench_name: str) -> list[dict]:
    """Ladda benchmark-samples till en enhetlig lista.

    Returnerar lista av dicts med:
        image: PIL.Image
        bbox: [x_center, y_center, w, h] (normaliserat 0-1)
        instruction: str
        platform: str
        label_type: str (text/icon)
    """
    from PIL import Image as PILImage
    import io

    info = _BENCHMARK_REGISTRY[bench_name]
    hf_name = info["hf_dataset"]
    snapshot_dir = bench_dir / "snapshot"

    # Försök FiftyOne-format (samples.json) — ScreenSpot/GroundUI använder detta
    samples_json = snapshot_dir / "samples.json"
    if samples_json.exists():
        return _load_fiftyone_benchmark(snapshot_dir, samples_json, bench_name)

    # Fallback: load_dataset via HuggingFace
    try:
        from datasets import load_dataset
    except ImportError:
        run(f"{sys.executable} -m pip install datasets")
        from datasets import load_dataset

    try:
        if snapshot_dir.exists():
            ds = load_dataset(str(snapshot_dir), trust_remote_code=True)
        else:
            ds = load_dataset(hf_name, trust_remote_code=True)
    except Exception as e:
        log(f"Kunde inte ladda benchmark {bench_name}: {e}", "ERR")
        return []

    # Hämta split
    split_name = "test" if "test" in ds else ("train" if "train" in ds else list(ds.keys())[0])
    split = ds[split_name]
    columns = split.column_names
    log(f"  Benchmark split: {split_name}, {len(split)} samples", "INFO")
    log(f"  Kolumner: {columns}", "INFO")

    # Detektera kolumner
    img_col = None
    for c in ("image", "screenshot", "img"):
        if c in columns:
            img_col = c
            break

    samples = []
    for idx, row in enumerate(split):
        img = None
        if img_col and row.get(img_col) is not None:
            val = row[img_col]
            if isinstance(val, PILImage.Image):
                img = val
            elif isinstance(val, dict) and "bytes" in val:
                img = PILImage.open(io.BytesIO(val["bytes"]))
            elif isinstance(val, bytes):
                img = PILImage.open(io.BytesIO(val))
        if img is None:
            continue

        # Extrahera bbox och metadata
        instruction = row.get("instruction", row.get("text", ""))
        platform = row.get("data_source", row.get("platform", "unknown"))
        label_type = row.get("label", row.get("type", "unknown"))

        # Hitta bbox — varierande kolumnnamn
        bbox = None
        for c in ("bbox", "bounding_box", "box", "target"):
            if c in columns and row.get(c) is not None:
                bbox = row[c]
                break

        # Nested detections (FiftyOne-stil)
        if bbox is None:
            for c in ("action_detection", "detections", "detection"):
                if c in columns and row.get(c) is not None:
                    det = row[c]
                    if isinstance(det, dict):
                        bbox = det.get("bounding_box", det.get("bbox"))
                        if not label_type or label_type == "unknown":
                            label_type = det.get("label", "unknown")
                    elif isinstance(det, list) and det:
                        d0 = det[0]
                        if isinstance(d0, dict):
                            bbox = d0.get("bounding_box", d0.get("bbox"))
                            if not label_type or label_type == "unknown":
                                label_type = d0.get("label", "unknown")
                    break

        if bbox is None or len(bbox) < 4:
            continue

        # Normalisera bbox till [x_center, y_center, w, h] (0-1)
        bx, by, bw, bh = float(bbox[0]), float(bbox[1]), float(bbox[2]), float(bbox[3])
        img_w, img_h = img.size

        # Kolla om redan normaliserat (alla < 2.0) eller pixlar
        if bw > 2.0 or bh > 2.0:
            # Troligen pixlar — normalisera
            # Kolla om [x1,y1,x2,y2] eller [x,y,w,h]
            if bw > bx and bh > by:
                # [x1,y1,x2,y2]
                bw_real = bw - bx
                bh_real = bh - by
                bx_center = (bx + bw / 2) / img_w
                by_center = (by + bh / 2) / img_h
                bw_norm = bw_real / img_w
                bh_norm = bh_real / img_h
            else:
                # [x,y,w,h] pixlar
                bx_center = (bx + bw / 2) / img_w
                by_center = (by + bh / 2) / img_h
                bw_norm = bw / img_w
                bh_norm = bh / img_h
        else:
            # Redan normaliserat [x,y,w,h] (FiftyOne/ScreenSpot-stil: top-left + w,h)
            bx_center = bx + bw / 2
            by_center = by + bh / 2
            bw_norm = bw
            bh_norm = bh

        samples.append({
            "image": img,
            "bbox": [bx_center, by_center, bw_norm, bh_norm],
            "instruction": instruction,
            "platform": str(platform),
            "label_type": str(label_type),
            "img_w": img_w,
            "img_h": img_h,
        })

    log(f"  Laddade {len(samples)} benchmark-samples", "OK")
    return samples


def _load_fiftyone_benchmark(snapshot_dir: Path, samples_json: Path,
                             bench_name: str) -> list[dict]:
    """Ladda benchmark från FiftyOne samples.json."""
    import json
    from PIL import Image as PILImage

    with open(samples_json, "r") as f:
        data = json.load(f)

    raw_samples = data.get("samples", [])
    log(f"  FiftyOne: {len(raw_samples)} samples", "INFO")

    samples = []
    for s in raw_samples:
        filepath = s.get("filepath", "")
        img_path = snapshot_dir / filepath
        if not img_path.exists():
            # Prova utan leading /
            img_path = snapshot_dir / filepath.lstrip("/")
        if not img_path.exists():
            continue

        try:
            img = PILImage.open(img_path)
            img_w, img_h = img.size
        except Exception:
            continue

        # Extrahera detection
        det_field = None
        for field in ("action_detection", "detections", "ground_truth"):
            if field in s:
                det_field = s[field]
                break

        if det_field is None:
            continue

        # FiftyOne kan ha nested detections
        if isinstance(det_field, dict):
            dets = det_field.get("detections", [det_field])
        elif isinstance(det_field, list):
            dets = det_field
        else:
            continue

        for det in dets:
            bbox = det.get("bounding_box")
            if not bbox or len(bbox) < 4:
                continue

            bx, by, bw, bh = float(bbox[0]), float(bbox[1]), float(bbox[2]), float(bbox[3])
            # FiftyOne: [x, y, w, h] normaliserat (top-left)
            bx_center = bx + bw / 2
            by_center = by + bh / 2

            samples.append({
                "image": img,
                "bbox": [bx_center, by_center, bw, bh],
                "instruction": s.get("instruction", ""),
                "platform": s.get("data_source", s.get("platform", "unknown")),
                "label_type": det.get("label", "unknown"),
                "img_w": img_w,
                "img_h": img_h,
            })

    log(f"  Laddade {len(samples)} FiftyOne benchmark-samples", "OK")
    return samples


def run_benchmark(model_path: Path, bench_names: list[str], output_dir: Path,
                  imgsz: int = DEFAULT_IMGSZ, conf: float = 0.25) -> dict:
    """Kör modellen mot ett eller flera benchmarks och returnera resultat.

    Args:
        model_path: Sökväg till .pt eller .onnx modell
        bench_names: Lista av benchmark-namn (eller ["all"])
        output_dir: Sökväg för nedladdade benchmarks
        imgsz: Input-storlek för modellen
        conf: Confidence-tröskel

    Returns:
        Dict med resultat per benchmark
    """
    from ultralytics import YOLO

    if "all" in bench_names:
        bench_names = list(_BENCHMARK_REGISTRY.keys())

    log(f"Laddar modell: {model_path}", "STEP")
    model = YOLO(str(model_path))

    all_results = {}

    for bench_name in bench_names:
        if bench_name not in _BENCHMARK_REGISTRY:
            log(f"Okänt benchmark: {bench_name}, hoppar över", "WARN")
            continue

        info = _BENCHMARK_REGISTRY[bench_name]
        log(f"\n{'═' * 60}", "INFO")
        log(f"Benchmark: {info['name']}", "STEP")
        log(f"{'═' * 60}", "INFO")

        # Ladda ner om det behövs
        bench_dir = download_benchmark(bench_name, output_dir)

        # Ladda samples
        samples = _load_benchmark_samples(bench_dir, bench_name)
        if not samples:
            log(f"Inga samples laddade för {bench_name}", "WARN")
            all_results[bench_name] = {"error": "no_samples"}
            continue

        # Kör inference på varje sample
        results = _evaluate_benchmark(model, samples, info, imgsz, conf)
        all_results[bench_name] = results

        # Visa snabbresultat
        _print_benchmark_summary(bench_name, info, results)

    return all_results


def _evaluate_benchmark(model, samples: list[dict], info: dict,
                        imgsz: int, conf: float) -> dict:
    """Kör YOLO-modellen mot benchmark-samples och beräkna metriker.

    Beräknar:
    - Click accuracy (punkt-i-box): modellens center-prediction i GT bbox
    - IoU (Intersection over Union): genomsnittlig bbox-overlap
    - mAP-liknande: andel detections med IoU > tröskel
    - Per-plattform och per-label breakdown
    """
    total = len(samples)
    click_correct = 0
    iou_scores = []
    iou_at_50 = 0  # IoU > 0.5
    iou_at_25 = 0  # IoU > 0.25

    # Per-plattform och per-label tracking
    platform_stats = {}
    label_stats = {}

    for idx, sample in enumerate(samples):
        if idx % 200 == 0 and idx > 0:
            pct = idx * 100 // total
            log(f"  [{pct:3d}%] {idx}/{total} samples evaluerade...", "INFO")

        img = sample["image"]
        gt_bbox = sample["bbox"]  # [x_center, y_center, w, h] normaliserat
        platform = sample.get("platform", "unknown")
        label_type = sample.get("label_type", "unknown")

        # Kör YOLO inference
        try:
            results = model.predict(img, imgsz=imgsz, conf=conf, verbose=False)
        except Exception:
            continue

        if not results or len(results) == 0:
            # Ingen detection — miss
            iou_scores.append(0.0)
            _update_stats(platform_stats, platform, False, 0.0)
            _update_stats(label_stats, label_type, False, 0.0)
            continue

        # Hitta bästa matchande detection (högst IoU med GT)
        best_iou = 0.0
        best_det_center = None
        img_w, img_h = sample["img_w"], sample["img_h"]

        for result in results:
            boxes = result.boxes
            if boxes is None or len(boxes) == 0:
                continue

            for i in range(len(boxes)):
                # YOLO ger [x1,y1,x2,y2] i pixlar
                xyxy = boxes.xyxy[i].cpu().numpy()
                x1, y1, x2, y2 = xyxy

                # Normalisera till [x_center, y_center, w, h]
                det_xc = ((x1 + x2) / 2) / img_w
                det_yc = ((y1 + y2) / 2) / img_h
                det_w = (x2 - x1) / img_w
                det_h = (y2 - y1) / img_h

                iou = _compute_iou(
                    gt_bbox,
                    [det_xc, det_yc, det_w, det_h]
                )
                if iou > best_iou:
                    best_iou = iou
                    best_det_center = (det_xc, det_yc)

        # Click accuracy: modellens center inom GT bbox?
        click_hit = False
        if best_det_center is not None:
            gt_xc, gt_yc, gt_w, gt_h = gt_bbox
            gt_x1 = gt_xc - gt_w / 2
            gt_y1 = gt_yc - gt_h / 2
            gt_x2 = gt_xc + gt_w / 2
            gt_y2 = gt_yc + gt_h / 2
            px, py = best_det_center
            if gt_x1 <= px <= gt_x2 and gt_y1 <= py <= gt_y2:
                click_hit = True
                click_correct += 1

        iou_scores.append(best_iou)
        if best_iou >= 0.50:
            iou_at_50 += 1
        if best_iou >= 0.25:
            iou_at_25 += 1

        _update_stats(platform_stats, platform, click_hit, best_iou)
        _update_stats(label_stats, label_type, click_hit, best_iou)

    # Beräkna aggregerade metriker
    avg_iou = sum(iou_scores) / max(len(iou_scores), 1)

    return {
        "total_samples": total,
        "evaluated": len(iou_scores),
        "click_accuracy": click_correct / max(len(iou_scores), 1),
        "click_correct": click_correct,
        "avg_iou": avg_iou,
        "iou_at_50": iou_at_50 / max(len(iou_scores), 1),
        "iou_at_25": iou_at_25 / max(len(iou_scores), 1),
        "platform_breakdown": _finalize_stats(platform_stats),
        "label_breakdown": _finalize_stats(label_stats),
    }


def _compute_iou(box_a: list, box_b: list) -> float:
    """Beräkna IoU mellan två [x_center, y_center, w, h] bboxar (normaliserade)."""
    ax, ay, aw, ah = box_a
    bx, by, bw, bh = box_b

    a_x1, a_y1 = ax - aw / 2, ay - ah / 2
    a_x2, a_y2 = ax + aw / 2, ay + ah / 2
    b_x1, b_y1 = bx - bw / 2, by - bh / 2
    b_x2, b_y2 = bx + bw / 2, by + bh / 2

    inter_x1 = max(a_x1, b_x1)
    inter_y1 = max(a_y1, b_y1)
    inter_x2 = min(a_x2, b_x2)
    inter_y2 = min(a_y2, b_y2)

    inter_area = max(0, inter_x2 - inter_x1) * max(0, inter_y2 - inter_y1)
    a_area = aw * ah
    b_area = bw * bh
    union_area = a_area + b_area - inter_area

    if union_area <= 0:
        return 0.0
    return inter_area / union_area


def _update_stats(stats: dict, key: str, click_hit: bool, iou: float):
    """Uppdatera per-kategori statistik."""
    if key not in stats:
        stats[key] = {"total": 0, "click_correct": 0, "iou_sum": 0.0}
    stats[key]["total"] += 1
    if click_hit:
        stats[key]["click_correct"] += 1
    stats[key]["iou_sum"] += iou


def _finalize_stats(stats: dict) -> dict:
    """Beräkna slutliga metriker per kategori."""
    result = {}
    for key, s in stats.items():
        total = s["total"]
        result[key] = {
            "total": total,
            "click_accuracy": s["click_correct"] / max(total, 1),
            "avg_iou": s["iou_sum"] / max(total, 1),
        }
    return result


def _print_benchmark_summary(bench_name: str, info: dict, results: dict):
    """Visa benchmark-resultat i terminalen."""
    if "error" in results:
        log(f"  {info['name']}: FEL — {results['error']}", "ERR")
        return

    print()
    print(f"  ┌{'─' * 56}┐")
    print(f"  │  {info['name']:^52}  │")
    print(f"  ├{'─' * 56}┤")
    print(f"  │  {'Metric':<28} {'Värde':>10} {'Samples':>12}  │")
    print(f"  ├{'─' * 56}┤")

    ca = results["click_accuracy"]
    ai = results["avg_iou"]
    i50 = results["iou_at_50"]
    i25 = results["iou_at_25"]
    n = results["evaluated"]

    print(f"  │  {'Click Accuracy':<28} {ca:>9.1%} {n:>12}  │")
    print(f"  │  {'Avg IoU':<28} {ai:>10.4f} {n:>12}  │")
    print(f"  │  {'IoU ≥ 0.50':<28} {i50:>9.1%} {n:>12}  │")
    print(f"  │  {'IoU ≥ 0.25':<28} {i25:>9.1%} {n:>12}  │")
    print(f"  └{'─' * 56}┘")

    # Per-plattform breakdown
    platform_data = results.get("platform_breakdown", {})
    if len(platform_data) > 1:
        print()
        print(f"  Per plattform:")
        print(f"  {'Platform':<16} {'Click Acc':>10} {'Avg IoU':>10} {'N':>8}")
        print(f"  {'─' * 46}")
        for plat, ps in sorted(platform_data.items()):
            print(f"  {plat:<16} {ps['click_accuracy']:>9.1%} {ps['avg_iou']:>10.4f} {ps['total']:>8}")

    # Per-label breakdown
    label_data = results.get("label_breakdown", {})
    if len(label_data) > 1:
        print()
        print(f"  Per label-typ:")
        print(f"  {'Label':<16} {'Click Acc':>10} {'Avg IoU':>10} {'N':>8}")
        print(f"  {'─' * 46}")
        for lbl, ls in sorted(label_data.items()):
            print(f"  {lbl:<16} {ls['click_accuracy']:>9.1%} {ls['avg_iou']:>10.4f} {ls['total']:>8}")

    print()


def generate_benchmark_report(model_path: Path, all_results: dict,
                              version: str, output_dir: Path) -> Path:
    """Generera fullständig benchmark-rapport (JSON + Markdown + terminal).

    Sparar:
      - benchmark-report-{version}.json — maskinläsbar
      - benchmark-report-{version}.md  — visuell rapport

    Returns:
        Sökväg till JSON-rapporten
    """
    import json

    report = {
        "version": version,
        "model": str(model_path),
        "timestamp": time.strftime("%Y-%m-%d %H:%M:%S"),
        "benchmarks": {},
    }

    # Samla resultat
    for bench_name, results in all_results.items():
        if "error" in results:
            report["benchmarks"][bench_name] = {"error": results["error"]}
            continue

        info = _BENCHMARK_REGISTRY.get(bench_name, {})
        report["benchmarks"][bench_name] = {
            "name": info.get("name", bench_name),
            "click_accuracy": results["click_accuracy"],
            "avg_iou": results["avg_iou"],
            "iou_at_50": results["iou_at_50"],
            "iou_at_25": results["iou_at_25"],
            "total_samples": results["total_samples"],
            "evaluated": results["evaluated"],
            "platform_breakdown": results.get("platform_breakdown", {}),
            "label_breakdown": results.get("label_breakdown", {}),
        }

    # Sammanfattad score (genomsnitt av click accuracy över alla benchmarks)
    valid_scores = [
        r["click_accuracy"] for r in report["benchmarks"].values()
        if isinstance(r, dict) and "click_accuracy" in r
    ]
    report["overall_click_accuracy"] = (
        sum(valid_scores) / len(valid_scores) if valid_scores else 0
    )

    # Spara JSON
    output_dir.mkdir(parents=True, exist_ok=True)
    json_path = output_dir / f"benchmark-report-{version}.json"
    with open(json_path, "w") as f:
        json.dump(report, f, indent=2, ensure_ascii=False)

    # Generera Markdown
    md_path = output_dir / f"benchmark-report-{version}.md"
    md_lines = [
        f"# Benchmark Report — {version}",
        "",
        f"**Modell:** `{model_path}`",
        f"**Datum:** {report['timestamp']}",
        f"**Overall Click Accuracy:** {report['overall_click_accuracy']:.1%}",
        "",
        "---",
        "",
        "## Sammanfattning",
        "",
        "| Benchmark | Click Accuracy | Avg IoU | IoU≥0.50 | IoU≥0.25 | Samples |",
        "|-----------|---------------|---------|----------|----------|---------|",
    ]

    for bench_name, r in report["benchmarks"].items():
        if "error" in r:
            md_lines.append(f"| {bench_name} | ERROR | — | — | — | — |")
            continue
        md_lines.append(
            f"| {r['name']} "
            f"| {r['click_accuracy']:.1%} "
            f"| {r['avg_iou']:.4f} "
            f"| {r['iou_at_50']:.1%} "
            f"| {r['iou_at_25']:.1%} "
            f"| {r['evaluated']} |"
        )

    # Per-benchmark detaljer
    for bench_name, r in report["benchmarks"].items():
        if "error" in r:
            continue

        md_lines.extend([
            "",
            "---",
            "",
            f"## {r['name']}",
            "",
        ])

        # Plattform
        pf = r.get("platform_breakdown", {})
        if pf:
            md_lines.extend([
                "### Per plattform",
                "",
                "| Platform | Click Accuracy | Avg IoU | N |",
                "|----------|---------------|---------|---|",
            ])
            for plat, ps in sorted(pf.items()):
                md_lines.append(
                    f"| {plat} | {ps['click_accuracy']:.1%} "
                    f"| {ps['avg_iou']:.4f} | {ps['total']} |"
                )
            md_lines.append("")

        # Label-typ
        lf = r.get("label_breakdown", {})
        if lf:
            md_lines.extend([
                "### Per element-typ",
                "",
                "| Type | Click Accuracy | Avg IoU | N |",
                "|------|---------------|---------|---|",
            ])
            for lbl, ls in sorted(lf.items()):
                md_lines.append(
                    f"| {lbl} | {ls['click_accuracy']:.1%} "
                    f"| {ls['avg_iou']:.4f} | {ls['total']} |"
                )
            md_lines.append("")

    md_lines.extend([
        "",
        "---",
        "",
        f"*Genererad av AetherAgent Vision Training Pipeline — {report['timestamp']}*",
    ])

    md_path.write_text("\n".join(md_lines))

    # Terminal-sammanfattning
    print("\n" + "═" * 70)
    log("BENCHMARK REPORT", "OK")
    print("═" * 70)
    print(f"  Model:     {model_path}")
    print(f"  Version:   {version}")
    print(f"  Overall:   {report['overall_click_accuracy']:.1%} click accuracy")
    print()
    print(f"  {'Benchmark':<24} {'Click Acc':>10} {'Avg IoU':>10} {'Samples':>10}")
    print(f"  {'─' * 56}")
    for bench_name, r in report["benchmarks"].items():
        if "error" in r:
            print(f"  {bench_name:<24} {'ERROR':>10} {'—':>10} {'—':>10}")
        else:
            print(
                f"  {r['name']:<24} "
                f"{r['click_accuracy']:>9.1%} "
                f"{r['avg_iou']:>10.4f} "
                f"{r['evaluated']:>10}"
            )
    print("═" * 70)
    print(f"  JSON:     {json_path}")
    print(f"  Markdown: {md_path}")
    print()

    return json_path


def generate_report(
    dataset_dir: Path,
    best_pt: Path,
    onnx_path: Path,
    metrics: dict,
    deploy_path: Path,
    version: str,
    baseline_metrics: dict = None,
    epoch_history: list[dict] = None,
):
    """Generate a summary report with baseline comparison and progression."""
    report = {
        "version": version,
        "dataset": str(dataset_dir),
        "model_pt": str(best_pt),
        "model_onnx": str(onnx_path),
        "deployed_to": str(deploy_path),
        "onnx_size_mb": round(onnx_path.stat().st_size / (1024 * 1024), 2),
        "metrics": metrics,
        "baseline_metrics": baseline_metrics,
        "classes": UI_CLASSES,
        "input_size": DEFAULT_IMGSZ,
        "timestamp": time.strftime("%Y-%m-%d %H:%M:%S"),
    }
    if epoch_history:
        report["epoch_history"] = epoch_history

    report_path = deploy_path.parent / f"report-{version}.json"
    with open(report_path, "w") as f:
        json.dump(report, f, indent=2)

    print("\n" + "=" * 70)
    log("TRAINING COMPLETE", "OK")
    print("=" * 70)
    print(f"  Model version:  {version}")
    print(f"  ONNX path:      {onnx_path}")
    print(f"  ONNX size:      {report['onnx_size_mb']} MB")

    # Baseline vs Final comparison
    metric_labels = [
        ("mAP@50", "map50"),
        ("mAP@50-95", "map5095"),
        ("Precision", "precision"),
        ("Recall", "recall"),
    ]

    if baseline_metrics:
        print()
        print(f"  {'Metric':<14} {'Baseline':>10} {'Final':>10} {'Change':>12}")
        print(f"  {'─' * 48}")
        for label, key in metric_labels:
            bval = baseline_metrics.get(key, 0)
            fval = metrics.get(key, 0)
            change = _pct_change(bval, fval)
            print(f"  {label:<14} {bval:>10.4f} {fval:>10.4f} {change:>12}")
    else:
        print()
        for label, key in metric_labels:
            print(f"  {label:<14} {metrics.get(key, 0):.4f}")

    # Epoch progression summary (first, 25%, 50%, 75%, last)
    if epoch_history and len(epoch_history) > 1:
        total = len(epoch_history)
        checkpoints = [0, total // 4, total // 2, 3 * total // 4, total - 1]
        # Dedup while preserving order
        seen = set()
        checkpoints = [c for c in checkpoints if c not in seen and not seen.add(c)]
        print()
        print(f"  Epoch Progression:")
        print(f"  {'Epoch':>7} {'mAP@50':>10} {'mAP50-95':>10} {'box_loss':>10} {'cls_loss':>10}")
        print(f"  {'─' * 49}")
        for idx in checkpoints:
            ep = epoch_history[idx]
            epoch_num = int(ep.get("epoch", idx + 1))
            m50 = ep.get("map50", 0)
            m5095 = ep.get("map5095", 0)
            bloss = ep.get("box_loss", 0)
            closs = ep.get("cls_loss", 0)
            print(f"  {epoch_num:>7} {m50:>10.4f} {m5095:>10.4f} {bloss:>10.4f} {closs:>10.4f}")

        # Convergence: improvement last 25% vs first 25%
        q1_map = sum(e.get("map50", 0) for e in epoch_history[:total // 4]) / max(total // 4, 1)
        q4_map = sum(e.get("map50", 0) for e in epoch_history[3 * total // 4:]) / max(total - 3 * total // 4, 1)
        if q1_map > 0:
            convergence = ((q4_map - q1_map) / q1_map) * 100
            print(f"\n  Convergence: Q4 vs Q1 mAP@50 {convergence:+.1f}%")
            if abs(convergence) < 2.0:
                print("  → Modellen har konvergerat — fler epochs ger troligen lite")
            elif convergence < 0:
                print("  → Negativ trend — möjlig overfitting, prova fewer epochs")

    print()
    print(f"  Report:         {report_path}")
    print("=" * 70)
    print()
    print("  To use in AetherAgent:")
    print(f'    model_bytes = open("{deploy_path}", "rb").read()')
    print(f'    config = {{"model_version": "{version}"}}')
    print()
    print("  Or via HTTP API:")
    print(f"    curl -X POST http://localhost:3000/api/parse-screenshot \\")
    print(f'      -d \'{{"png_base64": "...", "model_base64": "...", "goal": "find buttons"}}\'')
    print()


# ---------------------------------------------------------------------------
# Interactive Mode
# ---------------------------------------------------------------------------

def interactive_mode(args=None):
    """Step-by-step interactive wizard.

    Om args skickas med respekteras --download, --format, --version etc.
    som förvalda värden i wizarden.
    """
    print(BANNER)
    print("This wizard will guide you through the full training pipeline.\n")

    # Förval från CLI-argument
    cli_format = getattr(args, "format", None) if args else None
    cli_version = getattr(args, "version", "v1") if args else "v1"
    cli_download = getattr(args, "download", False) if args else False
    cli_device = getattr(args, "device", None) if args else None
    cli_server = getattr(args, "server", "http://localhost:3000") if args else "http://localhost:3000"
    cli_fresh = getattr(args, "fresh", False) if args else False
    cli_extended = getattr(args, "extended_classes", False) if args else False

    # Step 1: Modellval
    print("[1/7] BASMODELL")
    model_base = prompt_model_selection()

    # Step 2: Dataset
    print("\n[2/7] DATASET")
    # Visa externa dataset-alternativ
    ext_datasets = list(_DATASET_REGISTRY.keys())
    print("  a) Lokalt dataset (ange sökväg)")
    print("  b) Syntetiskt starter-dataset (för testning)")
    for idx, ds_name in enumerate(ext_datasets):
        info = _DATASET_REGISTRY[ds_name]
        print(f"  {chr(99 + idx)}) Ladda ner: {info['name']} ({info['size_hint']})")
    print(f"  m) Slå ihop flera dataset (incremental merge)")

    # Automatiskt välja om --download --format angavs
    if cli_download and cli_format and cli_format in ext_datasets:
        ds_idx = ext_datasets.index(cli_format)
        default_choice = chr(99 + ds_idx)
        log(f"  (förval från --download --format {cli_format})", "INFO")
    else:
        default_choice = None

    prompt_str = f"  Val [{default_choice}]: " if default_choice else "  Val [a/b/m/...]: "
    choice = input(prompt_str).strip().lower() or (default_choice or "")

    dataset_dir = None
    if choice == "a":
        path = input("  Dataset-sökväg: ").strip()
        dataset_dir = Path(path)
        if not dataset_dir.exists():
            log(f"Sökvägen finns inte: {dataset_dir}", "ERR")
            sys.exit(1)
    elif choice == "b":
        dataset_dir = Path("dataset")
        download_starter_dataset(dataset_dir)
    elif choice == "m":
        # Merge-läge: samla sökvägar
        print("\n  Ange sökvägar till dataset att slå ihop (en per rad, tom rad avslutar):")
        merge_paths = []
        while True:
            p = input(f"    Dataset {len(merge_paths) + 1} (enter = klar): ").strip()
            if not p:
                break
            pp = Path(p)
            if not pp.exists():
                log(f"  Sökvägen finns inte: {pp}", "WARN")
                continue
            merge_paths.append(pp)
        if len(merge_paths) < 2:
            log("Minst 2 dataset krävs för merge", "ERR")
            sys.exit(1)
        merged_dir = Path("dataset") / "merged"
        dataset_dir = merge_datasets(merge_paths, merged_dir)
    else:
        # Matcha mot dataset-lista (c, d, e, ...)
        ds_offset = ord(choice) - ord('c') if len(choice) == 1 and choice >= 'c' else -1
        if 0 <= ds_offset < len(ext_datasets):
            fmt_name = ext_datasets[ds_offset]
            info = _DATASET_REGISTRY[fmt_name]
            log(f"Laddar ner {info['name']}...", "STEP")
            ensure_deps()
            base_dir = Path("dataset")
            raw_dir = download_dataset(fmt_name, base_dir)
            converted_dir = base_dir / f"{fmt_name}_converted"
            dataset_dir = convert_dataset(raw_dir, converted_dir, fmt_name,
                                          extended=cli_extended)
        else:
            log(f"Ogiltigt val: {choice}", "ERR")
            sys.exit(1)

    # Step 3: Config
    print("\n[3/7] TRAINING CONFIG")
    epochs = input(f"  Epochs (max) [{DEFAULT_EPOCHS}]: ").strip()
    epochs = int(epochs) if epochs else DEFAULT_EPOCHS
    batch = input(f"  Batch size [{DEFAULT_BATCH}]: ").strip()
    batch = int(batch) if batch else DEFAULT_BATCH
    version = input(f"  Model version [{cli_version}]: ").strip() or cli_version

    # Early stopping
    print("\n  Early stopping (stoppar automatiskt vid mål eller platå):")
    es_choice = input(f"  Aktivera early-stop? [Y/n]: ").strip().lower()
    early_stop = es_choice != "n"
    target_map50 = DEFAULT_TARGET_MAP50
    target_map5095 = DEFAULT_TARGET_MAP5095
    patience = DEFAULT_PATIENCE
    if early_stop:
        t50 = input(f"  mAP@50 mål [{DEFAULT_TARGET_MAP50}]: ").strip()
        target_map50 = float(t50) if t50 else DEFAULT_TARGET_MAP50
        t5095 = input(f"  mAP@50-95 mål [{DEFAULT_TARGET_MAP5095}]: ").strip()
        target_map5095 = float(t5095) if t5095 else DEFAULT_TARGET_MAP5095
        pat = input(f"  Patience (epochs utan förbättring) [{DEFAULT_PATIENCE}]: ").strip()
        patience = int(pat) if pat else DEFAULT_PATIENCE

    # Step 4: Export format
    print("\n[4/7] EXPORTFORMAT")
    export_fmt = prompt_export_format()

    # Step 5: Confirm
    print(f"\n  Modell:      {model_base}")
    print(f"  Dataset:     {dataset_dir}")
    print(f"  Epochs (max): {epochs}")
    print(f"  Batch:       {batch}")
    print(f"  Export:      {export_fmt['label']}")
    print(f"  Version:     {version}")
    if early_stop:
        print(f"  Early-stop:  ON (mAP@50≥{target_map50}, mAP@50-95≥{target_map5095}, patience={patience})")
    else:
        print(f"  Early-stop:  OFF (kör alla {epochs} epochs)")
    confirm = input("\n  Start training? [Y/n]: ").strip().lower()
    if confirm == "n":
        print("Cancelled.")
        sys.exit(0)

    # Run pipeline
    run_pipeline(
        dataset_dir=dataset_dir,
        epochs=epochs,
        batch=batch,
        model_base=model_base,
        version=version,
        server_url=cli_server,
        export_fmt=export_fmt,
        device=cli_device,
        fresh=cli_fresh,
        early_stop=early_stop,
        target_map50=target_map50,
        target_map5095=target_map5095,
        patience=patience,
    )


# ---------------------------------------------------------------------------
# Main Pipeline
# ---------------------------------------------------------------------------

def run_pipeline(
    dataset_dir: Path,
    epochs: int = DEFAULT_EPOCHS,
    batch: int = DEFAULT_BATCH,
    imgsz: int = DEFAULT_IMGSZ,
    model_base: str = DEFAULT_MODEL_BASE,
    version: str = "v1",
    server_url: str = None,
    deploy_dir: Path = None,
    skip_verify: bool = False,
    device: str = None,
    fresh: bool = False,
    export_fmt: dict = None,
    early_stop: bool = False,
    target_map50: float = DEFAULT_TARGET_MAP50,
    target_map5095: float = DEFAULT_TARGET_MAP5095,
    patience: int = DEFAULT_PATIENCE,
):
    """Run the full training pipeline."""
    print(BANNER)

    if deploy_dir is None:
        deploy_dir = Path("models")

    # Pre-flight
    log("Pre-flight checks...", "STEP")
    ensure_deps()

    # GPU-check: detekterar, installerar rätt PyTorch om nödvändigt
    detected_device = check_gpu()
    if device is None:
        device = detected_device

    if device == "cpu":
        log("CPU-träning: sänker batch till 8", "WARN")
        batch = min(batch, 8)
    else:
        log(f"Tränar på {device}", "OK")

    # Step 1: Dataset
    log("Step 1/6: Preparing dataset...", "STEP")
    data_yaml = create_data_yaml(dataset_dir, dataset_dir / "data.yaml")

    # Auto-chain: om ingen explicit --model-base angavs, leta efter senaste best.pt
    if not fresh and model_base == DEFAULT_MODEL_BASE:
        latest_pt = _find_latest_model()
        if latest_pt:
            log(f"Auto-chain: bygger vidare på {latest_pt}", "OK")
            model_base = str(latest_pt)
        else:
            log(f"Ingen tidigare modell hittades — startar från {model_base}", "INFO")

    # Step 2a: Baseline — mät basmodellens metrics före träning
    log("Step 2/7: Measuring baseline metrics...", "STEP")
    baseline_metrics = validate_model(Path(model_base), data_yaml, imgsz)

    # Step 2b: Train
    if early_stop:
        log(f"Step 3/7: Training {model_base} (early-stop: mAP@50≥{target_map50}, mAP@50-95≥{target_map5095}, patience={patience})...", "STEP")
    else:
        log(f"Step 3/7: Training {model_base}...", "STEP")
    best_pt = train_model(
        data_yaml=data_yaml,
        epochs=epochs,
        batch=batch,
        imgsz=imgsz,
        model_base=model_base,
        project=DEFAULT_PROJECT,
        name=f"{DEFAULT_NAME}-{version}",
        device=device,
        early_stop=early_stop,
        target_map50=target_map50,
        target_map5095=target_map5095,
        patience=patience,
    )

    # Step 4: Validate
    log("Step 4/7: Validating model...", "STEP")
    metrics = validate_model(best_pt, data_yaml, imgsz)

    # Läs epoch-progression från YOLO:s results.csv
    results_csv = Path(DEFAULT_PROJECT) / f"{DEFAULT_NAME}-{version}" / "results.csv"
    epoch_history = _parse_results_csv(results_csv)

    # Step 5: Export ONNX
    if export_fmt is None:
        export_fmt = prompt_export_format()
    log("Step 5/7: Exporting to ONNX...", "STEP")
    onnx_path = export_onnx(best_pt, imgsz, export_fmt=export_fmt)

    # Step 6: Deploy
    log("Step 6/7: Deploying model...", "STEP")
    deploy_path = copy_to_deploy(onnx_path, deploy_dir, version)

    # Try rten conversion (optional)
    convert_rten(onnx_path)

    # Step 7: Verify
    if not skip_verify and server_url:
        log("Step 7/7: Verifying with AetherAgent API...", "STEP")
        verify_with_server(onnx_path, server_url)
    else:
        log("Step 7/7: Skipping API verification", "INFO")

    # Report
    generate_report(
        dataset_dir, best_pt, onnx_path, metrics, deploy_path, version,
        baseline_metrics=baseline_metrics, epoch_history=epoch_history,
    )


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

def main():
    parser = argparse.ArgumentParser(
        description="AetherAgent Vision Training Pipeline — automated end-to-end",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  # Download Rico dataset, convert to YOLO, and train:
  python tools/train_vision.py --download --format rico --version v2

  # Download WebUI dataset, convert only (no training):
  python tools/train_vision.py --download-only --format webui

  # 2025-2026 HuggingFace datasets:
  python tools/train_vision.py --download --format osatlas --version v5     # OS-Atlas (3M web-element)
  python tools/train_vision.py --download --format guiactor --version v6    # GUI-Actor (10M element)
  python tools/train_vision.py --download --format showui-web --version v7  # ShowUI Web (576K element)
  python tools/train_vision.py --download --format waveui --version v8      # WaveUI-25K (curated)
  python tools/train_vision.py --download --format yashjain --version v9    # YOLO-format direkt
  python tools/train_vision.py --download --format klarna --version v10     # Klarna e-handel (51.7K)
  python tools/train_vision.py --download --format webclick --version v11   # WebClick intent-annoterat
  python tools/train_vision.py --download --format roboflow-ui --version v12 # Roboflow UI (1800, YOLO)

  # Full pipeline with your own local dataset:
  python tools/train_vision.py --dataset ./my-labeled-data

  # Interactive model selection + train:
  python tools/train_vision.py --download --format rico --select-model

  # Generate synthetic starter dataset + train:
  python tools/train_vision.py --download-starter

  # Export existing model:
  python tools/train_vision.py --export-only runs/detect/aether-ui-v1/weights/best.pt

  # Merge multiple datasets (incremental) and train:
  python tools/train_vision.py --merge-datasets dataset/rico_converted dataset/yashjain_raw dataset/webui_converted --version v3

  # Interactive wizard:
  python tools/train_vision.py --interactive

  # Benchmark against standard datasets:
  python tools/train_vision.py --benchmark all --model-pt runs/detect/best.pt --version v1
  python tools/train_vision.py --benchmark screenspot-v2 screenspot-pro --version v2
  python tools/train_vision.py --benchmark groundui-18k --benchmark-conf 0.15 --version v3
        """,
    )

    parser.add_argument("--dataset", type=Path, help="Path to labeled dataset directory")
    parser.add_argument("--format", type=str, default="yolo",
                        choices=["yolo", "rico", "coco", "webui",
                                 "osatlas", "guiactor", "showui-web", "waveui", "yashjain",
                                 "klarna", "webclick", "roboflow-ui"],
                        help="Dataset format (default: yolo). Converts to YOLO automatically. "
                             "2026 datasets: osatlas, guiactor, showui-web, waveui, yashjain, "
                             "klarna, webclick, roboflow-ui.")
    parser.add_argument("--extended-classes", action="store_true",
                        help="Use 16 agent-semantic classes (price, cta, product_card, nav, search, form) "
                             "instead of standard 10. Enables text heuristics for class upgrades.")
    parser.add_argument("--download", action="store_true",
                        help="Download dataset for the specified --format (rico, coco, webui), "
                             "convert to YOLO, and train. Combines download + convert + train in one step.")
    parser.add_argument("--download-only", action="store_true",
                        help="Download and convert dataset without training. Use with --format.")
    parser.add_argument("--download-starter", action="store_true", help="Download synthetic starter dataset")
    parser.add_argument("--epochs", type=int, default=DEFAULT_EPOCHS, help=f"Max training epochs (default: {DEFAULT_EPOCHS})")
    parser.add_argument("--batch", type=int, default=DEFAULT_BATCH, help=f"Batch size (default: {DEFAULT_BATCH}, auto-tuned per VRAM)")
    parser.add_argument("--early-stop", action="store_true",
                        help="Enable metric-based early stopping. Saves when targets reached or no improvement.")
    parser.add_argument("--target-map50", type=float, default=DEFAULT_TARGET_MAP50,
                        help=f"Target mAP@50 for early stop (default: {DEFAULT_TARGET_MAP50}). Implies --early-stop.")
    parser.add_argument("--target-map5095", type=float, default=DEFAULT_TARGET_MAP5095,
                        help=f"Target mAP@50-95 for early stop (default: {DEFAULT_TARGET_MAP5095}). Implies --early-stop.")
    parser.add_argument("--patience", type=int, default=DEFAULT_PATIENCE,
                        help=f"Stop after N epochs without improvement (default: {DEFAULT_PATIENCE}). Implies --early-stop.")
    parser.add_argument("--imgsz", type=int, default=DEFAULT_IMGSZ, help=f"Image size (default: {DEFAULT_IMGSZ})")
    parser.add_argument("--version", type=str, default="v1", help="Model version tag (default: v1)")
    parser.add_argument("--model-base", type=str, default=DEFAULT_MODEL_BASE,
                        help=f"Base model (default: {DEFAULT_MODEL_BASE}). "
                             f"Available: yolo26n, yolo26s, yolo11n, yolo11s, yolov8n")
    parser.add_argument("--select-model", action="store_true",
                        help="Interactive model selection prompt before training")
    parser.add_argument("--deploy-dir", type=Path, default=Path("models"), help="Model deployment directory")
    parser.add_argument("--server", type=str, default="http://localhost:3000", help="AetherAgent server URL for verification")
    parser.add_argument("--device", type=str, default=None,
                        help="Training device: 'cuda', 'cpu', or device ID. "
                             "Auto-detects and installs correct PyTorch if needed.")
    parser.add_argument("--fresh", action="store_true",
                        help="Start training from scratch (yolo26n.pt) instead of auto-chaining from latest model")
    parser.add_argument("--skip-verify", action="store_true", help="Skip API verification step")
    parser.add_argument("--interactive", action="store_true", help="Interactive step-by-step wizard")
    parser.add_argument("--merge-datasets", type=Path, nargs="+", metavar="DIR",
                        help="Merge multiple datasets into one before training. "
                             "Example: --merge-datasets dataset/rico dataset/webui dataset/yashjain")
    parser.add_argument("--export-only", type=Path, help="Only export .pt → ONNX (skip training)")
    parser.add_argument("--export-format", type=str, default=None,
                        choices=["fp32", "fp16", "int8"],
                        help="ONNX export format: fp32 (full), fp16 (recommended), int8 (smallest). "
                             "If omitted, interactive prompt shown after training.")
    parser.add_argument("--verify-only", type=Path, help="Only verify ONNX model against API")
    parser.add_argument("--benchmark", type=str, nargs="+",
                        metavar="NAME",
                        help="Run model against benchmark(s). Names: "
                             "screenspot-v2, screenspot-pro, groundui-18k, ui-vision, all. "
                             "Requires --model-pt or uses latest best.pt.")
    parser.add_argument("--model-pt", type=Path,
                        help="Path to .pt model for benchmark evaluation")
    parser.add_argument("--benchmark-conf", type=float, default=0.25,
                        help="Confidence threshold for benchmark inference (default: 0.25)")
    parser.add_argument("--benchmark-report-dir", type=Path, default=Path("reports"),
                        help="Output directory for benchmark reports (default: reports/)")

    args = parser.parse_args()

    # Interaktiv modellväljare om --select-model angetts
    if args.select_model:
        args.model_base = prompt_model_selection()

    # Resolve export format from CLI flag
    _export_fmt_map = {opt["key"]: opt for opt in EXPORT_OPTIONS}
    export_fmt = _export_fmt_map.get(args.export_format) if args.export_format else None

    # Auto-enable early_stop om mål eller patience ändrats från default
    early_stop = args.early_stop
    if (args.target_map50 != DEFAULT_TARGET_MAP50
            or args.target_map5095 != DEFAULT_TARGET_MAP5095
            or args.patience != DEFAULT_PATIENCE):
        early_stop = True

    # Mode: Interactive
    if args.interactive:
        interactive_mode(args)
        return

    # Mode: Export only
    if args.export_only:
        ensure_deps()
        if export_fmt is None:
            export_fmt = prompt_export_format()
        onnx_path = export_onnx(args.export_only, args.imgsz, export_fmt=export_fmt)
        deploy_path = copy_to_deploy(onnx_path, args.deploy_dir, args.version)
        convert_rten(onnx_path)
        log(f"Export complete: {deploy_path}", "OK")
        return

    # Mode: Verify only
    if args.verify_only:
        ensure_deps()
        verify_with_server(args.verify_only, args.server)
        return

    # Mode: Benchmark
    if args.benchmark:
        ensure_deps()

        # Hitta modell
        model_pt = args.model_pt
        if model_pt is None:
            # Sök senaste best.pt
            candidates = sorted(
                Path("runs").rglob("best.pt"),
                key=lambda p: p.stat().st_mtime,
                reverse=True,
            ) if Path("runs").exists() else []
            if not candidates:
                candidates = sorted(
                    args.deploy_dir.rglob("*.pt"),
                    key=lambda p: p.stat().st_mtime,
                    reverse=True,
                ) if args.deploy_dir.exists() else []
            if not candidates:
                log("Ingen modell hittad. Ange --model-pt <sökväg>", "ERR")
                sys.exit(1)
            model_pt = candidates[0]
            log(f"Auto-vald modell: {model_pt}", "INFO")

        bench_dir = Path("dataset")
        all_results = run_benchmark(
            model_path=model_pt,
            bench_names=args.benchmark,
            output_dir=bench_dir,
            imgsz=args.imgsz,
            conf=args.benchmark_conf,
        )
        generate_benchmark_report(
            model_path=model_pt,
            all_results=all_results,
            version=args.version,
            output_dir=args.benchmark_report_dir,
        )
        return

    # Mode: Download dataset + convert + (optionally) train
    if args.download or args.download_only:
        if args.format == "yolo":
            log("--download kräver --format (rico, coco, eller webui)", "ERR")
            log("YOLO-format har inget standarddataset att ladda ner.", "ERR")
            log("Använd --download-starter för syntetiskt testdata, eller --dataset för lokalt YOLO-dataset.", "INFO")
            sys.exit(1)

        ensure_deps()

        base_dir = Path("dataset")
        log(f"Laddar ner {args.format}-dataset...", "STEP")
        raw_dir = download_dataset(args.format, base_dir)

        log(f"Konverterar {args.format} → YOLO...", "STEP")
        converted_dir = base_dir / f"{args.format}_converted"
        dataset_path = convert_dataset(raw_dir, converted_dir, args.format,
                                       extended=args.extended_classes)

        log(f"Dataset klart: {dataset_path}", "OK")

        if args.download_only:
            log("--download-only: hoppar över träning", "INFO")
            log(f"Kör manuellt: python tools/train_vision.py --dataset {dataset_path} --version v2", "INFO")
            return

        run_pipeline(
            dataset_dir=dataset_path,
            epochs=args.epochs,
            batch=args.batch,
            imgsz=args.imgsz,
            model_base=args.model_base,
            version=args.version,
            server_url=args.server,
            deploy_dir=args.deploy_dir,
            skip_verify=args.skip_verify,
            device=args.device,
            fresh=args.fresh,
            export_fmt=export_fmt,
            early_stop=early_stop,
            target_map50=args.target_map50,
            target_map5095=args.target_map5095,
            patience=args.patience,
        )
        return

    # Mode: Download starter + train
    if args.download_starter:
        dataset_dir = Path("dataset")
        ensure_deps()
        download_starter_dataset(dataset_dir)
        run_pipeline(
            dataset_dir=dataset_dir,
            epochs=args.epochs,
            batch=args.batch,
            imgsz=args.imgsz,
            model_base=args.model_base,
            version=args.version,
            server_url=args.server,
            deploy_dir=args.deploy_dir,
            skip_verify=args.skip_verify,
            device=args.device,
            fresh=args.fresh,
            export_fmt=export_fmt,
            early_stop=early_stop,
            target_map50=args.target_map50,
            target_map5095=args.target_map5095,
            patience=args.patience,
        )
        return

    # Mode: Merge datasets + train
    if args.merge_datasets:
        ensure_deps()
        merged_dir = Path("dataset") / "merged"
        dataset_path = merge_datasets(args.merge_datasets, merged_dir)
        data_yaml = create_data_yaml(dataset_path, dataset_path / "data.yaml")
        run_pipeline(
            dataset_dir=dataset_path,
            epochs=args.epochs,
            batch=args.batch,
            imgsz=args.imgsz,
            model_base=args.model_base,
            version=args.version,
            server_url=args.server,
            deploy_dir=args.deploy_dir,
            skip_verify=args.skip_verify,
            device=args.device,
            fresh=args.fresh,
            export_fmt=export_fmt,
            early_stop=early_stop,
            target_map50=args.target_map50,
            target_map5095=args.target_map5095,
            patience=args.patience,
        )
        return

    # Mode: Full pipeline with dataset
    if args.dataset:
        if not args.dataset.exists():
            log(f"Dataset path not found: {args.dataset}", "ERR")
            sys.exit(1)

        # Konvertera om formatet inte redan är YOLO
        dataset_path = args.dataset
        if args.format != "yolo":
            converted_dir = Path("dataset") / f"{args.format}_converted"
            log(f"Konverterar {args.format} → YOLO...", "STEP")
            dataset_path = convert_dataset(args.dataset, converted_dir, args.format,
                                           extended=args.extended_classes)

        run_pipeline(
            dataset_dir=dataset_path,
            epochs=args.epochs,
            batch=args.batch,
            imgsz=args.imgsz,
            model_base=args.model_base,
            version=args.version,
            server_url=args.server,
            deploy_dir=args.deploy_dir,
            skip_verify=args.skip_verify,
            device=args.device,
            fresh=args.fresh,
            export_fmt=export_fmt,
            early_stop=early_stop,
            target_map50=args.target_map50,
            target_map5095=args.target_map5095,
            patience=args.patience,
        )
        return

    # No mode specified
    parser.print_help()
    print("\nQuick start:")
    print("  python tools/train_vision.py --download --format rico   # download Rico + train")
    print("  python tools/train_vision.py --download --format webui  # download WebUI + train")
    print("  python tools/train_vision.py --download-starter         # synthetic data + train")
    print("  python tools/train_vision.py --interactive              # step-by-step wizard")


if __name__ == "__main__":
    main()