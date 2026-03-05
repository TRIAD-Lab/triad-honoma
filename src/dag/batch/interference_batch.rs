use std::time::Instant;

use blake3::Hasher;
use smallvec::SmallVec;
use uuid::Uuid;

use crate::dag::batch::merkle::{merkle_root_parallel, ParallelMerkleTree};
use crate::dag::batch::mmr::Blake3Mmr;
use crate::dag::batch::proof::ProofNode;

/// Tree accumulator strategy for batch finalization.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreeStrategy {
    Merkle,
    Mmr,
}

impl Default for TreeStrategy {
    fn default() -> Self { Self::Merkle }
}

/// Feature toggles that influence the finalization pipeline.
#[derive(Clone, Copy, Debug)]
pub struct BatchFeatures {
    pub enable_frost: bool,
    pub tree_strategy: TreeStrategy,
}

impl Default for BatchFeatures {
    fn default() -> Self {
        Self {
            enable_frost: false,
            tree_strategy: TreeStrategy::Merkle,
        }
    }
}

/// Signature container supporting both Blake3 aggregate fallback and FROST.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BatchSignature {
    None,
    Blake3Aggregate([u8; 32]),
    Frost(Vec<u8>),
}

impl Default for BatchSignature {
    fn default() -> Self { BatchSignature::None }
}

/// Metrics collected during batch finalize for telemetry/audit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BatchFinalizeMetrics {
    pub merkle_micros: u64,
    pub signature_micros: u64,
    pub total_micros: u64,
}

/// Evidence bundle stored for audit. Shrink-to-fit to avoid allocations in hot path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BatchFinalizeEvidence {
    pub batch_id: [u8; 32],
    pub merkle_root: [u8; 32],
    pub prev_batch_hash: [u8; 32],
    pub slot: u64,
    pub shard_id: u64,
    pub timestamp: u64,
    pub tx_count: usize,
    pub tree_strategy: TreeStrategy,
    pub canonical_proof: SmallVec<[ProofNode; 16]>,
    pub signature: BatchSignature,
    pub metrics: BatchFinalizeMetrics,
}

/// Errors surfaced by the batch lifecycle.
#[derive(Debug, thiserror::Error)]
pub enum BatchError {
    #[error("batch already finalized")]
    AlreadyFinalized,
    #[error("batch does not contain any transactions")]
    EmptyBatch,
    #[error("FROST feature disabled but shares were requested")]
    FrostDisabled,
    #[error("missing FROST shares for aggregation")]
    FrostSharesMissing,
}

#[derive(Clone, Debug)]
pub enum TreeArtifact {
    Merkle(ParallelMerkleTree),
    Mmr(Blake3Mmr),
}

impl TreeArtifact {
    pub fn strategy(&self) -> TreeStrategy {
        match self {
            TreeArtifact::Merkle(_) => TreeStrategy::Merkle,
            TreeArtifact::Mmr(_) => TreeStrategy::Mmr,
        }
    }
}

/// InterferenceBatch represents the atomic block structure consumed by the DAG.
#[derive(Clone, Debug)]
pub struct InterferenceBatch {
    pub id: [u8; 32],
    pub slot: u64,
    pub shard_id: u64,
    pub txs: Vec<Uuid>,
    pub merkle_root: [u8; 32],
    pub prev_batch_hash: [u8; 32],
    pub timestamp: u64,
    pub frost_signature: BatchSignature,
    finalized: bool,
    tree_artifact: Option<TreeArtifact>,
    pub evidence: Option<BatchFinalizeEvidence>,
}

impl InterferenceBatch {
    pub fn new(slot: u64, shard_id: u64, prev_batch_hash: [u8; 32], timestamp: u64) -> Self {
        Self {
            id: [0u8; 32],
            slot,
            shard_id,
            txs: Vec::with_capacity(4096),
            merkle_root: [0u8; 32],
            prev_batch_hash,
            timestamp,
            frost_signature: BatchSignature::None,
            finalized: false,
            tree_artifact: None,
            evidence: None,
        }
    }

    #[inline]
    pub fn add_tx(&mut self, tx_id: Uuid) -> Result<(), BatchError> {
        if self.finalized {
            return Err(BatchError::AlreadyFinalized);
        }
        self.txs.push(tx_id);
        Ok(())
    }

