#!/usr/bin/env bash
set -euo pipefail

# ============================================================================
# AetherAgent — Komplett WSL Bootstrap
# ============================================================================
#
# Installerar ALLT från scratch i en ren WSL (Ubuntu 22.04/24.04):
#   - Systempaket (build-essential, pkg-config, libssl-dev, ...)
#   - Rust (rustup, stable toolchain, wasm32 target, clippy, rustfmt)
#   - wasm-pack
#   - Bygger AetherAgent: lib, server, MCP-server, WASM
#   - Python-venv med bindings + requests + wasmtime
#   - Node.js (via nvm) + Node-bindings
#   - Kör hela testsviten + clippy + fmt
#   - Valfritt: vision-träningspipeline
#
# Kör:
#   chmod +x tools/bootstrap_wsl.sh && ./tools/bootstrap_wsl.sh
#
# Flaggor:
#   --skip-node          Hoppa över Node.js/nvm
#   --skip-python        Hoppa över Python-venv
#   --skip-wasm          Hoppa över WASM-build
#   --skip-tests         Hoppa över cargo test/clippy/fmt
#   --with-vision        Installera vision-träning (PyTorch + Ultralytics)
#   --server-port PORT   Portnummer för servern (default: 3000)
#   --help               Visa hjälp
#
# ============================================================================

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
VENV_DIR="$PROJECT_DIR/.venv"
NVM_DIR_LOCAL="$HOME/.nvm"
NODE_VERSION="20"

SKIP_NODE=false
SKIP_PYTHON=false
SKIP_WASM=false
SKIP_TESTS=false
WITH_VISION=false
SERVER_PORT=3000

# ---------------------------------------------------------------------------
# Färger
# ---------------------------------------------------------------------------
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
CYAN='\033[0;36m'
MAGENTA='\033[0;35m'
BOLD='\033[1m'
NC='\033[0m'

log()  { echo -e "${CYAN}[INFO]${NC} $1"; }
ok()   { echo -e "${GREEN}[OK]${NC}   $1"; }
warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
err()  { echo -e "${RED}[FEL]${NC}  $1"; }
step() { echo -e "\n${MAGENTA}${BOLD}═══ $1 ═══${NC}\n"; }

# ---------------------------------------------------------------------------
# Parsning av flaggor
# ---------------------------------------------------------------------------
usage() {
    echo "Användning: $0 [flaggor]"
    echo ""
    echo "Flaggor:"
    echo "  --skip-node          Hoppa över Node.js-installation"
    echo "  --skip-python        Hoppa över Python-venv"
    echo "  --skip-wasm          Hoppa över WASM-build"
    echo "  --skip-tests         Hoppa över test/clippy/fmt"
    echo "  --with-vision        Installera vision-träning (PyTorch + Ultralytics)"
    echo "  --server-port PORT   Portnummer för servern (default: 3000)"
    echo "  --help               Visa detta meddelande"
    exit 0
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --skip-node)    SKIP_NODE=true; shift ;;
        --skip-python)  SKIP_PYTHON=true; shift ;;
        --skip-wasm)    SKIP_WASM=true; shift ;;
        --skip-tests)   SKIP_TESTS=true; shift ;;
        --with-vision)  WITH_VISION=true; shift ;;
        --server-port)  SERVER_PORT="$2"; shift 2 ;;
        --help|-h)      usage ;;
        *)              err "Okänd flagga: $1"; usage ;;
    esac
done

# ---------------------------------------------------------------------------
# Kravkontroll
# ---------------------------------------------------------------------------
check_wsl() {
    if grep -qi microsoft /proc/version 2>/dev/null; then
        log "WSL detekterad"
    else
        warn "Inte WSL — skriptet fungerar men är optimerat för WSL/Ubuntu"
    fi
}

# ---------------------------------------------------------------------------
# Steg 0: Systempaket
# ---------------------------------------------------------------------------
install_system_deps() {
    step "Steg 0/8: Systempaket"

    local pkgs=(
        build-essential
        pkg-config
        libssl-dev
        ca-certificates
        curl
        wget
        git
        python3
        python3-venv
        python3-pip
        python3-dev
    )

    # Extra paket för vision
    if $WITH_VISION; then
        pkgs+=(libgl1-mesa-glx libglib2.0-0)
    fi

    log "Uppdaterar paketlistor..."
    sudo apt-get update -qq

    log "Installerar: ${pkgs[*]}"
    sudo apt-get install -y -qq "${pkgs[@]}"

    ok "Systempaket installerade"
}

# ---------------------------------------------------------------------------
# Steg 1: Rust via rustup
# ---------------------------------------------------------------------------
install_rust() {
    step "Steg 1/8: Rust toolchain"

    if command -v rustup &>/dev/null; then
        log "rustup finns redan — uppdaterar..."
        rustup update stable --no-self-update
    else
        log "Installerar rustup (stable)..."
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
        # shellcheck source=/dev/null
        source "$HOME/.cargo/env"
    fi

    # Komponenter
    rustup component add clippy rustfmt

    # WASM-target
    if ! $SKIP_WASM; then
        rustup target add wasm32-unknown-unknown
    fi

    ok "Rust $(rustc --version | awk '{print $2}') installerad"
}

