Träningsstrategi
1. Dataset — börja smått, skala upp
Steg 1: Syntetiskt (det du kör nu)     → 200 bilder  → mAP ~0.3-0.5
Steg 2: + Roboflow/Rico riktiga bilder → 1000 bilder → mAP ~0.6-0.7
Steg 3: + Egna screenshots av målsajter → 2000 bilder → mAP ~0.8+

Blanda inte olika datasets rakt av. Konvertera alla till samma YOLO-format och samma klasslista (UI_CLASSES). Rico har t.ex. fler klasser — mappa dem till dina 10.

2. Hur mycket träna?
Parameter Startpunkt  Med riktigt data
Epochs  150 (ditt default)  200-300
Batch 32 (bra för 5090) 32
Imgsz 640 640
Early stopping är inbyggt i Ultralytics — den slutar automatiskt om mAP inte förbättras på 50 epochs (patience=50). Så 300 epochs betyder inte att den kör alla 300.

3. När är modellen "bra nog"?
Kolla dessa metrics som skriptet skriver ut i Steg 6 (validate):

Metric  Dåligt  OK  Bra Produktion
mAP@50  <0.40 0.40-0.60 0.60-0.80 >0.80
mAP@50-95 <0.25 0.25-0.40 0.40-0.55 >0.55
Precision <0.50 0.50-0.70 0.70-0.85 >0.85
Recall  <0.50 0.50-0.70 0.70-0.85 >0.85
4. Hur ser man om det blir bättre?
Ditt skript sparar plots i runs/detect/aether-ui-v1/. Kolla dessa filer:

results.png — mAP/loss per epoch. Loss ska gå ner, mAP ska gå upp.
confusion_matrix.png — vilka klasser förväxlas
val_batch0_pred.png — visuella prediktioner på val-data
5. Iterationsloop
Kör 1: Syntetiskt data (v1)
  → Kolla mAP@50
  → Om < 0.5: behöver riktigt data

Kör 2: Lägg till riktigt data (v2)
  ./tools/train.sh --dataset ./dataset-v2 --version v2
  → Jämför mAP@50 mot v1

Kör 3: Analysera confusion matrix
  → Om button/link förväxlas: lägg till fler exempel av dem
  → Kör igen med --version v3

6. Vanliga fallgropar
Overfit: Train-loss sjunker men val-loss stiger → mer data, inte fler epochs
Alla klasser bra utom en: Lägg till fler bilder av just den klassen (minst 100 per klass)
mAP platåar: Pröva --imgsz 1280 (dubbel upplösning, långsammare men bättre för små element)
Sammanfattning
Kör det syntetiska först. Kolla mAP@50 i output. Är du under 0.5 → lägg till riktigt data från Roboflow. Kör --version v2. Jämför. Upprepa tills mAP@50 > 0.7 — då har du en användbar modell.



# Ladda ner Rico (66K Android-skärmdumpar) + konvertera + träna v2:
python tools/train_vision.py --download --format rico --version v2

# Ladda ner WebUI (7K web-skärmdumpar) + konvertera + träna v2:
python tools/train_vision.py --download --format webui --version v2

# Finjustera v1-modellen med Rico-data:
python tools/train_vision.py --download --format rico \
  --model-base runs/detect/aether-ui-v1/weights/best.pt --version v2

# Bara ladda ner + konvertera (utan träning):
python tools/train_vision.py --download-only --format rico




Rico — tre format stöds nu
Format  Struktur  Typiskt från
A combined/ (componentLabel + bounds tree)  Rico Semantic
B semantic_annotations/ Rico toolkit
C views[] med viewType + bounds + text  Ditt analysförslag

Textheuristik (automatisk)
Pris: "129 kr", "$19.99" → price (extended) eller behåller text (standard)
CTA: "Buy", "Lägg i kundvagn" → cta (extended) eller uppgraderar text → button (standard)
Rubriker: Kort, stor bokstav, inga meningar → heading
Utökade klasser (--extended-classes)
# Standard: 10 klasser (button, input, link, ...)
./tools/train.sh --dataset ./rico --format rico

# Extended: 16 agentsemantiska klasser
./tools/train.sh --dataset ./rico --format rico --extended-classes

De 6 nya: price, cta, product_card, nav, search, form

Din pipeline-idé
COCO (pretrain) → WebUI 7k (web layout) → Rico (UI structure) → Custom (agent semantics)

Fungerar nu direkt — kör varje steg med --format och det konverteras till samma YOLO-format. Vision-detektionerna kopplas sedan till AetherAgents semantic scoring, precis som du beskrev:

YOLO → candidates → Aether semantic scoring → final selection