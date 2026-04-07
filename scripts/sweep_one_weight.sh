#!/bin/bash
# Usage: ./sweep_one_weight.sh 0.30
# Sets BM25_WEIGHT, builds server, starts it, runs test, stops it.
set -e

WEIGHT=${1:?Usage: sweep_one_weight.sh <weight>}
echo "=== Testing BM25_WEIGHT = $WEIGHT ==="

# Kill any existing server
pkill -f aether-server 2>/dev/null || true
sleep 1

# Set weight
sed -i "s/const BM25_WEIGHT: f32 = [0-9.]*;/const BM25_WEIGHT: f32 = $WEIGHT;/" src/resonance.rs
grep "const BM25_WEIGHT" src/resonance.rs

# Build
echo "Building..."
cargo build --bin aether-server --features server 2>&1 | tail -2

# Start server
echo "Starting server..."
cargo run --bin aether-server --features server > /tmp/aether-server.log 2>&1 &
SERVER_PID=$!

# Wait for server
for i in $(seq 1 20); do
    if curl -s http://localhost:3000/health > /dev/null 2>&1; then
        echo "Server ready"
        break
    fi
    sleep 1
done

# Run test
echo "Running test..."
python3 scripts/test_local_convergence.py 2>&1 | tee "/tmp/sweep_${WEIGHT}.txt"

# Stop server
kill $SERVER_PID 2>/dev/null || true
wait $SERVER_PID 2>/dev/null || true
echo "=== Done with $WEIGHT ==="
