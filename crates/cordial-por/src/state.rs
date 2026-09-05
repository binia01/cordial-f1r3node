use cordial_miners_core::NodeId;

use crate::{
    error::PorError,
    types::{
        RatingRecord, ReputationBlock, ReputationEntry, ReputationList, ReputationRound,
        ReputationVector, ReputationWeight,
    },
};

/// Local Proof-of-Reputation state.
///
/// This stores PoR data only.
/// It does not calculate reputation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReputationState {
    current_round: ReputationRound,

    reputation_list: ReputationList,

    pending_ratings: Vec<RatingRecord>,

    latest_block: Option<ReputationBlock>,
}

impl ReputationState {
    pub fn new(round: ReputationRound) -> Self {
        Self {
            current_round: round,

            reputation_list: ReputationList {
                round,
                entries: Vec::new(),
            },

            pending_ratings: Vec::new(),

            latest_block: None,
        }
    }

    pub fn round(&self) -> ReputationRound {
        self.current_round
    }

    pub fn reputation_list(&self) -> &ReputationList {
        &self.reputation_list
    }

    pub fn reputation_list_mut(&mut self) -> &mut ReputationList {
        &mut self.reputation_list
    }

    pub fn pending_ratings(&self) -> &[RatingRecord] {
        &self.pending_ratings
    }

    pub fn latest_block(&self) -> Option<&ReputationBlock> {
        self.latest_block.as_ref()
    }

    pub fn add_rating(&mut self, rating: RatingRecord) {
        self.pending_ratings.push(rating);
    }

    pub fn set_reputation(&mut self, node_id: NodeId, reputation: ReputationWeight) {
        match self
            .reputation_list
            .entries
            .binary_search_by(|entry| entry.node_id.cmp(&node_id))
        {
            Ok(index) => {
                // Do not un-eject a permanently excluded node via set_reputation.
                if !self.reputation_list.entries[index].is_excluded {
                    self.reputation_list.entries[index].reputation = reputation;
                }
            }
            Err(index) => {
                self.reputation_list
                    .entries
                    .insert(index, ReputationEntry::new(node_id, reputation));
            }
        }
    }

    /// Permanently eject a validator key from the active set.
    ///
    /// Sets `is_excluded = true` and zeroes the node's reputation weight.
    /// Ejection is irreversible: no subsequent call can restore the key.
    ///
    /// Returns `PorError::UnknownNode` if the node is not present in the
    /// current state.
    pub fn eject_validator(&mut self, node_id: &NodeId) -> Result<(), PorError> {
        match self
            .reputation_list
            .entries
            .binary_search_by(|entry| entry.node_id.cmp(node_id))
        {
            Ok(index) => {
                self.reputation_list.entries[index].is_excluded = true;
                self.reputation_list.entries[index].reputation = 0;
                Ok(())
            }
            Err(_) => Err(PorError::UnknownNode),
        }
    }

    /// Apply a finalized reputation vector as the current state snapshot.
    ///
    /// The vector is expected to be the already-computed output of the
    /// calculation pipeline. This method validates canonical ordering, takes
    /// ownership of the vector entries, and replaces the state's reputation
    /// list; it does not recompute ratings, Liquid Rank, transition, or
    /// clamping.
    ///
    /// Entries already marked `is_excluded = true` in the incoming vector are
    /// preserved as ejected. If a node was ejected in the previous state but
    /// arrives with `is_excluded = false` in the new vector, ejection is
    /// re-applied to maintain the invariant that ejection is permanent.
    pub fn apply_reputation_vector(&mut self, vector: ReputationVector) -> Result<(), PorError> {
        validate_reputation_vector(&vector)?;

        // Build a fast lookup of currently ejected node ids before replacing.
        let ejected: std::collections::HashSet<NodeId> = self
            .reputation_list
            .entries
            .iter()
            .filter(|e| e.is_excluded)
            .map(|e| e.node_id.clone())
            .collect();

        // Replace the list, then re-apply ejection for any previously excluded node.
        self.current_round = vector.round;
        self.reputation_list = ReputationList {
            round: vector.round,
            entries: vector
                .values
                .into_iter()
                .map(|mut entry| {
                    if ejected.contains(&entry.node_id) {
                        entry.is_excluded = true;
                        entry.reputation = 0;
                    }
                    entry
                })
                .collect(),
        };

        Ok(())
    }
}

impl Default for ReputationState {
    fn default() -> Self {
        Self::new(0)
    }
}

fn validate_reputation_vector(vector: &ReputationVector) -> Result<(), PorError> {
    for entries in vector.values.windows(2) {
        match entries[0].node_id.cmp(&entries[1].node_id) {
            std::cmp::Ordering::Less => {}
            std::cmp::Ordering::Equal => return Err(PorError::DuplicateReputationEntry),
            std::cmp::Ordering::Greater => return Err(PorError::UnsortedReputationVector),
        }
    }

    Ok(())
}
