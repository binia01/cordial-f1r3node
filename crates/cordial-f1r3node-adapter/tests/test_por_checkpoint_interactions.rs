use std::collections::HashSet;

use cordial_f1r3node_adapter::{
    ordered_output::OrderedFinalizedOutput,
    por_finality::{PorFinalityError, finalized_prefix_checkpoint},
    por_interactions::{
        PorInteractionError, admit_checkpoint_block_production_interactions,
        extract_checkpoint_block_production_evidence,
    },
    por_ratings::{build_finalized_checkpoint_rating_batch, verify_checkpoint_rating_signature},
};
use cordial_miners_core::{
    Block, BlockContent, BlockIdentity, Blocklace, NodeId, crypto::CryptoVerifier,
};
use cordial_por::{PorConfig, PorError, ReputationState};
use k256::ecdsa::SigningKey;

struct AcceptAll;

impl CryptoVerifier for AcceptAll {
    type Error = String;

    fn verify_block(
        &self,
        _content: &BlockContent,
        _signature: &[u8],
        _creator: &NodeId,
    ) -> Result<(), Self::Error> {
        Ok(())
    }
}

fn key(seed: u8) -> [u8; 32] {
    [seed; 32]
}

fn node(seed: u8) -> NodeId {
    let key = SigningKey::from_slice(&key(seed)).unwrap();
    NodeId(
        key.verifying_key()
            .to_encoded_point(true)
            .as_bytes()
            .to_vec(),
    )
}

fn fixture() -> (Blocklace, OrderedFinalizedOutput) {
    let mut blocklace = Blocklace::new();
    let mut blocks: Vec<BlockIdentity> = Vec::new();
    for tag in 1..=32u8 {
        let creator = if tag <= 16 {
            if tag % 2 == 0 { 2 } else { 1 }
        } else if tag % 2 == 0 {
            3
        } else {
            2
        };
        let mut hash = [0; 32];
        hash[0] = tag;
        let block = Block {
            identity: BlockIdentity {
                content_hash: hash,
                creator: node(creator),
                signature: vec![tag],
            },
            content: BlockContent {
                payload: vec![tag],
                predecessors: blocks.last().cloned().into_iter().collect::<HashSet<_>>(),
            },
        };
        blocklace.insert(block.clone(), &AcceptAll).unwrap();
        blocks.push(block.identity);
    }
    let output = OrderedFinalizedOutput::new(blocks.clone(), blocks.last().cloned(), 3, 32, 32)
        .with_timestamp(0);
    (blocklace, output)
}

fn state(round: u64) -> ReputationState {
    let mut state = ReputationState::new(round);
    for seed in [1, 2, 3, 9] {
        state.set_reputation(node(seed), 100);
    }
    state
}

#[test]
fn second_checkpoint_uses_only_blocks_seventeen_through_thirty_two() {
    let (blocklace, output) = fixture();
    let checkpoint = finalized_prefix_checkpoint(&output, 2).unwrap().unwrap();
    let evidence =
        extract_checkpoint_block_production_evidence(&blocklace, &output, &checkpoint, &node(9))
            .unwrap();

    assert_eq!(evidence.len(), 2);
    assert!(
        evidence
            .iter()
            .all(|item| item.source == checkpoint.rating_source())
    );
    assert_eq!(evidence[0].recipient, node(2));
    assert_eq!(evidence[0].evidence_ref[0], 17);
    assert_eq!(evidence[1].recipient, node(3));
    assert_eq!(evidence[1].evidence_ref[0], 18);
}

#[test]
fn checkpoint_batch_admits_scores_and_signs_the_selected_window() {
    let (blocklace, output) = fixture();
    let checkpoint = finalized_prefix_checkpoint(&output, 2).unwrap().unwrap();
    let batch = build_finalized_checkpoint_rating_batch(
        &blocklace,
        &output,
        &checkpoint,
        &node(9),
        &state(1),
        &PorConfig::default(),
        &key(9),
    )
    .unwrap();

    assert_eq!(batch.round, 2);
    assert_eq!(batch.ratings.len(), 2);
    for rating in &batch.ratings {
        verify_checkpoint_rating_signature(rating, &checkpoint.rating_source()).unwrap();
        assert!(rating.interaction_ref.as_ref().unwrap()[0] >= 17);
    }
}

#[test]
fn checkpoint_admission_rejects_a_changed_prefix_and_wrong_state_round() {
    let (blocklace, output) = fixture();
    let checkpoint = finalized_prefix_checkpoint(&output, 2).unwrap().unwrap();
    let mut altered = checkpoint.clone();
    altered.prefix_hash[0] ^= 1;
    assert_eq!(
        extract_checkpoint_block_production_evidence(&blocklace, &output, &altered, &node(9)),
        Err(PorInteractionError::Checkpoint(
            PorFinalityError::PrefixCheckpointMismatch
        ))
    );
    assert_eq!(
        admit_checkpoint_block_production_interactions(
            &blocklace,
            &output,
            &checkpoint,
            &node(9),
            &state(0),
        ),
        Err(PorInteractionError::Admission(
            PorError::InvalidInteractionStateRound
        ))
    );
}
