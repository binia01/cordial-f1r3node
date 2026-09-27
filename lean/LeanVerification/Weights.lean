/-
Validator weights for the weighted-quorum reasoning (KR3): natural-number
weights and the strict two-thirds supermajority threshold, checked by
cross-multiplication (`3 * support > 2 * total`) rather than division.

Owned by Issue 01 (KR3 — Weighted Quorum Reasoning).
-/
import Mathlib

variable {Validator : Type*} [DecidableEq Validator] [Fintype Validator]

/-- The total weight of a set of validators. -/
abbrev wt (w : Validator → ℕ) (S : Finset Validator) : ℕ := ∑ p ∈ S, w p

/-- The total weight of every validator. -/
abbrev totalWeight (w : Validator → ℕ) : ℕ := wt w Finset.univ

omit [DecidableEq Validator] in
lemma wt_le_totalWeight (w : Validator → ℕ) (S : Finset Validator) :
    wt w S ≤ totalWeight w :=
  Finset.sum_le_sum_of_subset (Finset.subset_univ S)

omit [Fintype Validator] in
lemma wt_union_add_wt_inter (w : Validator → ℕ) (A B : Finset Validator) :
    wt w (A ∪ B) + wt w (A ∩ B) = wt w A + wt w B :=
  Finset.sum_union_inter

lemma wt_add_wt_compl (w : Validator → ℕ) (S : Finset Validator) :
    wt w S + wt w Sᶜ = totalWeight w :=
  Finset.sum_add_sum_compl S w

/-- `S ∈ Open_θ` for `θ = p / q`: the weight of `S` strictly exceeds `θ` of
the total, encoded by cross-multiplication so no division is needed. -/
abbrev OpenTheta (w : Validator → ℕ) (p q : ℕ) (S : Finset Validator) : Prop :=
  q * wt w S > p * totalWeight w

/-- `S ∈ Open_F`: the weight excluded by `S` is at most `F`. -/
abbrev OpenF (w : Validator → ℕ) (F : ℕ) (S : Finset Validator) : Prop :=
  wt w Sᶜ ≤ F

/-- If `A` and `B` each clear their own `θ` threshold, their intersection
clears the combined threshold `θ₁ + θ₂ - 1`. The hypothesis is non-strict
(`q ≤ p₁ + p₂`, not `<`) so the boundary case (e.g. `θ = 2/3`) is still
covered, since `OpenTheta` membership is itself already strict. -/
lemma weighted_overlap (w : Validator → ℕ) (p₁ p₂ q : ℕ) (hpq : q ≤ p₁ + p₂)
    (A B : Finset Validator) (hA : OpenTheta w p₁ q A) (hB : OpenTheta w p₂ q B) :
    OpenTheta w (p₁ + p₂ - q) q (A ∩ B) := by
  unfold OpenTheta at hA hB ⊢
  have hunion : wt w (A ∪ B) ≤ totalWeight w := wt_le_totalWeight w (A ∪ B)
  have hsum : wt w (A ∪ B) + wt w (A ∩ B) = wt w A + wt w B := wt_union_add_wt_inter w A B
  have hsumq : q * wt w (A ∪ B) + q * wt w (A ∩ B) = q * wt w A + q * wt w B := by
    rw [← Nat.mul_add, ← Nat.mul_add, hsum]
  have hmulunion : q * wt w (A ∪ B) ≤ q * totalWeight w := Nat.mul_le_mul_left q hunion
  have hp : p₁ * totalWeight w + p₂ * totalWeight w = (p₁ + p₂) * totalWeight w := by
    rw [← Nat.add_mul]
  have hsub : (p₁ + p₂ - q) * totalWeight w + q * totalWeight w = (p₁ + p₂) * totalWeight w := by
    rw [← Nat.add_mul, Nat.sub_add_cancel hpq]
  omega

