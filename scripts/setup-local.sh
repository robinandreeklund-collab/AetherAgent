#!/usr/bin/env bash
set -euo pipefail

# ─── AetherAgent Local Setup ─────────────────────────────────────────────────
# Bygger allt, installerar Chromium, startar MCP + HTTP server.
#
# Användning:
#   ./scripts/setup-local.sh          # Full setup + starta servrar
#   ./scripts/setup-local.sh build    # Bara bygga
#   ./scripts/setup-local.sh start    # Bara starta (förutsätter byggt)
#   ./scripts/setup-local.sh mcp      # Bara starta MCP-server (stdio)
#   ./scripts/setup-local.sh http     # Bara starta HTTP-server (port 3000)
# ─────────────────────────────────────────────────────────────────────────────

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_DIR"

# Färger
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log()  { echo -e "${GREEN}[✓]${NC} $*"; }
warn() { echo -e "${YELLOW}[!]${NC} $*"; }
err()  { echo -e "${RED}[✗]${NC} $*"; }
info() { echo -e "${BLUE}[i]${NC} $*"; }

# ─── 1. Kolla beroenden ─────────────────────────────────────────────────────

check_deps() {
    info "Kollar beroenden..."

    # Rust
    if ! command -v cargo &>/dev/null; then
        err "Rust/Cargo saknas. Installera: https://rustup.rs"
        exit 1
    fi
    log "Rust $(rustc --version | awk '{print $2}')"

    # Chromium / Chrome
    CHROME_BIN=""
    for bin in chromium chromium-browser google-chrome google-chrome-stable; do
        if command -v "$bin" &>/dev/null; then
            CHROME_BIN="$bin"
            break
        fi
    done

    if [ -z "$CHROME_BIN" ]; then
        warn "Chromium/Chrome saknas — installerar..."
        install_chromium
    else
        log "Chrome: $CHROME_BIN ($($CHROME_BIN --version 2>/dev/null || echo 'ok'))"
    fi

    # Vision-modell (valfri)
    if [ -n "${AETHER_MODEL_PATH:-}" ] && [ -f "$AETHER_MODEL_PATH" ]; then
        log "Vision-modell: $AETHER_MODEL_PATH"
    elif [ -f "$PROJECT_DIR/models/aether-ui-latest.rten" ]; then
        export AETHER_MODEL_PATH="$PROJECT_DIR/models/aether-ui-latest.rten"
        log "Vision-modell: $AETHER_MODEL_PATH"
    else
        warn "Ingen vision-modell hittad (YOLO-detektion inaktiv)"
        warn "Sätt AETHER_MODEL_PATH eller ladda ner till models/"
    fi
}

install_chromium() {
    if [[ "$OSTYPE" == "darwin"* ]]; then
        if command -v brew &>/dev/null; then
            brew install --cask chromium 2>/dev/null || brew install chromium 2>/dev/null || true
        else
            err "Installera Homebrew först: https://brew.sh"
            err "Sedan: brew install --cask chromium"
            exit 1
        fi
    elif [[ "$OSTYPE" == "linux-gnu"* ]]; then
        if command -v apt-get &>/dev/null; then
            sudo apt-get update && sudo apt-get install -y chromium || sudo apt-get install -y chromium-browser || true
        elif command -v dnf &>/dev/null; then
            sudo dnf install -y chromium
        elif command -v pacman &>/dev/null; then
            sudo pacman -S --noconfirm chromium
        else
            err "Kan inte auto-installera Chromium. Installera manuellt."
            exit 1
        fi
    else
        err "Okänt OS: $OSTYPE — installera Chromium manuellt"
        exit 1
    fi

    # Verifiera
    for bin in chromium chromium-browser google-chrome; do
        if command -v "$bin" &>/dev/null; then
            CHROME_BIN="$bin"
            log "Chromium installerad: $bin"
            return
        fi
    done
    warn "Chromium installation misslyckades — Tier 2 CDP inaktiv, Tier 1 Blitz fungerar"
}

# ─── 2. Bygga ───────────────────────────────────────────────────────────────

