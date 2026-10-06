use cordial_miners_core::NodeId;
use cordial_por::{
    EquivocationPenalty, InactivityPenalty, PorConfig, PorError, RatingRecord, ReputationBlock,
    ReputationBlockContext, ReputationEntry, ReputationPenaltyEvents, ReputationState,
    ReputationVector, build_rating_batch, build_reputation_block, decode_reputation_state_snapshot,
    encode_reputation_state_snapshot, replay_reputation_transition,
    replay_reputation_transition_with_penalties, reputation_list_commitment,
    verify_reputation_transition, verify_reputation_transition_with_penalties,
};

fn node(id: u8) -> NodeId {
    NodeId(vec![id])
}
fn previous() -> ReputationVector {
    ReputationVector {
        round: 0,
        values: (1..=4)
            .map(|id| ReputationEntry::new(node(id), 1000))
            .collect(),
    }
}
fn config() -> PorConfig {
    PorConfig::new(1000, 200)
}
fn events(slashed: &[u8], inactive: &[u8]) -> ReputationPenaltyEvents {
    ReputationPenaltyEvents {
        round: 1,
        equivocations: slashed
            .iter()
            .map(|id| EquivocationPenalty {
                offender: node(*id),
                evidence: vec![*id],
            })
            .collect(),
        inactivity: inactive
            .iter()
            .map(|id| InactivityPenalty {
                offender: node(*id),
                missed_rounds: 1,
            })
            .collect(),
    }
}
fn context() -> ReputationBlockContext<'static> {
    ReputationBlockContext {
        shard_id: b"root",
        source_finalized_wave: 0,
        previous_block: None,
    }
}
fn block(events: &ReputationPenaltyEvents) -> ReputationBlock {
    let config = config();
    let list =
        replay_reputation_transition_with_penalties(&previous(), &[], 1, &config, Some(events))
            .unwrap();
    build_reputation_block(
        context(),
        &build_rating_batch(1, vec![], &config).unwrap(),
        list,
        &config,
    )
    .unwrap()
}

#[test]
fn replay_and_audit_apply_isolated_slash_and_inactivity() {
    let events = events(&[1], &[2]);
    let block = block(&events);
    let weights: Vec<_> = block
        .reputation_list
        .entries
        .iter()
        .map(|entry| entry.reputation)
        .collect();
    assert_eq!(weights, vec![750, 990, 1000, 1000]);
    verify_reputation_transition_with_penalties(
        &previous(),
        &[],
        &block,
        context(),
        &config(),
        Some(&events),
    )
    .unwrap();
    assert_eq!(
        verify_reputation_transition(&previous(), &[], &block, context(), &config()),
        Err(PorError::ReputationValueMismatch)
    );
}

#[test]
fn replay_applies_correlated_full_slash_independent_of_event_order() {
    let mut events = events(&[1, 2], &[]);
    let first = block(&events);
    events.equivocations.reverse();
    assert_eq!(block(&events), first);
    assert_eq!(first.reputation_list.entries[0].reputation, 0);
    assert_eq!(first.reputation_list.entries[1].reputation, 0);
    verify_reputation_transition_with_penalties(
        &previous(),
        &[],
        &first,
        context(),
        &config(),
        Some(&events),
    )
    .unwrap();
}

#[test]
fn forged_penalty_result_is_rejected_even_with_recomputed_root() {
    let events = events(&[1], &[2]);
    let mut block = block(&events);
    block.reputation_list.entries[0].reputation += 1;
    block.header.reputation_root = reputation_list_commitment(&block.reputation_list).unwrap();
    assert_eq!(
        verify_reputation_transition_with_penalties(
            &previous(),
            &[],
            &block,
            context(),
            &config(),
            Some(&events)
        ),
        Err(PorError::ReputationValueMismatch)
    );
}

#[test]
fn configuration_change_is_rejected_before_replay() {
    let events = events(&[1], &[]);
    let block = block(&events);
    let mut config = config();
    config.base_slash_penalty += 1;
    assert_eq!(
        verify_reputation_transition_with_penalties(
            &previous(),
            &[],
            &block,
            context(),
            &config,
            Some(&events)
        ),
        Err(PorError::ReputationBlockConfigHashMismatch)
    );
}

#[test]
fn absence_alone_does_not_trigger_decay_and_none_preserves_old_api() {
    let expected = replay_reputation_transition(&previous(), &[], 1, &config()).unwrap();
    assert_eq!(expected.entries, previous().values);
    assert_eq!(
        replay_reputation_transition_with_penalties(&previous(), &[], 1, &config(), None).unwrap(),
        expected
    );
    assert_eq!(
        replay_reputation_transition_with_penalties(
            &previous(),
            &[],
            1,
            &config(),
            Some(&events(&[], &[]))
        )
        .unwrap(),
        expected
    );
}

