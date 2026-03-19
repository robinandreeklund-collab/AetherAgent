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
Fas 4: + egna screenshots (finjustering)
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

## Övriga datasets (ej integrerade, för manuell användning)

| Dataset | Storlek | HuggingFace | Användning |
|---------|---------|-------------|------------|
| UGround | 10M element, 95% web | ICLR 2025 Oral | Störst web-grounding |
| AGUVis Stage 1 | 4.2M samples | `xlangai/aguvis-stage1` | +41% på ScreenSpot-v2 |
| Klarna Product Pages | 51.7K e-handelssidor | `klarna/product-page-dataset` | price/cta/product_card |
| Hcompany/WebClick | 100+ sajter | `Hcompany/WebClick` | Intent-annoterat |
| Explorer-Web | 720K screenshots, 33M element | ACL 2025 | Massivt trajectory-data |
| Roboflow Screenshots | 1K+ web | `public.roboflow.com` | 8 klasser, YOLO-format |

### Benchmarks (validering, ej träning)

| Benchmark | HuggingFace | Vad |
|-----------|-------------|-----|
| ScreenSpot-v2 | `Voxel51/ScreenSpot-v2` | Standard GUI-grounding eval |
| ScreenSpot-Pro | `Voxel51/ScreenSpot-Pro` | Svår: element = 0.07% av bild |
| GroundUI-18k | `Voxel51/GroundUI-18k` | Cross-platform (ICLR 2025) |
| ServiceNow/ui-vision | `ServiceNow/ui-vision` | 83 appar, MIT-licens |
