use ahash::AHashMap;
use rayon::prelude::*;
use uuid::Uuid;

use crate::dag::batch::proof::{hash_pair, ProofNode, ProofPosition};

/// Minimal Blake3-based MMR for append-only batch accumulation.
#[derive(Clone, Debug)]
pub struct Blake3Mmr {
    peaks: Vec<[u8; 32]>,
    leaves: Vec<[u8; 32]>,
}

impl Blake3Mmr {
    pub fn new() -> Self {
        Self { peaks: Vec::new(), leaves: Vec::new() }
    }

    pub fn from_tx_ids(ids: &[Uuid]) -> Self {
        let leaves: Vec<[u8; 32]> = ids.par_iter().map(hash_leaf).collect();
        let peaks = build_peaks(&leaves);
        Self { peaks, leaves }
    }

    pub fn append(&mut self, tx_id: Uuid) {
        let leaf = hash_leaf(&tx_id);
        self.leaves.push(leaf);
        self.peaks = build_peaks(&self.leaves);
    }

    pub fn root(&self) -> [u8; 32] {
        fold_peaks(&self.peaks)
    }

    pub fn leaves_len(&self) -> usize {
        self.leaves.len()
    }

    pub fn proof(&self, idx: usize) -> Option<MmrProof> {
        if idx >= self.leaves.len() {
            return None;
        }
        let mut proof_nodes = Vec::new();
        let mut index = idx;
        let mut layer = self.leaves.clone();
        while layer.len() > 1 {
            let mut next = Vec::new();
            for chunk in layer.chunks(2) {
                match chunk {
                    [left, right] => {
                        if index / 2 == next.len() {
                            let pos = if index % 2 == 0 {
                                ProofPosition::Right
                            } else {
                                ProofPosition::Left
                            };
                            proof_nodes.push(ProofNode::new(chunk[1 - (index % 2)], pos));
                        }
                        next.push(hash_pair(left, right));
                    }
                    [single] => {
                        next.push(hash_pair(single, single));
                    }
                    _ => {}
                }
            }
            index /= 2;
            layer = next;
        }
        Some(MmrProof { proof: proof_nodes })
    }
}

#[derive(Clone, Debug)]
pub struct MmrProof {
    pub proof: Vec<ProofNode>,
}

pub fn verify_mmr_proof(tx_id: &Uuid, proof: &MmrProof, expected_root: &[u8; 32]) -> bool {
    let leaf = hash_leaf(tx_id);
    let mut acc = leaf;
    for node in &proof.proof {
        match node.position {
            ProofPosition::Left => acc = hash_pair(&node.sibling, &acc),
            ProofPosition::Right => acc = hash_pair(&acc, &node.sibling),
        }
    }
    &acc == expected_root
}

fn hash_leaf(tx_id: &Uuid) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"triad.mmr.leaf");
    hasher.update(&tx_id.as_u128().to_le_bytes());
    let digest = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(digest.as_bytes());
    out
}

fn build_peaks(leaves: &[[u8; 32]]) -> Vec<[u8; 32]> {
    let mut peaks = Vec::new();
    let mut idx = 0;
    while idx < leaves.len() {
        let mut size = 1;
        let mut hash = leaves[idx];
        while idx + size * 2 <= leaves.len() {
            let left = hash;
            let right = fold_range(leaves, idx + size, size);
            hash = hash_pair(&left, &right);
            size *= 2;
        }
        peaks.push(hash);
        idx += size;
    }
    peaks
}

fn fold_range(leaves: &[[u8; 32]], start: usize, len: usize) -> [u8; 32] {
    if len == 1 {
        return leaves[start];
    }
    let mid = len / 2;
    let left = fold_range(leaves, start, mid);
    let right = fold_range(leaves, start + mid, mid);
    hash_pair(&left, &right)
}

fn fold_peaks(peaks: &[[u8; 32]]) -> [u8; 32] {
    if peaks.is_empty() {
        return [0u8; 32];
    }
    let mut acc = [0u8; 32];
    let mut started = false;
    for peak in peaks {
        if !started {
            acc = *peak;
            started = true;
        } else {
            acc = hash_pair(peak, &acc);
        }
    }
    acc
}


