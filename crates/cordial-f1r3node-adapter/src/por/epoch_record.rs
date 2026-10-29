//! Durable records and journal for future-only PoR weight epochs.
//!
//! This file is deliberately separate from the active-weight marker. Writing
//! it does not activate weights. Replay checks reconstruct the sealed prefix
//! from the full blocklace and reject a changed source, validator set, or
//! checkpoint before a later runtime slice can consume the proposal.

use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};

use cordial_miners_core::{
    BlockIdentity, Blocklace, NodeId,
    consensus::{
        WeightedEpochBoundary, WeightedEpochError, depth, wave_of_round,
        weighted_tau_after_verified_epoch_boundary, weighted_tau_at_leader,
    },
    crypto::{Blake2b256Hasher, Hasher},
};
use cordial_por::{
    MAX_REPUTATION_STATE_ENTRIES, MAX_REPUTATION_STATE_NODE_ID_LEN,
    MAX_REPUTATION_STATE_SNAPSHOT_LEN,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    live_ingress::{LiveIngress, PorWeightActivationError},
    ordered_output::OrderedFinalizedOutput,
    shared_ordered_output::ReadOrderedOutput,
};

use super::{
    activation::authorized_weight_commitment,
    epoch_policy::{PorWeightEpochPolicy, PorWeightEpochPolicyError},
    finality::{FinalizedPrefixCheckpoint, PorFinalityError, validate_finalized_prefix_checkpoint},
    persistence::POR_STATE_DIRECTORY,
};

pub const POR_WEIGHT_EPOCH_FILE_NAME: &str = "future-weight-epoch.bin";
const TEMP_FILE_NAME: &str = ".future-weight-epoch.bin.tmp";
const MAGIC: &[u8] = b"cordial-por-future-weight-epoch";
const VERSION: u16 = 1;
const CHECKSUM_DOMAIN: &[u8] = b"cordial-por:future-weight-epoch-record:v1";
const PREFIX_DOMAIN: &[u8] = b"cordial-por:sealed-epoch-prefix:v1";
const CHECKSUM_LEN: usize = 32;
const MAX_RECORD_LEN: usize = MAX_REPUTATION_STATE_SNAPSHOT_LEN;

/// A prepared cutover. This record is not evidence that PoR weights are active.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PorWeightEpochRecord {
    por_round: u64,
    reputation_checkpoint_hash: [u8; 32],
    source: FinalizedPrefixCheckpoint,
    delay_waves: u64,
    effective_wave: u64,
    wavelength: u64,
    sealed_leader: BlockIdentity,
    sealed_prefix_len: u64,
    sealed_prefix_hash: [u8; 32],
    old_weights: Vec<(NodeId, u64)>,
    new_weights: Vec<(NodeId, u64)>,
    old_weights_commitment: [u8; 32],
    new_weights_commitment: [u8; 32],
}

