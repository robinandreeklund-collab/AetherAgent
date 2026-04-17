#!/bin/sh
# Entrypoint som:
#   1. Försöker restore:a /data/aether.db från GCS via Litestream (om LITESTREAM_BUCKET satt)
#   2. Kör aether-server under Litestreams replicate-supervisor
#      — Litestream streamar WAL-ändringar till GCS kontinuerligt.
#   Om LITESTREAM_BUCKET inte är satt kör vi servern direkt (ephemeral storage).

set -eu

mkdir -p /data

if [ -n "${LITESTREAM_BUCKET:-}" ]; then
    echo "[litestream] bucket=${LITESTREAM_BUCKET}"

    # Restore om DB saknas lokalt och replica finns i GCS.
    # -if-db-not-exists: ingen overwrite av ev. befintlig lokal DB.
    # -if-replica-exists: ingen panik om bucket är tom (första starten).
    litestream restore \
        -if-db-not-exists \
        -if-replica-exists \
        -config /etc/litestream.yml \
        /data/aether.db || echo "[litestream] restore skipped (no replica or local db exists)"

    # Starta litestream replicate som parent-process och exec servern via -exec.
    # Litestream vidarebefordrar signaler till servern och flushar WAL vid shutdown.
    exec litestream replicate -config /etc/litestream.yml -exec "aether-server"
else
    echo "[litestream] LITESTREAM_BUCKET not set — running without persistence replication"
    exec aether-server
fi
