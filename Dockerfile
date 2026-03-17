FROM rust:1.88-slim as builder

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src/ src/
COPY benches/ benches/

# Build the server binary with release optimizations (vision = YOLOv8 inference)
RUN cargo build --release --features server,vision --bin aether-server

# ─── Runtime stage ────────────────────────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/aether-server /usr/local/bin/aether-server

ENV PORT=10000

# Vision-modell: sätt AETHER_MODEL_URL till publik URL (t.ex. GitHub Release)
# eller AETHER_MODEL_PATH till lokal fil i containern.
# Servern laddar modellen vid startup och exponerar /api/vision/parse.
# ENV AETHER_MODEL_URL=https://github.com/.../aether-ui-latest.onnx

EXPOSE 10000

CMD ["aether-server"]