# ---------------------------------------------------------------------------
# Steg 2: wasm-pack
# ---------------------------------------------------------------------------
install_wasm_pack() {
    if $SKIP_WASM; then return; fi

    step "Steg 2/8: wasm-pack"

    if command -v wasm-pack &>/dev/null; then
        ok "wasm-pack redan installerad ($(wasm-pack --version))"
    else
        log "Installerar wasm-pack..."
        curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh
        ok "wasm-pack installerad"
    fi
}

# ---------------------------------------------------------------------------
# Steg 3: Bygg AetherAgent (native)
# ---------------------------------------------------------------------------
build_native() {
    step "Steg 3/8: Bygger AetherAgent (native)"

    cd "$PROJECT_DIR"

    log "Bygger lib (alla features)..."
    cargo build --release --features server,mcp,vision 2>&1 | tail -3

    log "Bygger aether-server..."
    cargo build --release --features server --bin aether-server 2>&1 | tail -3

    log "Bygger aether-mcp..."
    cargo build --release --features mcp --bin aether-mcp 2>&1 | tail -3

    local server_bin="$PROJECT_DIR/target/release/aether-server"
    local mcp_bin="$PROJECT_DIR/target/release/aether-mcp"

    if [[ -f "$server_bin" ]]; then
        local size
        size=$(du -h "$server_bin" | cut -f1)
        ok "aether-server byggt ($size)"
    fi
    if [[ -f "$mcp_bin" ]]; then
        ok "aether-mcp byggt"
    fi
}

# ---------------------------------------------------------------------------
# Steg 4: Bygg WASM
# ---------------------------------------------------------------------------
build_wasm() {
    if $SKIP_WASM; then return; fi

    step "Steg 4/8: Bygger WASM"

    cd "$PROJECT_DIR"

    log "wasm-pack build --target web --release"
    wasm-pack build --target web --release 2>&1 | tail -5

    local wasm_file="$PROJECT_DIR/pkg/aether_agent_bg.wasm"
    if [[ -f "$wasm_file" ]]; then
        local size
        size=$(du -h "$wasm_file" | cut -f1)
        ok "WASM byggt: $size"

        # Storlekskontroll (<6 MB)
        local bytes
        bytes=$(stat -c%s "$wasm_file")
        if (( bytes > 6291456 )); then
            warn "WASM-binären är >6MB ($size) — överväg att minska features"
        fi
    else
        err "WASM-build misslyckades — pkg/aether_agent_bg.wasm saknas"
        return 1
    fi
}

# ---------------------------------------------------------------------------
# Steg 5: Python-venv + bindings
# ---------------------------------------------------------------------------
setup_python() {
    if $SKIP_PYTHON; then return; fi

    step "Steg 5/8: Python-venv + bindings"

    if [[ -d "$VENV_DIR" ]]; then
        log "Venv finns redan: $VENV_DIR"
    else
        log "Skapar venv: $VENV_DIR"
        python3 -m venv "$VENV_DIR"
    fi

    # shellcheck source=/dev/null
    source "$VENV_DIR/bin/activate"

    log "Installerar Python-beroenden..."
    pip install --quiet --upgrade pip

    # Grundpaket för bindings
    pip install --quiet \
        requests \
        wasmtime

    # Vision-paket om begärt
    if $WITH_VISION; then
        log "Installerar vision-paket (PyTorch CUDA 12.4 + Ultralytics)..."
        pip install --quiet \
            torch torchvision torchaudio \
            --index-url https://download.pytorch.org/whl/cu124

        pip install --quiet \
            ultralytics \
            pillow \
            opencv-python-headless \
            onnx \
            onnxsim \
            tqdm \
            pyyaml \
            matplotlib \
            seaborn \
            pandas
    fi

    # Testa att bindings kan importeras
    log "Verifierar Python-bindings..."
    python3 -c "
import sys
sys.path.insert(0, '$PROJECT_DIR/bindings/python')
from aether_agent import AetherAgent
print('  AetherAgent (HTTP-klient) OK')
" 2>/dev/null && ok "Python-bindings fungerar" || warn "Python-bindings kunde inte importeras"

    deactivate
}

# ---------------------------------------------------------------------------
# Steg 6: Node.js via nvm
# ---------------------------------------------------------------------------
setup_node() {
    if $SKIP_NODE; then return; fi

    step "Steg 6/8: Node.js $NODE_VERSION via nvm"

    # Installera nvm om det saknas
    if [[ ! -d "$NVM_DIR_LOCAL" ]]; then
        log "Installerar nvm..."
        curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.1/install.sh | bash
    fi

    export NVM_DIR="$NVM_DIR_LOCAL"
    # shellcheck source=/dev/null
    [ -s "$NVM_DIR/nvm.sh" ] && . "$NVM_DIR/nvm.sh"

    if ! command -v nvm &>/dev/null; then
        warn "nvm kunde inte laddas — hoppar över Node.js"
        return
    fi

    log "Installerar Node.js $NODE_VERSION..."
    nvm install "$NODE_VERSION" --default
    nvm use "$NODE_VERSION"

    ok "Node.js $(node --version) installerad"

    # Testa Node-bindings
    local node_dir="$PROJECT_DIR/bindings/node"
    if [[ -f "$node_dir/package.json" ]]; then
        log "Installerar Node-bindings..."
        cd "$node_dir"
        npm install --silent 2>/dev/null || true

        log "Verifierar Node-bindings..."
        node -e "
try {
    const pkg = require('./package.json');
    console.log('  @aether-agent/node v' + pkg.version + ' OK');
} catch(e) {
    console.log('  Node-paket laddat (WASM behöver byggas separat)');
}
" && ok "Node-bindings verifierade" || warn "Node-bindings kunde inte laddas"
    fi
}

