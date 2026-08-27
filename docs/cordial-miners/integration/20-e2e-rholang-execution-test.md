# 20 — End-to-End Rholang Execution Test (Phase 4.2)

## What this test proves

`crates/cordial-f1r3space-adapter/tests/test_e2e_execution.rs` exercises the
full adapter stack end-to-end:

```
ExecutionRequest
  → F1r3RspaceRuntime::execute_block          (cordial-f1r3space-adapter)
  → F1r3RuntimeManager::compute_state         (f1r3node, real LMDB + Rholang)
  → ExecutionResult
```

This is the critical Phase 4.2 milestone: it proves the Cordial adapter layer
can reach actual f1r3node execution machinery — not just translate types.

### Test inventory

| Test name | Invariant checked |
|-----------|-------------------|
| `execute_block_changes_state_hash` | Executing a valid deploy mutates the tuplespace: post-hash ≠ pre-hash |
| `deploy_appears_in_processed_list` | The submitted deploy is visible in `ExecutionResult.processed_deploys` with matching term bytes |
| `failed_deploy_surfaces_as_is_failed` | A runtime-failing deploy appears in `processed_deploys` with `is_failed: true` — it is not silently dropped |
| `close_block_system_deploy_executes` | `SystemDeployRequest::CloseBlock` runs without error and produces a succeeded `ProcessedSystemDeploy` |
| `multiple_deploys_all_appear_in_result` | All deploys in a multi-deploy block appear in `processed_deploys` |

### Why `rejected_deploys` is always empty

`ExecutionResult.rejected_deploys` is always `vec![]` in the adapter's output.
This is not a bug — it reflects how f1r3node works: `compute_state` does not
return a separate rejected-deploy list. Deploys that fail at runtime appear in
`processed_deploys` with `is_failed: true`. See `lib.rs:161` and the
`failed_deploy_surfaces_as_is_failed` test.

---

## Why tests are `#[ignore]` by default

Constructing a real `F1r3RuntimeManager` requires:

1. Initialising an LMDB data directory with pre-computed genesis state
2. Starting a bootstrapped Rholang interpreter and RSpace history repository
3. Executing initial genesis deploys (PoS, REV Vault contracts)

This process takes ~10–30 seconds on the first run. Marking the tests `#[ignore]`
ensures that standard `cargo test` and `just test` commands run fast in CI,
while `just e2e-rholang` explicitly triggers the full E2E execution suite.

---

## Running locally

### How the bootstrap works

Each test calls the shared `setup()` async helper, which uses f1r3node's own
`test_utils` — the exact same infrastructure f1r3node's casper integration
tests use:

```rust
// Initializes tracing subscriber for RUST_LOG environment variables
let _ = tracing_subscriber::fmt()
    .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
    .try_init();

// Bootstraps genesis state into a shared temp LMDB directory.
// Result is process-wide cached (OnceLock) — only runs once per test binary.
let genesis_ctx = genesis_context().await?;

// Connect a RuntimeManager to the genesis LMDB scope.
let mut kvm = mk_test_rnode_store_manager_from_genesis(&genesis_ctx);
let (rt, _history_repo) = mk_runtime_manager_with_history_at(&mut *kvm).await;

// Get the genesis post-state hash from the genesis block header.
let genesis_hash = genesis_ctx.genesis_block.body
    .state.post_state_hash.to_vec();
```

No manual setup is needed. The `GenesisBuilder` inside `genesis_context()`
handles everything automatically: validator key generation, vault bootstrapping,
Rholang interpreter initialisation, and genesis block production.

The genesis bootstrap is cached with a process-wide `OnceLock`, so the 10–30 s
cost is paid only once even when multiple tests run sequentially.

## Required dependencies

The `[dev-dependencies]` in `Cargo.toml` include:

```toml
tempfile = "3"
tracing-subscriber = "0.3"
tokio = { version = "1", features = ["full"] }
```

And `casper` in `[dependencies]` includes `features = ["test-utils"]`.

### Running the tests

Via Justfile (recommended):

```sh
just e2e-rholang
```

This runs:

```sh
cargo +nightly-2025-06-15 test -p cordial-f1r3space-adapter \
    --test test_e2e_execution -- --ignored --nocapture
```

Running a single test:

```sh
cargo +nightly-2025-06-15 test -p cordial-f1r3space-adapter \
    --test test_e2e_execution execute_block_changes_state_hash \
    -- --ignored --nocapture
```

### Inspecting Internal f1r3node Logs (`RUST_LOG`)

You can view internal execution traces (RSpace tuplespace reads/writes, Rholang VM evaluation, LMDB state transitions) by setting `RUST_LOG`:

```sh
# High-level state transitions and cache info
RUST_LOG=info just e2e-rholang

# Full execution and evaluation trace
RUST_LOG=debug just e2e-rholang
```

### Compile-only check (no live environment needed)

```sh
cargo +nightly-2025-06-15 test -p cordial-f1r3space-adapter \
    --test test_e2e_execution --no-run
```

This confirms the test file compiles correctly without running any tests.

---

## Relation to existing tests

| File | Scope |
|------|-------|
| `tests/test_translation.rs` | Pure type translation — no `RuntimeManager`, no LMDB, always runs in CI |
| `tests/test_e2e_execution.rs` (this file) | Full stack — real `RuntimeManager`, real LMDB, `#[ignore]` by default |

The translation tests remain unaffected by this addition.

---

## Enabling in CI (future work)

To enable these tests in CI, the CI workflow can run `cargo test --workspace -- --ignored` or `just e2e-rholang` in a dedicated integration test step after checking out the `f1r3node` sibling directory.

---

## See also

- [`test_translation.rs`](file:///home/bini/Documents/repos/cordial-f1r3node/crates/cordial-f1r3space-adapter/tests/test_translation.rs) — pure translation unit tests
- [`cordial-f1r3space-adapter/src/lib.rs`](file:///home/bini/Documents/repos/cordial-f1r3node/crates/cordial-f1r3space-adapter/src/lib.rs) — the adapter implementation under test
- [`PRODUCTION_ROADMAP.md § Phase 4.2`](file:///home/bini/Documents/repos/cordial-f1r3node/docs/PRODUCTION_ROADMAP.md) — roadmap context
