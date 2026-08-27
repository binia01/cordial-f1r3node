//! Acceptance tests for exporting externally selected validator weights.

use cordial_miners_core::NodeId;
use cordial_por::{ReputationState, selected_validator_weights};

#[test]
fn empty_selection_exports_empty_weights() {
    let mut state = ReputationState::new(1);
    state.set_reputation(NodeId(vec![1]), 750);

    let weights = selected_validator_weights(&state, &[]);

    assert!(weights.is_empty());
}

#[test]
fn single_selected_validator_exports_exact_weight() {
    let node = NodeId(vec![1]);
    let mut state = ReputationState::new(1);
    state.set_reputation(node.clone(), 750);

    let weights = selected_validator_weights(&state, std::slice::from_ref(&node));

    assert_eq!(weights.len(), 1);
    assert_eq!(weights.get(&node), Some(&750));
}

#[test]
fn multiple_selected_validators_export_exact_weights() {
    let node_a = NodeId(vec![1]);
    let node_b = NodeId(vec![2]);
    let node_c = NodeId(vec![3]);
    let mut state = ReputationState::new(1);
    state.set_reputation(node_a.clone(), 900);
    state.set_reputation(node_b.clone(), 600);
    state.set_reputation(node_c.clone(), 300);

    let weights =
        selected_validator_weights(&state, &[node_a.clone(), node_b.clone(), node_c.clone()]);

    assert_eq!(weights.len(), 3);
    assert_eq!(weights.get(&node_a), Some(&900));
    assert_eq!(weights.get(&node_b), Some(&600));
    assert_eq!(weights.get(&node_c), Some(&300));
}

#[test]
fn excludes_unselected_validators() {
    let selected = NodeId(vec![1]);
    let unselected = NodeId(vec![2]);
    let mut state = ReputationState::new(1);
    state.set_reputation(selected.clone(), 700);
    state.set_reputation(unselected.clone(), 400);

    let weights = selected_validator_weights(&state, std::slice::from_ref(&selected));

    assert!(weights.contains_key(&selected));
    assert!(!weights.contains_key(&unselected));
}

#[test]
fn omits_selected_validators_missing_from_reputation_state() {
    let known = NodeId(vec![1]);
    let unknown = NodeId(vec![2]);
    let mut state = ReputationState::new(1);
    state.set_reputation(known.clone(), 700);

    let weights = selected_validator_weights(&state, &[known.clone(), unknown.clone()]);

    assert_eq!(weights.len(), 1);
    assert_eq!(weights.get(&known), Some(&700));
    assert!(!weights.contains_key(&unknown));
}

#[test]
fn duplicate_selection_does_not_change_exported_weight() {
    let node = NodeId(vec![9]);
    let mut state = ReputationState::new(7);
    state.set_reputation(node.clone(), 987_654_321);

    let weights = selected_validator_weights(&state, &[node.clone(), node.clone()]);

    assert_eq!(weights.len(), 1);
    assert_eq!(weights.get(&node), Some(&987_654_321));
}
