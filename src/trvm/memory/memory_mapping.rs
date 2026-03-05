use solana_rbpf::memory_region::MemoryRegion;
use crate::trvm::accounts::RawAccount;
use rayon::prelude::*;

/// Manages zero-copy memory regions for the sBPF VM.
pub struct MemoryManager;

impl MemoryManager {
    /// Creates a set of memory regions for sBPF from raw accounts.
    /// Uses Rayon to process large sets of accounts in parallel.
    pub fn map_accounts(accounts: &[RawAccount]) -> Vec<MemoryRegion> {
        accounts.par_iter().map(|acc| {
            MemoryRegion::new_writable(
                acc.data.as_ptr(),
                0, // vm_addr (will be mapped by the loader)
                acc.data_len as u64,
            )
        }).collect()
    }
}
