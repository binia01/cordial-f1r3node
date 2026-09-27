# 01 — Weighted Quorum Reasoning (WCert)

This document covers `lean/LeanVerification/Weights.lean` and
`lean/LeanVerification/WCert.lean` — the Lean formalization of weighted
quorum certificates for KR3. It corresponds to parent issue 01 of the Lean
Verification initiative (see `lean/README.md` for the full initiative
plan and `docs/lean/00-project-bootstrap.md` for the project-wide
toolchain/build setup).

## Why this exists

Classical BFT counts validators: "two-thirds of *n* nodes agree."
Proof-of-Reputation replaces counting with **weight** — each validator
has a reputation score, and thresholds are checked against the *sum* of
weights, not headcount. The Rust code
(`crates/cordial-miners-core/src/consensus/cordiality.rs`) already
implements weighted-threshold checks and gets the arithmetic right on the
cases its tests exercise, but nobody had proven, in general, that "two
coalitions each holding more than two-thirds of total weight must share
weight" — the fact the entire safety story rests on. This module proves
it.

## Definitions

All definitions live in `Weights.lean`, parameterized over a validator
type `Validator` (with `[DecidableEq Validator] [Fintype Validator]`) and
a weight function `w : Validator → ℕ`.

| Concept | Lean signature | Meaning |
|---|---|---|
| Weight of a set | `abbrev wt (w) (S : Finset Validator) : ℕ := ∑ p ∈ S, w p` | `wt(S) = Σ_{p∈S} w(p)` |
| Total weight | `abbrev totalWeight (w) : ℕ := wt w Finset.univ` | `W = wt(Π)` |
| Threshold-open family | `abbrev OpenTheta (w) (p q : ℕ) (S) : Prop := q * wt w S > p * totalWeight w` | `S ∈ Open_θ` for `θ = p/q`, i.e. `wt(S) > θW` |
| All-but-`F` family | `abbrev OpenF (w) (F : ℕ) (S) : Prop := wt w Sᶜ ≤ F` | `S ∈ Open_F`, i.e. `wt(Π\S) ≤ F` |
| Weighted certificate | `structure WCert (Validator) where accepted : Finset Validator; weight : ℕ` | an incremental evidence collector (see below) |

**Why cross-multiplication, not rationals.** A threshold `θ ∈ (0,1)` is
represented as a pair `(p, q : ℕ)` with `θ = p/q`, and every comparison is
stated as `q * wt(S) > p * W` instead of `wt(S)/W > θ` or `wt(S) > θ*W`
with `θ : ℚ`. This mirrors the Rust side exactly (`3 * support > 2 *
total`, no division, no floating point) and is what makes
`strict_two_thirds_iff_openTheta` below a near-trivial bridge instead of a
lossy approximation.

**`WCert`.** Rather than waiting for a fixed-arity join over a known
committee, a `WCert` accepts evidence incrementally: `WCert.accept w c p`
validates one incoming signer `p`, deduplicates (a repeated signer doesn't
double-count), and updates the running `weight` field. The invariant

```lean
def WCert.Invariant (w : Validator → ℕ) (c : WCert Validator) : Prop :=
  c.weight = wt w c.accepted
```

says the incrementally-maintained `weight` never drifts from recomputing
`wt w accepted` from scratch — proved for the empty collector
(`WCert.empty_invariant`) and preserved by every `accept` step
(`WCert.accept_invariant`). This is what lets a certificate be checked in
O(1) against `weight` rather than re-summing the accepted set on every
incoming signature.

## Statements

Full proofs are in `Weights.lean`; only the statements and the intuition
behind each are given here.

### Weighted-overlap lemma

```lean
lemma weighted_overlap (w) (p₁ p₂ q : ℕ) (hpq : q ≤ p₁ + p₂)
    (A B : Finset Validator) (hA : OpenTheta w p₁ q A) (hB : OpenTheta w p₂ q B) :
    OpenTheta w (p₁ + p₂ - q) q (A ∩ B)
```

If `wt(A) > θ₁W` and `wt(B) > θ₂W`, then `wt(A ∩ B) > (θ₁+θ₂-1)W`. Intuitively:
`wt(A) + wt(B) ≤ W + wt(A ∩ B)` (inclusion–exclusion, since `wt(A∪B) ≤
W`), so the "excess" weight `wt(A)+wt(B)-W` that has nowhere else to go
must sit in the overlap. For `θ₁=θ₂=2/3` this gives the textbook
`wt(A∩B) > W/3`: two coalitions each holding more than two-thirds of the
weight cannot possibly be disjoint. The hypothesis is stated as `≤` rather
than `<` because `OpenTheta`'s own membership is already strict, so the
conclusion still holds at the boundary — this is what lets 3-twinedness
below reach the exact `θ = 2/3` case rather than requiring `θ > 2/3`.