/-- If the family clears `θ = p/q` at the top size `n`, the same
cross-multiplied threshold, scaled down, still holds at every smaller
prefix size `k ≤ n`. This is what lets `weighted_overlap` be chained one
set at a time in `n_twinedness_theta` without breaking its hypothesis. -/
private lemma cross_mul_mono (p q n : ℕ) (hn : q * (n - 1) ≤ p * n) :
    ∀ k, 1 ≤ k → k ≤ n → q * (k - 1) ≤ p * k := by
  intro k hk1 hkn
  by_cases h : q ≤ p
  · calc q * (k - 1) ≤ p * (k - 1) := Nat.mul_le_mul_right _ h
      _ ≤ p * k := Nat.mul_le_mul_left _ (Nat.sub_le k 1)
  · have h' : p < q := by omega
    obtain ⟨d, hd⟩ := Nat.exists_eq_add_of_le hkn
    obtain ⟨j, hj⟩ := Nat.exists_eq_add_of_le hk1
    have hjk : k - 1 = j := by omega
    have hn1 : n - 1 = j + d := by omega
    rw [hjk]
    rw [hn1, hd] at hn
    nlinarith [hn, h', Nat.zero_le d]

set_option linter.unusedVariables false in
/-- If `θ ≥ (n-1)/n`, any `n` sets each clearing the `θ` threshold have a
common member. Proved by induction: chain `weighted_overlap` across the
family one set at a time, using `cross_mul_mono` to show the intermediate
threshold hypothesis needed at every step follows from the hypothesis at
the full family size `n`. -/
theorem n_twinedness_theta (w : Validator → ℕ) (p q n : ℕ) (hq : 0 < q) (hn : 0 < n)
    (hθ : q * (n - 1) ≤ p * n) (O : ℕ → Finset Validator)
    (hO : ∀ i < n, OpenTheta w p q (O i)) :
    ((Finset.range n).inf O).Nonempty := by
  have key : ∀ k, k ≤ n → 1 ≤ k →
      OpenTheta w (p * k - q * (k - 1)) q ((Finset.range k).inf O) := by
    intro k
    induction k with
    | zero => intro _ h1; omega
    | succ m ih =>
      intro hkn hk1
      rcases Nat.eq_zero_or_pos m with hm0 | hmpos
      · have hm1 : m + 1 = 1 := by omega
        rw [hm1]
        simp only [Finset.range_one, Finset.inf_singleton]
        have h0 : OpenTheta w p q (O 0) := hO 0 (by omega)
        have heq : p * 1 - q * (1 - 1) = p := by omega
        rw [heq]
        exact h0
      · have hmn : m ≤ n := by omega
        have ihm := ih hmn hmpos
        have hnext : OpenTheta w p q (O m) := hO m (by omega)
        have hmono1 : q * (m - 1) ≤ p * m := cross_mul_mono p q n hθ m hmpos hmn
        have hmono2raw : q * ((m + 1) - 1) ≤ p * (m + 1) :=
          cross_mul_mono p q n hθ (m + 1) (by omega) hkn
        have hstep : (m + 1) - 1 = m := by omega
        rw [hstep] at hmono2raw
        have hdistrib : q * (m - 1) + q = q * m := by
          have hm1 : m - 1 + 1 = m := by omega
          calc q * (m - 1) + q = q * ((m - 1) + 1) := by rw [Nat.mul_add, Nat.mul_one]
            _ = q * m := by rw [hm1]
        have hdistrib2 : p * (m + 1) = p * m + p := by rw [Nat.mul_add, Nat.mul_one]
        have hpq_step : q ≤ p + (p * m - q * (m - 1)) := by omega
        have hinf : (Finset.range (m + 1)).inf O = O m ⊓ (Finset.range m).inf O := by
          rw [Finset.range_add_one, Finset.inf_insert]
        rw [hinf]
        have hov := weighted_overlap w p (p * m - q * (m - 1)) q hpq_step
          (O m) ((Finset.range m).inf O) hnext ihm
        have heq2 : p + (p * m - q * (m - 1)) - q = p * (m + 1) - q * m := by omega
        rw [heq2] at hov
        rw [hstep]
        exact hov
  have hfinal := key n le_rfl (by omega)
  unfold OpenTheta at hfinal
  rw [Finset.nonempty_iff_ne_empty]
  intro hempty
  rw [hempty] at hfinal
  simp only [wt, Finset.sum_empty, Nat.mul_zero] at hfinal
  omega

omit [DecidableEq Validator] in
/-- If two sets each clear a `θ ≥ 2/3`-style threshold `q ≤ p·k` for `k=2`
against `q`, and one is combined with the other, the running weight can
never bottom out empty. Small shared helper for the final `.Nonempty` step
below. -/
private lemma openTheta_nonempty (w : Validator → ℕ) (a q : ℕ)
    {S : Finset Validator} (h : OpenTheta w a q S) : S.Nonempty := by
  unfold OpenTheta at h
  rw [Finset.nonempty_iff_ne_empty]
  intro hempty
  rw [hempty] at h
  simp only [wt, Finset.sum_empty, Nat.mul_zero] at h
  omega

/-- If `A` and `B` each exclude at most `F₁`/`F₂` weight, their
intersection excludes at most the sum. -/
lemma openF_inter (w : Validator → ℕ) (F₁ F₂ : ℕ) (A B : Finset Validator)
    (hA : OpenF w F₁ A) (hB : OpenF w F₂ B) : OpenF w (F₁ + F₂) (A ∩ B) := by
  unfold OpenF at hA hB ⊢
  rw [Finset.compl_inter]
  have hsub := wt_union_add_wt_inter w Aᶜ Bᶜ
  omega

/-- If `W > n·F`, any `n` sets each excluding at most `F` weight have a
common member: the excluded weight of the intersection's complement is a
union bound of at most `n·F`, which can't cover everything once `W > n·F`. -/
theorem n_twinedness_F (w : Validator → ℕ) (F n : ℕ) (hW : n * F < totalWeight w)
    (O : ℕ → Finset Validator) (hO : ∀ i < n, OpenF w F (O i)) :
    ((Finset.range n).inf O).Nonempty := by
  have key : ∀ k, k ≤ n → OpenF w (k * F) ((Finset.range k).inf O) := by
    intro k
    induction k with
    | zero =>
      intro _
      simp [OpenF, Finset.range_zero, Finset.inf_empty, Finset.top_eq_univ, Finset.compl_univ]
    | succ m ih =>
      intro hkn
      have hmn : m ≤ n := by omega
      have ihm := ih hmn
      have hnext : OpenF w F (O m) := hO m (by omega)
      have hinf : (Finset.range (m + 1)).inf O = O m ⊓ (Finset.range m).inf O := by
        rw [Finset.range_add_one, Finset.inf_insert]
      rw [hinf]
      have hcomb := openF_inter w F (m * F) (O m) ((Finset.range m).inf O) hnext ihm
      have heq : F + m * F = (m + 1) * F := by ring
      rwa [heq] at hcomb
  have hfinal := key n le_rfl
  unfold OpenF at hfinal
  rw [Finset.nonempty_iff_ne_empty]
  intro hempty
  rw [hempty] at hfinal
  simp only [Finset.compl_empty] at hfinal
  have hbridge : totalWeight w = wt w Finset.univ := rfl
  omega

/-- If the honest validators `H` alone clear the `θ` threshold, then any
two `θ`-threshold sets share an honest member. Proved directly with two
applications of `weighted_overlap` rather than the general induction,
since a fixed triple doesn't need it. -/
theorem honest_triple_intersection (w : Validator → ℕ) (p q : ℕ) (hθ : q * 2 ≤ p * 3)
    (H A B : Finset Validator)
    (hH : OpenTheta w p q H) (hA : OpenTheta w p q A) (hB : OpenTheta w p q B) :
    (A ∩ B ∩ H).Nonempty := by
  have hpq1 : q ≤ p + p := by omega
  have hAB := weighted_overlap w p p q hpq1 A B hA hB
  have hpq2 : q ≤ (p + p - q) + p := by omega
  have hABH := weighted_overlap w (p + p - q) p q hpq2 (A ∩ B) H hAB hH
  exact openTheta_nonempty w _ q hABH

omit [DecidableEq Validator] in
/-- Bridge to the Rust implementation: `strict_two_thirds` in
`cordiality.rs` computes exactly `support_weight * 3 > total_weight * 2`,
which is definitionally `OpenTheta w 2 3 S` up to commuting the
multiplication. -/
theorem strict_two_thirds_iff_openTheta (w : Validator → ℕ) (S : Finset Validator) :
    wt w S * 3 > totalWeight w * 2 ↔ OpenTheta w 2 3 S := by
  unfold OpenTheta
  omega
