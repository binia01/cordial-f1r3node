//! Tests for `snapshot` — constructing `CasperSnapshot` from a blocklace.

use std::collections::{HashMap, HashSet};

use cordial_miners_core::Block;
use cordial_miners_core::blocklace::Blocklace;
use cordial_miners_core::crypto::hash_content;
use cordial_miners_core::execution::{
    BlockState, Bond as CmBond, CordialBlockPayload, Deploy, DeploySignatureAlgorithm,
    ProcessedDeploy, SignedDeploy,
};
use cordial_miners_core::types::{BlockContent, BlockIdentity, NodeId};

use cordial_f1r3node_adapter::snapshot::{CasperShardConf, SnapshotError, build_snapshot};
use cordial_miners_core::crypto::CryptoVerifier;

struct MockVerifier;

impl CryptoVerifier for MockVerifier {
    type Error = String;
    fn verify_block(
        &self,
        _content: &BlockContent,
        _sig: &[u8],
        _creator: &NodeId,
    ) -> Result<(), Self::Error> {
        Ok(()) // Always allow in tests
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────

fn node(b: u8) -> NodeId {
    NodeId(vec![b])
}

fn bonds(entries: &[(u8, u64)]) -> HashMap<NodeId, u64> {
    entries.iter().map(|(b, s)| (node(*b), *s)).collect()
}

fn make_block(
    creator: NodeId,
    payload: CordialBlockPayload,
    predecessors: HashSet<BlockIdentity>,
    sig_tag: u8,
) -> Block {
    let content = BlockContent {
        payload: payload.to_bytes(),
        predecessors,
    };
    Block {
        identity: BlockIdentity {
            content_hash: hash_content(&content),
            creator,
            signature: vec![sig_tag; 64],
        },
        content,
    }
}

fn simple_payload(block_number: u64, bonds: Vec<CmBond>) -> CordialBlockPayload {
    CordialBlockPayload {
        state: BlockState {
            pre_state_hash: vec![0x00; 32],
            post_state_hash: vec![block_number as u8; 32],
            bonds,
            block_number,
        },
        deploys: vec![],
        rejected_deploys: vec![],
        system_deploys: vec![],
    }
}

fn default_shard_conf() -> CasperShardConf {
    CasperShardConf {
        fault_tolerance_threshold: 0.1,
        shard_name: "root".to_string(),
        max_number_of_parents: 16,
        max_parent_depth: None,
        deploy_lifespan: 50,
        min_phlo_price: 1,
    }
}

// ── Empty blocklace ──────────────────────────────────────────────────────

#[test]
fn empty_blocklace_produces_empty_snapshot() {
    let bl = Blocklace::new();
    let b = bonds(&[(1, 100)]);
    let snap = build_snapshot(&bl, &b, default_shard_conf(), "root").unwrap();

    assert!(snap.dag.dag_set.is_empty());
    assert!(snap.dag.latest_messages_map.is_empty());
    assert!(snap.dag.child_map.is_empty());
    assert!(snap.dag.height_map.is_empty());
    assert!(snap.dag.last_finalized_block_hash.is_empty());
    assert!(snap.last_finalized_block.is_empty());
    assert!(snap.ordered_finalized_blocks.is_empty());
    assert!(snap.lca.is_empty());
    assert!(snap.tips.is_empty());
    assert!(snap.parents.is_empty());
    assert!(snap.justifications.is_empty());
    assert_eq!(snap.max_block_num, 0);
    assert!(snap.deploys_in_scope.is_empty());
}

// ── dag_set and block_number_map ─────────────────────────────────────────

#[test]
fn dag_set_contains_all_block_hashes() {
    let mut bl = Blocklace::new();
    let v1 = node(1);
    let g = make_block(v1.clone(), simple_payload(0, vec![]), HashSet::new(), 1);
    bl.insert(g.clone(), &MockVerifier).unwrap();

    let mut preds = HashSet::new();
    preds.insert(g.identity.clone());
    let b2 = make_block(v1.clone(), simple_payload(1, vec![]), preds, 2);
    bl.insert(b2.clone(), &MockVerifier).unwrap();

    let bonds_map = bonds(&[(1, 100)]);
    let snap = build_snapshot(&bl, &bonds_map, default_shard_conf(), "root").unwrap();

    assert_eq!(snap.dag.dag_set.len(), 2);
    assert!(
        snap.dag
            .dag_set
            .contains(g.identity.content_hash.as_slice())
    );
    assert!(
        snap.dag
            .dag_set
            .contains(b2.identity.content_hash.as_slice())
    );

    // block_number_map should reflect per-block block_number
    assert_eq!(
        snap.dag.block_number_map[&g.identity.content_hash.to_vec()],
        0
    );
    assert_eq!(
        snap.dag.block_number_map[&b2.identity.content_hash.to_vec()],
        1
    );
}

#[test]
fn height_map_groups_blocks_by_block_number() {
    let mut bl = Blocklace::new();
    let v1 = node(1);
    let v2 = node(2);

    // Two genesis blocks at block_number 0 from different validators.
    // We distinguish them via the pre_state_hash so their content hashes
    // differ (the blocklace is keyed by content_hash, not creator).
    let mut p1 = simple_payload(0, vec![]);
    p1.state.pre_state_hash = vec![0xaa; 32];
    let mut p2 = simple_payload(0, vec![]);
    p2.state.pre_state_hash = vec![0xbb; 32];

    let g1 = make_block(v1, p1, HashSet::new(), 1);
    let g2 = make_block(v2, p2, HashSet::new(), 2);
    bl.insert(g1.clone(), &MockVerifier).unwrap();
    bl.insert(g2.clone(), &MockVerifier).unwrap();

    let bonds_map = bonds(&[(1, 100), (2, 100)]);
    let snap = build_snapshot(&bl, &bonds_map, default_shard_conf(), "root").unwrap();

    let at_height_0 = snap.dag.height_map.get(&0).unwrap();
    assert_eq!(at_height_0.len(), 2);
}

// ── child_map (inverted predecessor relation) ────────────────────────────

#[test]
fn child_map_inverts_predecessor_relation() {
    let mut bl = Blocklace::new();
    let v1 = node(1);

    let g = make_block(v1.clone(), simple_payload(0, vec![]), HashSet::new(), 1);
    bl.insert(g.clone(), &MockVerifier).unwrap();

    let mut preds = HashSet::new();
    preds.insert(g.identity.clone());
    let b2 = make_block(v1.clone(), simple_payload(1, vec![]), preds, 2);
    bl.insert(b2.clone(), &MockVerifier).unwrap();

    let bonds_map = bonds(&[(1, 100)]);
    let snap = build_snapshot(&bl, &bonds_map, default_shard_conf(), "root").unwrap();

    // g should have b2 as a child
    let children_of_g = snap
        .dag
        .child_map
        .get(g.identity.content_hash.as_slice())
        .unwrap();
    assert!(children_of_g.contains(&b2.identity.content_hash.to_vec()));

    // b2 has no children
    assert!(
        !snap
            .dag
            .child_map
            .contains_key(b2.identity.content_hash.as_slice())
    );
}

// ── latest_messages_map ──────────────────────────────────────────────────

#[test]
fn latest_messages_map_tracks_each_validator_tip() {
    let mut bl = Blocklace::new();
    let v1 = node(1);
    let v2 = node(2);

    let g = make_block(v1.clone(), simple_payload(0, vec![]), HashSet::new(), 1);
    bl.insert(g.clone(), &MockVerifier).unwrap();

    // v1 extends
    let mut preds_a = HashSet::new();
    preds_a.insert(g.identity.clone());
    let mut p_a = simple_payload(1, vec![]);
    p_a.state.post_state_hash = vec![1; 32];
    let b1a = make_block(v1.clone(), p_a, preds_a, 2);
    bl.insert(b1a.clone(), &MockVerifier).unwrap();

    // v2 builds on g
    let mut preds_b = HashSet::new();
    preds_b.insert(g.identity.clone());
    let mut p_b = simple_payload(1, vec![]);
    p_b.state.post_state_hash = vec![2; 32];
    let b2a = make_block(v2.clone(), p_b, preds_b, 3);
    bl.insert(b2a.clone(), &MockVerifier).unwrap();

    let bonds_map = bonds(&[(1, 100), (2, 100)]);
    let snap = build_snapshot(&bl, &bonds_map, default_shard_conf(), "root").unwrap();

    assert_eq!(snap.dag.latest_messages_map.len(), 2);
    assert_eq!(
        snap.dag.latest_messages_map[&vec![1u8]],
        b1a.identity.content_hash.to_vec()
    );
    assert_eq!(
        snap.dag.latest_messages_map[&vec![2u8]],
        b2a.identity.content_hash.to_vec()
    );
}

#[test]
fn latest_messages_map_excludes_equivocators() {
    let mut bl = Blocklace::new();
    let v1 = node(1);

    // v1 creates TWO incomparable genesis blocks with different content —
    // equivocation, rather than two signatures for the same block content.
    let g1 = make_block(v1.clone(), simple_payload(0, vec![]), HashSet::new(), 1);
    let mut alternate_payload = simple_payload(0, vec![]);
    alternate_payload.state.pre_state_hash = vec![0xff; 32];
    let g2 = make_block(v1.clone(), alternate_payload, HashSet::new(), 2);
    bl.insert(g1, &MockVerifier).unwrap();
    bl.insert(g2, &MockVerifier).unwrap();

    let bonds_map = bonds(&[(1, 100)]);
    let snap = build_snapshot(&bl, &bonds_map, default_shard_conf(), "root").unwrap();

    // v1 is an equivocator → should not appear in latest_messages_map
    assert!(snap.dag.latest_messages_map.is_empty());
}

// ── Finality fields ──────────────────────────────────────────────────────

#[test]
fn last_finalized_block_matches_finality_detector() {
    let mut bl = Blocklace::new();
    let v1 = node(1);
    let v2 = node(2);
    let v3 = node(3);
    let v4 = node(4);

    // The adapter now uses paper-native leader finality, so we need a full
    // wave where the elected leader (node 1 under the round-robin default)
    // is super-ratified.
    let g = make_block(v1.clone(), simple_payload(0, vec![]), HashSet::new(), 1);
    bl.insert(g.clone(), &MockVerifier).unwrap();

    let mut r1_preds = HashSet::new();
    r1_preds.insert(g.identity.clone());
    let mut p2 = simple_payload(1, vec![]);
    p2.state.post_state_hash = vec![2; 32];
    let mut p3 = simple_payload(1, vec![]);
    p3.state.post_state_hash = vec![3; 32];
    let mut p4 = simple_payload(1, vec![]);
    p4.state.post_state_hash = vec![4; 32];
    let b2 = make_block(v2.clone(), p2, r1_preds.clone(), 2);
    let b3 = make_block(v3.clone(), p3, r1_preds.clone(), 3);
    let b4 = make_block(v4.clone(), p4, r1_preds, 4);
    bl.insert(b2.clone(), &MockVerifier).unwrap();
    bl.insert(b3.clone(), &MockVerifier).unwrap();
    bl.insert(b4.clone(), &MockVerifier).unwrap();

    let mut r2_preds = HashSet::new();
    r2_preds.insert(b2.identity.clone());
    r2_preds.insert(b3.identity.clone());
    r2_preds.insert(b4.identity.clone());
    let mut c2_p = simple_payload(2, vec![]);
    c2_p.state.post_state_hash = vec![12; 32];
    let mut c3_p = simple_payload(2, vec![]);
    c3_p.state.post_state_hash = vec![13; 32];
    let mut c4_p = simple_payload(2, vec![]);
    c4_p.state.post_state_hash = vec![14; 32];
    let c2 = make_block(v2.clone(), c2_p, r2_preds.clone(), 5);
    let c3 = make_block(v3.clone(), c3_p, r2_preds.clone(), 6);
    let c4 = make_block(v4.clone(), c4_p, r2_preds, 7);
    bl.insert(c2, &MockVerifier).unwrap();
    bl.insert(c3, &MockVerifier).unwrap();
    bl.insert(c4, &MockVerifier).unwrap();

    let bonds_map = bonds(&[(1, 100), (2, 100), (3, 100), (4, 100)]);
    let snap = build_snapshot(&bl, &bonds_map, default_shard_conf(), "root").unwrap();

    // Last finalized block should be non-empty (some block is finalized)
    assert!(!snap.dag.last_finalized_block_hash.is_empty());
    assert_eq!(
        snap.last_finalized_block,
        snap.dag.last_finalized_block_hash
    );
    assert!(!snap.ordered_finalized_blocks.is_empty());
    assert!(
        snap.ordered_finalized_blocks
            .contains(&snap.last_finalized_block)
    );
    assert!(
        snap.ordered_finalized_blocks
            .iter()
            .all(|hash| snap.dag.dag_set.contains(hash))
    );

    // The LFB must be a real block in the lace
    assert!(snap.dag.dag_set.contains(&snap.last_finalized_block));

    // finalized_blocks_set includes the LFB and its ancestors
    assert!(
        snap.dag
            .finalized_blocks_set
            .contains(&snap.last_finalized_block)
    );
}

#[test]
fn no_finality_when_single_validator_has_no_supermajority() {
    // With 3 independent genesis blocks from 3 validators, none of them are
    // in the ancestry of > 2/3 of the validators' tips, so no LFB.
    let mut bl = Blocklace::new();
    let mut p1 = simple_payload(0, vec![]);
    p1.state.post_state_hash = vec![1; 32];
    let mut p2 = simple_payload(0, vec![]);
    p2.state.post_state_hash = vec![2; 32];
    let mut p3 = simple_payload(0, vec![]);
    p3.state.post_state_hash = vec![3; 32];
    let g1 = make_block(node(1), p1, HashSet::new(), 1);
    let g2 = make_block(node(2), p2, HashSet::new(), 2);
    let g3 = make_block(node(3), p3, HashSet::new(), 3);
    bl.insert(g1, &MockVerifier).unwrap();
    bl.insert(g2, &MockVerifier).unwrap();
    bl.insert(g3, &MockVerifier).unwrap();

    let bonds_map = bonds(&[(1, 100), (2, 100), (3, 100)]);
    let snap = build_snapshot(&bl, &bonds_map, default_shard_conf(), "root").unwrap();

    assert!(snap.dag.last_finalized_block_hash.is_empty());
    assert!(snap.last_finalized_block.is_empty());
    assert!(snap.ordered_finalized_blocks.is_empty());
    assert!(snap.dag.finalized_blocks_set.is_empty());
}

// ── Tips / LCA / parents ─────────────────────────────────────────────────

#[test]
fn tips_and_parents_reflect_fork_choice() {
    let mut bl = Blocklace::new();
    let v1 = node(1);

    let g = make_block(v1.clone(), simple_payload(0, vec![]), HashSet::new(), 1);
    bl.insert(g.clone(), &MockVerifier).unwrap();

    let bonds_map = bonds(&[(1, 100)]);
    let snap = build_snapshot(&bl, &bonds_map, default_shard_conf(), "root").unwrap();

    // Single validator, single block → single tip = g
    assert_eq!(snap.tips.len(), 1);
    assert_eq!(snap.tips[0], g.identity.content_hash.to_vec());
    assert_eq!(snap.lca, g.identity.content_hash.to_vec());

    // Parents should contain one BlockMessage with the same hash
    assert_eq!(snap.parents.len(), 1);
    assert_eq!(snap.parents[0].block_hash, g.identity.content_hash.to_vec());
    assert_eq!(snap.parents[0].sender, vec![1]);
}

// ── Justifications ───────────────────────────────────────────────────────

#[test]
fn justifications_contain_one_entry_per_validator_tip() {
    let mut bl = Blocklace::new();
    let v1 = node(1);
    let v2 = node(2);

    let g = make_block(v1.clone(), simple_payload(0, vec![]), HashSet::new(), 1);
    bl.insert(g.clone(), &MockVerifier).unwrap();

    let mut preds = HashSet::new();
    preds.insert(g.identity.clone());
    let b2 = make_block(v2.clone(), simple_payload(1, vec![]), preds, 2);
    bl.insert(b2.clone(), &MockVerifier).unwrap();

    let bonds_map = bonds(&[(1, 100), (2, 100)]);
    let snap = build_snapshot(&bl, &bonds_map, default_shard_conf(), "root").unwrap();

    assert_eq!(snap.justifications.len(), 2);
    let validators: HashSet<Vec<u8>> = snap
        .justifications
        .iter()
        .map(|j| j.validator.clone())
        .collect();
    assert!(validators.contains(&vec![1u8]));
    assert!(validators.contains(&vec![2u8]));
}

// ── max_block_num and max_seq_nums ───────────────────────────────────────

#[test]
fn max_block_num_reflects_highest_block_number_in_lace() {
    let mut bl = Blocklace::new();
    let v1 = node(1);

    let g = make_block(v1.clone(), simple_payload(0, vec![]), HashSet::new(), 1);
    bl.insert(g.clone(), &MockVerifier).unwrap();

    let mut preds1 = HashSet::new();
    preds1.insert(g.identity.clone());
    let b1 = make_block(v1.clone(), simple_payload(1, vec![]), preds1, 2);
    bl.insert(b1.clone(), &MockVerifier).unwrap();

    let mut preds2 = HashSet::new();
    preds2.insert(b1.identity.clone());
    let b2 = make_block(v1.clone(), simple_payload(5, vec![]), preds2, 3);
    bl.insert(b2, &MockVerifier).unwrap();

    let bonds_map = bonds(&[(1, 100)]);
    let snap = build_snapshot(&bl, &bonds_map, default_shard_conf(), "root").unwrap();

    assert_eq!(snap.max_block_num, 5);
}

#[test]
fn max_seq_nums_counts_blocks_per_validator() {
    let mut bl = Blocklace::new();
    let v1 = node(1);
    let v2 = node(2);

    let g = make_block(v1.clone(), simple_payload(0, vec![]), HashSet::new(), 1);
    bl.insert(g.clone(), &MockVerifier).unwrap();

    let mut p1 = HashSet::new();
    p1.insert(g.identity.clone());
    let b1 = make_block(v1.clone(), simple_payload(1, vec![]), p1, 2);
    bl.insert(b1.clone(), &MockVerifier).unwrap();

    let mut p2 = HashSet::new();
    p2.insert(b1.identity.clone());
    let b2 = make_block(v2.clone(), simple_payload(2, vec![]), p2, 3);
    bl.insert(b2, &MockVerifier).unwrap();

    let bonds_map = bonds(&[(1, 100), (2, 100)]);
    let snap = build_snapshot(&bl, &bonds_map, default_shard_conf(), "root").unwrap();

    // v1 has 2 blocks, v2 has 1
    assert_eq!(snap.max_seq_nums[&vec![1u8]], 2);
    assert_eq!(snap.max_seq_nums[&vec![2u8]], 1);
}

// ── on_chain_state ───────────────────────────────────────────────────────

#[test]
fn on_chain_state_populates_bonds_and_active_validators() {
    let bl = Blocklace::new();
    let bonds_map = bonds(&[(1, 100), (2, 200)]);
    let snap = build_snapshot(&bl, &bonds_map, default_shard_conf(), "root").unwrap();

    assert_eq!(snap.on_chain_state.bonds_map.len(), 2);
    assert_eq!(snap.on_chain_state.bonds_map[&vec![1u8]], 100);
    assert_eq!(snap.on_chain_state.bonds_map[&vec![2u8]], 200);

    assert_eq!(snap.on_chain_state.active_validators.len(), 2);
    assert!(snap.on_chain_state.active_validators.contains(&vec![1u8]));
    assert!(snap.on_chain_state.active_validators.contains(&vec![2u8]));
}

#[test]
fn equivocator_excluded_from_active_validators() {
    let mut bl = Blocklace::new();
    let v1 = node(1);

    // v1 creates two incomparable blocks with different content. Merely changing
    // the signature would be an identity collision, not a second block.
    let g1 = make_block(v1.clone(), simple_payload(0, vec![]), HashSet::new(), 1);
    let mut alternate_payload = simple_payload(0, vec![]);
    alternate_payload.state.pre_state_hash = vec![0xff; 32];
    let g2 = make_block(v1.clone(), alternate_payload, HashSet::new(), 2);
    bl.insert(g1, &MockVerifier).unwrap();
    bl.insert(g2, &MockVerifier).unwrap();

    let bonds_map = bonds(&[(1, 100), (2, 100)]);
    let snap = build_snapshot(&bl, &bonds_map, default_shard_conf(), "root").unwrap();

    // v1 is still in bonds_map (bond exists on-chain)
    assert_eq!(snap.on_chain_state.bonds_map.len(), 2);
    // but only v2 is active
    assert_eq!(snap.on_chain_state.active_validators.len(), 1);
    assert_eq!(snap.on_chain_state.active_validators[0], vec![2u8]);
}

// ── deploys_in_scope ─────────────────────────────────────────────────────

#[test]
fn deploys_in_scope_collects_from_tip_ancestry() {
    let mut bl = Blocklace::new();
    let v1 = node(1);

    let deploy_sig = vec![0x42; 64];
    let signed = SignedDeploy {
        deploy: Deploy {
            term: b"tx".to_vec(),
            timestamp: 1000,
            phlo_price: 1,
            phlo_limit: 100,
            valid_after_block_number: 0,
            shard_id: "root".to_string(),
            expiration_timestamp: None,
        },
        deployer: vec![0x01; 32],
        signature: deploy_sig.clone(),
        signature_algorithm: DeploySignatureAlgorithm::Secp256k1,
    };
    let processed = ProcessedDeploy {
        deploy: signed,
        cost: 10,
        is_failed: false,
    };

    let mut genesis_payload = simple_payload(0, vec![]);
    genesis_payload.deploys = vec![processed];
    let g = make_block(v1.clone(), genesis_payload, HashSet::new(), 1);
    bl.insert(g.clone(), &MockVerifier).unwrap();

    let bonds_map = bonds(&[(1, 100)]);
    let snap = build_snapshot(&bl, &bonds_map, default_shard_conf(), "root").unwrap();

    assert!(snap.deploys_in_scope.contains(&deploy_sig));
}

// ── Error paths ──────────────────────────────────────────────────────────

#[test]
fn undecodable_payload_produces_error() {
    let mut bl = Blocklace::new();
    // Block with invalid (non-bincode) payload bytes
    let content = BlockContent {
        payload: vec![0xff, 0xff, 0xff], // not a valid CordialBlockPayload
        predecessors: HashSet::new(),
    };
    let block = Block {
        identity: BlockIdentity {
            content_hash: hash_content(&content),
            creator: node(1),
            signature: vec![0x00; 64],
        },
        content,
    };
    bl.insert(block, &MockVerifier).unwrap();

    let bonds_map = bonds(&[(1, 100)]);
    let err = build_snapshot(&bl, &bonds_map, default_shard_conf(), "root").unwrap_err();
    assert!(matches!(err, SnapshotError::PayloadDecodeFailed { .. }));
}

// ── Ambiguous content hash resolution (Issue #278) ─────────────────────────

fn test_signing_key(seed: u8) -> Vec<u8> {
    let mut key = vec![0u8; 32];
    key[0] = seed;
    for (i, item) in key.iter_mut().enumerate().skip(1) {
        *item = ((seed as u16).wrapping_mul(i as u16 + 1)) as u8;
    }
    key
}

fn derive_snapshot_key(content_hash: &[u8; 32], creator: &NodeId) -> Vec<u8> {
    use cordial_f1r3node_adapter::crypto_bridge::{Blake2b256Hasher, Hasher};
    let mut input = Vec::with_capacity(23 + 32 + creator.0.len());
    input.extend_from_slice(b"cordial-snapshot-key-v1");
    input.extend_from_slice(content_hash);
    input.extend_from_slice(&creator.0);
    Blake2b256Hasher.hash(&input).to_vec()
}

#[test]
fn same_content_from_two_creators_stays_distinct() {
    let mut bl = Blocklace::new();
    let v1 = node(1);
    let v2 = node(2);
    let payload = simple_payload(1, vec![]);
    let b1 = make_block(v1.clone(), payload.clone(), HashSet::new(), 1);
    let b2 = make_block(v2.clone(), payload, HashSet::new(), 2);
    assert_eq!(b1.identity.content_hash, b2.identity.content_hash);

    bl.insert(b1.clone(), &MockVerifier).unwrap();
    bl.insert(b2.clone(), &MockVerifier).unwrap();

    let bonds_map = bonds(&[(1, 100), (2, 100)]);
    let snap = build_snapshot(&bl, &bonds_map, default_shard_conf(), "root").unwrap();

    assert_eq!(snap.dag.dag_set.len(), 2);
    let key1 = derive_snapshot_key(&b1.identity.content_hash, &v1);
    let key2 = derive_snapshot_key(&b2.identity.content_hash, &v2);

    assert!(snap.dag.dag_set.contains(&key1));
    assert!(snap.dag.dag_set.contains(&key2));
    assert_ne!(key1, key2);
    assert_ne!(key1, b1.identity.content_hash.to_vec());
    assert_ne!(key2, b2.identity.content_hash.to_vec());
}

#[test]
fn same_content_keys_are_deterministic_and_order_independent() {
    let v_a = node(1);
    let v_b = node(2);
    let payload = simple_payload(1, vec![]);
    let b_a = make_block(v_a.clone(), payload.clone(), HashSet::new(), 1);
    let b_b = make_block(v_b.clone(), payload, HashSet::new(), 2);
    let bonds_map = bonds(&[(1, 100), (2, 100)]);

    let mut bl1 = Blocklace::new();
    bl1.insert(b_a.clone(), &MockVerifier).unwrap();
    bl1.insert(b_b.clone(), &MockVerifier).unwrap();
    let snap1 = build_snapshot(&bl1, &bonds_map, default_shard_conf(), "root").unwrap();

    let mut bl2 = Blocklace::new();
    bl2.insert(b_b.clone(), &MockVerifier).unwrap();
    bl2.insert(b_a.clone(), &MockVerifier).unwrap();
    let snap2 = build_snapshot(&bl2, &bonds_map, default_shard_conf(), "root").unwrap();

    assert_eq!(snap1.dag.dag_set, snap2.dag.dag_set);
    assert_eq!(snap1.dag.block_number_map, snap2.dag.block_number_map);
    assert_eq!(snap1.dag.child_map, snap2.dag.child_map);

    let expected_a = derive_snapshot_key(&b_a.identity.content_hash, &v_a);
    let expected_b = derive_snapshot_key(&b_b.identity.content_hash, &v_b);
    assert!(snap1.dag.dag_set.contains(&expected_a));
    assert!(snap1.dag.dag_set.contains(&expected_b));
}

#[test]
fn child_edges_follow_the_right_creator() {
    let mut bl = Blocklace::new();
    let v_a = node(1);
    let v_b = node(2);
    let v_c = node(3);

    let payload = simple_payload(1, vec![]);
    let b_a = make_block(v_a.clone(), payload.clone(), HashSet::new(), 1);
    let b_b = make_block(v_b.clone(), payload, HashSet::new(), 2);

    let mut child_predecessors = HashSet::new();
    child_predecessors.insert(b_a.identity.clone());
    let child = make_block(
        v_c.clone(),
        simple_payload(2, vec![]),
        child_predecessors,
        3,
    );

    bl.insert(b_a.clone(), &MockVerifier).unwrap();
    bl.insert(b_b.clone(), &MockVerifier).unwrap();
    bl.insert(child.clone(), &MockVerifier).unwrap();

    let bonds_map = bonds(&[(1, 100), (2, 100), (3, 100)]);
    let snap = build_snapshot(&bl, &bonds_map, default_shard_conf(), "root").unwrap();

    let key_a = derive_snapshot_key(&b_a.identity.content_hash, &v_a);
    let key_b = derive_snapshot_key(&b_b.identity.content_hash, &v_b);
    let key_child = child.identity.content_hash.to_vec();

    let children_of_a = snap
        .dag
        .child_map
        .get(&key_a)
        .expect("child_map for A exists");
    assert!(children_of_a.contains(&key_child));

    if let Some(children_of_b) = snap.dag.child_map.get(&key_b) {
        assert!(!children_of_b.contains(&key_child));
    }
}

#[test]
fn latest_messages_keep_each_creator() {
    let mut bl = Blocklace::new();
    let v_a = node(1);
    let v_b = node(2);
    let payload = simple_payload(1, vec![]);
    let b_a = make_block(v_a.clone(), payload.clone(), HashSet::new(), 1);
    let b_b = make_block(v_b.clone(), payload, HashSet::new(), 2);

    bl.insert(b_a.clone(), &MockVerifier).unwrap();
    bl.insert(b_b.clone(), &MockVerifier).unwrap();

    let bonds_map = bonds(&[(1, 100), (2, 100)]);
    let snap = build_snapshot(&bl, &bonds_map, default_shard_conf(), "root").unwrap();

    let key_a = derive_snapshot_key(&b_a.identity.content_hash, &v_a);
    let key_b = derive_snapshot_key(&b_b.identity.content_hash, &v_b);

    assert_eq!(snap.dag.latest_messages_map.get(&v_a.0), Some(&key_a));
    assert_eq!(snap.dag.latest_messages_map.get(&v_b.0), Some(&key_b));
    assert_ne!(key_a, key_b);
}

#[test]
fn snapshot_references_are_all_in_dag_set() {
    let mut bl = Blocklace::new();
    let v_a = node(1);
    let v_b = node(2);
    let v_c = node(3);

    let payload = simple_payload(1, vec![]);
    let b_a = make_block(v_a.clone(), payload.clone(), HashSet::new(), 1);
    let b_b = make_block(v_b.clone(), payload, HashSet::new(), 2);

    let mut preds = HashSet::new();
    preds.insert(b_a.identity.clone());
    preds.insert(b_b.identity.clone());
    let b_c = make_block(v_c.clone(), simple_payload(2, vec![]), preds, 3);

    bl.insert(b_a.clone(), &MockVerifier).unwrap();
    bl.insert(b_b.clone(), &MockVerifier).unwrap();
    bl.insert(b_c.clone(), &MockVerifier).unwrap();

    let bonds_map = bonds(&[(1, 100), (2, 100), (3, 100)]);
    let snap = build_snapshot(&bl, &bonds_map, default_shard_conf(), "root").unwrap();

    for tip in &snap.tips {
        assert!(snap.dag.dag_set.contains(tip));
    }
    if !snap.lca.is_empty() {
        assert!(snap.dag.dag_set.contains(&snap.lca));
    }
    for just in &snap.justifications {
        if !just.latest_block_hash.is_empty() {
            assert!(snap.dag.dag_set.contains(&just.latest_block_hash));
        }
    }
    for fb in &snap.ordered_finalized_blocks {
        assert!(snap.dag.dag_set.contains(fb));
    }
    for fb in &snap.dag.finalized_blocks_set {
        assert!(snap.dag.dag_set.contains(fb));
    }
    for msg in &snap.parents {
        assert!(snap.dag.dag_set.contains(&msg.block_hash));
    }
}

#[test]
fn unique_content_hashes_keep_raw_hash_keys() {
    let mut bl = Blocklace::new();
    let v1 = node(1);
    let v2 = node(2);
    let p1 = simple_payload(1, vec![]);
    let p2 = simple_payload(2, vec![]);
    let b1 = make_block(v1.clone(), p1, HashSet::new(), 1);
    let b2 = make_block(
        v2.clone(),
        p2,
        [b1.identity.clone()].into_iter().collect(),
        2,
    );

    bl.insert(b1.clone(), &MockVerifier).unwrap();
    bl.insert(b2.clone(), &MockVerifier).unwrap();

    let bonds_map = bonds(&[(1, 100), (2, 100)]);
    let snap = build_snapshot(&bl, &bonds_map, default_shard_conf(), "root").unwrap();

    assert_eq!(snap.dag.dag_set.len(), 2);
    let raw1 = b1.identity.content_hash.to_vec();
    let raw2 = b2.identity.content_hash.to_vec();
    assert!(snap.dag.dag_set.contains(&raw1));
    assert!(snap.dag.dag_set.contains(&raw2));
    for key in &snap.dag.dag_set {
        assert!(key == &raw1 || key == &raw2);
    }
}

#[test]
fn idle_native_round_snapshot_is_lossless() {
    use cordial_f1r3node_adapter::proposer::{
        CordialProposer, DisseminationTipSelector, RecordingBroadcaster, RuntimeExecutionEngine,
        Secp256k1BlockSigner,
    };
    use cordial_miners_core::execution::{DeployPool, DeployPoolConfig, MockRuntime};

    let v_a = node(1);
    let v_b = node(2);
    let bonds_map = bonds(&[(1, 100), (2, 100)]);

    let key_a = test_signing_key(1);
    let key_b = test_signing_key(2);
    let signer_a = Secp256k1BlockSigner::new(key_a);
    let signer_b = Secp256k1BlockSigner::new(key_b);

    let broadcaster_a = RecordingBroadcaster::default();
    let broadcaster_b = RecordingBroadcaster::default();
    let runtime_a = MockRuntime::new();
    let runtime_b = MockRuntime::new();

    let mut proposer_a = CordialProposer::new(
        DisseminationTipSelector,
        RuntimeExecutionEngine::new(runtime_a),
        signer_a,
        broadcaster_a,
        v_a.clone(),
        bonds_map.clone(),
        DeployPoolConfig::default(),
    );

    let mut proposer_b = CordialProposer::new(
        DisseminationTipSelector,
        RuntimeExecutionEngine::new(runtime_b),
        signer_b,
        broadcaster_b,
        v_b.clone(),
        bonds_map.clone(),
        DeployPoolConfig::default(),
    );

    let deploy_pool = DeployPool::new(DeployPoolConfig::default());
    let mut blocklace = Blocklace::new();

    // Round 1
    let block_a1 = proposer_a
        .propose_at_time(&blocklace, &deploy_pool, 1000)
        .expect("propose A1");
    let block_b1 = proposer_b
        .propose_at_time(&blocklace, &deploy_pool, 1000)
        .expect("propose B1");

    assert_eq!(
        block_a1.identity.content_hash,
        block_b1.identity.content_hash
    );

    blocklace
        .insert(block_a1.clone(), &MockVerifier)
        .expect("insert A1");
    blocklace
        .insert(block_b1.clone(), &MockVerifier)
        .expect("insert B1");

    let snap_r1 = build_snapshot(&blocklace, &bonds_map, default_shard_conf(), "root")
        .expect("snapshot R1 should succeed");
    assert_eq!(snap_r1.dag.dag_set.len(), 2);

    // Round 2
    let block_a2 = proposer_a
        .propose_at_time(&blocklace, &deploy_pool, 2000)
        .expect("propose A2");
    let block_b2 = proposer_b
        .propose_at_time(&blocklace, &deploy_pool, 2000)
        .expect("propose B2");

    assert_eq!(
        block_a2.identity.content_hash,
        block_b2.identity.content_hash
    );

    let mut blocklace_r2 = Blocklace::new();
    blocklace_r2
        .insert(block_a1, &MockVerifier)
        .expect("insert A1 into r2");
    blocklace_r2
        .insert(block_b1, &MockVerifier)
        .expect("insert B1 into r2");
    blocklace_r2
        .insert(block_a2, &MockVerifier)
        .expect("insert A2 into r2");
    blocklace_r2
        .insert(block_b2, &MockVerifier)
        .expect("insert B2 into r2");

    let snap_r2 = build_snapshot(&blocklace_r2, &bonds_map, default_shard_conf(), "root")
        .expect("snapshot R2 should succeed");
    assert_eq!(snap_r2.dag.dag_set.len(), 4);
}
