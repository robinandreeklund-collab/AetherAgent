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
    pip install ultralytics pillow requests tqdm pyyaml
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

UI_CLASSES = [
    "button", "input", "link", "icon", "text",
    "image", "checkbox", "radio", "select", "heading",
]

# RTX 5090 optimized defaults (24 GB VRAM)
DEFAULT_EPOCHS = 150
DEFAULT_BATCH = 32
DEFAULT_IMGSZ = 640
DEFAULT_MODEL_BASE = "yolov8n.pt"  # nano — keeps ONNX < 6 MB
DEFAULT_PROJECT = "runs/detect"
DEFAULT_NAME = "aether-ui"

BANNER = r"""
 ╔═══════════════════════════════════════════════════════════════╗
 ║          AetherAgent Vision Training Pipeline                ║
 ║          YOLOv8-nano · Ultralytics · RTX 5090                ║
 ╚═══════════════════════════════════════════════════════════════╝
"""


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def log(msg: str, level: str = "INFO"):
    colors = {"INFO": "\033[36m", "OK": "\033[32m", "WARN": "\033[33m", "ERR": "\033[31m", "STEP": "\033[35m"}
    reset = "\033[0m"
    color = colors.get(level, "")
    print(f"{color}[{level}]{reset} {msg}")


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


def check_gpu():
    """Check CUDA availability and GPU info."""
    try:
        import torch
        if torch.cuda.is_available():
            name = torch.cuda.get_device_name(0)
            mem = torch.cuda.get_device_properties(0).total_mem / (1024**3)
            log(f"GPU: {name} ({mem:.1f} GB VRAM)", "OK")
            return True
        else:
            log("No CUDA GPU detected — training will use CPU (slow!)", "WARN")
            return False
    except ImportError:
        log("PyTorch not installed — cannot detect GPU", "WARN")
        return False


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
# Step 1: Dataset Preparation
# ---------------------------------------------------------------------------

def create_data_yaml(dataset_dir: Path, output_path: Path) -> Path:
    """Create or validate data.yaml for the dataset."""
    import yaml

    yaml_path = dataset_dir / "data.yaml"
    if yaml_path.exists():
        with open(yaml_path) as f:
            data = yaml.safe_load(f)
        # Validate
        if "names" in data and "train" in data:
            nc = len(data["names"]) if isinstance(data["names"], list) else len(data["names"].values())
            log(f"Existing data.yaml found: {nc} classes", "OK")
            return yaml_path
        log("data.yaml exists but is incomplete, regenerating", "WARN")

    # Generate data.yaml
    train_imgs = dataset_dir / "images" / "train"
    val_imgs = dataset_dir / "images" / "val"
    test_imgs = dataset_dir / "images" / "test"

    if not train_imgs.exists():
        log(f"Expected directory not found: {train_imgs}", "ERR")
        log("Dataset must have structure: dataset/images/{{train,val}}/", "ERR")
        sys.exit(1)

    # Count images
    train_count = len(list(train_imgs.glob("*.png")) + list(train_imgs.glob("*.jpg")))
    val_count = len(list(val_imgs.glob("*.png")) + list(val_imgs.glob("*.jpg"))) if val_imgs.exists() else 0

    data = {
        "path": str(dataset_dir.resolve()),
        "train": "images/train",
        "val": "images/val" if val_imgs.exists() else "images/train",
        "test": "images/test" if test_imgs.exists() else "",
        "nc": len(UI_CLASSES),
        "names": {i: name for i, name in enumerate(UI_CLASSES)},
    }

    with open(yaml_path, "w") as f:
        yaml.dump(data, f, default_flow_style=False)

    log(f"Created data.yaml: {train_count} train, {val_count} val images", "OK")
    return yaml_path