#[test]
fn explicit_inactivity_compounds_once_per_consecutive_round() {
    let first = replay_reputation_transition_with_penalties(
        &previous(),
        &[],
        1,
        &config(),
        Some(&events(&[], &[1])),
    )
    .unwrap();
    let previous = ReputationVector {
        round: 1,
        values: first.entries,
    };
    let mut second_events = events(&[], &[1]);
    second_events.round = 2;
    let next = replay_reputation_transition_with_penalties(
        &previous,
        &[],
        2,
        &config(),
        Some(&second_events),
    )
    .unwrap();
    assert_eq!(next.entries[0].reputation, 980);
}

#[test]
fn invalid_or_ambiguous_events_are_rejected() {
    let mut wrong_round = events(&[1], &[]);
    wrong_round.round = 2;
    let mut empty_evidence = events(&[1], &[]);
    empty_evidence.equivocations[0].evidence.clear();
    let mut cumulative_inactivity = events(&[], &[1]);
    cumulative_inactivity.inactivity[0].missed_rounds = 2;
    for events in [
        wrong_round,
        empty_evidence,
        cumulative_inactivity,
        events(&[1, 1], &[]),
        events(&[5], &[]),
        events(&[], &[5]),
        events(&[1], &[1]),
        events(&[], &[1, 1]),
    ] {
        assert!(matches!(
            replay_reputation_transition_with_penalties(
                &previous(),
                &[],
                1,
                &config(),
                Some(&events)
            ),
            Err(PorError::InvalidPenaltyEvents(_))
        ));
    }
}

#[test]
fn inactive_node_must_be_absent_from_ratings() {
    let ratings = vec![RatingRecord::new(1, node(1), node(2), 1000, vec![1])];
    for id in [1, 2] {
        assert!(matches!(
            replay_reputation_transition_with_penalties(
                &previous(),
                &ratings,
                1,
                &config(),
                Some(&events(&[], &[id]))
            ),
            Err(PorError::InvalidPenaltyEvents(_))
        ));
    }
}

#[test]
fn excluded_keys_do_not_inflate_active_weight_or_receive_new_penalties() {
    let mut previous = previous();
    previous.values[3] = ReputationEntry::ejected(node(4));
    let next = replay_reputation_transition_with_penalties(
        &previous,
        &[],
        1,
        &config(),
        Some(&events(&[1], &[])),
    )
    .unwrap();
    // One third of active weight exceeds 30%; the excluded fourth key is not counted.
    assert_eq!(next.entries[0].reputation, 0);
    assert!(next.entries[3].is_excluded);
    assert!(matches!(
        replay_reputation_transition_with_penalties(
            &previous,
            &[],
            1,
            &config(),
            Some(&events(&[4], &[]))
        ),
        Err(PorError::InvalidPenaltyEvents(_))
    ));
}

#[test]
fn aggregate_weights_larger_than_u64_are_supported() {
    let mut previous = previous();
    for entry in &mut previous.values {
        entry.reputation = u64::MAX;
    }
    let config = PorConfig::new(1000, 0);
    let next = replay_reputation_transition_with_penalties(
        &previous,
        &[],
        1,
        &config,
        Some(&events(&[1], &[])),
    )
    .unwrap();
    assert_eq!(
        next.entries[0].reputation,
        ((u128::from(u64::MAX) * 750) / 1000) as u64
    );
}

#[test]
fn state_application_is_atomic_and_snapshot_round_trips() {
    let events = events(&[1], &[2]);
    let block = block(&events);
    let mut state = ReputationState::new(0);
    for entry in previous().values {
        state.set_reputation(entry.node_id, entry.reputation);
    }
    let original = state.clone();
    let mut invalid = block.clone();
    invalid.reputation_list.entries[0].reputation += 1;
    invalid.header.reputation_root = reputation_list_commitment(&invalid.reputation_list).unwrap();
    assert!(
        state
            .apply_reputation_block_with_penalties(
                b"root",
                0,
                &[],
                invalid,
                &config(),
                Some(&events)
            )
            .is_err()
    );
    assert_eq!(state, original);
    state
        .apply_reputation_block_with_penalties(b"root", 0, &[], block, &config(), Some(&events))
        .unwrap();
    assert_eq!(state.reputation_list().entries[0].reputation, 750);
    assert_eq!(
        decode_reputation_state_snapshot(&encode_reputation_state_snapshot(&state).unwrap())
            .unwrap(),
        state
    );
}

#[test]
fn slash_uses_previous_weight_without_rating_rewards_or_reclamping() {
    let ratings = vec![RatingRecord::new(1, node(2), node(1), 1000, vec![1])];
    let config = config();
    let next = replay_reputation_transition_with_penalties(
        &previous(),
        &ratings,
        1,
        &config,
        Some(&events(&[1], &[])),
    )
    .unwrap();
    assert_eq!(next.entries[0].reputation, 750);
}

#[test]
fn zero_active_total_is_a_configuration_error() {
    let mut previous = previous();
    for entry in &mut previous.values {
        entry.reputation = 0;
    }
    assert!(matches!(
        replay_reputation_transition_with_penalties(
            &previous,
            &[],
            1,
            &config(),
            Some(&events(&[1], &[]))
        ),
        Err(PorError::InvalidConfiguration(_))
    ));
}
