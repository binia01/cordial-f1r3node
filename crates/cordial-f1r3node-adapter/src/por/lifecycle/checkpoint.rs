//! Closed checkpoint rating round for the all-rater PoC path.

use cordial_miners_core::execution::PorEquivocationAuthorization;
use cordial_por::{PorError, RatingBatch, RatingSourceCheckpoint, ReputationPenaltyEvents};
use std::collections::BTreeSet;

use crate::por::finality::FinalizedPrefixCheckpoint;

/// A complete, evidence-checked rating batch for one finalized-prefix source.
///
/// Construction is restricted to the exchange closure path. This value has
/// not yet committed a reputation transition or activated Cordial weights.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletedCheckpointRatingRound {
    checkpoint: FinalizedPrefixCheckpoint,
    batch: RatingBatch,
    pub(in crate::por) finalized_authorizations: BTreeSet<PorEquivocationAuthorization>,
    pub(in crate::por) admitted_authorizations: BTreeSet<PorEquivocationAuthorization>,
    pub(in crate::por) penalties: Option<ReputationPenaltyEvents>,
}

impl CompletedCheckpointRatingRound {
    pub(crate) fn new(
        checkpoint: FinalizedPrefixCheckpoint,
        batch: RatingBatch,
        finalized_authorizations: BTreeSet<PorEquivocationAuthorization>,
    ) -> Self {
        Self {
            checkpoint,
            batch,
            finalized_authorizations,
            admitted_authorizations: BTreeSet::new(),
            penalties: None,
        }
    }

    pub fn penalties(&self) -> Option<&ReputationPenaltyEvents> {
        self.penalties.as_ref()
    }

    pub(in crate::por) fn validate_complete_authorizations(
        &self,
        shard_id: &[u8],
    ) -> Result<(), PorError> {
        let expected: BTreeSet<_> = self
            .finalized_authorizations
            .iter()
            .filter(|carrier| carrier.shard_id == shard_id)
            .cloned()
            .collect();
        if expected != self.admitted_authorizations {
            return Err(PorError::InvalidPenaltyEvents(
                "checkpoint penalties omit or add finalized authorizations".into(),
            ));
        }
        Ok(())
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
