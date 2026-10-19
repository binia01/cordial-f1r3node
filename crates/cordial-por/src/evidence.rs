//! Verified proof -> protocol authorization -> penalty capability.
//!
//! Policies are trusted protocol implementations, never selected by a peer.
//! No byte-only constructor or decoder can create an authorized penalty.

use cordial_miners_core::{
    NodeId,
    consensus::VerifiedEquivocationEvidence,
    crypto::{Blake2b256Hasher, Hasher},
};

use crate::{
    EquivocationPenalty, MAX_PENALTY_EVIDENCE_LEN, MAX_REPUTATION_BLOCK_SHARD_ID_LEN, PorError,
    ReputationBlockContext, rating_round_from_finalized_wave,
};

pub const POR_EQUIVOCATION_AUTHORIZATION_DOMAIN: &[u8] =
    b"cordial-por:equivocation-authorization:v1";

/// Context approved by the protocol. Cordial depths and PoR rounds are distinct.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EquivocationAuthorizationContext {
    pub shard_id: Vec<u8>,
    pub source_finalized_wave: u64,
    pub round: u64,
}

/// The adapter must supply a policy backed by its trusted authorization path.
/// Proof authentication has already succeeded when this method is called.
pub trait EquivocationAuthorizationPolicy {
    fn authorize(
        &self,
        context: &EquivocationAuthorizationContext,
        evidence: &VerifiedEquivocationEvidence,
    ) -> Result<(), PorError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EquivocationAuthorization {
    context: EquivocationAuthorizationContext,
    binding: [u8; 32],
    proof_round: u64,
}

/// Exact payload a host may finalize to authorize this proof in this context.
/// Building this payload grants no authority; the policy must establish approval.
pub fn equivocation_authorization_payload(
    context: &EquivocationAuthorizationContext,
    evidence: &VerifiedEquivocationEvidence,
) -> Result<Vec<u8>, PorError> {
    authorization_payload(context, evidence.validator(), &evidence.canonical_bytes())
}

fn authorization_payload(
    context: &EquivocationAuthorizationContext,
    offender: &NodeId,
    evidence: &[u8],
) -> Result<Vec<u8>, PorError> {
    if context.shard_id.is_empty()
        || context.shard_id.len() > MAX_REPUTATION_BLOCK_SHARD_ID_LEN
        || rating_round_from_finalized_wave(context.source_finalized_wave)? != context.round
        || evidence.is_empty()
        || evidence.len() > MAX_PENALTY_EVIDENCE_LEN
    {
        return Err(PorError::InvalidPenaltyEvents(
            "invalid equivocation authorization context or proof size".into(),
        ));
    }
    fn put_bytes(output: &mut Vec<u8>, bytes: &[u8]) {
        output.extend_from_slice(&(bytes.len() as u64).to_be_bytes());
        output.extend_from_slice(bytes);
    }
    let mut payload = POR_EQUIVOCATION_AUTHORIZATION_DOMAIN.to_vec();
    put_bytes(&mut payload, &context.shard_id);
    payload.extend_from_slice(&context.source_finalized_wave.to_be_bytes());
    payload.extend_from_slice(&context.round.to_be_bytes());
    put_bytes(&mut payload, &offender.0);
    put_bytes(&mut payload, evidence);
    Ok(payload)
}

impl EquivocationPenalty {
    /// Retain a raw claim for inspection. This value always fails penalty replay
    /// until replaced by the result of authorize with an authenticated proof.
    pub fn unverified(offender: NodeId, evidence: Vec<u8>) -> Self {
        Self {
            offender,
            evidence,
            authorization: None,
        }
    }

    /// Derive the offender and exact proof bytes; callers cannot substitute them.
    pub fn authorize(
        proof: &VerifiedEquivocationEvidence,
        context: EquivocationAuthorizationContext,
        policy: &impl EquivocationAuthorizationPolicy,
    ) -> Result<Self, PorError> {
        let evidence = proof.canonical_bytes();
        let payload = authorization_payload(&context, proof.validator(), &evidence)?;
        policy.authorize(&context, proof)?;
        Ok(Self {
            offender: proof.validator().clone(),
            evidence,
            authorization: Some(EquivocationAuthorization {
                context,
                binding: Blake2b256Hasher.hash(&payload),
                proof_round: proof.round(),
            }),
        })
    }

    pub(crate) fn proof_round(&self) -> Result<u64, PorError> {
        self.authorization
            .as_ref()
            .map(|authorization| authorization.proof_round)
            .ok_or_else(|| {
                PorError::InvalidPenaltyEvents("equivocation has no verified authorization".into())
            })
    }

    pub(crate) fn validate_authorization(&self, round: u64) -> Result<(), PorError> {
        let authorization = self.authorization.as_ref().ok_or_else(|| {
            PorError::InvalidPenaltyEvents("equivocation has no verified authorization".into())
        })?;
        let payload =
            authorization_payload(&authorization.context, &self.offender, &self.evidence)?;
        if authorization.context.round != round
            || authorization.binding != Blake2b256Hasher.hash(&payload)
        {
            return Err(PorError::InvalidPenaltyEvents(
                "equivocation proof, offender or authorized round was changed".into(),
            ));
        }
        Ok(())
    }

    pub(crate) fn validate_context(
        &self,
        context: ReputationBlockContext<'_>,
    ) -> Result<(), PorError> {
        let authorization = self.authorization.as_ref().ok_or_else(|| {
            PorError::InvalidPenaltyEvents("equivocation has no verified authorization".into())
        })?;
        if authorization.context.shard_id != context.shard_id
            || authorization.context.source_finalized_wave != context.source_finalized_wave
        {
            return Err(PorError::InvalidPenaltyEvents(
                "equivocation authorization belongs to another shard or finalized wave".into(),
            ));
        }
        Ok(())
    }
}

pub(crate) fn validate_penalty_context(
    penalties: Option<&crate::ReputationPenaltyEvents>,
    context: ReputationBlockContext<'_>,
) -> Result<(), PorError> {
    if let Some(events) = penalties {
        for event in &events.equivocations {
            event.validate_context(context)?;
        }
    }
    Ok(())
}
