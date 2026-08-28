use crate::types::{RatingScore, ReputationWeight};

/// How the transition treats a node that appears in only one of the two vectors
/// it blends.
///
/// A round is sparse when a node receives no ratings: Liquid Rank emits no
/// contribution entry for it. Naming the fallback keeps reputation from being
/// carried forward silently, which is what the original strict rejection
/// guarded against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MissingEntryPolicy {
    /// Reject the round unless both vectors cover the same node set.
    Reject,

    /// Treat an unrated node's contribution as its previous reputation and take
    /// the finalized value from previous reputation, so it is unchanged.
    ///
    /// Absence of ratings is not evidence of inactivity: a node can be online
    /// and simply not interacted with. Punishing that belongs to the inactivity
    /// penalty stage, which knows the missed-round count. The clamp is skipped
    /// because it is not idempotent: applying it to an already-finalized value
    /// would decay reputation every sparse round. The previous value is copied
    /// rather than trusting the blended entry, so a hand-built blend cannot
    /// preserve an arbitrary unclamped value.
    #[default]
    CarryForward,

    /// Treat an unrated node's contribution as `initial_reputation`, drifting
    /// unrated nodes toward the configured baseline.
    Neutral,
}

/// Configuration parameters for PoR calculations and transitions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PorConfig {
    /// Fixed point scale.
    pub scale: ReputationWeight,

    /// Initial reputation.
    pub initial_reputation: ReputationWeight,

    /// Fixed-point alpha used to blend Liquid-Rank contribution with prior
    /// reputation.
    pub liquid_rank_alpha: ReputationWeight,

    /// Minimum accepted rating.
    pub minimum_rating: RatingScore,

    /// Maximum accepted rating.
    pub maximum_rating: RatingScore,

    /// Fallback applied when the contribution and previous reputation vectors
    /// cover different node sets.
    pub missing_entry_policy: MissingEntryPolicy,
}

impl PorConfig {
    pub const DEFAULT_SCALE: ReputationWeight = 1_000_000_000;

    pub const DEFAULT_INITIAL_REPUTATION: ReputationWeight = 200_000_000;

    pub fn new(scale: ReputationWeight, initial_reputation: ReputationWeight) -> Self {
        Self {
            scale,
            initial_reputation,

            liquid_rank_alpha: 600_000_000,

            minimum_rating: 0,

            maximum_rating: scale,

            missing_entry_policy: MissingEntryPolicy::default(),
        }
    }
}

impl Default for PorConfig {
    fn default() -> Self {
        Self::new(Self::DEFAULT_SCALE, Self::DEFAULT_INITIAL_REPUTATION)
    }
}
