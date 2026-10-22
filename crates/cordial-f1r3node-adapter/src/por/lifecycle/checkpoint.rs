//! Closed checkpoint rating round for the all-rater PoC path.

use cordial_por::{RatingBatch, RatingSourceCheckpoint};

use crate::por::finality::FinalizedPrefixCheckpoint;

/// A complete, evidence-checked rating batch for one finalized-prefix source.
///
/// Construction is restricted to the exchange closure path. This value has
/// not yet committed a reputation transition or activated Cordial weights.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletedCheckpointRatingRound {
    checkpoint: FinalizedPrefixCheckpoint,
    batch: RatingBatch,
}

impl CompletedCheckpointRatingRound {
    pub(crate) fn new(checkpoint: FinalizedPrefixCheckpoint, batch: RatingBatch) -> Self {
        Self { checkpoint, batch }
    }

    pub fn checkpoint(&self) -> &FinalizedPrefixCheckpoint {
        &self.checkpoint
    }

    pub fn source(&self) -> RatingSourceCheckpoint {
        self.checkpoint.rating_source()
    }

    pub fn batch(&self) -> &RatingBatch {
        &self.batch
    }
}
