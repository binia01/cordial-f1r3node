/-
Weighted certificates (WCert): the evidence structure a set of validator
signatures must satisfy to count as a valid weighted quorum certificate.

Owned by Issue 01 (KR3 — Weighted Quorum Reasoning).
-/
import LeanVerification.Weights

variable {Validator : Type*} [DecidableEq Validator] [Fintype Validator]

/-- Evidence collected incrementally as signatures arrive: the accepted
signer set, and a running weight total maintained alongside it so a
certificate can be checked in O(1) against `weight` rather than re-summing
`accepted` on every incoming signature. -/
structure WCert (Validator : Type*) where
  accepted : Finset Validator
  weight : ℕ

/-- `weight` never drifts from recomputing `wt w accepted` from scratch. -/
def WCert.Invariant (w : Validator → ℕ) (c : WCert Validator) : Prop :=
  c.weight = wt w c.accepted

/-- The empty collector: no signers accepted yet, zero weight. -/
def WCert.empty : WCert Validator :=
  { accepted := ∅, weight := 0 }

/-- Validate one incoming signer `p`. A repeat of an already-accepted
signer is a no-op (deduplicated, not double-counted); a new signer is
added to `accepted` and its weight folded into the running total. -/
def WCert.accept (w : Validator → ℕ) (c : WCert Validator) (p : Validator) : WCert Validator :=
  if p ∈ c.accepted then c
  else { accepted := insert p c.accepted, weight := c.weight + w p }

omit [DecidableEq Validator] [Fintype Validator] in
theorem WCert.empty_invariant (w : Validator → ℕ) :
    (WCert.empty (Validator := Validator)).Invariant w := by
  unfold WCert.Invariant WCert.empty wt
  simp

omit [Fintype Validator] in
theorem WCert.accept_invariant (w : Validator → ℕ) (c : WCert Validator) (p : Validator)
    (hc : c.Invariant w) : (WCert.accept w c p).Invariant w := by
  unfold WCert.Invariant WCert.accept at *
  by_cases hp : p ∈ c.accepted
  · simp [hp, hc]
  · simp only [hp, if_false]
    rw [wt, Finset.sum_insert hp, ← wt, ← hc]
    ring
