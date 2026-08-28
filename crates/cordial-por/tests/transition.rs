use cordial_miners_core::NodeId;
use cordial_por::{
    MissingEntryPolicy, PorConfig, PorError, ReputationEntry, ReputationVector,
    blend_reputation_transition,
};

fn cfg(scale: u64, alpha: u64) -> PorConfig {
    cfg_with(scale, alpha, 0, MissingEntryPolicy::default())
}

fn cfg_with(
    scale: u64,
    alpha: u64,
    initial_reputation: u64,
    missing_entry_policy: MissingEntryPolicy,
) -> PorConfig {
    PorConfig {
        scale,
        initial_reputation,
        liquid_rank_alpha: alpha,
        minimum_rating: 0,
        maximum_rating: scale,
        missing_entry_policy,
    }
}

fn reject(scale: u64, alpha: u64) -> PorConfig {
    cfg_with(scale, alpha, 0, MissingEntryPolicy::Reject)
}

fn entry(node: u8, reputation: u64) -> ReputationEntry {
    ReputationEntry::new(NodeId(vec![node]), reputation)
}

fn vector(round: u64, values: Vec<ReputationEntry>) -> ReputationVector {
    ReputationVector { round, values }
}

#[test]
fn blends_contribution_with_previous_reputation() {
    let contribution = vector(7, vec![entry(1, 90), entry(2, 20)]);
    let previous = vector(6, vec![entry(1, 50), entry(2, 80)]);

    let next = blend_reputation_transition(&contribution, &previous, &cfg(100, 60)).unwrap();

    assert_eq!(next.round, 7);
    assert_eq!(next.values, vec![entry(1, 74), entry(2, 44)]);
}

#[test]
fn alpha_zero_preserves_previous_reputation() {
    let contribution = vector(7, vec![entry(1, 90)]);
    let previous = vector(6, vec![entry(1, 50)]);

    let next = blend_reputation_transition(&contribution, &previous, &cfg(100, 0)).unwrap();

    assert_eq!(next.values, vec![entry(1, 50)]);
}

#[test]
fn alpha_equal_to_scale_uses_only_contribution() {
    let contribution = vector(7, vec![entry(1, 90)]);
    let previous = vector(6, vec![entry(1, 50)]);

    let next = blend_reputation_transition(&contribution, &previous, &cfg(100, 100)).unwrap();

    assert_eq!(next.values, vec![entry(1, 90)]);
}

#[test]
fn missing_previous_reputation_is_rejected() {
    let contribution = vector(7, vec![entry(1, 90), entry(2, 20)]);
    let previous = vector(6, vec![entry(1, 50)]);

    assert_eq!(
        blend_reputation_transition(&contribution, &previous, &reject(100, 60)),
        Err(PorError::MissingPreviousReputation)
    );
}

#[test]
fn extra_previous_reputation_entry_is_rejected() {
    let contribution = vector(7, vec![entry(1, 90), entry(3, 40)]);
    let previous = vector(6, vec![entry(1, 50), entry(2, 70), entry(3, 20)]);

    assert_eq!(
        blend_reputation_transition(&contribution, &previous, &reject(100, 50)),
        Err(PorError::MissingContributionEntry)
    );
}

#[test]
fn different_equal_length_node_sets_are_rejected() {
    let contribution = vector(7, vec![entry(1, 90), entry(2, 40)]);
    let previous = vector(6, vec![entry(1, 50), entry(3, 70)]);

    assert_eq!(
        blend_reputation_transition(&contribution, &previous, &reject(100, 50)),
        Err(PorError::MissingPreviousReputation)
    );
}

#[test]
fn alpha_greater_than_scale_is_rejected() {
    let contribution = vector(7, vec![entry(1, 90)]);
    let previous = vector(6, vec![entry(1, 50)]);

    assert_eq!(
        blend_reputation_transition(&contribution, &previous, &cfg(100, 101)),
        Err(PorError::InvalidLiquidRankAlpha)
    );
}

#[test]
fn zero_scale_is_rejected() {
    let contribution = vector(7, vec![entry(1, 90)]);
    let previous = vector(6, vec![entry(1, 50)]);

    assert_eq!(
        blend_reputation_transition(&contribution, &previous, &cfg(0, 0)),
        Err(PorError::InvalidTransitionScale)
    );
}

#[test]
fn widens_intermediate_terms_before_multiplication() {
    let contribution = vector(7, vec![entry(1, u64::MAX)]);
    let previous = vector(6, vec![entry(1, u64::MAX - 100)]);

    let next = blend_reputation_transition(&contribution, &previous, &cfg(100, 60)).unwrap();

    assert_eq!(next.values, vec![entry(1, u64::MAX - 40)]);
}

#[test]
fn preserves_canonical_contribution_order() {
    let contribution = vector(7, vec![entry(1, 90), entry(3, 40)]);
    let previous = vector(6, vec![entry(1, 50), entry(3, 20)]);

    let next = blend_reputation_transition(&contribution, &previous, &cfg(100, 50)).unwrap();

    assert_eq!(next.values, vec![entry(1, 70), entry(3, 30)]);
}

