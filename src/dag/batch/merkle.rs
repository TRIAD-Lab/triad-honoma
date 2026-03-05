use rayon::prelude::*;
use uuid::Uuid;

use crate::dag::batch::proof::{hash_pair, ProofNode, ProofPosition};

/// Blake3-based Merkle tree built with rayon parallelism.
#[derive(Clone, Debug)]
pub struct ParallelMerkleTree {
    levels: Vec<Vec<[u8; 32]>>,
}

impl ParallelMerkleTree {
    pub fn root(&self) -> [u8; 32] {
        self.levels
            .last()
            .and_then(|level| level.first().copied())
            .unwrap_or([0u8; 32])
    }

    pub fn leaf_hash(&self, idx: usize) -> Option<[u8; 32]> {
        self.levels.first().and_then(|leaves| leaves.get(idx).copied())
    }

    pub fn proof(&self, mut idx: usize) -> Vec<ProofNode> {
        if self.levels.is_empty() {
            return Vec::new();
        }
        let mut proof = Vec::with_capacity(self.levels.len().saturating_sub(1));
        for level in &self.levels[..self.levels.len() - 1] {
            let sibling_idx = if idx % 2 == 0 { idx + 1 } else { idx - 1 };
            let sibling = level
                .get(sibling_idx)
                .unwrap_or_else(|| level.get(idx).expect("sibling or self exists"));
            let position = if idx % 2 == 0 {
                ProofPosition::Right
            } else {
                ProofPosition::Left
            };
            proof.push(ProofNode::new(*sibling, position));
            idx /= 2;
        }
        proof
    }

    pub fn leaves_len(&self) -> usize {
        self.levels.first().map(|l| l.len()).unwrap_or(0)
    }
}

/// Compute merkle root and tree in parallel.
pub fn merkle_root_parallel(txs: &[Uuid]) -> ([u8; 32], ParallelMerkleTree) {
    if txs.is_empty() {
        return ([0u8; 32], ParallelMerkleTree { levels: Vec::new() });
    }

    let mut levels: Vec<Vec<[u8; 32]>> = Vec::new();
    let leaves: Vec<[u8; 32]> = txs
        .par_iter()
        .map(hash_leaf)
        .collect();
    levels.push(leaves);

    while levels.last().map(|level| level.len()).unwrap_or(0) > 1 {
        let prev = levels.last().unwrap();
        let next: Vec<[u8; 32]> = prev
            .par_chunks(2)
            .map(|chunk| match chunk {
                [left, right] => hash_pair(left, right),
                [single] => hash_pair(single, single),
                _ => unreachable!(),
            })
            .collect();
        levels.push(next);
    }

    let root = levels
        .last()
        .and_then(|lvl| lvl.first().copied())
        .unwrap_or([0u8; 32]);
    (root, ParallelMerkleTree { levels })
}

/// Verify inclusion proof for a transaction ID.
pub fn verify_merkle_proof(tx_id: &Uuid, proof: &[ProofNode], expected_root: &[u8; 32]) -> bool {
    if proof.is_empty() {
        return false;
    }
    let mut acc = hash_leaf(tx_id);
    for node in proof {
        match node.position {
            ProofPosition::Left => acc = hash_pair(&node.sibling, &acc),
            ProofPosition::Right => acc = hash_pair(&acc, &node.sibling),
        }
    }
    &acc == expected_root
}

fn hash_leaf(tx_id: &Uuid) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"triad.merkle.leaf");
    hasher.update(&tx_id.as_u128().to_le_bytes());
    let digest = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(digest.as_bytes());
    out
}


