# ─── Build stage ──────────────────────────────────────────────────────────────
FROM rust:1.88-slim AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev python3 \
    # Blitz rendering deps: fontconfig for font discovery
    libfontconfig1-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src/ src/
COPY benches/ benches/

# Build both server and MCP binaries with all features
# server = axum + blitz + js-eval + fetch
# vision = YOLOv8 inference via rten
# mcp = MCP stdio server (for local use, also included in image)
RUN cargo build --release --features server,vision --bin aether-server \
    && cargo build --release --features mcp,vision --bin aether-mcp

# ─── Runtime stage ────────────────────────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    python3-minimal \
    python3-pip \
    # Fonts for Blitz rendering (system fallback fonts)
    fonts-liberation \
    fonts-noto-color-emoji \
    fonts-dejavu-core \
    # Fontconfig runtime library (required by Blitz for font discovery)
    libfontconfig1 \
    # curl for health checks
    curl \
    # rten-convert for ONNX → rten model conversion at startup
    && pip3 install --no-cache-dir --break-system-packages rten-convert \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/aether-server /usr/local/bin/aether-server
COPY --from=builder /app/target/release/aether-mcp /usr/local/bin/aether-mcp

ENV PORT=10000

# Vision model: set AETHER_MODEL_URL to a public URL (e.g. GitHub Release)
# or AETHER_MODEL_PATH to a local file path inside the container.
# The server loads the model at startup and exposes /api/vision/parse.
# Supports both .onnx and .rten formats (auto-converts ONNX → rten at startup).
# ENV AETHER_MODEL_URL=https://github.com/.../aether-ui-latest.onnx

EXPOSE 10000

# Health check for Render and other platforms
HEALTHCHECK --interval=30s --timeout=5s --retries=3 \
    CMD curl -f http://localhost:10000/health || exit 1

CMD ["aether-server"]