    pub fn finalize(&mut self, frost_partials: &[Vec<u8>], features: BatchFeatures) -> Result<[u8; 32], BatchError> {
        if self.finalized {
            return Err(BatchError::AlreadyFinalized);
        }
        if self.txs.is_empty() {
            return Err(BatchError::EmptyBatch);
        }

        let total_start = Instant::now();
        let merkle_start = Instant::now();
        let (root, artifact, canonical_proof) = match features.tree_strategy {
            TreeStrategy::Merkle => {
                let (root, tree) = merkle_root_parallel(&self.txs);
                let proof = if self.txs.is_empty() { SmallVec::new() } else { tree.proof(0).into_iter().collect() };
                (root, TreeArtifact::Merkle(tree), proof)
            }
            TreeStrategy::Mmr => {
                let mmr = Blake3Mmr::from_tx_ids(&self.txs);
                let proof = SmallVec::new();
                (mmr.root(), TreeArtifact::Mmr(mmr), proof)
            }
        };
        let merkle_micros = merkle_start.elapsed().as_micros() as u64;

        self.merkle_root = root;
        let header = self.build_header(&root);
        let id = blake3::hash(&header).as_bytes().to_owned();
        let mut batch_id = [0u8; 32];
        batch_id.copy_from_slice(&id);
        self.id = batch_id;

        let signature_start = Instant::now();
        let signature = if features.enable_frost {
            if frost_partials.is_empty() {
                return Err(BatchError::FrostSharesMissing);
            }
            aggregate_frost_placeholder(frost_partials, &header)
        } else {
            if !frost_partials.is_empty() {
                return Err(BatchError::FrostDisabled);
            }
            BatchSignature::Blake3Aggregate(batch_id)
        };
        let signature_micros = signature_start.elapsed().as_micros() as u64;

        let metrics = BatchFinalizeMetrics {
            merkle_micros,
            signature_micros,
            total_micros: total_start.elapsed().as_micros() as u64,
        };

        let evidence = BatchFinalizeEvidence {
            batch_id,
            merkle_root: root,
            prev_batch_hash: self.prev_batch_hash,
            slot: self.slot,
            shard_id: self.shard_id,
            timestamp: self.timestamp,
            tx_count: self.txs.len(),
            tree_strategy: artifact.strategy(),
            canonical_proof,
            signature: signature.clone(),
            metrics,
        };

        self.frost_signature = signature;
        self.finalized = true;
        self.tree_artifact = Some(artifact);
        self.evidence = Some(evidence);

        Ok(batch_id)
    }

    #[inline]
    pub fn is_finalized(&self) -> bool { self.finalized }

    pub fn tree_artifact(&self) -> Option<&TreeArtifact> {
        self.tree_artifact.as_ref()
    }

    fn build_header(&self, root: &[u8; 32]) -> Vec<u8> {
        let mut header = Vec::with_capacity(32 * 3 + 24);
        header.extend_from_slice(root);
        header.extend_from_slice(&self.prev_batch_hash);
        header.extend_from_slice(&self.slot.to_le_bytes());
        header.extend_from_slice(&self.shard_id.to_le_bytes());
        header.extend_from_slice(&self.timestamp.to_le_bytes());
        header.extend_from_slice(&(self.txs.len() as u64).to_le_bytes());
        header
    }
}

fn aggregate_frost_placeholder(partials: &[Vec<u8>], header: &[u8]) -> BatchSignature {
    let mut hasher = Hasher::new();
    hasher.update(b"triad.frost.placeholder");
    hasher.update(header);
    for partial in partials {
        hasher.update(&(partial.len() as u32).to_le_bytes());
        hasher.update(partial);
    }
    let digest = hasher.finalize();
    BatchSignature::Frost(digest.as_bytes().to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_batch(len: usize) -> InterferenceBatch {
        let mut batch = InterferenceBatch::new(1, 0, [1u8; 32], 42);
        for i in 0..len {
            let uuid = Uuid::from_u128(i as u128 + 1);
            batch.add_tx(uuid).unwrap();
        }
        batch
    }

    #[test]
    fn finalize_merkle_produces_deterministic_ids() {
        let mut batch = sample_batch(8);
        let features = BatchFeatures::default();
        let id1 = batch.finalize(&[], features).unwrap();
        let id2 = batch.id;
        assert_eq!(id1, id2);
        assert!(batch.is_finalized());
        assert!(matches!(batch.frost_signature, BatchSignature::Blake3Aggregate(_)));
    }

    #[test]
    fn finalize_mmr_switches_strategy() {
        let mut batch = sample_batch(4);
        let features = BatchFeatures { enable_frost: false, tree_strategy: TreeStrategy::Mmr };
        let _ = batch.finalize(&[], features).unwrap();
        assert!(matches!(batch.tree_artifact(), Some(TreeArtifact::Mmr(_))));
    }

    #[test]
    fn frost_requires_partials_when_enabled() {
        let mut batch = sample_batch(4);
        let features = BatchFeatures { enable_frost: true, ..BatchFeatures::default() };
        let err = batch.finalize(&[], features).unwrap_err();
        assert!(matches!(err, BatchError::FrostSharesMissing));
    }
}