def auto_split_dataset(dataset_dir: Path, val_ratio: float = 0.15, test_ratio: float = 0.05):
    """Auto-split a flat image directory into train/val/test."""
    import random

    flat_imgs = dataset_dir / "images"
    if not flat_imgs.exists():
        flat_imgs = dataset_dir

    all_images = list(flat_imgs.glob("*.png")) + list(flat_imgs.glob("*.jpg"))
    if not all_images:
        log("No images found for splitting", "ERR")
        sys.exit(1)

    # Check if already split
    if (flat_imgs / "train").exists():
        log("Dataset already split into train/val/test", "OK")
        return

    log(f"Auto-splitting {len(all_images)} images ({1 - val_ratio - test_ratio:.0%}/{val_ratio:.0%}/{test_ratio:.0%})", "STEP")

    random.shuffle(all_images)
    n_val = int(len(all_images) * val_ratio)
    n_test = int(len(all_images) * test_ratio)
    n_train = len(all_images) - n_val - n_test

    splits = {
        "train": all_images[:n_train],
        "val": all_images[n_train:n_train + n_val],
        "test": all_images[n_train + n_val:],
    }

    labels_dir = dataset_dir / "labels"

    for split_name, images in splits.items():
        img_dir = dataset_dir / "images" / split_name
        lbl_dir = dataset_dir / "labels" / split_name
        img_dir.mkdir(parents=True, exist_ok=True)
        lbl_dir.mkdir(parents=True, exist_ok=True)

        for img_path in images:
            # Move image
            shutil.move(str(img_path), str(img_dir / img_path.name))
            # Move corresponding label if exists
            label_name = img_path.stem + ".txt"
            label_path = labels_dir / label_name
            if label_path.exists():
                shutil.move(str(label_path), str(lbl_dir / label_name))
            else:
                # Empty label file = negative sample
                (lbl_dir / label_name).touch()

        log(f"  {split_name}: {len(images)} images", "INFO")


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
) -> Path:
    """Train YOLOv8-nano with Ultralytics."""
    from ultralytics import YOLO

    log(f"Starting training: {epochs} epochs, batch={batch}, imgsz={imgsz}", "STEP")
    log(f"Base model: {model_base}", "INFO")

    model = YOLO(model_base)

    results = model.train(
        data=str(data_yaml),
        epochs=epochs,
        imgsz=imgsz,
        batch=batch,
        project=project,
        name=name,
        exist_ok=True,
        resume=resume,
        # RTX 5090 optimizations
        workers=8,
        amp=True,          # Mixed precision — huge speedup on Ada/Blackwell
        cache="ram",       # Cache images in RAM (fast with 24GB+ system RAM)
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
        # Logging
        verbose=True,
        plots=True,
    )

    best_pt = Path(project) / name / "weights" / "best.pt"
    if not best_pt.exists():
        log(f"Training finished but best.pt not found at {best_pt}", "ERR")
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

def export_onnx(best_pt: Path, imgsz: int) -> Path:
    """Export best.pt → ONNX (opset 17, simplified)."""
    from ultralytics import YOLO

    log("Exporting to ONNX format...", "STEP")

    model = YOLO(str(best_pt))
    onnx_path_str = model.export(
        format="onnx",
        imgsz=imgsz,
        opset=17,
        simplify=True,
        dynamic=False,
    )
    onnx_path = Path(onnx_path_str)

    size_mb = onnx_path.stat().st_size / (1024 * 1024)
    log(f"ONNX exported: {onnx_path} ({size_mb:.1f} MB)", "OK")

    if size_mb > 6:
        log(f"Model is {size_mb:.1f} MB (> 6 MB target). Consider pruning or using yolov8n.", "WARN")

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


