# AetherAgent — Lokal uppstarts-guide

> Komplett guide: alla sätt att köra AetherAgent lokalt.
> Extra fokus: **WSL + LM Studio** på Windows.

---

## Innehåll

1. [Förutsättningar](#1-förutsättningar)
2. [Miljövariabler](#2-miljövariabler)
3. [Bygga binärer](#3-bygga-binärer)
4. [Startmetod A — setup-local.sh (enklast)](#4-startmetod-a--setup-localsh)
5. [Startmetod B — HTTP-server manuellt](#5-startmetod-b--http-server-manuellt)
6. [Startmetod C — MCP-server (stdio)](#6-startmetod-c--mcp-server-stdio)
7. [Startmetod D — Docker](#7-startmetod-d--docker)
8. [Startmetod E — WASM (webbläsare)](#8-startmetod-e--wasm)
9. [Koppla LM Studio via MCP (stdio)](#9-koppla-lm-studio-via-mcp-stdio)
10. [Koppla LM Studio via HTTP](#10-koppla-lm-studio-via-http)
11. [Koppla Claude Desktop](#11-koppla-claude-desktop)
12. [Koppla Cursor / VS Code](#12-koppla-cursor--vs-code)
13. [Verifiera att allt fungerar](#13-verifiera-att-allt-fungerar)
14. [WSL-specifika tips](#14-wsl-specifika-tips)
15. [Felsökning](#15-felsökning)

---

## 1. Förutsättningar

### Krav

| Verktyg | Version | Installera |
|---------|---------|------------|
| Rust | 1.75+ | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| Cargo | (följer med Rust) | — |
| Chromium/Chrome | valfri | `sudo apt install chromium` (för Tier 2 CDP) |

### Valfritt

| Verktyg | Behövs för |
|---------|------------|
| Docker | Startmetod D |
| wasm-pack | Startmetod E (WASM) |
| Python 3.10+ | Vision-modell träning |
| LM Studio | MCP-klient |

### WSL (Windows)

```bash
# I Windows Terminal / PowerShell — installera WSL om det saknas
wsl --install

# Inuti WSL
sudo apt update && sudo apt install -y build-essential pkg-config libssl-dev libfontconfig1-dev
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env
```

---

## 2. Miljövariabler

Alla env-variabler AetherAgent läser:

| Variabel | Beskrivning | Standardvärde | Exempel |
|----------|-------------|---------------|---------|
| `PORT` | HTTP-serverns port | `3000` | `PORT=8080` |
| `AETHER_MODEL_PATH` | Lokal sökväg till ONNX/rten vision-modell | *(ingen)* | Se nedan |
| `AETHER_MODEL_URL` | URL till vision-modell (laddas vid startup) | *(ingen)* | `https://github.com/.../aether-ui-latest.onnx` |
| `CHROME_PATH` | Sökväg till Chromium/Chrome (Tier 2 CDP) | auto-detect | `/usr/bin/chromium` |

### Prioritet för vision-modell

Servern kollar i denna ordning:

1. `AETHER_MODEL_URL` — laddar ner från URL vid startup
2. `AETHER_MODEL_PATH` — läser lokal fil
3. `models/aether-ui-latest.rten` — default-sökväg (via setup-local.sh)

Om ingen sätts startar servern utan vision — alla andra endpoints fungerar.

### AETHER_MODEL_PATH på WSL (Windows-fil)

Windows-filer nås via `/mnt/c/...` i WSL:

```bash
# Din modell ligger på:
#   C:\Users\robin\Documents\GitHub\AetherAgent\aether-ui-latest.onnx
#
# I WSL blir det:
export AETHER_MODEL_PATH=/mnt/c/Users/robin/Documents/GitHub/AetherAgent/aether-ui-latest.onnx
```

> **Tips:** Kopiera modellen till WSL-filsystemet för snabbare laddning:
> ```bash
> mkdir -p ~/AetherAgent/models
> cp /mnt/c/Users/robin/Documents/GitHub/AetherAgent/aether-ui-latest.onnx ~/AetherAgent/models/
> export AETHER_MODEL_PATH=~/AetherAgent/models/aether-ui-latest.onnx
> ```

### Permanent: lägg i ~/.bashrc

```bash
echo 'export AETHER_MODEL_PATH=/mnt/c/Users/robin/Documents/GitHub/AetherAgent/aether-ui-latest.onnx' >> ~/.bashrc
source ~/.bashrc
```

---

## 3. Bygga binärer

### Alla features (rekommenderat för lokal utveckling)

```bash
cd ~/AetherAgent

# HTTP-server (alla features: vision, blitz-rendering, CDP, JS-eval)
cargo build --release --features server,vision,cdp --bin aether-server

# MCP-server (stdio, alla features)
cargo build --release --features mcp,vision,cdp --bin aether-mcp
```

### Snabbare bygg (server-release profil)

```bash
# ~3x snabbare kompilering med thin LTO
cargo build --profile server-release --features server,vision,cdp --bin aether-server
cargo build --profile server-release --features mcp,vision,cdp --bin aether-mcp
```

### Minimal (utan vision/CDP)

```bash
# Bara parsing + semantic + trust — ingen vision, inget Chrome
cargo build --release --features server --bin aether-server
cargo build --release --features mcp --bin aether-mcp
```

### Feature-flaggor

| Feature | Vad den aktiverar | Binärstorlek |
|---------|-------------------|--------------|
| `server` | HTTP API (axum), inkluderar js-eval, fetch, blitz, vision | stor |
| `mcp` | MCP stdio-server (rmcp), inkluderar blitz, vision | stor |
| `vision` | YOLOv8-nano ONNX inference (rten) | +~8 MB |
| `cdp` | Chrome DevTools Protocol (Tier 2 rendering) | +~3 MB |
| `js-eval` | Boa JS-sandbox | +~2 MB |
| `fetch` | HTTP page fetch, cookies, robots.txt | +~4 MB |
| `blitz` | Blitz HTML/CSS renderer (Tier 1 screenshots) | +~5 MB |

---

## 4. Startmetod A — setup-local.sh

**Enklaste sättet.** Bygger allt, installerar Chromium, startar servrar.

```bash
cd ~/AetherAgent

# Full setup: bygg + starta HTTP-server + visa MCP-info
./scripts/setup-local.sh

# Bara bygga (ingen start)
./scripts/setup-local.sh build

# Bara starta HTTP-server (port 3000)
./scripts/setup-local.sh http

# Bara starta MCP-server (stdio)
./scripts/setup-local.sh mcp

# Bara starta (förutsätter redan byggt)
./scripts/setup-local.sh start
```

### Med vision-modell

```bash
export AETHER_MODEL_PATH=/mnt/c/Users/robin/Documents/GitHub/AetherAgent/aether-ui-latest.onnx
./scripts/setup-local.sh
```

---

## 5. Startmetod B — HTTP-server manuellt

```bash
# Sätt modellsökväg (valfritt men rekommenderat)
export AETHER_MODEL_PATH=/mnt/c/Users/robin/Documents/GitHub/AetherAgent/aether-ui-latest.onnx

# Starta på port 3000 (default)
./target/release/aether-server

# Eller med annan port
PORT=8080 ./target/release/aether-server
```

### Tillgängliga endpoints

| Metod | Endpoint | Beskrivning |
|-------|----------|-------------|
| POST | `/api/parse` | HTML → semantic tree |
| POST | `/api/parse-top` | Top-N mest relevanta noder |
| POST | `/api/find-and-click` | Hitta klickbart element |
| POST | `/api/fill-form` | Fyll i formulär semantiskt |
| POST | `/api/extract-data` | Extrahera strukturerad data |
| POST | `/api/check-injection` | Prompt injection-scanning |
| POST | `/api/compile-goal` | Mål → handlingsplan |
| POST | `/api/diff-trees` | Jämför två semantic trees |
| POST | `/api/fetch-parse` | Hämta URL + parsea |
| POST | `/api/fetch-vision` | Hämta URL + screenshot + YOLO |
| POST | `/api/vision/parse` | Screenshot → UI-detektion (server-modell) |
| POST | `/api/parse-screenshot` | Screenshot → UI-detektion (klient-modell) |
| POST | `/api/tiered-screenshot` | Blitz/CDP auto-select rendering |
| POST | `/api/firewall/classify` | Semantic firewall |
| GET | `/health` | Health check |
| GET | `/api/tier-stats` | Tier 1/2 statistik |

---

## 6. Startmetod C — MCP-server (stdio)

MCP-servern pratar **stdio** (stdin/stdout JSON-RPC) — ingen HTTP-port.

```bash
export AETHER_MODEL_PATH=/mnt/c/Users/robin/Documents/GitHub/AetherAgent/aether-ui-latest.onnx

# Starta direkt (anslut via MCP-klient)
./target/release/aether-mcp
```

> MCP-servern exponerar 24 tools: parse, parse_top, find_and_click, fill_form,
> extract_data, check_injection, compile_goal, diff_trees, vision_parse, fetch_vision, m.fl.

---

## 7. Startmetod D — Docker

```bash
cd ~/AetherAgent

# Bygg Docker-image
docker build -t aether-agent .

# Kör med vision-modell från fil
docker run -p 3000:10000 \
  -v /mnt/c/Users/robin/Documents/GitHub/AetherAgent/aether-ui-latest.onnx:/models/model.onnx \
  -e AETHER_MODEL_PATH=/models/model.onnx \
  aether-agent

# Eller ladda modellen från URL
docker run -p 3000:10000 \
  -e AETHER_MODEL_URL=https://github.com/.../aether-ui-latest.onnx \
  aether-agent
```

### Docker Compose (valfritt)

```yaml
version: "3.8"
services:
  aether:
    build: .
    ports:
      - "3000:10000"
    environment:
      - AETHER_MODEL_PATH=/models/model.onnx
    volumes:
      - /mnt/c/Users/robin/Documents/GitHub/AetherAgent/aether-ui-latest.onnx:/models/model.onnx
```

---

## 8. Startmetod E — WASM

Kompilera till WebAssembly för användning i webbläsare:

```bash
# Installera wasm-pack
cargo install wasm-pack

# Bygg WASM-paket (ingen vision/CDP — de kräver native)
wasm-pack build --target web --release
```

Resultatet hamnar i `pkg/` — importera i JavaScript:

```javascript
import init, { parse_page, find_and_click } from './pkg/aether_agent.js';
await init();
const tree = parse_page(html, "find login button", "https://example.com");
```

---

## 9. Koppla LM Studio via MCP (stdio)

> **Det här är det rekommenderade sättet.**
> LM Studio >= 0.3 stödjer MCP-servrar via stdio.

### Steg 1: Bygg MCP-binären

```bash
# I WSL
cd ~/AetherAgent
cargo build --release --features mcp,vision,cdp --bin aether-mcp
```

### Steg 2: Konfigurera LM Studio

Öppna LM Studio → **Settings** → **MCP Servers** (eller Developer → MCP).

Lägg till en ny MCP-server med denna konfiguration:

```json
{
  "mcpServers": {
    "aether-agent": {
      "command": "wsl",
      "args": [
        "--exec",
        "/home/user/AetherAgent/target/release/aether-mcp"
      ],
      "env": {
        "AETHER_MODEL_PATH": "/mnt/c/Users/robin/Documents/GitHub/AetherAgent/aether-ui-latest.onnx"
      }
    }
  }
}
```

> **Varför `wsl --exec`?**
> LM Studio körs i Windows men binären ligger i WSL.
> `wsl --exec` startar binären i WSL-miljön med rätt Linux-libs.

### Steg 3: Verifiera

Starta om LM Studio. I chat-vyn ska du nu se AetherAgents 24 tools:
`parse`, `parse_top`, `find_and_click`, `fill_form`, `vision_parse`, `fetch_vision`, m.fl.

Testa med en prompt:
> "Använd parse för att analysera denna HTML: `<button>Köp nu</button>` med målet 'hitta köpknapp'"

### Alternativ: MCP-binären i Windows (kopia)

Om `wsl --exec` inte fungerar i din LM Studio-version:

```bash
# Kopiera binären till Windows
cp ~/AetherAgent/target/release/aether-mcp /mnt/c/Users/robin/aether-mcp
```

```json
{
  "mcpServers": {
    "aether-agent": {
      "command": "C:\\Users\\robin\\aether-mcp"
    }
  }
}
```

> **OBS:** Detta fungerar INTE — binären är kompilerad för Linux (ELF).
> Du måste antingen använda `wsl --exec` eller cross-kompilera:
> ```bash
> rustup target add x86_64-pc-windows-msvc
> cargo build --release --target x86_64-pc-windows-msvc --features mcp,vision --bin aether-mcp
> ```

---

## 10. Koppla LM Studio via HTTP

Om din version av LM Studio stödjer HTTP-baserade MCP-servrar eller externa API:er:

### Steg 1: Starta HTTP-servern i WSL

```bash
# Terminal 1 (WSL) — håll öppen
export AETHER_MODEL_PATH=/mnt/c/Users/robin/Documents/GitHub/AetherAgent/aether-ui-latest.onnx
./target/release/aether-server
```

Servern lyssnar nu på `http://localhost:3000`.

> WSL 2 delar `localhost` med Windows — `http://localhost:3000` fungerar direkt
> från LM Studio/webbläsare i Windows.

### Steg 2: Anropa endpoints från LM Studio

Du kan använda LM Studio's "tool calling" eller function calling och peka
custom tool-implementationen mot AetherAgent:

```
POST http://localhost:3000/api/parse
Content-Type: application/json

{
  "html": "<button>Köp nu</button>",
  "goal": "hitta köpknapp",
  "url": "https://example.com"
}
```

### Steg 3: MCP Streamable HTTP (om stöd finns)

HTTP-servern exponerar även MCP via Streamable HTTP:

```
POST http://localhost:3000/mcp
Content-Type: application/json

{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"lm-studio","version":"1.0"}}}
```

---

## 11. Koppla Claude Desktop

### Alt A: Lokal binär (snabbast)

Lägg till i `~/.claude/claude_desktop_config.json` (macOS/Linux) eller
`%APPDATA%\Claude\claude_desktop_config.json` (Windows):

```json
{
  "mcpServers": {
    "aether-agent": {
      "command": "wsl",
      "args": ["--exec", "/home/user/AetherAgent/target/release/aether-mcp"],
      "env": {
        "AETHER_MODEL_PATH": "/mnt/c/Users/robin/Documents/GitHub/AetherAgent/aether-ui-latest.onnx"
      }
    }
  }
}
```

### Alt B: Remote (Render)

```json
{
  "mcpServers": {
    "aether-agent": {
      "url": "https://aether-agent-api.onrender.com/mcp"
    }
  }
}
```

---

## 12. Koppla Cursor / VS Code

Cursor och VS Code med MCP-stöd — samma konfiguration:

### .cursor/mcp.json (projektlokal)

```json
{
  "mcpServers": {
    "aether-agent": {
      "command": "wsl",
      "args": ["--exec", "/home/user/AetherAgent/target/release/aether-mcp"],
      "env": {
        "AETHER_MODEL_PATH": "/mnt/c/Users/robin/Documents/GitHub/AetherAgent/aether-ui-latest.onnx"
      }
    }
  }
}
```

---

## 13. Verifiera att allt fungerar

### HTTP-server

```bash
# Health check
curl http://localhost:3000/health

# Testa parsing
curl -s -X POST http://localhost:3000/api/parse \
  -H "Content-Type: application/json" \
  -d '{"html":"<button>Köp</button>","goal":"hitta knapp","url":"https://test.se"}' | python3 -m json.tool

# Testa vision (kräver AETHER_MODEL_PATH)
curl -s -X POST http://localhost:3000/api/vision/parse \
  -H "Content-Type: application/json" \
  -d "{\"png_base64\":\"$(base64 -w0 screenshot.png)\",\"goal\":\"find buttons\"}" | python3 -m json.tool

# Testa fetch + vision (allt-i-ett)
curl -s -X POST http://localhost:3000/api/fetch-vision \
  -H "Content-Type: application/json" \
  -d '{"url":"https://example.com","goal":"find links"}' | python3 -m json.tool
```

### MCP-server (manuellt test via stdio)

```bash
# Skicka initialize-request via stdin
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}' | ./target/release/aether-mcp
```

### Fullständigt testsvit

```bash
# Alla tester + clippy + format
cargo test && cargo clippy -- -D warnings && cargo fmt --check
```

---

## 14. WSL-specifika tips

### Filsystemperformance

| Sökväg | Typ | Hastighet |
|--------|-----|-----------|
| `/home/user/...` | Native WSL (ext4) | Snabb |
| `/mnt/c/...` | Windows-mount (9P) | 5-10x långsammare |

**Rekommendation:** Bygg och kör från WSL-filsystemet (`~/AetherAgent`).
Vision-modellen kan ligga på `/mnt/c/` — den laddas en gång vid startup.

### localhost-delning

WSL 2 exponerar `localhost` automatiskt till Windows.
`http://localhost:3000` i WSL = `http://localhost:3000` i Windows.

Om det inte fungerar, kolla:

```powershell
# PowerShell (Windows)
netsh interface portproxy show all
```

Eller använd WSL:s IP direkt:

```bash
# I WSL — visa IP
hostname -I
# Exempel: 172.28.160.1

# I Windows, peka mot: http://172.28.160.1:3000
```

### Chromium i WSL (Tier 2 CDP)

```bash
# Installera headless Chromium
sudo apt install -y chromium-browser

# Eller Google Chrome
wget -q https://dl.google.com/linux/direct/google-chrome-stable_current_amd64.deb
sudo dpkg -i google-chrome-stable_current_amd64.deb
sudo apt -f install -y

# Verifiera
which chromium-browser || which google-chrome
```

---

## 15. Felsökning

| Problem | Orsak | Lösning |
|---------|-------|---------|
| `Ingen vision-modell konfigurerad` | `AETHER_MODEL_PATH` ej satt | `export AETHER_MODEL_PATH=/mnt/c/.../aether-ui-latest.onnx` |
| `Kunde inte läsa modell-fil` | Fel sökväg / fil saknas | Kontrollera med `ls -la $AETHER_MODEL_PATH` |
| `Connection refused` på port 3000 | Servern körs inte | Starta med `./target/release/aether-server` |
| LM Studio ser inga tools | MCP-config saknas | Lägg till JSON-config i Settings → MCP |
| `wsl --exec` misslyckas | WSL ej installerat | `wsl --install` i PowerShell |
| Långsam modell-laddning | Fil på `/mnt/c/` | Kopiera till `~/AetherAgent/models/` |
| CDP timeout | Chrome saknas i WSL | `sudo apt install chromium-browser` |
| `address already in use` | Port 3000 upptagen | `PORT=8080 ./target/release/aether-server` |
| Vision returnerar 0 detections | Modell laddad men screenshot-format fel | Skicka PNG (inte JPEG), base64-kodat |

### Debug-loggar

```bash
# Verbose Rust-logging
RUST_LOG=debug ./target/release/aether-server

# Se exakt vad MCP-servern får/skickar
RUST_LOG=debug ./target/release/aether-mcp 2>mcp-debug.log
```

---

## Snabbreferens — exakta kommandon (WSL + LM Studio)

```bash
# ─── EN GÅNG: Installera ─────────────────────────────────────────
cd ~/AetherAgent
cargo build --release --features mcp,vision,cdp --bin aether-mcp
cargo build --release --features server,vision,cdp --bin aether-server

# ─── VARJE GÅNG: Starta HTTP-server (valfritt) ──────────────────
export AETHER_MODEL_PATH=/mnt/c/Users/robin/Documents/GitHub/AetherAgent/aether-ui-latest.onnx
./target/release/aether-server
# → Lyssnar på http://localhost:3000

# ─── LM STUDIO: MCP-config ──────────────────────────────────────
# Settings → MCP Servers → Lägg till:
# {
#   "mcpServers": {
#     "aether-agent": {
#       "command": "wsl",
#       "args": ["--exec", "/home/user/AetherAgent/target/release/aether-mcp"],
#       "env": {
#         "AETHER_MODEL_PATH": "/mnt/c/Users/robin/Documents/GitHub/AetherAgent/aether-ui-latest.onnx"
#       }
#     }
#   }
# }
# → Starta om LM Studio → 24 AetherAgent tools syns i chat
```