#[derive(Debug, Error)]
pub enum PorWeightEpochRecordError {
    #[error("PoR future epoch storage I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("malformed PoR future epoch record: {0}")]
    Malformed(&'static str),
    #[error("PoR future epoch record has unsupported version {0}")]
    UnsupportedVersion(u16),
    #[error("PoR future epoch record checksum mismatch")]
    ChecksumMismatch,
    #[error("a different PoR future epoch record is already stored")]
    ConflictingRecord,
    #[error("PoR future epoch record exceeds the storage size limit")]
    TooLarge,
    #[error("PoR future epoch writer lock is poisoned")]
    WriterLockPoisoned,
    #[error("PoR future epoch preview failed: {0}")]
    Preview(#[from] PorWeightActivationError),
    #[error("PoR future epoch source validation failed: {0}")]
    Source(#[from] PorFinalityError),
    #[error("PoR future epoch policy validation failed: {0}")]
    Policy(#[from] PorWeightEpochPolicyError),
    #[error("PoR future epoch ordering replay failed: {0:?}")]
    Ordering(WeightedEpochError),
}

impl PorWeightEpochRecord {
    /// Prepare a record from an audited checkpoint projection and published
    /// old-weight boundary. The caller supplies the audited reputation block
    /// hash; replay later compares it with committed state before activation.
    pub fn prepare<A>(
        ingress: &LiveIngress<A>,
        source: &FinalizedPrefixCheckpoint,
        reputation_checkpoint_hash: [u8; 32],
        projected: &HashMap<NodeId, u64>,
        policy: PorWeightEpochPolicy,
    ) -> Result<Self, PorWeightEpochRecordError> {
        let (effective_wave, _) =
            ingress.preview_checkpoint_weight_epoch(projected, source, policy)?;
        let sealed = ingress.ordered_output_reader().latest().ok_or(
            PorWeightEpochRecordError::Malformed("missing sealed output"),
        )?;
        let sealed_leader = sealed
            .anchor
            .clone()
            .ok_or(PorWeightEpochRecordError::Malformed(
                "missing sealed leader",
            ))?;
        let old_weights = sorted_weights(ingress.bonds());
        let new_weights = sorted_weights(projected);
        let record = Self {
            por_round: source.por_round,
            reputation_checkpoint_hash,
            source: source.clone(),
            delay_waves: policy.delay_waves(),
            effective_wave,
            wavelength: sealed.wavelength,
            sealed_leader,
            sealed_prefix_len: u64::try_from(sealed.blocks.len())
                .map_err(|_| PorWeightEpochRecordError::TooLarge)?,
            sealed_prefix_hash: sealed_prefix_hash(&sealed.blocks),
            old_weights_commitment: authorized_weight_commitment(ingress.bonds()),
            new_weights_commitment: authorized_weight_commitment(projected),
            old_weights,
            new_weights,
        };
        record.validate_structure()?;
        Ok(record)
    }

    pub fn por_round(&self) -> u64 {
        self.por_round
    }

    pub fn effective_wave(&self) -> u64 {
        self.effective_wave
    }

    pub fn delay_waves(&self) -> u64 {
        self.delay_waves
    }

    pub fn reputation_checkpoint_hash(&self) -> [u8; 32] {
        self.reputation_checkpoint_hash
    }

    pub fn wavelength(&self) -> u64 {
        self.wavelength
    }

    pub fn sealed_leader(&self) -> &BlockIdentity {
        &self.sealed_leader
    }

    pub fn old_weights(&self) -> HashMap<NodeId, u64> {
        self.old_weights.iter().cloned().collect()
    }

    pub fn source(&self) -> &FinalizedPrefixCheckpoint {
        &self.source
    }

    pub fn new_weights_commitment(&self) -> [u8; 32] {
        self.new_weights_commitment
    }

    pub fn new_weights(&self) -> HashMap<NodeId, u64> {
        self.new_weights.iter().cloned().collect()
    }

    /// Rebuild the recorded boundary from retained blocks, including when a
    /// later head has arrived. The caller supplies its authorized validator
    /// set and the checkpoint hash recovered from committed reputation state.
    pub fn verify_replay(
        &self,
        blocklace: &Blocklace,
        authorized: &[NodeId],
        committed_checkpoint_hash: [u8; 32],
    ) -> Result<OrderedFinalizedOutput, PorWeightEpochRecordError> {
        self.verify_replay_from_prefix(blocklace, authorized, committed_checkpoint_hash, None, None)
    }

    fn verify_replay_from_prefix(
        &self,
        blocklace: &Blocklace,
        authorized: &[NodeId],
        committed_checkpoint_hash: [u8; 32],
        previous: Option<&OrderedFinalizedOutput>,
        target: Option<&BlockIdentity>,
    ) -> Result<OrderedFinalizedOutput, PorWeightEpochRecordError> {
        self.validate_structure()?;
        if self.reputation_checkpoint_hash != committed_checkpoint_hash {
            return Err(PorWeightEpochRecordError::Malformed(
                "reputation checkpoint changed",
            ));
        }
        let mut authorized = authorized.to_vec();
        authorized.sort();
        if authorized
            != self
                .old_weights
                .iter()
                .map(|(node, _)| node.clone())
                .collect::<Vec<_>>()
        {
            return Err(PorWeightEpochRecordError::Malformed(
                "authorized validator set changed",
            ));
        }
        let old_weights: HashMap<_, _> = self.old_weights.iter().cloned().collect();
        let new_weights: HashMap<_, _> = self.new_weights.iter().cloned().collect();
        let boundary_wave = depth(blocklace, &self.sealed_leader)
            .and_then(|round| wave_of_round(round, self.wavelength))
            .ok_or(PorWeightEpochRecordError::Malformed(
                "sealed leader wave is unavailable",
            ))?;
        if boundary_wave.checked_add(1) != Some(self.effective_wave) {
            return Err(PorWeightEpochRecordError::Malformed(
                "sealed leader is not immediately before the effective wave",
            ));
        }
        let leader_of_wave = |wave: u64| {
            let index = usize::try_from(wave).ok()? % authorized.len();
            Some(authorized[index].clone())
        };
        let sealed = if let Some(previous) = previous {
            if previous.anchor.as_ref() != Some(&self.sealed_leader)
                || previous.wavelength != self.wavelength
            {
                return Err(PorWeightEpochRecordError::Malformed(
                    "journal boundary changed",
                ));
            }
            previous.blocks.clone()
        } else {
            weighted_tau_at_leader(
                blocklace,
                &self.sealed_leader,
                self.wavelength,
                &old_weights,
                leader_of_wave,
            )
            .map_err(WeightedEpochError::from)
            .map_err(PorWeightEpochRecordError::Ordering)?
        };
        if u64::try_from(sealed.len()).ok() != Some(self.sealed_prefix_len)
            || sealed_prefix_hash(&sealed) != self.sealed_prefix_hash
        {
            return Err(PorWeightEpochRecordError::Malformed(
                "sealed Cordial prefix changed",
            ));
        }
        let sealed_output = OrderedFinalizedOutput::new(
            sealed.clone(),
            Some(self.sealed_leader.clone()),
            self.wavelength,
            authorized.len(),
            blocklace.dom().len(),
        );
        validate_finalized_prefix_checkpoint(&sealed_output, &self.source)?;
        let policy = PorWeightEpochPolicy::new(self.delay_waves)?;
        if policy.effective_wave(blocklace, &sealed_output, &self.source)? != self.effective_wave {
            return Err(PorWeightEpochRecordError::Malformed(
                "effective wave changed",
            ));
        }
        let (blocks, anchor) = weighted_tau_after_verified_epoch_boundary(
            blocklace,
            self.wavelength,
            &old_weights,
            &new_weights,
            WeightedEpochBoundary {
                final_leader: &self.sealed_leader,
                ordered_prefix: &sealed,
                effective_wave: self.effective_wave,
            },
            target,
            leader_of_wave,
        )
        .map_err(PorWeightEpochRecordError::Ordering)?;
        Ok(OrderedFinalizedOutput::new(
            blocks,
            Some(anchor),
            self.wavelength,
            authorized.len(),
            blocklace.dom().len(),
        ))
    }

    fn validate_structure(&self) -> Result<(), PorWeightEpochRecordError> {
        if self.por_round == 0
            || self.source.por_round != self.por_round
            || self.source.finalized_prefix_len == 0
            || self.source.finalized_prefix_len > self.sealed_prefix_len
            || self.delay_waves == 0
            || self.effective_wave == 0
            || self.wavelength == 0
            || !self.sealed_leader.signature.is_empty()
            || !self.source.terminal_block.signature.is_empty()
        {
            return Err(PorWeightEpochRecordError::Malformed("invalid epoch fields"));
        }
        for table in [&self.old_weights, &self.new_weights] {
            if table.is_empty()
                || table.len() > MAX_REPUTATION_STATE_ENTRIES
                || table.iter().any(|(node, _)| {
                    node.0.is_empty() || node.0.len() > MAX_REPUTATION_STATE_NODE_ID_LEN
                })
                || table.windows(2).any(|pair| pair[0].0 >= pair[1].0)
                || table
                    .iter()
                    .map(|(_, weight)| u128::from(*weight))
                    .sum::<u128>()
                    == 0
            {
                return Err(PorWeightEpochRecordError::Malformed("invalid weight table"));
            }
        }
        if self
            .old_weights
            .iter()
            .map(|(node, _)| node)
            .ne(self.new_weights.iter().map(|(node, _)| node))
        {
            return Err(PorWeightEpochRecordError::Malformed(
                "validator membership changed",
            ));
        }
        let old: HashMap<_, _> = self.old_weights.iter().cloned().collect();
        let new: HashMap<_, _> = self.new_weights.iter().cloned().collect();
        if authorized_weight_commitment(&old) != self.old_weights_commitment
            || authorized_weight_commitment(&new) != self.new_weights_commitment
        {
            return Err(PorWeightEpochRecordError::Malformed(
                "weight commitment changed",
            ));
        }
        Ok(())
    }
}

/// Replay a contiguous epoch history, stopping each earlier epoch at the next
/// recorded boundary. This prevents later weights from rewriting old finality.
pub fn replay_epoch_journal(
    records: &[PorWeightEpochRecord],
    blocklace: &Blocklace,
    authorized: &[NodeId],
) -> Result<OrderedFinalizedOutput, PorWeightEpochRecordError> {
    validate_journal(records)?;
    let mut output = None;
    for (index, record) in records.iter().enumerate() {
        output = Some(record.verify_replay_from_prefix(
            blocklace,
            authorized,
            record.reputation_checkpoint_hash(),
            output.as_ref(),
            records.get(index + 1).map(|next| next.sealed_leader()),
        )?);
    }
    output.ok_or(PorWeightEpochRecordError::Malformed("empty epoch journal"))
}

fn validate_journal(records: &[PorWeightEpochRecord]) -> Result<(), PorWeightEpochRecordError> {
    for record in records {
        record.validate_structure()?;
    }
    for pair in records.windows(2) {
        if pair[0].por_round.checked_add(1) != Some(pair[1].por_round)
            || pair[0].effective_wave >= pair[1].effective_wave
            || pair[0].wavelength != pair[1].wavelength
            || pair[0].new_weights != pair[1].old_weights
            || pair[0].sealed_prefix_len >= pair[1].sealed_prefix_len
        {
            return Err(PorWeightEpochRecordError::ConflictingRecord);
        }
    }
    Ok(())
}

/// Append-only logical epoch history, atomically replaced and fsynced on disk.
/// Existing single-record files are accepted and upgraded on the next append.
#[derive(Debug)]
pub struct PorWeightEpochStore {
    directory: PathBuf,
    record_path: PathBuf,
    temporary_path: PathBuf,
    writer: Mutex<()>,
}

impl PorWeightEpochStore {
    pub fn open(data_dir: &Path) -> Result<Self, PorWeightEpochRecordError> {
        let directory = data_dir.join(POR_STATE_DIRECTORY);
        fs::create_dir_all(&directory)?;
        File::open(data_dir)?.sync_all()?;
        Ok(Self {
            record_path: directory.join(POR_WEIGHT_EPOCH_FILE_NAME),
            temporary_path: directory.join(TEMP_FILE_NAME),
            directory,
            writer: Mutex::new(()),
        })
    }

    pub fn record_path(&self) -> &Path {
        &self.record_path
    }

    pub fn restore(&self) -> Result<Option<PorWeightEpochRecord>, PorWeightEpochRecordError> {
        Ok(self.restore_journal()?.pop())
    }

    pub fn restore_journal(&self) -> Result<Vec<PorWeightEpochRecord>, PorWeightEpochRecordError> {
        let file = match File::open(&self.record_path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        };
        let mut encoded = Vec::new();
        file.take((MAX_RECORD_LEN + 1) as u64)
            .read_to_end(&mut encoded)?;
        if encoded.len() > MAX_RECORD_LEN {
            return Err(PorWeightEpochRecordError::TooLarge);
        }
        // Legacy v1 record, or a checksummed list of individually checked v1 records.
        if encoded.starts_with(MAGIC) {
            return Ok(vec![decode(&encoded)?]);
        }
        const JOURNAL: &[u8] = b"por-epoch-journal-v1";
        if !encoded.starts_with(JOURNAL) || encoded.len() < JOURNAL.len() + CHECKSUM_LEN {
            return Err(PorWeightEpochRecordError::Malformed("invalid journal"));
        }
        let end = encoded.len() - CHECKSUM_LEN;
        if encoded[end..] != Blake2b256Hasher.hash(&encoded[..end]) {
            return Err(PorWeightEpochRecordError::ChecksumMismatch);
        }
        let mut cursor = JOURNAL.len();
        let mut records = Vec::new();
        while cursor < end {
            if end - cursor < 8 {
                return Err(PorWeightEpochRecordError::Malformed(
                    "truncated journal length",
                ));
            }
            let length = u64::from_be_bytes(encoded[cursor..cursor + 8].try_into().unwrap());
            cursor += 8;
            let length =
                usize::try_from(length).map_err(|_| PorWeightEpochRecordError::TooLarge)?;
            let next = cursor
                .checked_add(length)
                .filter(|next| *next <= end)
                .ok_or(PorWeightEpochRecordError::Malformed(
                    "truncated journal entry",
                ))?;
            records.push(decode(&encoded[cursor..next])?);
            cursor = next;
        }
        validate_journal(&records)?;
        Ok(records)
    }

    fn write_journal(
        &self,
        records: &[PorWeightEpochRecord],
    ) -> Result<(), PorWeightEpochRecordError> {
        validate_journal(records)?;
        let mut encoded = b"por-epoch-journal-v1".to_vec();
        for record in records {
            let entry = encode(record)?;
            encoded.extend_from_slice(&(entry.len() as u64).to_be_bytes());
            encoded.extend_from_slice(&entry);
            if encoded.len() > MAX_RECORD_LEN - CHECKSUM_LEN {
                return Err(PorWeightEpochRecordError::TooLarge);
            }
        }
        let checksum = Blake2b256Hasher.hash(&encoded);
        encoded.extend_from_slice(&checksum);
        let mut options = OpenOptions::new();
        options.create(true).truncate(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut temporary = options.open(&self.temporary_path)?;
        temporary.write_all(&encoded)?;
        temporary.sync_all()?;
        drop(temporary);
        fs::rename(&self.temporary_path, &self.record_path)?;
        File::open(&self.directory)?.sync_all()?;
        Ok(())
    }

    pub fn persist(&self, record: &PorWeightEpochRecord) -> Result<(), PorWeightEpochRecordError> {
        record.validate_structure()?;
        let _writer = self
            .writer
            .lock()
            .map_err(|_| PorWeightEpochRecordError::WriterLockPoisoned)?;
        let mut records = self.restore_journal()?;
        if records.last() == Some(record) {
            return Ok(());
        }
        records.push(record.clone());
        self.write_journal(&records)
    }

    /// Remove only the identical tail proven not to have committed. Earlier
    /// epochs remain available for replay after a crash during the next commit.
    pub fn discard_uncommitted(
        &self,
        record: &PorWeightEpochRecord,
    ) -> Result<(), PorWeightEpochRecordError> {
        let _writer = self
            .writer
            .lock()
            .map_err(|_| PorWeightEpochRecordError::WriterLockPoisoned)?;
        let mut records = self.restore_journal()?;
        if records.last() != Some(record) {
            return Err(PorWeightEpochRecordError::ConflictingRecord);
        }
        records.pop();
        self.write_journal(&records)
    }
}

fn sorted_weights(weights: &HashMap<NodeId, u64>) -> Vec<(NodeId, u64)> {
    let mut entries: Vec<_> = weights
        .iter()
        .map(|(node, value)| (node.clone(), *value))
        .collect();
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    entries
}

fn sealed_prefix_hash(prefix: &[BlockIdentity]) -> [u8; 32] {
    let mut payload = Vec::new();
    payload.extend_from_slice(PREFIX_DOMAIN);
    payload.extend_from_slice(&(prefix.len() as u64).to_be_bytes());
    for identity in prefix {
        payload.extend_from_slice(&identity.content_hash);
        payload.extend_from_slice(&(identity.creator.0.len() as u64).to_be_bytes());
        payload.extend_from_slice(&identity.creator.0);
    }
    Blake2b256Hasher.hash(&payload)
}

fn encode(record: &PorWeightEpochRecord) -> Result<Vec<u8>, PorWeightEpochRecordError> {
    let payload = serde_json::to_vec(record)
        .map_err(|_| PorWeightEpochRecordError::Malformed("cannot encode epoch"))?;
    let mut encoded = Vec::new();
    encoded.extend_from_slice(MAGIC);
    encoded.extend_from_slice(&VERSION.to_be_bytes());
    encoded.extend_from_slice(&(payload.len() as u64).to_be_bytes());
    encoded.extend_from_slice(&payload);
    let mut checksum_input = Vec::new();
    checksum_input.extend_from_slice(CHECKSUM_DOMAIN);
    checksum_input.extend_from_slice(&encoded);
    encoded.extend_from_slice(&Blake2b256Hasher.hash(&checksum_input));
    Ok(encoded)
}

fn decode(encoded: &[u8]) -> Result<PorWeightEpochRecord, PorWeightEpochRecordError> {
    let header_len = MAGIC.len() + 2 + 8;
    if encoded.len() > MAX_RECORD_LEN || encoded.len() < header_len + CHECKSUM_LEN {
        return Err(PorWeightEpochRecordError::Malformed(
            "invalid record length",
        ));
    }
    if !encoded.starts_with(MAGIC) {
        return Err(PorWeightEpochRecordError::Malformed("invalid record magic"));
    }
    let version = u16::from_be_bytes(encoded[MAGIC.len()..MAGIC.len() + 2].try_into().unwrap());
    if version != VERSION {
        return Err(PorWeightEpochRecordError::UnsupportedVersion(version));
    }
    let payload_len = u64::from_be_bytes(encoded[MAGIC.len() + 2..header_len].try_into().unwrap());
    let payload_len = usize::try_from(payload_len)
        .map_err(|_| PorWeightEpochRecordError::Malformed("invalid payload length"))?;
    let checksum_start =
        header_len
            .checked_add(payload_len)
            .ok_or(PorWeightEpochRecordError::Malformed(
                "invalid payload length",
            ))?;
    if checksum_start.checked_add(CHECKSUM_LEN) != Some(encoded.len()) {
        return Err(PorWeightEpochRecordError::Malformed(
            "invalid payload length",
        ));
    }
    let mut checksum_input = Vec::new();
    checksum_input.extend_from_slice(CHECKSUM_DOMAIN);
    checksum_input.extend_from_slice(&encoded[..checksum_start]);
    if encoded[checksum_start..] != Blake2b256Hasher.hash(&checksum_input) {
        return Err(PorWeightEpochRecordError::ChecksumMismatch);
    }
    let record: PorWeightEpochRecord = serde_json::from_slice(&encoded[header_len..checksum_start])
        .map_err(|_| PorWeightEpochRecordError::Malformed("invalid record payload"))?;
    record.validate_structure()?;
    Ok(record)
}