def generate_report(
    dataset_dir: Path,
    best_pt: Path,
    onnx_path: Path,
    metrics: dict,
    deploy_path: Path,
    version: str,
):
    """Generate a summary report."""
    report = {
        "version": version,
        "dataset": str(dataset_dir),
        "model_pt": str(best_pt),
        "model_onnx": str(onnx_path),
        "deployed_to": str(deploy_path),
        "onnx_size_mb": round(onnx_path.stat().st_size / (1024 * 1024), 2),
        "metrics": metrics,
        "classes": UI_CLASSES,
        "input_size": DEFAULT_IMGSZ,
        "timestamp": time.strftime("%Y-%m-%d %H:%M:%S"),
    }

    report_path = deploy_path.parent / f"report-{version}.json"
    with open(report_path, "w") as f:
        json.dump(report, f, indent=2)

    print("\n" + "=" * 60)
    log("TRAINING COMPLETE", "OK")
    print("=" * 60)
    print(f"  Model version:  {version}")
    print(f"  ONNX path:      {onnx_path}")
    print(f"  ONNX size:      {report['onnx_size_mb']} MB")
    print(f"  mAP@50:         {metrics.get('map50', 'N/A'):.4f}")
    print(f"  mAP@50-95:      {metrics.get('map5095', 'N/A'):.4f}")
    print(f"  Precision:      {metrics.get('precision', 'N/A'):.4f}")
    print(f"  Recall:         {metrics.get('recall', 'N/A'):.4f}")
    print(f"  Report:         {report_path}")
    print("=" * 60)
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

def interactive_mode():
    """Step-by-step interactive wizard."""
    print(BANNER)
    print("This wizard will guide you through the full training pipeline.\n")

    # Step 1: Dataset
    print("[1/5] DATASET")
    print("  a) I have a labeled dataset ready")
    print("  b) Download starter dataset (synthetic, for testing)")
    choice = input("  Choice [a/b]: ").strip().lower()

    if choice == "b":
        dataset_dir = Path("dataset")
        download_starter_dataset(dataset_dir)
    else:
        path = input("  Dataset path: ").strip()
        dataset_dir = Path(path)
        if not dataset_dir.exists():
            log(f"Path not found: {dataset_dir}", "ERR")
            sys.exit(1)

    # Step 2: Config
    print("\n[2/5] TRAINING CONFIG")
    epochs = input(f"  Epochs [{DEFAULT_EPOCHS}]: ").strip()
    epochs = int(epochs) if epochs else DEFAULT_EPOCHS
    batch = input(f"  Batch size [{DEFAULT_BATCH}]: ").strip()
    batch = int(batch) if batch else DEFAULT_BATCH
    version = input("  Model version [v1]: ").strip() or "v1"

    # Step 3: Confirm
    print(f"\n  Dataset:  {dataset_dir}")
    print(f"  Epochs:   {epochs}")
    print(f"  Batch:    {batch}")
    print(f"  Version:  {version}")
    confirm = input("\n  Start training? [Y/n]: ").strip().lower()
    if confirm == "n":
        print("Cancelled.")
        sys.exit(0)

    # Run pipeline
    run_pipeline(
        dataset_dir=dataset_dir,
        epochs=epochs,
        batch=batch,
        version=version,
        server_url="http://localhost:3000",
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
):
    """Run the full training pipeline."""
    print(BANNER)

    if deploy_dir is None:
        deploy_dir = Path("models")

    # Pre-flight
    log("Pre-flight checks...", "STEP")
    ensure_deps()
    has_gpu = check_gpu()

    if not has_gpu:
        log("Reducing batch size to 8 for CPU training", "WARN")
        batch = min(batch, 8)

    # Step 1: Dataset
    log("Step 1/6: Preparing dataset...", "STEP")
    data_yaml = create_data_yaml(dataset_dir, dataset_dir / "data.yaml")

    # Step 2: Train
    log("Step 2/6: Training YOLOv8-nano...", "STEP")
    best_pt = train_model(
        data_yaml=data_yaml,
        epochs=epochs,
        batch=batch,
        imgsz=imgsz,
        model_base=model_base,
        project=DEFAULT_PROJECT,
        name=f"{DEFAULT_NAME}-{version}",
    )

    # Step 3: Validate
    log("Step 3/6: Validating model...", "STEP")
    metrics = validate_model(best_pt, data_yaml, imgsz)

    # Step 4: Export ONNX
    log("Step 4/6: Exporting to ONNX...", "STEP")
    onnx_path = export_onnx(best_pt, imgsz)

    # Step 5: Deploy
    log("Step 5/6: Deploying model...", "STEP")
    deploy_path = copy_to_deploy(onnx_path, deploy_dir, version)

    # Try rten conversion (optional)
    convert_rten(onnx_path)

    # Step 6: Verify
    if not skip_verify and server_url:
        log("Step 6/6: Verifying with AetherAgent API...", "STEP")
        verify_with_server(onnx_path, server_url)
    else:
        log("Step 6/6: Skipping API verification", "INFO")

    # Report
    generate_report(dataset_dir, best_pt, onnx_path, metrics, deploy_path, version)


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

