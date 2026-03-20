# AetherAgent Vision — 2026 Update

> YOLO26n, interaktiv modellväljare, 5 nya HuggingFace-datasets, metric-baserad early stopping.

---

## Modellväljare

Default är nu **YOLO26n** (inte YOLOv8n). NMS-free, 43% snabbare CPU.

### Interaktivt val

```bash
# Wizard (steg 1/6 frågar vilken modell)
python tools/train_vision.py --interactive

# Eller: lägg till --select-model i valfritt kommando
python tools/train_vision.py --download --format rico --select-model
```

Visar:
```
  Välj basmodell för träning:

   * 1) yolo26n.pt       YOLO26 nano  — NMS-free, edge-optimerad (REKOMMENDERAD)
     2) yolo26s.pt       YOLO26 small — högre precision
     3) yolo11n.pt       YOLO11 nano  — beprövad, stabil
     4) yolo11s.pt       YOLO11 small — mer precision
     5) yolov8n.pt       YOLOv8 nano  — legacy

  Modell [1-5, Enter=yolo26n.pt]:
```

### CLI utan prompt

```bash
python tools/train_vision.py --download --format rico --model-base yolo26s.pt
```

---

## Nya datasets (2025-2026)

Alla laddas ner via `--download --format <namn>`:

| Format | Dataset | Storlek | Källa | Beskrivning |
|--------|---------|---------|-------|-------------|
| `osatlas` | OS-Atlas Web | 270K screenshots, 3M+ element | `OS-Copilot/OS-Atlas-data` | ICLR 2025 Spotlight. Störst web-grounding corpus. |
| `guiactor` | GUI-Actor-Data | 1M screenshots, 10M element | `cckevinn/GUI-Actor-Data` | Merge av 6 datasets. Bbox supervision. |
| `showui-web` | ShowUI Web | 22K screenshots, 576K element | `Voxel51/ShowUI_Web` | CVPR 2025. Filtrerar bort statisk text. |
| `waveui` | WaveUI-25K | 25K curated samples | `agentsea/wave-ui-25k` | Dedup, LLM-berikad. Dec 2025. |
| `yashjain` | UI-Elements | YOLO-format direkt | `YashJain/UI-Elements-Detection-Dataset` | Web-fokus, balanserade klasser. Okt 2025. |
| `klarna` | Klarna Product Pages | 51.7K e-handelssidor | `klarna/product-page-dataset` | WTL-metadata + screenshots. E-handels-fokus. |
| `webclick` | Hcompany/WebClick | 1639 screenshots | `Hcompany/WebClick` | Intent-annoterat, ScreenSpot-format. |
| `roboflow-ui` | Roboflow UI Screenshots | 1800 bilder, 8 klasser | `webuiproject/ui-screenshots` | YOLO native, MIT-licens. |

### Existerande (redan tillgängliga)

| Format | Dataset | Storlek |
|--------|---------|---------|
| `rico` | Rico UI Screenshots | 66K Android-skärmdumpar |
| `webui` | WebUI-7K | 7K web-skärmdumpar |
| `coco` | COCO 2017 Val | 5K generella bilder |
| `--download-starter` | Syntetiskt | 200 genererade bilder |

### Kommandon

```bash
# Snabbaste sättet att komma igång (YOLO-format, inget att konvertera)
python tools/train_vision.py --download --format yashjain --version v1

# Störst web-dataset
python tools/train_vision.py --download --format osatlas --version v2

# Mest curated
python tools/train_vision.py --download --format waveui --version v3

# Agent-fokuserat (bara interaktiva element)
python tools/train_vision.py --download --format showui-web --version v4

# Massivt (10M element)
python tools/train_vision.py --download --format guiactor --version v5
```

---

## Rekommenderad träningsordning

```
Fas 1: yashjain (plug-and-play YOLO, snabb baseline)
Fas 2: + osatlas (3M web-element, bred coverage)
Fas 3: + showui-web (interaktiva element, agent-fokus)
Fas 4: + klarna + webclick + roboflow-ui (e-handel + intent + web-UI)
Fas 5: + egna screenshots (finjustering)
```

