# 00 — Lean Project Bootstrap

This document covers the Lean 4 project living at `lean/`: how to install
the toolchain, build it locally, the module layout, and what the CI job
checks. It corresponds to parent issue 00 of the Lean Verification
initiative — see `lean/README.md` for the full initiative plan.

## Installing the toolchain

The project uses [`elan`](https://github.com/leanprover/elan), the Lean
version manager (the Lean equivalent of `rustup`). It reads `lean/lean-toolchain`
and transparently installs and switches to the exact Lean version pinned
there — you do not need to manually select a Lean version.

```bash
curl https://raw.githubusercontent.com/leanprover/elan/master/elan-init.sh -sSf | sh
```

This also installs `lake`, Lean's build tool (the Lean equivalent of
`cargo`). Restart your shell, or `source ~/.profile`, so `elan`/`lake`/`lean`
are on `PATH`.

## Building

```bash
just lean-build
```

which runs `cd lean && lake build`. The first build fetches
[Mathlib](https://github.com/leanprover-community/mathlib4) and its
transitive dependencies as source, then automatically downloads Mathlib's
precompiled `.olean` cache (via Mathlib's own post-update hook, so no
separate `cache get` step is required) rather than compiling Mathlib from
scratch — expect the first run to download several hundred MB and take a
few minutes; every run after that is fast, since only `lean/LeanVerification/*`
needs to build against the cached Mathlib.

To check specifically for `sorry` (which `lake build`'s exit code does
*not* fail on by default — a `sorry` compiles successfully and only emits
a warning):

```bash
just lean-check-sorry
```

## Toolchain/Mathlib version pinning

`lean/lean-toolchain` and the `mathlib` `rev` in `lean/lakefile.toml` are
pinned together (currently `v4.33.0`) — Mathlib only builds against the
exact Lean version its own `lean-toolchain` specifies at that commit/tag.
When bumping either, always look up the target Mathlib tag's
`lean-toolchain` file first and match this project's `lean-toolchain` to
it, not the other way around.

## Module layout

All source lives under `lean/LeanVerification/`, imported as one unit from
the root `lean/LeanVerification.lean`. Every module below is currently an
empty placeholder (a header comment, no `sorry`); each is filled in by the
parent issue listed.

| Module | Owning issue |
|---|---|
| `Prelude.lean` | shared basics (`Validator`, `DecidableEq`/`Fintype`, notation) — used by 01 and 02 |
| `Weights.lean` | Issue 01 (KR3 — Weighted Quorum Reasoning) |
| `WCert.lean` | Issue 01 (KR3) |
| `Block.lean` | Issue 02 (KR1 — Blocklace Core) |
| `Blocklace.lean` | Issue 02 (KR1) |
| `Observe.lean` | Issue 02 (KR1) |
| `Equivocation.lean` | Issue 03 (KR2 — Equivocation and Exclusion) |
| `Approval.lean` | Issue 04 (KR4 — Finalized Leader Safety) |
| `Finality.lean` | Issue 04 (KR4) |
| `Ordering.lean` | Issue 04 (KR4) |
| `Trace.lean` | Issue 05 (KR5 — Proof-to-Test Mapping, Trace Replay, Conformance CI) |
| `Replay.lean` | Issue 05 (KR5) |

See `lean/README.md` for how these modules relate to each other and to the
Rust source in `crates/cordial-miners-core/src/consensus/`.

## CI

`.github/workflows/lean.yml` runs on every push to `lean-q1` and every pull
request into `lean-q1`, `dev`, or `master`, but **only** when the diff
touches `lean/**` — unrelated Rust PRs don't pay the Mathlib-download cost.
The job:

1. Restores a cache of `~/.elan` and `lean/.lake`, keyed on
   `lean-toolchain` + `lake-manifest.json`, so unchanged dependencies don't
   re-download every run.
2. Installs the pinned toolchain and runs `lake build` via
   [`leanprover/lean-action`](https://github.com/leanprover/lean-action) —
   the same pattern `.github/workflows/rust.yml` uses for Rust
   (`dtolnay/rust-toolchain@nightly`), just for Lean.
3. Re-runs `lake build` and greps its log for `declaration uses 'sorry'`,
   failing the job if found — this is the explicit check that step 2 alone
   cannot do, since a `sorry` is a warning, not a build error.

**Green** means: the project builds against the pinned Mathlib, and no
module — new or existing — contains a `sorry`. That is the entire bar for
this bootstrap issue; later issues (01–06) are where actual theorems and
proofs land.
