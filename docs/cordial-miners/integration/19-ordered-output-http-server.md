# Ordered Output HTTP Server

This note documents the lightweight HTTP server that exposes the Cordial
finalized ordered output over a network-accessible JSON API.

It closes the gap between the in-process `SharedOrderedOutput` pipeline and
external consumers — including future demo applications — without requiring
any f1r3node source changes.

## Context

| Component | File |
|---|---|
| Stable export type | `crates/cordial-f1r3node-adapter/src/ordered_output.rs` |
| Read-only consumer trait + container | `crates/cordial-f1r3node-adapter/src/shared_ordered_output.rs` |
| Ordered output method on LiveIngress | `crates/cordial-f1r3node-adapter/src/live_ingress.rs` (L540) |
| CLI inspection binary | `crates/cordial-f1r3node-adapter/src/bin/live_ordered_output.rs` |
| **HTTP server module** | `crates/cordial-f1r3node-adapter/src/ordered_output_server.rs` |
| **HTTP server binary** | `crates/cordial-f1r3node-adapter/src/bin/ordered_output_server.rs` |

## Endpoints

### `GET /ordered-output/latest`

Returns the full `OrderedFinalizedOutput` as JSON.

```json
{
  "blocks": [
    {
      "content_hash": [1, 2, 3, ...],
      "creator": {"0": [4, 5, 6, ...]},
      "signature": [7, 8, 9, ...]
    }
  ],
  "anchor": {
    "content_hash": [...],
    "creator": {...},
    "signature": [...]
  },
  "wavelength": 3,
  "bond_count": 4,
  "total_mirrored_blocks": 812,
  "computed_at_ns": 1753331400000000000
}
```

- Returns **200 OK** with the JSON body when a finalized output is available.
- Returns **503 Service Unavailable** when no finalized leader has been
  observed yet. The response body is:
  ```json
  {"error": "no finalized ordered output available yet"}
  ```

### `GET /ordered-output/status`

Returns a lightweight status object without the full block list. Suitable for
polling from a demo application without deserializing the entire ordered
sequence.

```json
{
  "anchor_hash": "a1b2c3...",
  "len": 812,
  "bond_count": 4,
  "wavelength": 3,
  "computed_at_ns": 1753331400000000000,
  "is_stale": false
}
```

- `anchor_hash` — hex-encoded anchor block content hash; `null` if no anchor.
- `len` — number of blocks in the finalized ordered prefix.
- `bond_count` — number of bonded validators at computation time.
- `wavelength` — consensus wave size used for finality.
- `computed_at_ns` — wall-clock nanoseconds since Unix epoch when last computed.
- `is_stale` — `true` when `computed_at_ns` is older than the configured
  staleness threshold (default: 30 seconds).

Returns **503** if no finalized output is available, same as `/latest`.

## Running the Server

### Basic invocation

```bash
cargo run --bin ordered_output_server -- \
  --grpc-url http://127.0.0.1:40401 \
  --addr     127.0.0.1:7080
```

### Full flag reference

| Flag | Default | Description |
|---|---|---|
| `--grpc-url` | `http://127.0.0.1:40401` | f1r3node gRPC endpoint to mirror |
| `--addr` | `127.0.0.1:7080` | HTTP bind address |
| `--depth` | `128` | Block history depth to fetch on startup |
| `--wave-length` | `3` | Consensus wavelength for tau ordering |
| `--poll-interval-ms` | `2000` | How often to recompute ordered output |
| `--shard-id` | `root` | Shard to observe |
| `--stale-threshold-secs` | `30` | Age in seconds before `/status` reports `is_stale: true` |

### Verify with curl

```bash
# Full output (may be large on a long-running cluster)
curl http://127.0.0.1:7080/ordered-output/latest | jq .

# Block count and staleness only
curl http://127.0.0.1:7080/ordered-output/status | jq .

# Watch staleness in real time (poll every 2 s)
watch -n 2 'curl -s http://127.0.0.1:7080/ordered-output/status | jq "{len,is_stale}"'
```

## Startup Sequence