Auto-chain gör att varje fas bygger vidare automatiskt.

---

## Early Stopping (metric-baserad)

Istället för att köra fasta 300 epochs stoppar träningen automatiskt när målen nåtts.

### Två stopmekanismer

1. **Metric targets** — stoppar när `mAP@50 ≥ 0.65` OCH `mAP@50-95 ≥ 0.50`
2. **Patience plateau** — stoppar efter 30 epochs utan förbättring (Ultralytics inbyggd)

Båda körs parallellt. Whichever triggers first sparar och avslutar.

### CLI-flaggor

| Flagga | Default | Beskrivning |
|--------|---------|-------------|
| `--early-stop` | off | Aktivera metric-baserad early stopping |
| `--target-map50` | 0.65 | Mål mAP@50 (implicit `--early-stop`) |
| `--target-map5095` | 0.50 | Mål mAP@50-95 (implicit `--early-stop`) |
| `--patience` | 30 | Epochs utan förbättring före stopp (implicit `--early-stop`) |
| `--epochs` | 300 | Max epochs (övre gräns) |

### Kommandon

```bash
# Enklast — defaults (mAP@50≥0.65, mAP@50-95≥0.50, patience=30)
python tools/train_vision.py --dataset ./data --early-stop

# Högre krav
python tools/train_vision.py --dataset ./data --target-map50 0.70 --target-map5095 0.55

# Snabbare stopp vid platå
python tools/train_vision.py --dataset ./data --patience 20

# Kombinera med dataset-download
python tools/train_vision.py --download --format osatlas --early-stop --version v2
```

### Hur det fungerar

```
Epoch  1/300  mAP@50=0.12  mAP@50-95=0.08   ...tränar
Epoch 10/300  mAP@50=0.35  mAP@50-95=0.24   ← progress-rapport
Epoch 20/300  mAP@50=0.52  mAP@50-95=0.38   ← progress-rapport
...
Epoch 67/300  mAP@50=0.65  mAP@50-95=0.51   ← MÅL NÅTT! Stoppar.
```

En Ultralytics callback läser `results.csv` efter varje epoch och sätter `trainer.stop = True`
när båda målen nåtts. `best.pt` uppdateras löpande — den bästa modellen sparas alltid.

### Interaktiv wizard

`--interactive` frågar nu efter early-stop-config i steg 3/7:

```
[3/7] TRAINING CONFIG
  Epochs (max) [300]:
  Batch size [32]:
  Model version [v1]:

  Early stopping (stoppar automatiskt vid mål eller platå):
  Aktivera early-stop? [Y/n]:
  mAP@50 mål [0.65]:
  mAP@50-95 mål [0.5]:
  Patience (epochs utan förbättring) [30]:
```

### Varför dessa defaults?

| Mål | Tröskel | Motivering |
|-----|---------|------------|
| mAP@50 ≥ 0.65 | Stark detection — de flesta UI-element hittas korrekt |
| mAP@50-95 ≥ 0.50 | Bra bounding-box lokalisering — tight boxes |
| Patience = 30 | UI-modeller har ofta platåer runt epoch 40-60 innan ett andra hopp |

För produktionsmodeller med bättre data (osatlas + showui-web merged): sätt `--target-map50 0.75`.

---

## Klarna Product Pages (nytt)

