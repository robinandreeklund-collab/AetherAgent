# Persistent CRFR Learning — Architecture & Troubleshooting

> How CRFR data survives server restarts, deploys, and content changes.
> Last updated: 2026-04-07

---

## Overview

CRFR fields (causal memory, propagation weights, concept memory) are persisted to SQLite on a Render Persistent Disk at `/data/aether.db`. This means all learning survives deploys, restarts, and scale events.

---

## Architecture

```
Request → get_or_build_field_with_variant()
              │
              ├─ 1. Check RAM cache (256 entries, 3min TTL)
              │     Hit → return field
              │
              ├─ 2. Check SQLite (persistent disk)
              │     Found → two-level hash validation:
              │       ├─ content_hash matches → full hit, return as-is
              │       ├─ content_hash differs → migrate learning to fresh field
              │       └─ content_hash == 0 → old field, set hash, return
              │
              └─ 3. Build new field from scratch
                    Set content_hash, return

After propagation → save_field() → SQLite (immediate)
Every 60s → checkpoint() → save ALL cached fields + total_requests
On startup → restore() → load all fields + domain profiles + counters
```

---

## Two-Level Hash Validation

### Problem
A news site like BBC has identical DOM structure every day (same `<article>`, `<h2>`, `<p>` tags) but completely different headlines. A structure-only hash would match → stale content served.

### Solution
Two hashes must both match for a cache hit:

| Hash | What it checks | Catches |
|------|---------------|---------|
| `structure_hash` | Top-20 role sequence (heading, text, link...) | DOM layout changes, redesigns |
| `content_hash` | FNV-1a over ALL node label text | Text changes, new headlines, updated prices |

### Scenarios

| Scenario | Structure | Content | Action |
|----------|-----------|---------|--------|
| Same page, nothing changed | Match | Match | **Full cache hit** — all weights preserved |
| New headlines (BBC, HN) | Match | **Mismatch** | Rebuild + **migrate learning** |
| Site redesign | **Mismatch** | Mismatch | Rebuild + migrate what matches |
| First visit | N/A | N/A | Build new field |
| Old field (no content_hash) | N/A | `== 0` | Use stored, set hash for future |

---

## Learning Migration

When content changes but we want to preserve learning:

```rust
fresh_field.migrate_learning_from(&old_field);
```

What gets migrated:
- **Causal memory** — node-level, matched by Hypervector similarity (same role + similar text → transfer)
- **Propagation stats** — goal-clustered weights (e.g., "heading→child is strong for news queries")
- **Concept memory** — goal-token associations (e.g., "news" → boost)
- **Counters** — total_feedback, total_queries, total_successful_nodes

What gets rebuilt:
- Node IDs (new DOM → new IDs)
- BM25 index (new text content)
- HDC bitvectors (new text → new encodings)

---

## Data Flow

### Per-request
```
parse_crfr() called
  → get_or_build_field_with_variant()  // RAM → SQLite → build
  → field.propagate_top_k()            // CRFR scoring
  → save_field(&field)                 // RAM cache + SQLite write
```

### Every 60 seconds (checkpoint)
```
spawn_memory_monitor → checkpoint()
  → export_cached_fields()          // all fields from RAM cache
  → save_field() for each           // write to SQLite
  → save_global_stat("total_requests", N)  // persist counter
  → save domain profiles
```

### On startup (restore)
```
main() → persist::init("/data/aether.db")
       → persist::restore()
           → load_all_fields()         → import to RAM cache
           → load_all_domain_profiles() → import to domain registry
       → load_global_stat("total_requests") → set AtomicU64
```

---

## SQLite Schema

```sql
-- CRFR resonance fields (serialized as JSON blob)
CREATE TABLE resonance_fields (
    url_hash INTEGER PRIMARY KEY,
    url TEXT NOT NULL,
    data BLOB NOT NULL,        -- serde_json::to_vec(ResonanceField)
    updated_at INTEGER NOT NULL
);

-- Domain-level shared learning
CREATE TABLE domain_profiles (
    domain_hash INTEGER PRIMARY KEY,
    data BLOB NOT NULL,        -- serde_json::to_vec(DomainProfile)
    updated_at INTEGER NOT NULL
);

-- Global counters (total_requests, etc.)
CREATE TABLE global_stats (
    key TEXT PRIMARY KEY,
    value INTEGER NOT NULL
);
```

---

## Render Deployment

### Persistent Disk
- **Mount path:** `/data`
- **Size:** 1 GB
- **Env var:** `AETHER_DB_PATH=/data/aether.db` (set in Dockerfile)

### What survives deploys
- All CRFR fields (causal memory, weights, concepts)
- Domain profiles (cross-URL learning)
- Global stats (total_requests)

### What resets on deploy
- RAM cache (repopulated from SQLite on first access)
- Uptime counter (AppState.started_at)
- In-flight request state

---

## Troubleshooting

### Check if persist is working
```bash
curl https://www.slaash.ai/api/live-stats | jq '.db'
```
Expected:
```json
{
  "fields_stored": 5,
  "domains_stored": 3,
  "size_bytes": 45678,
  "persistent": true
}
```

### Data disappears after deploy
1. Check `db.persistent` is `true` (not `false`)
2. Check `db.fields_stored` > 0 after running some queries
3. Check Render logs for `[PERSIST]` messages:
   - `[PERSIST] SQLite initialized at /data/aether.db` — good
   - `[PERSIST] Restored: X domain profiles, Y resonance fields` — good
   - `[PERSIST] save_field write error: ...` — SQLite write failing
   - `[PERSIST] WARNING: Failed to init DB` — disk not mounted

### Fields stored but not loading
- `[PERSIST] Restored: 0 domain profiles, 0 resonance fields` after deploy
  when `db.fields_stored` > 0 → deserialization issue (schema changed?)
- Check if new fields added to ResonanceField have `#[serde(default)]`

### Content hash issues
- Check Render logs for `[CRFR] Content changed for URL (hash X → Y), migrating learning`
- If you see this on every request for the same URL → content is dynamic (ads, timestamps)
- Solution: those URLs won't get cache hits but migration preserves learning

---

## Key Files

| File | What |
|------|------|
| `src/resonance.rs` | ResonanceField, content_hash, migrate_learning_from, get_or_build_field |
| `src/persist.rs` | SQLite save/load/checkpoint/restore, global_stats |
| `src/lib.rs` | parse_crfr entry points |
| `src/bin/server.rs` | /api/live-stats, checkpoint timer, request counter |
| `Dockerfile` | `ENV AETHER_DB_PATH=/data/aether.db`, `RUN mkdir -p /data` |

---

## Tests

```bash
# All in-memory tests (704 total)
cargo test --lib

# Persist-specific tests (run individually due to global singleton)
cargo test --lib --features persist -- persist::tests::test_field_survives_save_load_cycle
cargo test --lib --features persist -- persist::tests::test_global_stats_persist

# Content hash tests
cargo test --lib -- resonance::tests::test_compute_content
cargo test --lib -- resonance::tests::test_migrate
cargo test --lib -- resonance::tests::test_cache_hit
cargo test --lib -- resonance::tests::test_learning_survives
```