build() {
    info "Bygger AetherAgent (server + mcp + vision + cdp)..."

    echo ""
    info "Bygger HTTP-server..."
    cargo build --release --features server,vision,cdp --bin aether-server
    log "aether-server klar"

    info "Bygger MCP-server..."
    cargo build --release --features mcp,vision,cdp --bin aether-mcp
    log "aether-mcp klar"

    echo ""
    log "Binärer:"
    ls -lh target/release/aether-server target/release/aether-mcp 2>/dev/null

    echo ""
    info "Kör tester..."
    cargo test --quiet 2>&1 | tail -3
    log "Tester ok"
}

# ─── 3. Starta servrar ──────────────────────────────────────────────────────

start_http() {
    local port="${PORT:-3000}"
    info "Startar HTTP-server på port $port..."
    info "Endpoints:"
    echo "  POST /api/fetch-vision      — Screenshot + YOLO"
    echo "  POST /api/parse             — HTML → semantic tree"
    echo "  POST /api/compile-goal      — Goal → action plan"
    echo "  POST /api/find-safest-path  — Causal graph navigation"
    echo "  POST /api/tiered-screenshot — Blitz/CDP auto-select"
    echo "  GET  /api/tier-stats        — Tier 1/2 statistik"
    echo "  GET  /health                — Health check"
    echo ""
    PORT=$port exec target/release/aether-server
}

start_mcp() {
    info "Startar MCP-server (stdio)..."
    info "Koppla i LM Studio / Claude Desktop / Cursor:"
    echo ""
    echo "  Binär: $PROJECT_DIR/target/release/aether-mcp"
    echo ""
    echo "  LM Studio MCP config:"
    echo "  {"
    echo "    \"mcpServers\": {"
    echo "      \"aether-agent\": {"
    echo "        \"command\": \"$PROJECT_DIR/target/release/aether-mcp\""
    echo "      }"
    echo "    }"
    echo "  }"
    echo ""
    echo "  Claude Desktop (~/.claude/claude_desktop_config.json):"
    echo "  {"
    echo "    \"mcpServers\": {"
    echo "      \"aether-agent\": {"
    echo "        \"command\": \"$PROJECT_DIR/target/release/aether-mcp\""
    echo "      }"
    echo "    }"
    echo "  }"
    echo ""
    exec target/release/aether-mcp
}

start_both() {
    local port="${PORT:-3000}"
    info "Startar HTTP-server (port $port) i bakgrunden + MCP-info..."

    PORT=$port target/release/aether-server &
    HTTP_PID=$!
    sleep 1

    if kill -0 $HTTP_PID 2>/dev/null; then
        log "HTTP-server igång (PID $HTTP_PID, port $port)"
        log "Health: http://localhost:$port/health"
    else
        err "HTTP-server startade inte"
    fi

    echo ""
    echo "═══════════════════════════════════════════════════════════"
    echo ""
    log "AetherAgent redo!"
    echo ""
    echo "  HTTP API:  http://localhost:$port"
    echo "  MCP binär: $PROJECT_DIR/target/release/aether-mcp"
    echo ""
    echo "  LM Studio MCP-sökväg:"
    echo "  $PROJECT_DIR/target/release/aether-mcp"
    echo ""
    echo "═══════════════════════════════════════════════════════════"
    echo ""
    info "Tryck Ctrl+C för att stoppa"
    wait $HTTP_PID
}

# ─── Main ────────────────────────────────────────────────────────────────────

CMD="${1:-all}"

case "$CMD" in
    build)
        check_deps
        build
        ;;
    start)
        start_both
        ;;
    mcp)
        start_mcp
        ;;
    http)
        start_http
        ;;
    all)
        echo ""
        echo "╔═══════════════════════════════════════════════════════╗"
        echo "║     AetherAgent Local Setup                         ║"
        echo "║     Blitz (Tier 1) + Chrome CDP (Tier 2) + YOLO    ║"
        echo "╚═══════════════════════════════════════════════════════╝"
        echo ""
        check_deps
        build
        echo ""
        start_both
        ;;
    *)
        echo "Användning: $0 [all|build|start|mcp|http]"
        exit 1
        ;;
esac
