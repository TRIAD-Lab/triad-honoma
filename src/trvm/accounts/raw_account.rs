use std::ptr::NonNull;
use crate::sdk::pubkey::Pubkey;
use super::generation::AccountGeneration;

/// Zero-copy account representation in TRVM.
/// Contains direct pointers to host memory.
#[derive(Debug, Clone)]
pub struct RawAccount {
    pub lamports: NonNull<u64>,
    pub data: NonNull<u8>,
    pub data_len: usize,
    pub owner: NonNull<Pubkey>,
    pub executable: NonNull<bool>,
    pub rent_epoch: NonNull<u64>,
    
    /// Reference to the generation counter for pointer validity checks
    pub generation: NonNull<AccountGeneration>,
    pub current_gen_value: u64,
}

impl RawAccount {
    /// Checks if the memory generation has changed (e.g., due to realloc)
    #[inline]
    pub fn is_valid(&self) -> bool {
        unsafe { self.generation.as_ref().current() == self.current_gen_value }
    }

    /// Converts an access error into a Trace signal (for DX)
    #[inline]
    pub fn check_safety(&self) -> Result<(), String> {
        let actual_gen = unsafe { self.generation.as_ref().current() };
        if actual_gen != self.current_gen_value {
            return Err(format!(
                "Trace::InvalidGenerationAccess: expected {}, found {}",
                self.current_gen_value, actual_gen
            ));
        }
        Ok(())
    }
}

// RawAccount contains raw pointers, so Send/Sync implementation 
// must be deliberate. In the context of TRVM Waves, this is safe
// as the Scheduler guarantees exclusive access to Write-accounts.
unsafe impl Send for RawAccount {}
unsafe impl Sync for RawAccount {}
