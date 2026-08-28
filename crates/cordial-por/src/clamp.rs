//! Deterministic fixed-point reputation clamping.
//!
//! Implements the paper sigmoid-style clamp:
//!
//! R_clamped = R / sqrt(1 + R^2)
//!
//! with all values represented as fixed-point integers using the crate's
//! `scale` (e.g. 1.0 -> 1_000_000_000). No floating-point arithmetic is used.
//!
//! Overflow and error policy (explicit):
//! - `scale == 0` => returns `Err(PorError::InvalidClampScale)`
//! - any intermediate arithmetic overflow (checked operations) => returns
//!   `Err(PorError::ClampOverflow)` (no panic)
//! - the implementation does not silently convert intermediate arithmetic
//!   overflow into a saturated value. A defensive final saturation on the
//!   conversion back to `u64` remains, but intermediate overflows are treated
//!   as errors per the policy above.

use crate::config::{MissingEntryPolicy, PorConfig};
use crate::error::PorError;
use crate::types::{ReputationEntry, ReputationVector, ReputationWeight};

/// Integer square root for u128 returning floor(sqrt(n)).
fn integer_sqrt_u128(n: u128) -> u128 {
    // Binary search over 0..=2^64 because (2^64)^2 = 2^128 which covers u128 range.
    let mut low: u128 = 0;
    let mut high: u128 = 1u128 << 64; // exclusive upper bound (2^64)
    while low + 1 < high {
        let mid = (low + high) / 2;
        match mid.checked_mul(mid) {
            Some(mid_sq) => {
                if mid_sq == n {
                    return mid;
                }
                if mid_sq < n {
                    low = mid;
                } else {
                    high = mid;
                }
            }
            None => {
                // mid^2 overflowed (mid >= 2^64). Treat as too large.
                high = mid;
            }
        }
    }
    // Adjust low upward if (low+1)^2 <= n using checked_mul to avoid overflow.
    loop {
        let next = low + 1;
        match next.checked_mul(next) {
            Some(sq) => {
                if sq <= n {
                    low += 1;
                } else {
                    break;
                }
            }
            None => {
                // (low+1)^2 overflowed, so it's > n
                break;
            }
        }
    }
    // Adjust low downward if low^2 > n
    loop {
        match low.checked_mul(low) {
            Some(sq) => {
                if sq > n {
                    // safe to decrement because low > 0 when sq > n
                    low -= 1;
                } else {
                    break;
                }
            }
            None => {
                // overflowed (shouldn't happen), decrement defensively
                low -= 1;
            }
        }
    }
    low
}

/// Clamp single fixed-point reputation value using integer-only arithmetic.
///
/// Formula (fixed-point derivation):
/// clamp_fixed = round( (r * S) / sqrt(S^2 + r^2) )
/// where `r` is the fixed-point reputation and `S` is the fixed-point scale.
pub fn clamp_reputation_value(
    value: ReputationWeight,
    scale: ReputationWeight,
) -> Result<ReputationWeight, PorError> {
    if scale == 0 {
        return Err(PorError::InvalidClampScale);
    }
    if value == 0 {
        return Ok(0);
    }

    let r = value as u128;
    let s = scale as u128;

    // Compute s^2 and r^2 with checked/saturating arithmetic using u128
    let s2 = s.checked_mul(s).ok_or(PorError::ClampOverflow)?;
    let r2 = r.checked_mul(r).ok_or(PorError::ClampOverflow)?;

    let sum = s2.checked_add(r2).ok_or(PorError::ClampOverflow)?;

    // denom = sqrt(s^2 + r^2)
    let denom = integer_sqrt_u128(sum);
    if denom == 0 {
        return Err(PorError::ClampOverflow);
    }

    // numerator = r * s
    let numerator = r.checked_mul(s).ok_or(PorError::ClampOverflow)?;

    // rounding: (numerator + denom/2) / denom
    let half = denom / 2;
    let numerator_rounded = numerator.checked_add(half).ok_or(PorError::ClampOverflow)?;

    let result = numerator_rounded / denom;

    // result should be <= scale, but be defensive and saturate to u64::MAX if needed
    let result_u64 = if result > (ReputationWeight::MAX as u128) {
        ReputationWeight::MAX
    } else {
        result as ReputationWeight
    };

    Ok(result_u64)
}

