#!/bin/bash
# Reconvert datasets from 10 → 22 classes without re-downloading.
# Removes marker + YOLO output, keeps raw snapshot data intact.

set -e

DATASETS=("waveui_raw" "showui-web_raw" "webclick_raw" "yashjain_raw")
FORMATS=("waveui" "showui-web" "webclick" "yashjain")

echo "=== Rensar YOLO-output (behåller snapshot-rådata) ==="
for d in "${DATASETS[@]}"; do
  dir="dataset/$d"
  if [ -d "$dir" ]; then
    rm -f "$dir/.hf_download_complete"
    rm -rf "$dir/images" "$dir/labels" "$dir/data.yaml"
    echo "  Rensat: $dir"
  else
    echo "  Saknas: $dir (skippad)"
  fi
done

echo ""
echo "=== Konverterar om med 22 klasser ==="
for fmt in "${FORMATS[@]}"; do
  echo ""
  echo "--- $fmt ---"
  python tools/train_vision.py --download-only --format "$fmt"
done

echo ""
echo "=== Verifiering ==="
grep "nc:" dataset/*/data.yaml 2>/dev/null || echo "Inga data.yaml hittades"
