# AetherAgent Vision — Komplett träningsguide

> Från noll till produktion: träna YOLOv8-nano för UI-elementdetektion,
> validera, deploya till Render.

---

## Innehåll

1. [Översikt — Hur det hänger ihop](#1-översikt)
2. [Förutsättningar](#2-förutsättningar)
3. [Steg-för-steg: Träning](#3-träning-steg-för-steg)
4. [Auto-chaining — Modellen bygger vidare](#4-auto-chaining)
5. [Kvalitetskontroll](#5-kvalitetskontroll)
6. [Export till ONNX](#6-export-till-onnx)
7. [Deploy till Render](#7-deploy-till-render)
8. [Använda modellen via API](#8-använda-modellen-via-api)
9. [Iterationsloop](#9-iterationsloop)
10. [Vanliga problem & lösningar](#10-vanliga-problem)
11. [Referens: Klasser & dataset](#11-referens)

---

## 1. Översikt

```
┌────────────────────────────────────────────────────────┐
│                    TRÄNINGSFLÖDE                        │
│                                                        │
│  Dataset (Rico/WebUI/Custom)                           │
│       ↓                                                │
│  train_vision.py  →  YOLOv8-nano  →  best.pt          │
│       ↓                                                │
│  Export ONNX  →  models/aether-ui-latest.onnx          │
│       ↓                                                │
│  Upload (GitHub Release / S3)                          │
│       ↓                                                │
│  Render: AETHER_MODEL_URL → server laddar vid start    │
│       ↓                                                │
│  POST /api/vision/parse  →  UI-element detekterade!    │
└────────────────────────────────────────────────────────┘
```

**Kort version:**
1. Träna lokalt med `train_vision.py`
2. Modellen auto-exporteras till ONNX
3. Ladda upp ONNX-filen till publik URL
4. Sätt `AETHER_MODEL_URL` i Render Dashboard
5. Servern laddar modellen vid startup — klart!

---

## 2. Förutsättningar

### Hårdvara
| | Minimum | Rekommenderat |
|---|---|---|
| GPU | Ingen (CPU fungerar) | NVIDIA med CUDA |
| RAM | 8 GB | 16 GB |
| Disk | 10 GB ledigt | 50 GB (för Rico) |

### Mjukvara
```bash
# Python 3.10+
python --version

# Pip-paket (installeras automatiskt av scriptet)
pip install ultralytics tqdm requests
```

### WSL-varning (Windows-användare)
> **Viktigt:** Kör INTE träning på `/mnt/c/` (Windows-filsystemet).
> Extrahering av 66K Rico-filer tar 5 min på native Linux men **30+ min på /mnt/c/**.
>
> Använd istället native WSL-filsystemet:
> ```bash
> cd ~  # /home/user — 10x snabbare I/O
> git clone <repo> && cd AetherAgent
> ```

---

## 3. Träning steg-för-steg

### Steg 1: Syntetiskt data (snabbtest)

```bash
# Generera 200 syntetiska bilder + träna
python tools/train_vision.py --download-starter --version v1
```

Förväntad tid: ~10 min (GPU), ~30 min (CPU)
Förväntad mAP@50: **0.3–0.5**

### Steg 2: Rico (66K Android-skärmdumpar)

```bash
# Ladda ner Rico + konvertera till YOLO + träna
python tools/train_vision.py --download --format rico --version v2
```

> Rico-nedladdning är ~150 MB (zip) men extrahering skapar ~66K filer.
> **Var tålmodig** — extraheringen kan ta 5–30 min beroende på disk.
> Tips: Öppna ett nytt terminalfönster och kör:
> ```bash
> watch -n 2 'find dataset/rico_raw -type f 2>/dev/null | wc -l'
> ```
> för att se att filer faktiskt skapas.

Förväntad mAP@50: **0.5–0.7**

### Steg 3: WebUI (7K web-skärmdumpar)

```bash
# WebUI bygger vidare på Rico-modellen automatiskt (auto-chain)
python tools/train_vision.py --download --format webui --version v3
```

Auto-chain detekterar `runs/detect/aether-ui-v2/weights/best.pt` och
bygger vidare. Ingen `--model-base` behövs!

Förväntad mAP@50: **0.6–0.8**

### Steg 4: Egna screenshots (finjustering)

```bash
# Skapa dataset-mapp med dina egna screenshots + labels
# Använd Roboflow, CVAT, eller Label Studio för annotering
python tools/train_vision.py --dataset ./my-screenshots --version v4
```

Förväntad mAP@50: **0.7–0.9** (med 500+ egna bilder)

---

## 4. Auto-chaining

**Ny funktion:** Träningen bygger automatiskt vidare på senaste modellen.

```
Kör 1 (Rico v2):     yolov8n.pt → best.pt (v2)
Kör 2 (WebUI v3):    best.pt (v2) → best.pt (v3)  ← automatiskt!
Kör 3 (Custom v4):   best.pt (v3) → best.pt (v4)  ← automatiskt!
```

### Hur det fungerar
- Scriptet söker i `runs/detect/aether-ui-*/weights/best.pt`
- Väljer den med senaste mtime (= senaste färdiga träning)
- Loggar: `[OK] Auto-chain: bygger vidare på runs/detect/aether-ui-v2/weights/best.pt`

### Override
```bash
# Starta från scratch (ignorera tidigare modeller)
python tools/train_vision.py --download --format rico --version v2 --fresh

# Välj specifik modell att bygga vidare på
python tools/train_vision.py --dataset ./data --version v5 \
  --model-base runs/detect/aether-ui-v2/weights/best.pt
```

---

## 5. Kvalitetskontroll

### Metrics att kolla

Scriptet skriver ut dessa efter träning (Steg 6/6: Validate):

| Metric | Dåligt | OK | Bra | Produktion |
|--------|--------|-----|-----|------------|
| mAP@50 | <0.40 | 0.40–0.60 | 0.60–0.80 | >0.80 |
| mAP@50-95 | <0.25 | 0.25–0.40 | 0.40–0.55 | >0.55 |
| Precision | <0.50 | 0.50–0.70 | 0.70–0.85 | >0.85 |
| Recall | <0.50 | 0.50–0.70 | 0.70–0.85 | >0.85 |

### Visuell inspektion

Kolla dessa filer i `runs/detect/aether-ui-vN/`:

| Fil | Vad den visar |
|-----|--------------|
| `results.png` | mAP/loss per epoch. Loss ner, mAP upp = bra. |
| `confusion_matrix.png` | Vilka klasser förväxlas |
| `val_batch0_pred.png` | Visuella prediktioner på valideringsdata |

---

## 6. Export till ONNX

Sker **automatiskt** som Steg 4/6 i pipeline. Resultat:

```
models/
├── aether-ui-v2.onnx         # Versionerad kopia
├── aether-ui-latest.onnx     # Alltid senaste
└── aether-ui-v2.rten         # Rten-format (om tillgängligt)
```

Modellen är ~6 MB (YOLOv8-nano).

### Manuell export (om du behöver)
```bash
# Från en best.pt
yolo export model=runs/detect/aether-ui-v2/weights/best.pt format=onnx
cp runs/detect/aether-ui-v2/weights/best.onnx models/aether-ui-v2.onnx
```

---

## 7. Deploy till Render

### Steg 1: Ladda upp modellen

Välj ETT av dessa alternativ:

#### Alt A: GitHub Release (rekommenderat)
```bash
# Skapa en GitHub release med ONNX-filen
gh release create vision-v2 models/aether-ui-latest.onnx \
  --title "Vision Model v2" \
  --notes "Rico + WebUI trained, mAP@50: 0.72"
```

Kopiera URL:en till ONNX-filen, t.ex.:
`https://github.com/<org>/<repo>/releases/download/vision-v2/aether-ui-latest.onnx`

#### Alt B: S3 / Cloudflare R2
```bash
aws s3 cp models/aether-ui-latest.onnx s3://your-bucket/models/aether-ui-latest.onnx --acl public-read
```

### Steg 2: Konfigurera Render

1. Gå till **Render Dashboard** → aether-agent-api → Environment
2. Lägg till env var:
   ```
   AETHER_MODEL_URL = https://github.com/.../aether-ui-latest.onnx
   ```
3. Klicka **Save Changes** → servern startar om automatiskt

### Steg 3: Verifiera

```bash
# Kolla att modellen laddades
curl https://aether-agent-api.onrender.com/health

# Testa vision-endpoint
curl -X POST https://aether-agent-api.onrender.com/api/vision/parse \
  -H "Content-Type: application/json" \
  -d '{
    "png_base64": "'$(base64 -w0 screenshot.png)'",
    "goal": "find login button"
  }'
```

### Steg 4: Uppdatera modellen (utan redeploy!)

```bash
# Träna ny version
python tools/train_vision.py --download --format webui --version v3

# Ladda upp ny ONNX
gh release create vision-v3 models/aether-ui-latest.onnx

# Uppdatera URL i Render Dashboard → server startar om → ny modell aktiv
```

### Render-konfiguration (render.yaml)

```yaml
services:
  - type: web
    name: aether-agent-api
    runtime: docker
    plan: free
    dockerfilePath: ./Dockerfile
    envVars:
      - key: PORT
        value: 10000
      - key: AETHER_MODEL_URL
        sync: false  # Sätt manuellt i Dashboard
    healthCheckPath: /health
```

Dockerfile bygger med `--features server,vision` så vision-koden ingår.

---

## 8. Använda modellen via API

### Endpoint: `/api/vision/parse` (server-modell — rekommenderat)

Kräver att `AETHER_MODEL_URL` eller `AETHER_MODEL_PATH` är satt.
Klienten behöver INTE skicka modellen.

```bash
curl -X POST http://localhost:3000/api/vision/parse \
  -H "Content-Type: application/json" \
  -d '{
    "png_base64": "<base64-encoded PNG>",
    "goal": "find add to cart button"
  }'
```

### Endpoint: `/api/parse-screenshot` (klient-modell — legacy)

Klienten skickar modellen med varje request:

```python
import base64, requests

with open('models/aether-ui-latest.onnx', 'rb') as f:
    model_b64 = base64.b64encode(f.read()).decode()

with open('screenshot.png', 'rb') as f:
    png_b64 = base64.b64encode(f.read()).decode()

resp = requests.post('http://localhost:3000/api/parse-screenshot', json={
    'png_base64': png_b64,
    'model_base64': model_b64,
    'goal': 'find login button'
})
print(resp.json())
```

### Svar (båda endpoints)

```json
{
  "elements": [
    {
      "class": "button",
      "confidence": 0.87,
      "bbox": { "x": 120, "y": 340, "w": 200, "h": 48 },
      "relevance": 0.92
    }
  ],
  "model_version": "v2",
  "inference_ms": 45
}
```

---

## 9. Iterationsloop

```
Kör 1: Syntetiskt (v1) → mAP@50: 0.35
   → Under 0.5 → behöver riktigt data

Kör 2: + Rico (v2) → mAP@50: 0.62
   → Kolla confusion_matrix.png
   → button/link förväxlas → lägg till fler exempel

Kör 3: + WebUI (v3) → mAP@50: 0.74
   → Bra! Men input-fält missar ofta
   → Annotera 200 egna screenshots med inputs

Kör 4: + Custom (v4) → mAP@50: 0.83
   → Produktion! Deploy till Render.
```

**Tumregel:** mAP@50 > 0.70 = användbar. > 0.80 = produktionsklar.

---

## 10. Vanliga problem

### Träning

| Problem | Orsak | Lösning |
|---------|-------|---------|
| Rico-extrahering hänger (30+ min) | WSL /mnt/c/ är långsam | Flytta repo till `~/` (native WSL) |
| `DeprecationWarning: tar filter` | Python 3.12+ | Ignorera — fungerar ändå |
| mAP platåar | Behöver mer varierad data | Lägg till egna screenshots |
| Overfit (train-loss ner, val-loss upp) | För lite data | Mer data, inte fler epochs |
| En klass dålig, resten bra | För få exempel | Minst 100 bilder per klass |
| GPU out of memory | Batch för stor | `--batch 8` eller `--batch 4` |
| Långsam CPU-träning | Inget GPU | Minska epochs: `--epochs 50` för test |

### Deploy

| Problem | Orsak | Lösning |
|---------|-------|---------|
| 503 på /api/vision/parse | Ingen modell laddad | Sätt AETHER_MODEL_URL i Render |
| Modell laddar inte | URL kräver auth | Använd publik URL (GitHub Release) |
| Render build timeout | Vision-feature kompilerar långsamt | Free tier har 15 min limit — upgradera eller pre-builda |

---

## 11. Referens

### Standard-klasser (10 st)
```
button, input, link, text, image, checkbox, select, radio, heading, container
```

### Extended-klasser (16 st, `--extended-classes`)
```
+ price, cta, product_card, nav, search, form
```

### Datasets

| Dataset | Storlek | Format | Beskrivning |
|---------|---------|--------|-------------|
| Syntetiskt | 200 bilder | YOLO | Genererade UI-layouter |
| Rico | 66K bilder | Rico → YOLO | Android-skärmdumpar |
| WebUI | 7K bilder | WebUI → YOLO | Web-skärmdumpar |
| Custom | Valfritt | YOLO | Egna annoterade screenshots |

### Textheuristik (automatisk vid Rico-konvertering)
- Pris: `"129 kr"`, `"$19.99"` → **price** (extended) / **text** (standard)
- CTA: `"Buy"`, `"Lägg i kundvagn"` → **cta** (extended) / **button** (standard)
- Rubriker: kort, stor bokstav → **heading**

### Rekommenderad träningsordning
```
COCO (pretrain, inbyggt i YOLOv8) → Rico (UI-struktur) → WebUI (web-layout) → Custom (agent-semantik)
```

### Alla kommandon

```bash
# Syntetiskt (snabbtest)
python tools/train_vision.py --download-starter --version v1

# Rico
python tools/train_vision.py --download --format rico --version v2

# WebUI (bygger vidare automatiskt)
python tools/train_vision.py --download --format webui --version v3

# Eget dataset
python tools/train_vision.py --dataset ./my-data --version v4

# Bara ladda ner (utan träning)
python tools/train_vision.py --download-only --format rico

# Starta från scratch (ignorera auto-chain)
python tools/train_vision.py --download --format rico --version v2 --fresh

# Specifik bas-modell
python tools/train_vision.py --dataset ./data --version v5 \
  --model-base runs/detect/aether-ui-v2/weights/best.pt

# Extended classes
python tools/train_vision.py --download --format rico --version v2 --extended-classes

# GPU/CPU override
python tools/train_vision.py --download --format rico --version v2 --device cpu
```
