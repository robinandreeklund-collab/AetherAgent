FROM rust:1.88-slim as builder

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src/ src/
COPY benches/ benches/

# Build the server binary with release optimizations (vision = YOLOv8 inference)
RUN cargo build --release --features server,vision --bin aether-server

# ─── Runtime stage ────────────────────────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    python3-minimal \
    python3-pip \
    chromium \
    fonts-liberation \
    fonts-noto-color-emoji \
    && pip3 install --no-cache-dir --break-system-packages rten-convert \
    && rm -rf /var/lib/apt/lists/*

# Chromium headless för server-side screenshots
ENV CHROMIUM_PATH=/usr/bin/chromium

COPY --from=builder /app/target/release/aether-server /usr/local/bin/aether-server

ENV PORT=10000

# Vision-modell: sätt AETHER_MODEL_URL till publik URL (t.ex. GitHub Release)
# eller AETHER_MODEL_PATH till lokal fil i containern.
# Servern laddar modellen vid startup och exponerar /api/vision/parse.
# Stöder både .onnx och .rten format (auto-konverterar ONNX → rten vid startup).
# ENV AETHER_MODEL_URL=https://github.com/.../aether-ui-latest.onnx

EXPOSE 10000

CMD ["aether-server"]
