use crate::sdk::hash::{Hash, hasher::Hasher};

/// Handles state hashing and execution traces for deterministic verification.
pub struct DeterminismEngine;

impl DeterminismEngine {
    /// Generates a unified hash for the entire wave's execution trace.
    /// This hash is used for bisect-debugging and network consensus.
    pub fn hash_wave_trace(phases_hashes: &[[u8; 32]]) -> [u8; 32] {
        let mut hasher = Hasher::default();
        for hash in phases_hashes {
            hasher.hash(hash);
        }
        hasher.result().to_bytes()
    }
}