**Källa:** [klarna/product-page-dataset](https://github.com/klarna/product-page-dataset) (AWS S3 + Zenodo)

51 700 e-handelssidor från 8 175 sajter i 8 regioner. WTL-snapshots med elementmetadata
(bounding boxes, font-storlekar). 5 annoterade element per sida:

| Klarna-label | Standard klass | Extended klass |
|--------------|---------------|----------------|
| Price | text (4) | price (10) |
| Name | heading (9) | heading (9) |
| Main picture | img (5) | img (5) |
| Add to cart | button (0) | cta (11) |
| Cart | button (0) | cta (11) |

Med `--extended-classes` får du bättre separation — Price mappas till dedikerad `price`-klass
och Add to cart/Cart till `cta`.

```bash
# Standard 10-klasser
python tools/train_vision.py --download --format klarna --version v10

# Med utökade klasser (rekommenderat för e-handel)
python tools/train_vision.py --download --format klarna --extended-classes --version v10e
```

---

## Hcompany/WebClick (nytt)

**Källa:** [Hcompany/WebClick](https://huggingface.co/datasets/Hcompany/WebClick) (HuggingFace)

1 639 engelska web-screenshots från 100+ sajter. ScreenSpot-format:
varje bild har exakt en annoterad element med naturligt-språk instruktion + exakt bbox.

Instruktioner klassificeras automatiskt via nyckelord:

| Nyckelord | UI-klass |
|-----------|----------|
| "click", "button", "submit", "press", "tap" | button (0) |
| "type", "enter", "search", "fill", "write" | textbox (1) |
| "link", "navigate", "go to", "open" | link (2) |
| "icon", "logo" | icon (3) |
| "image", "photo", "picture" | img (5) |
| "check", "toggle" | checkbox (6) |
| "select", "dropdown", "choose" | combobox (8) |
| "heading", "title" | heading (9) |
| Default (generellt klick) | button (0) |

```bash
python tools/train_vision.py --download --format webclick --version v11
```

---

## Roboflow UI Screenshots (nytt)

**Källa:** [webuiproject/ui-screenshots](https://universe.roboflow.com/webuiproject/ui-screenshots) (Roboflow Universe)

1 800 web-UI screenshots, MIT-licens. Nativt YOLO-format med automatisk klassommappning.

**Kräver** `ROBOFLOW_API_KEY` i miljön (gratis konto på roboflow.com).

| Roboflow-klass | AetherAgent-klass |
|----------------|-------------------|
| button (0) | button (0) |
| field (1) | textbox (1) |
| heading (2) | heading (9) |
| iframe (3) | img (5) |
| image (4) | img (5) |
| label (5) | text (4) |
| link (6) | link (2) |
| text (7) | text (4) |

```bash
export ROBOFLOW_API_KEY="din_nyckel_här"
python tools/train_vision.py --download --format roboflow-ui --version v12
```

---

## Alla nya datasets — kommandon

```bash
# Ladda ner var för sig (utan träning):
python tools/train_vision.py --download-only --format klarna
python tools/train_vision.py --download-only --format webclick
python tools/train_vision.py --download-only --format roboflow-ui

# Träna merged med enbart de tre nya:
python tools/train_vision.py --merge-datasets \
  dataset/klarna_raw \
  dataset/webclick_raw \
  dataset/roboflow-ui_raw \
  --version v_new --epochs 300 --early-stop

# Bästa mix — alla datasets:
python tools/train_vision.py --merge-datasets \
  dataset/yashjain_raw \
  dataset/showui-web_raw \
  dataset/waveui_raw \
  dataset/klarna_raw \
  dataset/webclick_raw \
  dataset/roboflow-ui_raw \
  --version v1.004 --early-stop --target-map50 0.70
```

---

## Övriga datasets (ej integrerade, för manuell användning)

| Dataset | Storlek | HuggingFace | Användning |
|---------|---------|-------------|------------|
| UGround | 10M element, 95% web | ICLR 2025 Oral | Störst web-grounding |
| AGUVis Stage 1 | 4.2M samples | `xlangai/aguvis-stage1` | +41% på ScreenSpot-v2 |
| Explorer-Web | 720K screenshots, 33M element | ACL 2025 | Massivt trajectory-data |

---

## Benchmarks — automatisk validering

Kör din tränade modell mot standardiserade GUI grounding benchmarks.
Laddar ner automatiskt, kör inference, och genererar detaljerade rapporter.

### Tillgängliga benchmarks

| Namn | Dataset | Samples | Metric | Beskrivning |
|------|---------|---------|--------|-------------|
| `screenspot-v2` | `Voxel51/ScreenSpot-v2` | 1 272 | Click Accuracy | Standard GUI grounding — web, desktop, mobile |
| `screenspot-pro` | `Voxel51/ScreenSpot-Pro` | 1 581 | Click Accuracy | Svår — professionella appar, element = 0.07% av bild |
| `groundui-18k` | `Voxel51/GroundUI-18k` | 18 026 | Click Accuracy + IoU | Cross-platform, 5 datakällor (ICLR 2025) |
| `ui-vision` | `ServiceNow/ui-vision` | ~5 000 | mAP + Click Accuracy | 83 desktop-appar, MIT-licens (ICML 2025) |

### Kommandon

```bash
# Kör alla benchmarks mot senaste modellen:
python tools/train_vision.py --benchmark all --version v1

# Specifika benchmarks:
python tools/train_vision.py --benchmark screenspot-v2 screenspot-pro --version v2

# Ange modell explicit:
python tools/train_vision.py --benchmark all --model-pt runs/detect/best.pt --version v3

# Lägre confidence-tröskel (hittar fler element, fler false positives):
python tools/train_vision.py --benchmark groundui-18k --benchmark-conf 0.15 --version v4

# Rapport-katalog:
python tools/train_vision.py --benchmark all --benchmark-report-dir reports/experiment-1/
```

### Rapportformat

Varje körning genererar två filer:

**`reports/benchmark-report-{version}.json`** — maskinläsbar:
```json
{
  "version": "v1",
  "model": "runs/detect/best.pt",
  "overall_click_accuracy": 0.423,
  "benchmarks": {
    "screenspot-v2": {
      "click_accuracy": 0.45,
      "avg_iou": 0.31,
      "iou_at_50": 0.28,
      "iou_at_25": 0.52,
      "platform_breakdown": {
        "web": {"click_accuracy": 0.48, "avg_iou": 0.33, "total": 436},
        "desktop": {"click_accuracy": 0.41, "avg_iou": 0.28, "total": 334},
        "mobile": {"click_accuracy": 0.44, "avg_iou": 0.31, "total": 502}
      },
      "label_breakdown": {
        "text": {"click_accuracy": 0.52, "avg_iou": 0.35, "total": 800},
        "icon": {"click_accuracy": 0.35, "avg_iou": 0.24, "total": 472}
      }
    }
  }
}
```

**`reports/benchmark-report-{version}.md`** — visuell rapport med tabeller:
- Sammanfattning per benchmark
- Per plattform (web/desktop/mobile)
- Per element-typ (text/icon)
- Jämför enkelt mellan versioner

### Metriker

| Metric | Beskrivning | Varför |
|--------|-------------|--------|
| **Click Accuracy** | Andel där modellens center-punkt träffar GT bbox | Standard ScreenSpot-metric — mäter "kan agenten klicka rätt?" |
| **Avg IoU** | Genomsnittlig Intersection over Union | Mäter bbox-precision — hur väl matchar detektionens storlek? |
| **IoU ≥ 0.50** | Andel med IoU > 50% | COCO-standard, strikt bbox-match |
| **IoU ≥ 0.25** | Andel med IoU > 25% | Tolerant — accepterar ungefärlig position |

### Rekommenderad benchmark-loop

```bash
# 1. Träna
python tools/train_vision.py --merge-datasets \
  dataset/yashjain_raw dataset/showui-web_raw dataset/klarna_raw \
  --version v1.004 --early-stop

# 2. Benchmarka
python tools/train_vision.py --benchmark all --version v1.004

# 3. Jämför med förra versionen
diff reports/benchmark-report-v1.003.json reports/benchmark-report-v1.004.json

# 4. Läs Markdown-rapporten
cat reports/benchmark-report-v1.004.md
```

### CLI-flaggor

| Flagga | Default | Beskrivning |
|--------|---------|-------------|
| `--benchmark NAME [NAME ...]` | — | Benchmark-namn eller `all` |
| `--model-pt PATH` | auto (senaste best.pt) | Sökväg till .pt modell |
| `--benchmark-conf` | 0.25 | Confidence-tröskel |
| `--benchmark-report-dir` | `reports/` | Rapport-katalog |
| `--version` | v1 | Versionstagg i rapporter |
