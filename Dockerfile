# ─── Build stage ──────────────────────────────────────────────────────────────
# Ubuntu 24.04 har glibc 2.39 — krävs av ORT:s prebuilt binaries (glibc 2.38+)
FROM ubuntu:24.04 AS builder

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y --no-install-recommends \
    curl ca-certificates \
    pkg-config libssl-dev python3 \
    # Blitz rendering deps: fontconfig for font discovery
    libfontconfig1-dev \
    # ORT (ONNX Runtime) kräver libstdc++ vid länkning
    g++ \
    # Build essentials for cc linker
    build-essential \
    # libclang for bindgen (used by stylo/servo dependencies)
    libclang-dev \
    && rm -rf /var/lib/apt/lists/*

# Installera Rust via rustup (CACHE_BUST forces rebuild when version changes)
ARG RUST_VERSION=1.94.1
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain ${RUST_VERSION}
ENV PATH="/root/.cargo/bin:${PATH}"

WORKDIR /app

# ── Dependency cache layer ────────────────────────────────────────────────────
# Copy only Cargo manifests first, create stub source files, and build deps.
# This layer is cached as long as Cargo.toml/Cargo.lock don't change.
# Rebuilds of our code reuse the cached ~300 compiled dependencies.
COPY Cargo.toml Cargo.lock ./
COPY vendor/ vendor/

# Create minimal stubs so cargo can resolve and compile all dependencies
# Also stub examples/ (excluded by .dockerignore but declared in Cargo.toml)
RUN mkdir -p src/bin benches examples && \
    echo "pub fn stub() {}" > src/lib.rs && \
    echo "fn main() {}" > src/bin/server.rs && \
    echo "fn main() {}" > src/bin/mcp_server.rs && \
    echo "fn main() {}" > benches/bench_main.rs && \
    echo "fn main() {}" > examples/render_demo.rs

# Pre-compile all dependencies (this is the slow step, ~10-15 min first time)
# Subsequent builds reuse this layer if only src/ code changed.
RUN cargo build --profile server-release --features server,vision,cdp --bin aether-server 2>&1 || true && \
    cargo build --profile server-release --features mcp,vision,cdp --bin aether-mcp 2>&1 || true

# Remove the stub artifacts so cargo rebuilds our actual code
RUN rm -rf src/ benches/ && \
    rm -f target/server-release/aether-server target/server-release/aether-mcp && \
    rm -f target/server-release/deps/aether_agent-* && \
    rm -f target/server-release/deps/libaether_agent-*

# ── Build our code (fast: only our crate, deps are cached) ───────────────────
COPY src/ src/
COPY benches/ benches/
COPY dashboard.html dashboard.html
COPY landing-pages/ landing-pages/
# Stub examples (excluded by .dockerignore but required by Cargo.toml [[example]])
RUN mkdir -p examples && echo "fn main() {}" > examples/render_demo.rs

RUN cargo build --profile server-release --features server,vision,cdp --bin aether-server && \
    cargo build --profile server-release --features mcp,vision,cdp --bin aether-mcp

# ─── Runtime stage ────────────────────────────────────────────────────────────
# Måste matcha builder:ns glibc (2.39), annars kraschar binären vid start
FROM ubuntu:24.04

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    python3-minimal \
    # Fonts for Blitz rendering (system fallback fonts)
    fonts-liberation \
    fonts-noto-color-emoji \
    fonts-dejavu-core \
    # Fontconfig runtime library (required by Blitz for font discovery)
    libfontconfig1 \
    # curl for health checks
    curl \
    # Chromium for Tier 2 CDP rendering (headless Chrome screenshots)
    chromium-browser \
    # ORT (ONNX Runtime) kräver libstdc++ vid runtime
    libstdc++6 \
    && rm -rf /var/lib/apt/lists/*

# Chromium sökväg för headless_chrome crate
ENV CHROME_PATH=/usr/bin/chromium-browser

COPY --from=builder /app/target/server-release/aether-server /usr/local/bin/aether-server
COPY --from=builder /app/target/server-release/aether-mcp /usr/local/bin/aether-mcp

ENV PORT=10000

# Persistence: SQLite database path (mount persistent disk here on Render)
ENV AETHER_DB_PATH=/data/aether.db
RUN mkdir -p /data

# Vision model: set AETHER_MODEL_URL to a public URL (e.g. GitHub Release)
# or AETHER_MODEL_PATH to a local file path inside the container.
# The server loads the model at startup and exposes /api/vision/parse.
# ORT laddar .onnx-modellen direkt (ingen konvertering behövs).
# ENV AETHER_MODEL_URL=https://github.com/.../aether-ui-latest.onnx

EXPOSE 10000

# Health check for Render and other platforms
HEALTHCHECK --interval=30s --timeout=5s --retries=3 \
    CMD curl -f http://localhost:10000/health || exit 1

CMD ["aether-server"]
