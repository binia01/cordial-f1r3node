//! Acceptance tests for exporting selected PoR consensus-group weights.

use cordial_miners_core::NodeId;
use cordial_por::{
    consensus_group_weights,
    types::{ConsensusGroup, ConsensusGroupMember},
};

#[test]
fn empty_consensus_group_exports_empty_weights() {
    // An empty selection must not create validator weights.
    let group = ConsensusGroup {
        round: 1,
        members: vec![],
    };

    let weights = consensus_group_weights(&group);
    assert!(weights.is_empty());
}

#[test]
fn single_member_exports_exact_weight() {
    let node = NodeId(vec![1u8]);

    let group = ConsensusGroup {
        round: 1,
        members: vec![ConsensusGroupMember {
            node_id: node.clone(),
            reputation: 750,
        }],
    };

    let weights = consensus_group_weights(&group);
    assert_eq!(weights.len(), 1);
    assert_eq!(weights.get(&node), Some(&750));
}

#[test]
fn multiple_members_export_exact_weights() {
    let node_a = NodeId(vec![1u8]);
    let node_b = NodeId(vec![2u8]);
    let node_c = NodeId(vec![3u8]);

    let group = ConsensusGroup {
        round: 1,
        members: vec![
            ConsensusGroupMember {
                node_id: node_a.clone(),
                reputation: 900,
            },
            ConsensusGroupMember {
                node_id: node_b.clone(),
                reputation: 600,
            },
            ConsensusGroupMember {
                node_id: node_c.clone(),
                reputation: 300,
            },
        ],
    };

    let weights = consensus_group_weights(&group);

    assert_eq!(weights.len(), 3);
    assert_eq!(weights.get(&node_a), Some(&900));
    assert_eq!(weights.get(&node_b), Some(&600));
    assert_eq!(weights.get(&node_c), Some(&300));
}

#[test]
fn excludes_non_group_validators() {
    let member = NodeId(vec![1u8]);
    let non_member = NodeId(vec![2u8]);

    // Only validators represented in `members` may appear in the output.
    let group = ConsensusGroup {
        round: 1,
        members: vec![ConsensusGroupMember {
            node_id: member.clone(),
            reputation: 700,
        }],
    };

    let weights = consensus_group_weights(&group);

    assert!(weights.contains_key(&member));
    assert!(!weights.contains_key(&non_member));
}
#[test]
fn preserves_reputation_values_exactly() {
    let node = NodeId(vec![9u8]);

    let group = ConsensusGroup {
        round: 7,
        members: vec![ConsensusGroupMember {
            node_id: node.clone(),
            reputation: 987_654_321,
        }],
    };

    let weights = consensus_group_weights(&group);

    // Exporting must not normalize, clamp, or otherwise transform reputation.
    assert_eq!(weights.get(&node), Some(&987_654_321));
}
