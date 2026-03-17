FROM rust:1.88-slim as builder

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src/ src/
COPY benches/ benches/

# Build the server binary with release optimizations
RUN cargo build --release --features server --bin aether-server

# ─── Runtime stage ────────────────────────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/aether-server /usr/local/bin/aether-server

ENV PORT=10000

EXPOSE 10000

CMD ["aether-server"]