# ---------------------------------------------------------------------------
# Steg 7: Kör testsvit
# ---------------------------------------------------------------------------
run_tests() {
    if $SKIP_TESTS; then return; fi

    step "Steg 7/8: Testsvit"

    cd "$PROJECT_DIR"

    log "cargo fmt --check"
    if cargo fmt --check 2>/dev/null; then
        ok "Formatering OK"
    else
        warn "Formateringsfel — kör 'cargo fmt' för att fixa"
    fi

    log "cargo clippy -- -D warnings"
    if cargo clippy -- -D warnings 2>&1 | tail -3; then
        ok "Clippy OK"
    else
        warn "Clippy-varningar — se output ovan"
    fi

    log "cargo test"
    if cargo test 2>&1 | tail -10; then
        ok "Alla tester passerade"
    else
        warn "Vissa tester misslyckades — se output ovan"
    fi
}

# ---------------------------------------------------------------------------
# Steg 8: Sammanfattning
# ---------------------------------------------------------------------------
print_summary() {
    step "Steg 8/8: Klart!"

    echo -e "${BOLD}AetherAgent är installerat och byggt.${NC}\n"

    echo -e "${BOLD}Binärer:${NC}"
    echo "  Server:    $PROJECT_DIR/target/release/aether-server"
    echo "  MCP:       $PROJECT_DIR/target/release/aether-mcp"
    if ! $SKIP_WASM; then
        echo "  WASM:      $PROJECT_DIR/pkg/aether_agent_bg.wasm"
    fi

    echo ""
    echo -e "${BOLD}Starta servern:${NC}"
    echo "  cd $PROJECT_DIR"
    echo "  PORT=$SERVER_PORT ./target/release/aether-server"
    echo "  # Hälsokontroll: curl http://localhost:$SERVER_PORT/health"

    echo ""
    echo -e "${BOLD}MCP-server (Claude Desktop / Cursor):${NC}"
    echo "  ./target/release/aether-mcp"

    if ! $SKIP_PYTHON; then
        echo ""
        echo -e "${BOLD}Python:${NC}"
        echo "  source $VENV_DIR/bin/activate"
        echo "  python -c \"from bindings.python.aether_agent import AetherAgent; a = AetherAgent('http://localhost:$SERVER_PORT'); print(a.health())\""
    fi

    if ! $SKIP_NODE; then
        echo ""
        echo -e "${BOLD}Node.js:${NC}"
        echo "  cd bindings/node && npm test"
    fi

    if $WITH_VISION; then
        echo ""
        echo -e "${BOLD}Vision-träning:${NC}"
        echo "  source $VENV_DIR/bin/activate"
        echo "  ./tools/train.sh"
    fi

    echo ""
    echo -e "${BOLD}Miljövariabler (lägg i .bashrc):${NC}"
    echo "  export PATH=\"\$HOME/.cargo/bin:\$PATH\""
    if ! $SKIP_NODE; then
        echo "  export NVM_DIR=\"$NVM_DIR_LOCAL\""
        echo "  [ -s \"\$NVM_DIR/nvm.sh\" ] && . \"\$NVM_DIR/nvm.sh\""
    fi

    echo ""
    ok "Bootstrap klar! Allt installerat i: $PROJECT_DIR"
}

# ============================================================================
# Main
# ============================================================================
main() {
    echo -e "\n${MAGENTA}${BOLD}"
    echo "    ╔═══════════════════════════════════════════╗"
    echo "    ║  AetherAgent — Komplett WSL Bootstrap     ║"
    echo "    ╚═══════════════════════════════════════════╝"
    echo -e "${NC}\n"

    check_wsl

    local start_time
    start_time=$(date +%s)

    install_system_deps
    install_rust

    # Ladda cargo env om nyinstallerat
    if [[ -f "$HOME/.cargo/env" ]]; then
        # shellcheck source=/dev/null
        source "$HOME/.cargo/env"
    fi

    install_wasm_pack
    build_native
    build_wasm
    setup_python
    setup_node
    run_tests
    print_summary

    local end_time elapsed_min elapsed_sec
    end_time=$(date +%s)
    elapsed_sec=$(( end_time - start_time ))
    elapsed_min=$(( elapsed_sec / 60 ))
    elapsed_sec=$(( elapsed_sec % 60 ))

    echo ""
    log "Total tid: ${elapsed_min}m ${elapsed_sec}s"
}

main "$@"
