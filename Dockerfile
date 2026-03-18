# ─── Build stage ──────────────────────────────────────────────────────────────
FROM rust:1.88-slim AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev python3 \
    # Blitz rendering deps: fontconfig for font discovery
    libfontconfig1-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# ── Dependency cache layer ────────────────────────────────────────────────────
# Copy only Cargo manifests first, create stub source files, and build deps.
# This layer is cached as long as Cargo.toml/Cargo.lock don't change.
# Rebuilds of our code reuse the cached ~300 compiled dependencies.
COPY Cargo.toml Cargo.lock ./

# Create minimal stubs so cargo can resolve and compile all dependencies
RUN mkdir -p src/bin && \
    echo "pub fn stub() {}" > src/lib.rs && \
    echo "fn main() {}" > src/bin/server.rs && \
    echo "fn main() {}" > src/bin/mcp_server.rs && \
    echo "fn main() {}" > benches/bench_main.rs

# Pre-compile all dependencies (this is the slow step, ~10-15 min first time)
# Subsequent builds reuse this layer if only src/ code changed.
RUN cargo build --profile server-release --features server,vision --bin aether-server 2>&1 || true && \
    cargo build --profile server-release --features mcp,vision --bin aether-mcp 2>&1 || true

# Remove the stub artifacts so cargo rebuilds our actual code
RUN rm -rf src/ benches/ && \
    rm -f target/server-release/aether-server target/server-release/aether-mcp && \
    rm -f target/server-release/deps/aether_agent-* && \
    rm -f target/server-release/deps/libaether_agent-*

# ── Build our code (fast: only our crate, deps are cached) ───────────────────
COPY src/ src/
COPY benches/ benches/

RUN cargo build --profile server-release --features server,vision --bin aether-server && \
    cargo build --profile server-release --features mcp,vision --bin aether-mcp

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

COPY --from=builder /app/target/server-release/aether-server /usr/local/bin/aether-server
COPY --from=builder /app/target/server-release/aether-mcp /usr/local/bin/aether-mcp

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
