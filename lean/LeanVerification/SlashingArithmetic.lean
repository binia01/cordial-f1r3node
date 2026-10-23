/-
Cordial tiered-slashing arithmetic.

The pure definitions in this module use unbounded natural and rational
numbers.  A separate checked layer below models the fixed-width failure
conditions of the Rust implementation without making machine overflow part of
the mathematical policy.
-/

import Mathlib
import LeanVerification.Slashing

namespace CordialMiners

/-! ## Mathematical policy -/

/-- The four values that determine Cordial's tiered-slashing calculation.

Rust represents `fullSlash` implicitly as `PorConfig.scale`; keeping it
explicit here makes the equality an auditable well-formedness obligation.
-/
structure SlashingConfig where
  scale : Nat
  correlationThreshold : Nat
  basePenalty : Nat
  fullSlash : Nat
  deriving DecidableEq, Repr

/-- Preconditions validated by Rust before slashing arithmetic is evaluated. -/
def SlashingConfig.WellFormed (config : SlashingConfig) : Prop :=
  0 < config.scale ∧
    config.correlationThreshold ≤ config.scale ∧
    config.basePenalty ≤ config.scale ∧
    config.fullSlash = config.scale

/-- The production defaults: a 30% boundary and a 25% base penalty. -/
def defaultSlashingConfig : SlashingConfig where
  scale := 1_000_000_000
  correlationThreshold := 300_000_000
  basePenalty := 250_000_000
  fullSlash := 1_000_000_000

theorem defaultSlashingConfig_wellFormed : defaultSlashingConfig.WellFormed := by
  norm_num [SlashingConfig.WellFormed, defaultSlashingConfig]

/-- The mathematical fraction of active weight that equivocated. -/
def correlatedRatio (equivocatingWeight totalWeight : Nat) : ℚ :=
  (equivocatingWeight : ℚ) / (totalWeight : ℚ)

/-- The configured correlation threshold interpreted as a rational fraction. -/
def correlationThresholdRatio (config : SlashingConfig) : ℚ :=
  (config.correlationThreshold : ℚ) / (config.scale : ℚ)

/-- Exact comparison used by Rust, before any integer division can round. -/
abbrev atOrBelowCorrelationThreshold
    (config : SlashingConfig) (equivocatingWeight totalWeight : Nat) : Prop :=
  equivocatingWeight * config.scale ≤
    totalWeight * config.correlationThreshold

/-- Select the base penalty at or below the threshold and a full slash above it. -/
def selectSlashPenalty
    (config : SlashingConfig) (equivocatingWeight totalWeight : Nat) : Nat :=
  if atOrBelowCorrelationThreshold config equivocatingWeight totalWeight then
    config.basePenalty
  else
    config.fullSlash

/-- Cross multiplication expresses the ordinary rational comparison exactly. -/
theorem atOrBelowCorrelationThreshold_iff_ratio_le
    (config : SlashingConfig) (equivocatingWeight totalWeight : Nat)
    (hScale : 0 < config.scale) (hTotal : 0 < totalWeight) :
    atOrBelowCorrelationThreshold config equivocatingWeight totalWeight ↔
      correlatedRatio equivocatingWeight totalWeight ≤
        correlationThresholdRatio config := by
  rw [correlatedRatio, correlationThresholdRatio]
  rw [div_le_div_iff₀]
  · norm_cast
    simp only [atOrBelowCorrelationThreshold]
    rw [Nat.mul_comm totalWeight config.correlationThreshold]
  · exact_mod_cast hTotal
  · exact_mod_cast hScale

/-- At or below the exact threshold, the configured base tier is selected. -/
theorem selectSlashPenalty_eq_base
    (config : SlashingConfig) (equivocatingWeight totalWeight : Nat)
    (h : atOrBelowCorrelationThreshold config equivocatingWeight totalWeight) :
    selectSlashPenalty config equivocatingWeight totalWeight = config.basePenalty := by
  simp [selectSlashPenalty, h]

/-- Strictly above the exact threshold, the configured full-slash tier is selected. -/
theorem selectSlashPenalty_eq_full
    (config : SlashingConfig) (equivocatingWeight totalWeight : Nat)
    (h : ¬ atOrBelowCorrelationThreshold config equivocatingWeight totalWeight) :
    selectSlashPenalty config equivocatingWeight totalWeight = config.fullSlash := by
  simp [selectSlashPenalty, h]