```
ordered_output_server
        │
        ├─ 1. Connect to f1r3node gRPC
        ├─ 2. Fetch --depth recent blocks, mirror into LiveIngress
        ├─ 3. Compute initial OrderedFinalizedOutput → SharedOrderedOutput
        ├─ 4. Spawn background task (poll every --poll-interval-ms)
        │        └── reconnect → fetch → mirror fresh LiveIngress
        │                    → recompute → SharedOrderedOutput.update()
        └─ 5. Start axum listener on --addr
```

The server is ready to accept requests immediately after step 5. Steps 1–3
happen synchronously before the listener opens; steps 4–5 run concurrently
thereafter.

## Architecture

```
LiveIngress (background task)
    │
    │  polls every --poll-interval-ms
    ▼
SharedOrderedOutput  ←─  Arc<Mutex<SharedOrderedOutput>>
    │
    ▼
axum Router
    ├── GET /ordered-output/latest  → full JSON
    └── GET /ordered-output/status  → lightweight JSON
```

The HTTP handlers lock the `Arc<Mutex<SharedOrderedOutput>>`, clone the latest
output (or return 503 if empty), release the lock, and serialize. Lock
contention between the background task and HTTP handlers is minimal because
serialization happens after the lock is released.

## Staleness Semantics

`is_stale` in `/ordered-output/status` is `true` when:

```
wall_clock_now_ns - computed_at_ns > stale_threshold_ns
```

where `stale_threshold_ns = --stale-threshold-secs × 1_000_000_000`.

An **empty container** (no output ever computed) is always considered stale by
the underlying `ReadOrderedOutput::is_stale` trait method, but the server
returns 503 before this check is reached.

A stale response means the background mirror task has stopped making progress
(network partition, crashed f1r3node, etc.). The `len` and `anchor_hash` fields
still reflect the last good state and can be used to determine whether finality
was ever reached.

## Prefix Invariant

The server never regresses its output. If the background task computes a new
`OrderedFinalizedOutput` that does not start with the previous output's block
list (a prefix violation), the update is rejected and the previous output is
kept. The server will report `is_stale: true` via `/status` rather than serve
a regressed sequence.

This matches the invariant enforced by `SharedOrderedOutput::update()` and
ensures that any consumer reading successive responses always sees:

```
response[t].blocks starts_with response[t-1].blocks
```

## Running alongside Existing Binaries

All three binaries can run concurrently against the same f1r3node cluster:

```bash
# Terminal 1 — one-shot inspection of ordered output
cargo run --bin live_ordered_output -- --grpc-url http://127.0.0.1:40401

# Terminal 2 — full mirror diagnostic
cargo run --bin live_mirror_check -- \
  --grpc-url http://127.0.0.1:40401 \
  --http-url http://127.0.0.1:40403 \
  --skip-http-compare

# Terminal 3 — persistent HTTP server
cargo run --bin ordered_output_server -- \
  --grpc-url http://127.0.0.1:40401 \
  --addr 127.0.0.1:7080
```

Each binary maintains its own independent in-memory mirror. The HTTP server
binary is the only one that keeps the server running; the other two are
single-shot diagnostic tools.

## How a Demo Application Should Consume the Endpoint

The simplest integration pattern for a demo application:

1. **Poll `/ordered-output/status`** every few seconds.
   - If `is_stale: true`, display a "waiting for finalization" state.
   - If `503`, display "node not yet ready".
   - When `len` increases, trigger a fetch of `/ordered-output/latest`.

2. **Fetch `/ordered-output/latest`** only when new blocks are expected.
   - Deserialize `blocks` as an append-only sequence.
   - Verify that the new `blocks` array starts with the previously seen blocks
     (prefix invariant) before updating local state.
   - Use `anchor_hash` to correlate with any block-explorer views.

3. **Do not re-implement tau ordering** in the demo application. The server is
   the single source of truth for finalized order.

## Empty State

Both endpoints return `503 Service Unavailable` when the container is empty.
A demo application must handle this case gracefully — it is the expected state
immediately after startup on a cluster where no block has yet been finalized.

## Out of Scope

- gRPC StreamUpdates push/streaming endpoint
- Authentication and TLS
- Prometheus metrics (tracked in Phase 5)
- Multi-shard routing