#[test]
fn unsorted_contribution_is_rejected() {
    let contribution = vector(7, vec![entry(3, 40), entry(1, 90)]);
    let previous = vector(6, vec![entry(1, 50), entry(3, 20)]);

    assert_eq!(
        blend_reputation_transition(&contribution, &previous, &cfg(100, 50)),
        Err(PorError::UnsortedReputationVector)
    );
}

#[test]
fn unsorted_previous_reputation_is_rejected() {
    let contribution = vector(7, vec![entry(1, 90), entry(3, 40)]);
    let previous = vector(6, vec![entry(3, 20), entry(1, 50)]);

    assert_eq!(
        blend_reputation_transition(&contribution, &previous, &cfg(100, 50)),
        Err(PorError::UnsortedReputationVector)
    );
}

#[test]
fn non_consecutive_rounds_are_rejected() {
    let contribution = vector(10, vec![entry(1, 90)]);
    let previous = vector(3, vec![entry(1, 50)]);

    assert_eq!(
        blend_reputation_transition(&contribution, &previous, &cfg(100, 50)),
        Err(PorError::InvalidTransitionRound)
    );
}

#[test]
fn maximum_previous_round_is_rejected_without_overflow() {
    let contribution = vector(0, vec![entry(1, 90)]);
    let previous = vector(u64::MAX, vec![entry(1, 50)]);

    assert_eq!(
        blend_reputation_transition(&contribution, &previous, &cfg(100, 50)),
        Err(PorError::InvalidTransitionRound)
    );
}

#[test]
fn duplicate_contribution_entries_are_rejected() {
    let contribution = vector(7, vec![entry(1, 90), entry(1, 80)]);
    let previous = vector(6, vec![entry(1, 50)]);

    assert_eq!(
        blend_reputation_transition(&contribution, &previous, &cfg(100, 50)),
        Err(PorError::DuplicateReputationEntry)
    );
}

#[test]
fn duplicate_previous_entries_are_rejected() {
    let contribution = vector(7, vec![entry(1, 90)]);
    let previous = vector(6, vec![entry(1, 50), entry(1, 40)]);

    assert_eq!(
        blend_reputation_transition(&contribution, &previous, &cfg(100, 50)),
        Err(PorError::DuplicateReputationEntry)
    );
}

#[test]
fn empty_matching_vectors_return_empty_transition() {
    let contribution = vector(7, Vec::new());
    let previous = vector(6, Vec::new());

    let next = blend_reputation_transition(&contribution, &previous, &cfg(100, 50)).unwrap();

    assert_eq!(next.round, 7);
    assert!(next.values.is_empty());
}

#[test]
fn sums_weighted_terms_before_dividing() {
    let contribution = vector(7, vec![entry(1, 1)]);
    let previous = vector(6, vec![entry(1, 1)]);

    let next = blend_reputation_transition(&contribution, &previous, &cfg(100, 50)).unwrap();

    assert_eq!(next.values, vec![entry(1, 1)]);
}

#[test]
fn carry_forward_leaves_an_unrated_node_unchanged() {
    let contribution = vector(7, vec![entry(1, 90)]);
    let previous = vector(6, vec![entry(1, 50), entry(2, 80)]);
    let config = cfg_with(100, 50, 20, MissingEntryPolicy::CarryForward);

    let next = blend_reputation_transition(&contribution, &previous, &config).unwrap();

    assert_eq!(next.values, vec![entry(1, 70), entry(2, 80)]);
}

#[test]
fn neutral_drifts_an_unrated_node_toward_initial_reputation() {
    let contribution = vector(7, vec![entry(1, 90)]);
    let previous = vector(6, vec![entry(1, 50), entry(2, 80)]);
    let config = cfg_with(100, 50, 20, MissingEntryPolicy::Neutral);

    let next = blend_reputation_transition(&contribution, &previous, &config).unwrap();

    assert_eq!(next.values, vec![entry(1, 70), entry(2, 50)]);
}

#[test]
fn new_node_is_seeded_from_initial_reputation() {
    let contribution = vector(7, vec![entry(1, 90), entry(2, 60)]);
    let previous = vector(6, vec![entry(1, 50)]);
    let config = cfg_with(100, 50, 20, MissingEntryPolicy::CarryForward);

    let next = blend_reputation_transition(&contribution, &previous, &config).unwrap();

    assert_eq!(next.values, vec![entry(1, 70), entry(2, 40)]);
}

#[test]
fn output_covers_the_union_in_canonical_order() {
    let contribution = vector(7, vec![entry(1, 90), entry(3, 60)]);
    let previous = vector(6, vec![entry(2, 40), entry(3, 80)]);
    let config = cfg_with(100, 50, 20, MissingEntryPolicy::CarryForward);

    let next = blend_reputation_transition(&contribution, &previous, &config).unwrap();

    assert_eq!(next.values, vec![entry(1, 55), entry(2, 40), entry(3, 70)]);
}

#[test]
fn reject_policy_still_refuses_a_new_node() {
    let contribution = vector(7, vec![entry(1, 90), entry(2, 60)]);
    let previous = vector(6, vec![entry(1, 50)]);

    assert_eq!(
        blend_reputation_transition(&contribution, &previous, &reject(100, 50)),
        Err(PorError::MissingPreviousReputation)
    );
}
