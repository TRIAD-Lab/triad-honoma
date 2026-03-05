//! Abstraction for hashing algorithms to support both optimized Blake3
//! and legacy SHA256 within the TRIAD SVM.

use crate::hash::{Hash, HASH_BYTES};

pub trait HashProvider: Send + Sync {
    /// Compute the hash of a single data slice.
    fn hash(&self, data: &[u8]) -> Hash;

    /// Compute the hash of multiple data slices.
    fn hashv(&self, vals: &[&[u8]]) -> Hash;
}

/// The default TRIAD hash provider, powered by Blake3 for maximum performance.
pub struct TriadHashProvider;

impl HashProvider for TriadHashProvider {
    fn hash(&self, data: &[u8]) -> Hash {
        let hash = blake3::hash(data);
        Hash::new_from_array(*hash.as_bytes())
    }

    fn hashv(&self, vals: &[&[u8]]) -> Hash {
        let mut hasher = blake3::Hasher::new();
        for val in vals {
            hasher.update(val);
        }
        let result = hasher.finalize();
        Hash::new_from_array(*result.as_bytes())
    }
}

/// Legacy SHA256 provider for extreme compatibility with Solana-calculated hashes.
pub struct CompatShaProvider;

impl HashProvider for CompatShaProvider {
    fn hash(&self, data: &[u8]) -> Hash {
        use sha2::{Digest, Sha256};
        let result = Sha256::digest(data);
        Hash::new_from_array(result.into())
    }

    fn hashv(&self, vals: &[&[u8]]) -> Hash {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        for val in vals {
            hasher.update(val);
        }
        let result = hasher.finalize();
        Hash::new_from_array(result.into())
    }
}

/// Global/Contextual discovery of the active HashProvider.
/// In TRIAD, this defaults to TriadHashProvider (Blake3).
pub fn get_hash_provider() -> &'static dyn HashProvider {
    static TRIAD_PROVIDER: TriadHashProvider = TriadHashProvider;
    &TRIAD_PROVIDER
}
