use crate::sdk::account::AccountSharedData;
use crate::trvm::accounts::{AccountGeneration, RawAccount};
use std::ptr::NonNull;

/// Backend that owns AccountSharedData (off-VM) and exposes a RawAccount view (zero-copy).
#[derive(Debug)]
pub struct AccountBackend {
    pub raw: RawAccount,
    pub backing: Box<AccountSharedData>,
}

impl AccountBackend {
    pub fn from_account_shared_data(mut account: AccountSharedData, generation: NonNull<AccountGeneration>) -> Self {
        // SAFETY: AccountSharedData stays pinned inside backing Box; pointers remain valid during wave.
        let mut backing = Box::new(account);
        let data_slice = backing.data_mut();
        let raw = RawAccount {
            lamports: NonNull::from(&mut backing.lamports),
            data: NonNull::new(data_slice.as_mut_ptr()).expect("account data ptr"),
            data_len: data_slice.len(),
            owner: NonNull::from(&mut backing.owner),
            executable: NonNull::from(&mut backing.executable),
            rent_epoch: NonNull::from(&mut backing.rent_epoch),
            generation,
            current_gen_value: unsafe { generation.as_ref().current() },
        };
        Self { raw, backing }
    }

    #[inline]
    pub fn bump_generation(&mut self) {
        let next = unsafe { self.raw.generation.as_ref().next() };
        self.raw.current_gen_value = next;
    }
}

unsafe impl Send for AccountBackend {}
unsafe impl Sync for AccountBackend {}
