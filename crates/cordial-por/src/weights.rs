use std::collections::HashMap;

use cordial_miners_core::NodeId;

use crate::state::ReputationState;
use crate::types::{ConsensusGroup, ReputationWeight};

/// Export reputation values as Cordial Miners weighted-path inputs.
///
/// This crate computes weights.
/// Cordial Miners consumes them.
pub fn reputation_weights(state: &ReputationState) -> HashMap<NodeId, ReputationWeight> {
    state
        .reputation_list()
        .entries
        .iter()
        .map(|entry| (entry.node_id.clone(), entry.reputation))
        .collect()
}

/// Export the selected consensus-group members as Cordial Miners weights.
///
/// Each member's reputation is copied unchanged into the returned map. This
/// adapter neither selects members nor performs any Cordial Miners consensus
/// operation.
pub fn consensus_group_weights(group: &ConsensusGroup) -> HashMap<NodeId, ReputationWeight> {
    group
        .members
        .iter()
        .map(|member| (member.node_id.clone(), member.reputation))
        .collect()
}