### `n`-twinedness, threshold family

```lean
theorem n_twinedness_theta (w) (p q n : ℕ) (hq : 0 < q) (hn : 0 < n)
    (hθ : q * (n - 1) ≤ p * n) (O : ℕ → Finset Validator)
    (hO : ∀ i < n, OpenTheta w p q (O i)) :
    ((Finset.range n).inf O).Nonempty
```

If `θ ≥ (n-1)/n`, any `n` sets each clearing the `θ` threshold have
nonempty intersection. Proved by induction, chaining `weighted_overlap`
across the family one set at a time: combining `k` sets already at
threshold `(k·p - (k-1)·q)/q` with one more set at `p/q` yields the
family combined at `((k+1)·p - k·q)/q`, and a small monotonicity lemma
(`cross_mul_mono`) shows the hypothesis needed at every intermediate step
follows from the hypothesis at the full size `n`. For `θ = 2/3, n = 3`
this is exactly 3-twinedness — the boundary case `q*(n-1) = p*n` (i.e.
`3·2 = 2·3`) is covered because the lemma is non-strict, matching how
`θ = 2/3` is used in practice.

### `n`-twinedness, all-but-`F` family

```lean
theorem n_twinedness_F (w) (F n : ℕ) (hW : n * F < totalWeight w)
    (O : ℕ → Finset Validator) (hO : ∀ i < n, OpenF w F (O i)) :
    ((Finset.range n).inf O).Nonempty
```

If `W > n·F`, any `n` sets each excluding at most `F` weight have
nonempty intersection. This is a plain union bound: the complement of the
intersection is the union of the `n` complements, whose combined excluded
weight is at most `n·F`; since `n·F < W`, that union can't be everything,
so the intersection is nonempty. Simpler than the threshold-family version
because `OpenF` bounds add linearly with no threshold-shifting needed.

### Honest triple intersection

```lean
theorem honest_triple_intersection (w) (p q : ℕ) (hθ : q * 2 ≤ p * 3)
    (H A B : Finset Validator)
    (hH : OpenTheta w p q H) (hA : OpenTheta w p q A) (hB : OpenTheta w p q B) :
    (A ∩ B ∩ H).Nonempty
```

If the honest validators `H` alone clear the `θ` threshold (`H` *is* the
honest kernel — no separate structure is needed beyond this hypothesis),
then any two nonempty `θ`-threshold sets share an honest member. Proved
directly by applying `weighted_overlap` twice (`A` with `B`, then that
result with `H`) rather than going through `n_twinedness_theta`'s general
induction, since for a fixed triple the direct route is simpler. This is
the single fact Issue 04's `no_conflicting_finals` is built on: every pair
of quorums intersects an honest validator.

### Bridge to the Rust implementation

```lean
theorem strict_two_thirds_iff_openTheta (w) (S : Finset Validator) :
    wt w S * 3 > totalWeight w * 2 ↔ OpenTheta w 2 3 S
```

`strict_two_thirds` in `cordiality.rs:375-384` computes exactly
`support_weight * 3 > total_weight * 2` (via `u128` checked arithmetic
that returns `false` on overflow — a finite-precision concern the Lean
side, over `ℕ`, doesn't have). That is *definitionally* `OpenTheta w 2 3
S` up to commuting the multiplication, which is exactly why the proof is
one line (`unfold OpenTheta; omega`): the cross-multiplied encoding was
chosen so this bridge needs no floating point and no rounding anywhere.

## Mapping to the Rust source

| Lean theorem | Rust function | File |
|---|---|---|
| `strict_two_thirds_iff_openTheta` | `strict_two_thirds` | `crates/cordial-miners-core/src/consensus/cordiality.rs:375-384` |
| `weighted_overlap`, `n_twinedness_theta` | `is_weighted_supermajority` | `crates/cordial-miners-core/src/consensus/cordiality.rs:348-367` |
| `honest_triple_intersection` | (justifies Issue 04's `no_conflicting_finals`, not yet in Rust) | — |
| `WCert`, `WCert.Invariant`, `accept_invariant` | `checked_bond_weight` (incremental accumulation pattern) | `crates/cordial-miners-core/src/consensus/cordiality.rs:369-373` |

## Checking this module

```bash
just lean-build         # builds LeanVerification, including Weights.lean and WCert.lean
just lean-check-sorry   # fails if any declaration (here or elsewhere) uses `sorry`
```

Both `Weights.lean` and `WCert.lean` build with zero errors, zero
warnings, and zero `sorry` as of this writing.