/-- A valid policy never selects a penalty greater than its fixed-point scale. -/
theorem selectSlashPenalty_le_scale
    (config : SlashingConfig) (equivocatingWeight totalWeight : Nat)
    (hConfig : config.WellFormed) :
    selectSlashPenalty config equivocatingWeight totalWeight ≤ config.scale := by
  by_cases hTier : atOrBelowCorrelationThreshold config equivocatingWeight totalWeight
  · rw [selectSlashPenalty_eq_base config equivocatingWeight totalWeight hTier]
    exact hConfig.2.2.1
  · rw [selectSlashPenalty_eq_full config equivocatingWeight totalWeight hTier]
    exact hConfig.2.2.2.le

/-- Exactly 30% remains in the 25% tier under the production defaults. -/
theorem default_exact_thirty_percent :
    selectSlashPenalty defaultSlashingConfig 3 10 = 250_000_000 := by
  norm_num [selectSlashPenalty, atOrBelowCorrelationThreshold,
    defaultSlashingConfig]

/-- The smallest integer unit above 30% is a full slash, even though a rounded
division to fixed-point units could discard that unit. -/
theorem default_smallest_above_thirty_percent :
    selectSlashPenalty defaultSlashingConfig 3_000_000_001 10_000_000_000 =
      1_000_000_000 := by
  norm_num [selectSlashPenalty, atOrBelowCorrelationThreshold,
    defaultSlashingConfig]

/-- Integer division would round the preceding above-threshold example back to
the threshold.  Together with `default_smallest_above_thirty_percent`, this
records why tier selection must happen by cross multiplication first. -/
theorem rounded_ratio_loses_smallest_above_threshold :
    3_000_000_001 * defaultSlashingConfig.scale / 10_000_000_000 =
      defaultSlashingConfig.correlationThreshold := by
  norm_num [defaultSlashingConfig]

/-! ## Applying the selected penalty -/

/-- Reputation retained after applying a fixed-point penalty.  Natural-number
division is floor division, exactly as in Rust. -/
def retainedReputation
    (config : SlashingConfig) (oldWeight penalty : Nat) : Nat :=
  oldWeight * (config.scale - penalty) / config.scale

/-- A valid penalty ratio makes subtraction exact rather than underflowing. -/
theorem survivalFraction_add_penalty
    (config : SlashingConfig) (penalty : Nat) (hPenalty : penalty ≤ config.scale) :
    config.scale - penalty + penalty = config.scale :=
  Nat.sub_add_cancel hPenalty

/-- Applying a valid penalty can never increase reputation. -/
theorem retainedReputation_le
    (config : SlashingConfig) (oldWeight penalty : Nat)
    (hPenalty : penalty ≤ config.scale) :
    retainedReputation config oldWeight penalty ≤ oldWeight := by
  have hExact : config.scale - penalty + penalty = config.scale :=
    survivalFraction_add_penalty config penalty hPenalty
  have hSurvival : config.scale - penalty ≤ config.scale := by omega
  apply Nat.div_le_of_le_mul
  calc
    oldWeight * (config.scale - penalty) ≤ oldWeight * config.scale :=
      Nat.mul_le_mul_left oldWeight hSurvival
    _ = config.scale * oldWeight := Nat.mul_comm _ _

/-- A full slash retains no reputation. -/
theorem retainedReputation_full_slash
    (config : SlashingConfig) (oldWeight : Nat) :
    retainedReputation config oldWeight config.scale = 0 := by
  simp [retainedReputation]

/-- Zero reputation remains zero under every penalty. -/
theorem retainedReputation_zero
    (config : SlashingConfig) (penalty : Nat) :
    retainedReputation config 0 penalty = 0 := by
  simp [retainedReputation]

/-- The base-tier result is precisely the documented floor-divided formula. -/
theorem retainedReputation_base_penalty
    (config : SlashingConfig) (oldWeight : Nat) :
    retainedReputation config oldWeight config.basePenalty =
      oldWeight * (config.scale - config.basePenalty) / config.scale :=
  rfl

/-- The discarded remainder is the only difference from exact rational
multiplication: multiplying the retained integer by the scale cannot exceed
the pre-division numerator. -/
theorem retainedReputation_floor_bound
    (config : SlashingConfig) (oldWeight penalty : Nat) :
    retainedReputation config oldWeight penalty * config.scale ≤
      oldWeight * (config.scale - penalty) := by
  exact Nat.div_mul_le_self _ _

example : retainedReputation defaultSlashingConfig 1_000_000_000 250_000_000 =
    750_000_000 := by native_decide

example : retainedReputation defaultSlashingConfig 10 250_000_000 = 7 := by
  native_decide

end CordialMiners