/// Clamp an entire ReputationVector, preserving round and entry ordering.
///
/// Does not mutate the input and returns a new ReputationVector with clamped
/// reputation values. Uses PorConfig.scale for the fixed-point scale.
///
/// This clamps every entry. After an alpha blend, use
/// [`clamp_reputation_transition`] instead: the sigmoid is not idempotent, so
/// clamping a CarryForward value that is already the previous finalized
/// reputation would decay it every round.
pub fn clamp_reputation_vector(
    reputation: &ReputationVector,
    config: &PorConfig,
) -> Result<ReputationVector, PorError> {
    let scale = config.scale;
    if scale == 0 {
        return Err(PorError::InvalidClampScale);
    }

    let mut values = Vec::with_capacity(reputation.values.len());
    for entry in &reputation.values {
        let clamped = clamp_reputation_value(entry.reputation, scale)?;
        values.push(ReputationEntry::new(entry.node_id.clone(), clamped));
    }

    Ok(ReputationVector {
        round: reputation.round,
        values,
    })
}

/// Clamp a blended transition, restoring CarryForward entries from previous.
///
/// `CarryForward` means the finalized reputation is the previous value. The
/// sigmoid `R / sqrt(1 + R^2)` is not idempotent for `R > 0`, so clamping that
/// already-finalized value would shrink it every sparse round — an implicit
/// inactivity penalty. Those entries are therefore taken from
/// `previous_reputation`, not from `blended`, so a hand-built blended vector
/// cannot preserve an arbitrary unclamped value. Rated nodes, newly seeded
/// nodes, and every entry under `Reject` or `Neutral` are still clamped.
///
/// `previous_reputation` and `contribution` must be the same vectors passed to
/// [`crate::blend_reputation_transition`], in canonical `NodeId` order.
pub fn clamp_reputation_transition(
    blended: &ReputationVector,
    previous_reputation: &ReputationVector,
    contribution: &ReputationVector,
    config: &PorConfig,
) -> Result<ReputationVector, PorError> {
    if config.missing_entry_policy != MissingEntryPolicy::CarryForward {
        return clamp_reputation_vector(blended, config);
    }

    let scale = config.scale;
    if scale == 0 {
        return Err(PorError::InvalidClampScale);
    }

    let mut previous_index = 0;
    let mut contribution_index = 0;
    let mut values = Vec::with_capacity(blended.values.len());

    for entry in &blended.values {
        advance_to(&mut previous_index, &previous_reputation.values, entry);
        advance_to(&mut contribution_index, &contribution.values, entry);

        let in_previous = has_node_at(previous_reputation, previous_index, entry);
        let in_contribution = has_node_at(contribution, contribution_index, entry);
        let reputation = if in_previous && !in_contribution {
            previous_reputation.values[previous_index].reputation
        } else {
            clamp_reputation_value(entry.reputation, scale)?
        };

        values.push(ReputationEntry::new(entry.node_id.clone(), reputation));
    }

    Ok(ReputationVector {
        round: blended.round,
        values,
    })
}

fn advance_to(index: &mut usize, entries: &[ReputationEntry], target: &ReputationEntry) {
    while *index < entries.len() && entries[*index].node_id < target.node_id {
        *index += 1;
    }
}

fn has_node_at(vector: &ReputationVector, index: usize, target: &ReputationEntry) -> bool {
    matches!(
        vector.values.get(index),
        Some(entry) if entry.node_id == target.node_id
    )
}