def main():
    parser = argparse.ArgumentParser(
        description="AetherAgent Vision Training Pipeline — automated end-to-end",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  # Full pipeline with your dataset:
  python tools/train_vision.py --dataset ./my-labeled-data

  # Generate synthetic starter dataset + train:
  python tools/train_vision.py --download-starter

  # Export existing model:
  python tools/train_vision.py --export-only runs/detect/aether-ui-v1/weights/best.pt

  # Interactive wizard:
  python tools/train_vision.py --interactive
        """,
    )

    parser.add_argument("--dataset", type=Path, help="Path to labeled dataset directory")
    parser.add_argument("--download-starter", action="store_true", help="Download synthetic starter dataset")
    parser.add_argument("--epochs", type=int, default=DEFAULT_EPOCHS, help=f"Training epochs (default: {DEFAULT_EPOCHS})")
    parser.add_argument("--batch", type=int, default=DEFAULT_BATCH, help=f"Batch size (default: {DEFAULT_BATCH}, tuned for RTX 5090)")
    parser.add_argument("--imgsz", type=int, default=DEFAULT_IMGSZ, help=f"Image size (default: {DEFAULT_IMGSZ})")
    parser.add_argument("--version", type=str, default="v1", help="Model version tag (default: v1)")
    parser.add_argument("--model-base", type=str, default=DEFAULT_MODEL_BASE, help=f"Base model (default: {DEFAULT_MODEL_BASE})")
    parser.add_argument("--deploy-dir", type=Path, default=Path("models"), help="Model deployment directory")
    parser.add_argument("--server", type=str, default="http://localhost:3000", help="AetherAgent server URL for verification")
    parser.add_argument("--skip-verify", action="store_true", help="Skip API verification step")
    parser.add_argument("--interactive", action="store_true", help="Interactive step-by-step wizard")
    parser.add_argument("--export-only", type=Path, help="Only export .pt → ONNX (skip training)")
    parser.add_argument("--verify-only", type=Path, help="Only verify ONNX model against API")

    args = parser.parse_args()

    # Mode: Interactive
    if args.interactive:
        interactive_mode()
        return

    # Mode: Export only
    if args.export_only:
        ensure_deps()
        onnx_path = export_onnx(args.export_only, args.imgsz)
        deploy_path = copy_to_deploy(onnx_path, args.deploy_dir, args.version)
        convert_rten(onnx_path)
        log(f"Export complete: {deploy_path}", "OK")
        return

    # Mode: Verify only
    if args.verify_only:
        ensure_deps()
        verify_with_server(args.verify_only, args.server)
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
        )
        return

    # Mode: Full pipeline with dataset
    if args.dataset:
        if not args.dataset.exists():
            log(f"Dataset path not found: {args.dataset}", "ERR")
            sys.exit(1)
        run_pipeline(
            dataset_dir=args.dataset,
            epochs=args.epochs,
            batch=args.batch,
            imgsz=args.imgsz,
            model_base=args.model_base,
            version=args.version,
            server_url=args.server,
            deploy_dir=args.deploy_dir,
            skip_verify=args.skip_verify,
        )
        return

    # No mode specified
    parser.print_help()
    print("\nQuick start:")
    print("  python tools/train_vision.py --download-starter   # synthetic data + train")
    print("  python tools/train_vision.py --interactive         # step-by-step wizard")


if __name__ == "__main__":
    main()
