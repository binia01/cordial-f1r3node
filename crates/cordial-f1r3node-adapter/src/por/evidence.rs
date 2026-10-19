//! Finalized Cordial authorization for PoR equivocation penalties.
//!
//! Evidence-pool entries are untrusted claims. The authorization payload must
//! be signed by an authorized validator and included in the trusted finalized
//! output for this PoR source wave. Checkpoint attestations alone do not
//! authenticate a proof or authorize its inclusion.
//!
//! Hosts retain raw proofs and complete ancestry so receivers can repeat
//! verification. Authority is never decoded from peer bytes.

use std::collections::HashMap;

use cordial_miners_core::{
    Blocklace, NodeId,
    consensus::{
        CordialEquivocationEvidence, VerifiedEquivocationEvidence, depth,
        verify_equivocation_evidence, wave_of_round,
    },
    crypto,
};
use cordial_por::{
    EquivocationAuthorizationContext, EquivocationAuthorizationPolicy, EquivocationPenalty,
    PorError, ReputationPenaltyEvents, equivocation_authorization_payload,
    penalty_events_commitment,
};

use crate::ordered_output::OrderedFinalizedOutput;

use super::{
    finality::FinalizedRatingRound, interactions::validate_finalized_rating_round,
    lifecycle::CompletedPorRatingRound,
};

/// Policy over trusted Cordial finalized output and membership.
/// Both inputs must come from the adapter's consensus path, never from a peer.
pub struct FinalizedEquivocationAuthorization<'a> {
    blocklace: &'a Blocklace,
    output: &'a OrderedFinalizedOutput,
    opened: FinalizedRatingRound,
    authorized_validators: &'a HashMap<NodeId, u64>,
}

impl<'a> FinalizedEquivocationAuthorization<'a> {
    pub fn new(
        blocklace: &'a Blocklace,
        output: &'a OrderedFinalizedOutput,
        opened: FinalizedRatingRound,
        authorized_validators: &'a HashMap<NodeId, u64>,
    ) -> Result<Self, PorError> {
        validate_finalized_rating_round(blocklace, output, opened)
            .map_err(|error| PorError::InvalidPenaltyEvents(error.to_string()))?;
        Ok(Self {
            blocklace,
            output,
            opened,
            authorized_validators,
        })
    }
}

impl EquivocationAuthorizationPolicy for FinalizedEquivocationAuthorization<'_> {
    fn authorize(
        &self,
        context: &EquivocationAuthorizationContext,
        evidence: &VerifiedEquivocationEvidence,
    ) -> Result<(), PorError> {
        let invalid = |message: &str| PorError::InvalidPenaltyEvents(message.into());
        if context.round != self.opened.rating_round
            || context.source_finalized_wave != self.opened.finalized_wave
            || self
                .authorized_validators
                .get(evidence.validator())
                .copied()
                .unwrap_or(0)
                == 0
        {
            return Err(invalid(
                "equivocation is outside the authorized validator/round context",
            ));
        }
        for block in evidence.blocks() {
            let round = cordial_miners_core::consensus::round::candidate_depth(
                self.blocklace,
                &block.content,
            )
            .ok_or_else(|| invalid("equivocation ancestry is unavailable"))?;
            if wave_of_round(round, self.output.wavelength)
                .is_none_or(|wave| wave > self.opened.finalized_wave)
            {
                return Err(invalid(
                    "equivocation is newer than the finalized source wave",
                ));
            }
        }
        let payload = equivocation_authorization_payload(context, evidence)?;
        for reference in &self.output.blocks {
            let block = self
                .blocklace
                .get(reference)
                .ok_or_else(|| invalid("finalized authorization block is unavailable"))?;
            if block.content.payload != payload {
                continue;
            }
            let block_round = depth(self.blocklace, &block.identity)
                .ok_or_else(|| invalid("authorization block ancestry is unavailable"))?;
            if wave_of_round(block_round, self.output.wavelength)
                != Some(self.opened.finalized_wave)
                || self
                    .authorized_validators
                    .get(&block.identity.creator)
                    .copied()
                    .unwrap_or(0)
                    == 0
                || crypto::hash_content(&block.content) != block.identity.content_hash
                || !crypto::verify(
                    &block.identity.content_hash,
                    &block.identity.creator.0,
                    &block.identity.signature,
                )
            {
                return Err(invalid(
                    "invalid finalized equivocation authorization block",
                ));
            }
            return Ok(());
        }
        Err(invalid(
            "verified equivocation has no finalized authorization payload",
        ))
    }
}

impl CompletedPorRatingRound {
    /// Verify raw retained evidence and require finalized protocol authorization.
    /// Admission is atomic. One immutable event set drives local construction,
    /// checkpoint replay, durable application, and weight export.
    pub fn with_authorized_equivocations(
        mut self,
        raw: &[CordialEquivocationEvidence],
        blocklace: &Blocklace,
        output: &OrderedFinalizedOutput,
        authorized_validators: &HashMap<NodeId, u64>,
        shard_id: &[u8],
    ) -> Result<Self, PorError> {
        let policy = FinalizedEquivocationAuthorization::new(
            blocklace,
            output,
            self.opened(),
            authorized_validators,
        )?;
        let context = EquivocationAuthorizationContext {
            shard_id: shard_id.to_vec(),
            source_finalized_wave: self.opened().finalized_wave,
            round: self.batch().round,
        };
        let equivocations = raw
            .iter()
            .map(|claim| {
                let proof = verify_equivocation_evidence(claim, blocklace)
                    .map_err(|error| PorError::InvalidPenaltyEvents(error.to_string()))?;
                EquivocationPenalty::authorize(&proof, context.clone(), &policy)
            })
            .collect::<Result<Vec<_>, PorError>>()?;
        let events = ReputationPenaltyEvents {
            round: self.batch().round,
            equivocations,
            inactivity: Vec::new(),
        };
        penalty_events_commitment(events.round, Some(&events))?;
        self.set_authorized_penalties(events);
        Ok(self)
    }
}
