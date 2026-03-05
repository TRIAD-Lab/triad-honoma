//! Data shared between program runtime and built-in programs as well as SBF programs.
#![deny(clippy::indexing_slicing)]

#[cfg(all(not(target_os = "solana"), debug_assertions))]
use crate::signature::Signature;
#[cfg(not(target_os = "solana"))]
use {
    crate::{
        rent::Rent,
        system_instruction::{
            MAX_PERMITTED_ACCOUNTS_DATA_ALLOCATIONS_PER_TRANSACTION, MAX_PERMITTED_DATA_LENGTH,
        },
    },
    solana_program::entrypoint::MAX_PERMITTED_DATA_INCREASE,
    std::mem::MaybeUninit,
};
use crate::{
    instruction::InstructionError,
    pubkey::Pubkey,
    shadow_borrow_tracker::ShadowBorrowTracker,
    account::{AccountSharedData, ReadableAccount, WritableAccount},
};
use std::{
    collections::HashSet,
    pin::Pin,
    sync::RwLock,
    // rc::Rc, // Rc is generally replaced by Arc for Send compatibility
    sync::atomic::{AtomicI64, AtomicBool, Ordering},
    sync::Arc,
};
use bincode::deserialize;

/// Index of an account inside of the TransactionContext or an InstructionContext.
pub type IndexOfAccount = u16;

/// Wave identifier used across TRVM wave execution.
pub type WaveId = Arc<str>;
/// Logical time/slot in TRIAD DAG pipeline.
pub type LogicalTime = u64;
/// Compute budget for wave execution (CU).
pub type CuBudget = u64;

/// Contains account meta data which varies between instruction.
///
/// It also contains indices to other structures for faster lookup.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstructionAccount {
    /// Points to the account and its key in the `TransactionContext`
    pub index_in_transaction: IndexOfAccount,
    /// Points to the first occurrence in the parent `InstructionContext`
    ///
    /// This excludes the program accounts.
    pub index_in_caller: IndexOfAccount,
    /// Points to the first occurrence in the current `InstructionContext`
    ///
    /// This excludes the program accounts.
    pub index_in_callee: IndexOfAccount,
    /// Is this account supposed to sign
    pub is_signer: bool,
    /// Is this account allowed to become writable
    pub is_writable: bool,
}

/// TRVM does not support legacy SVM execution.
/// All execution is wave-based, zero-copy, and lock-free.
///
/// An account key and the matching account
pub type TransactionAccount = (Pubkey, AccountSharedData);

#[derive(Debug)]
pub struct CloneableAtomicBool(AtomicBool);

impl Clone for CloneableAtomicBool {
    fn clone(&self) -> Self {
        Self(AtomicBool::new(self.0.load(Ordering::Relaxed)))
    }
}

impl PartialEq for CloneableAtomicBool {
    fn eq(&self, other: &Self) -> bool {
        self.0.load(Ordering::Relaxed) == other.0.load(Ordering::Relaxed)
    }
}

impl CloneableAtomicBool {
    pub fn new(val: bool) -> Self {
        Self(AtomicBool::new(val))
    }
}

impl std::ops::Deref for CloneableAtomicBool {
    type Target = AtomicBool;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TransactionAccounts {
    accounts: Vec<AccountBackend>,
    touched_flags: Box<[CloneableAtomicBool]>,
}

/// Backend for account storage: raw pointer only (legacy SVM is unsupported).
#[derive(Debug, Clone, PartialEq)]
pub struct AccountBackend {
    pub raw: RawAccountBackend,
    #[cfg(not(target_os = "solana"))]
    backing: Box<AccountSharedData>,
}

impl AccountBackend {
    #[cfg(not(target_os = "solana"))]
    pub fn from_account_shared_data(mut account: AccountSharedData) -> Self {
        let mut backing = Box::new(account);
        let raw = unsafe { RawAccountBackend::from_account_shared_data(&mut backing) };
        Self { raw, backing }
    }

    pub fn borrow(&self) -> std::cell::Ref<AccountSharedData> {
        panic!("Legacy path invoked: borrow() is unsupported in TRVM");
    }

    pub fn borrow_mut(&self) -> std::cell::RefMut<AccountSharedData> {
        panic!("Legacy path invoked: borrow_mut() is unsupported in TRVM");
    }
}

/// Raw pointers to account memory.
/// SAFETY: The Wave Scheduler guarantees exclusive access during wave execution.
#[derive(Debug, Clone)]
#[derive(PartialEq)]
pub struct RawAccountBackend {
    pub lamports: *mut u64,
    pub data: *mut u8,
    pub data_len: usize,
    pub owner: *mut Pubkey,
    pub executable: *mut bool,
    pub rent_epoch: *mut u64,
    /// Pointer to the atomic generation counter (managed by Wave Scheduler or ShadowBorrowTracker)
    pub generation_ptr: *const u64,
    /// The generation value when this backend was created/refreshed
    pub cached_generation_value: u64,
}

// SAFETY: RawAccountBackend is Send/Sync because the Wave Scheduler guarantees
// that only one thread accesses a given account at a time within a wave.
unsafe impl Send for RawAccountBackend {}
unsafe impl Sync for RawAccountBackend {}

impl RawAccountBackend {
    /// Create a new RawAccountBackend from raw pointers.
    /// 
    /// # Safety
    /// - All pointers must be valid and properly aligned.
    /// - The caller must ensure exclusive access during the lifetime of this backend.
    /// - Pointers must remain valid for the duration of the wave execution.
    pub unsafe fn new(
        lamports: *mut u64,
        data: *mut u8,
        data_len: usize,
        owner: *mut Pubkey,
        executable: *mut bool,
        rent_epoch: *mut u64,
        generation_ptr: *const u64,
        cached_generation_value: u64,
    ) -> Self {
        debug_assert!(!lamports.is_null(), "lamports pointer is null");
        debug_assert!(!owner.is_null(), "owner pointer is null");
        debug_assert!(!executable.is_null(), "executable pointer is null");
        debug_assert!(!rent_epoch.is_null(), "rent_epoch pointer is null");
        debug_assert!(data_len == 0 || !data.is_null(), "data pointer is null but data_len > 0");
        
        Self {
            lamports,
            data,
            data_len,
            owner,
            executable,
            rent_epoch,
            generation_ptr,
            cached_generation_value,
        }
    }

    /// Create from a mutable reference to AccountSharedData.
    /// This is useful for transitioning from Legacy to Native within a single wave.
    /// 
    /// # Safety
    /// - The AccountSharedData must remain valid and unmoved for the lifetime of this backend.
    /// - No other references to the AccountSharedData may exist.
    pub unsafe fn from_account_shared_data(account: &mut AccountSharedData) -> Self {
        let data_vec = std::sync::Arc::make_mut(&mut account.data);
        Self {
            lamports: &mut account.lamports as *mut u64,
            data: data_vec.as_mut_ptr(),
            data_len: data_vec.len(),
            owner: &mut account.owner as *mut Pubkey,
            executable: &mut account.executable as *mut bool,
            rent_epoch: &mut account.rent_epoch as *mut u64,
            generation_ptr: std::ptr::null(), // Needs to be set by the caller if tracking is needed
            cached_generation_value: 0,
        }
    }

    /// Returns true if this backend has valid (non-null) pointers for all required fields.
    pub fn is_valid(&self) -> bool {
        if self.lamports.is_null() 
            || self.owner.is_null() 
            || self.executable.is_null() 
            || self.rent_epoch.is_null()
            || self.generation_ptr.is_null()
        {
            return false;
        }
        
        // Logical generation check (DX: InvalidGenerationAccess)
        let current_gen = unsafe { std::ptr::read_volatile(self.generation_ptr) };
        current_gen == self.cached_generation_value
    }

    /// Verifies safety and returns error code for DX
    pub fn check_safety(&self) -> Result<(), String> {
        if self.generation_ptr.is_null() {
            return Err("Trace::HardFault: generation_ptr is null".to_string());
        }
        let current_gen = unsafe { std::ptr::read_volatile(self.generation_ptr) };
        if current_gen != self.cached_generation_value {
            return Err(format!(
                "Trace::InvalidGenerationAccess: expected {}, found {}",
                self.cached_generation_value, current_gen
            ));
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct AccountView {
    pub(crate) raw: RawAccountBackend,
}

impl AccountView {
    pub fn lamports(&self) -> u64 {
        unsafe { *self.raw.lamports }
    }

    pub fn set_lamports(&mut self, lamports: u64) {
        unsafe { *self.raw.lamports = lamports }
    }

    pub fn data(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.raw.data, self.raw.data_len) }
    }

    pub fn data_mut(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.raw.data, self.raw.data_len) }
    }
    
    pub fn owner(&self) -> &Pubkey {
        unsafe { &*self.raw.owner }
    }

    pub fn executable(&self) -> bool {
        unsafe { *self.raw.executable }
    }

    pub fn rent_epoch(&self) -> u64 {
        unsafe { *self.raw.rent_epoch }
    }

    pub fn set_owner(&mut self, owner: Pubkey) {
        unsafe { *self.raw.owner = owner }
    }

    pub fn set_executable(&mut self, executable: bool) {
        unsafe { *self.raw.executable = executable }
    }

    pub fn deserialize_data<T: serde::de::DeserializeOwned>(&self) -> Result<T, InstructionError> {
        bincode::deserialize(self.data())
            .map_err(|_| InstructionError::InvalidAccountData)
    }

    pub fn set_rent_epoch(&mut self, rent_epoch: u64) {
        unsafe { *self.raw.rent_epoch = rent_epoch }
    }

    pub fn copy_into_owner_from_slice(&mut self, source: &[u8]) {
        unsafe {
            std::ptr::copy_nonoverlapping(source.as_ptr(), self.raw.owner as *mut u8, source.len());
        }
    }

    pub fn set_data_from_slice(&mut self, data: &[u8]) {
        unsafe {
            if self.raw.data_len != data.len() {
                panic!("Native set_data_from_slice length mismatch");
            }
            std::ptr::copy_nonoverlapping(data.as_ptr(), self.raw.data, data.len());
        }
    }

    pub fn set_data(&mut self, data: Vec<u8>) {
        let _ = data;
        panic!("set_data(Vec) not supported for Native backend");
    }

    /// Resize account data.
    /// For Native backend: TRIAD Wave scheduler pre-allocates account space.
    /// Resize is only allowed if new_len <= current capacity (shrink or same).
    /// Growth requires reallocation which is handled by the Wave scheduler, not inline.
    pub fn resize(&mut self, new_len: usize, value: u8) {
        if new_len > self.raw.data_len {
            panic!(
                "Native resize: growth from {} to {} not supported inline. \
                        Use Wave scheduler realloc.",
                self.raw.data_len, new_len
            );
        }
        if new_len < self.raw.data_len {
            unsafe {
                std::ptr::write_bytes(self.raw.data.add(new_len), value, self.raw.data_len - new_len);
            }
        }
        self.raw.data_len = new_len;
    }

    /// Extend account data from slice.
    /// For Native backend: not supported as it requires dynamic growth.
    pub fn extend_from_slice(&mut self, data: &[u8]) {
        panic!(
            "Native extend_from_slice: dynamic growth not supported. \
                    Current len: {}, attempting to add {} bytes.",
            self.raw.data_len,
            data.len()
        );
    }

    pub fn reserve(&mut self, additional: usize) {
        let _ = additional;
    }

    pub fn make_data_mut(&mut self) {
        // Native backend operates on pre-allocated buffers; no-op.
    }

    pub fn capacity(&self) -> usize {
        self.raw.data_len
    }

    pub fn is_shared(&self) -> bool {
        false
    }
}

impl solana_sdk::account::ReadableAccount for AccountView {
    fn lamports(&self) -> u64 {
        self.lamports()
    }
    fn data(&self) -> &[u8] {
        self.data()
    }
    fn owner(&self) -> &Pubkey {
        self.owner()
    }
    fn executable(&self) -> bool {
        self.executable()
    }
    fn rent_epoch(&self) -> u64 {
        self.rent_epoch()
    }
}

impl PartialEq<AccountSharedData> for AccountView {
    fn eq(&self, other: &AccountSharedData) -> bool {
        self.lamports() == other.lamports()
            && self.data() == other.data()
            && self.owner() == other.owner()
            && self.executable() == other.executable()
            && self.rent_epoch() == other.rent_epoch()
    }
}

impl TransactionAccounts {
    #[cfg(not(target_os = "solana"))]
    fn new(accounts: Vec<AccountSharedData>, borrow_tracker: &ShadowBorrowTracker) -> TransactionAccounts {
        let mut backends: Vec<_> = accounts
            .into_iter()
            .map(AccountBackend::from_account_shared_data)
            .collect();

        for (i, backend) in backends.iter_mut().enumerate() {
            if let Ok(ptr) = borrow_tracker.get_generation_ptr(i as IndexOfAccount) {
                backend.raw.generation_ptr = ptr;
            }
        }

        let mut flags = Vec::with_capacity(backends.len());
        for _ in 0..backends.len() {
            flags.push(CloneableAtomicBool::new(false));
        }

        TransactionAccounts {
            touched_flags: flags.into_boxed_slice(),
            accounts: backends,
        }
    }

    fn len(&self) -> usize {
        self.accounts.len()
    }

    pub fn get(&self, index: IndexOfAccount) -> Option<&AccountBackend> {
        self.accounts.get(index as usize)
    }

    #[cfg(not(target_os = "solana"))]
    pub fn touch(&self, index: IndexOfAccount) -> Result<(), InstructionError> {
        self.touched_flags
            .get(index as usize)
            .ok_or(InstructionError::NotEnoughAccountKeys)?
            .store(true, Ordering::Release);
        Ok(())
    }

    #[cfg(not(target_os = "solana"))]
    pub fn touched_count(&self) -> usize {
        self.touched_flags
            .iter()
            .fold(0usize, |accumulator, was_touched| {
                if was_touched.load(Ordering::Acquire) {
                    accumulator + 1
                } else {
                    accumulator
                }
            })
    }

    pub fn try_borrow(
        &self,
        _index: IndexOfAccount,
    ) -> Result<std::cell::Ref<'_, AccountSharedData>, InstructionError> {
        panic!("Legacy path invoked: try_borrow is unsupported in TRVM");
    }

    pub fn try_borrow_mut(
        &self,
        _index: IndexOfAccount,
    ) -> Result<std::cell::RefMut<'_, AccountSharedData>, InstructionError> {
        panic!("Legacy path invoked: try_borrow_mut is unsupported in TRVM");
    }

    pub fn into_accounts(self) -> Vec<AccountSharedData> {
        self.accounts
            .into_iter()
            .map(|backend| *backend.backing)
            .collect()
    }
}

/// Loaded transaction shared between runtime and programs.
#[derive(Debug)]
pub struct CloneableAtomicI64(AtomicI64);

impl Clone for CloneableAtomicI64 {
    fn clone(&self) -> Self {
        Self(AtomicI64::new(self.0.load(Ordering::Relaxed)))
    }
}

impl PartialEq for CloneableAtomicI64 {
    fn eq(&self, other: &Self) -> bool {
        self.0.load(Ordering::Relaxed) == other.0.load(Ordering::Relaxed)
    }
}

impl CloneableAtomicI64 {
    pub fn new(val: i64) -> Self {
        Self(AtomicI64::new(val))
    }
}

impl std::ops::Deref for CloneableAtomicI64 {
    type Target = AtomicI64;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Loaded transaction shared between runtime and programs.
///
/// This context is valid for the entire duration of a transaction being processed.
#[derive(Debug)]
pub struct CloneableRwLock<T>(RwLock<T>);

impl<T: Clone> Clone for CloneableRwLock<T> {
    fn clone(&self) -> Self {
        Self(RwLock::new(self.0.read().unwrap().clone()))
    }
}

impl<T: PartialEq> PartialEq for CloneableRwLock<T> {
    fn eq(&self, other: &Self) -> bool {
        *self.0.read().unwrap() == *other.0.read().unwrap()
    }
}

impl<T> CloneableRwLock<T> {
    pub fn new(val: T) -> Self {
        Self(RwLock::new(val))
    }
}

impl<T> std::ops::Deref for CloneableRwLock<T> {
    type Target = RwLock<T>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TransactionContext {
    account_keys: Pin<Box<[Pubkey]>>,
    accounts: Arc<TransactionAccounts>,
    // Wave-scoped metadata (TRVM only)
    generation: u64,
    wave_id: WaveId,
    logical_time: LogicalTime,
    cu_budget: CuBudget,
    instruction_stack_capacity: usize,
    instruction_trace_capacity: usize,
    instruction_stack: Vec<usize>,
    instruction_trace: Vec<InstructionContext>,
    return_data: TransactionReturnData,
    accounts_resize_delta: CloneableAtomicI64,
    #[cfg(not(target_os = "solana"))]
    rent: Rent,
    /// Useful for debugging to filter by or to look it up on the explorer
    #[cfg(all(not(target_os = "solana"), debug_assertions))]
    signature: Signature,
    borrow_tracker: CloneableRwLock<ShadowBorrowTracker>,
}

impl TransactionContext {
    /// Constructs a new TransactionContext
    #[cfg(not(target_os = "solana"))]
    pub fn new(
        transaction_accounts: Vec<TransactionAccount>,
        rent: Rent,
        instruction_stack_capacity: usize,
        instruction_trace_capacity: usize,
    ) -> Self {
        let (account_keys, accounts): (Vec<_>, Vec<_>) = transaction_accounts
            .into_iter()
            .map(|(key, account)| (key, account))
            .unzip();
        let borrow_tracker = ShadowBorrowTracker::new(account_keys.len());
        let accounts = TransactionAccounts::new(accounts, &borrow_tracker);
        
        Self {
            account_keys: Pin::new(account_keys.into_boxed_slice()),
            accounts: Arc::new(accounts),
            generation: 0,
            wave_id: Arc::from("wave-uninitialized"),
            logical_time: 0,
            cu_budget: 0,
            instruction_stack_capacity,
            instruction_trace_capacity,
            instruction_stack: Vec::with_capacity(instruction_stack_capacity),
            instruction_trace: vec![InstructionContext::default()],
            return_data: TransactionReturnData::default(),
            accounts_resize_delta: CloneableAtomicI64::new(0),
            rent,
            #[cfg(all(not(target_os = "solana"), debug_assertions))]
            signature: Signature::default(),
            borrow_tracker: CloneableRwLock::new(borrow_tracker),
        }
    }

    /// Used in mock_process_instruction
    #[cfg(not(target_os = "solana"))]
    pub fn deconstruct_without_keys(self) -> Result<Vec<AccountSharedData>, InstructionError> {
        if !self.instruction_stack.is_empty() {
            return Err(InstructionError::CallDepth);
        }

        Ok(Arc::try_unwrap(self.accounts)
            .expect("transaction_context.accounts has unexpected outstanding refs")
            .into_accounts())
    }

    #[cfg(not(target_os = "solana"))]
    pub fn accounts(&self) -> &Arc<TransactionAccounts> {
        &self.accounts
    }

    /// Stores the signature of the current transaction
    #[cfg(all(not(target_os = "solana"), debug_assertions))]
    pub fn set_signature(&mut self, signature: &Signature) {
        self.signature = *signature;
    }

    /// Returns the signature of the current transaction
    #[cfg(all(not(target_os = "solana"), debug_assertions))]
    pub fn get_signature(&self) -> &Signature {
        &self.signature
    }

    /// Returns the total number of accounts loaded in this Transaction
    pub fn get_number_of_accounts(&self) -> IndexOfAccount {
        self.accounts.len() as IndexOfAccount
    }

    /// Searches for an account by its key
    pub fn get_key_of_account_at_index(
        &self,
        index_in_transaction: IndexOfAccount,
    ) -> Result<&Pubkey, InstructionError> {
        self.account_keys
            .get(index_in_transaction as usize)
            .ok_or(InstructionError::NotEnoughAccountKeys)
    }

    /// Searches for an account by its key
    #[cfg(not(target_os = "solana"))]
    pub fn get_account_at_index(
        &self,
        index_in_transaction: IndexOfAccount,
    ) -> Result<&AccountBackend, InstructionError> {
        self.accounts
            .get(index_in_transaction)
            .ok_or(InstructionError::NotEnoughAccountKeys)
    }

    /// Helper for zero-copy access to account data.
    /// Abstracts over RefCell (Linear/Legacy) and Raw Pointer (Wave/Native).
    pub fn get_account_view(&self, index: IndexOfAccount) -> Result<AccountView, InstructionError> {
        let backend = self.get_account_at_index(index)?;
        Ok(AccountView { raw: backend.raw.clone() })
    }

    /// Searches for an account by its key
    pub fn find_index_of_account(&self, pubkey: &Pubkey) -> Option<IndexOfAccount> {
        self.account_keys
            .iter()
            .position(|key| key == pubkey)
            .map(|index| index as IndexOfAccount)
    }

    /// Searches for a program account by its key
    pub fn find_index_of_program_account(&self, pubkey: &Pubkey) -> Option<IndexOfAccount> {
        self.account_keys
            .iter()
            .rposition(|key| key == pubkey)
            .map(|index| index as IndexOfAccount)
    }

    /// Gets the max length of the InstructionContext trace
    pub fn get_instruction_trace_capacity(&self) -> usize {
        panic!("Legacy path invoked: instruction trace is unsupported in TRVM");
    }

    /// Returns the instruction trace length.
    ///
    /// Not counting the last empty InstructionContext which is always pre-reserved for the next instruction.
    /// See also `get_next_instruction_context()`.
    pub fn get_instruction_trace_length(&self) -> usize {
        panic!("Legacy path invoked: instruction trace is unsupported in TRVM");
    }

    /// Gets an InstructionContext by its index in the trace
    pub fn get_instruction_context_at_index_in_trace(
        &self,
        index_in_trace: usize,
    ) -> Result<&InstructionContext, InstructionError> {
        let _ = index_in_trace;
        panic!("Legacy path invoked: instruction trace is unsupported in TRVM");
    }

    /// Gets an InstructionContext by its nesting level in the stack
    pub fn get_instruction_context_at_nesting_level(
        &self,
        nesting_level: usize,
    ) -> Result<&InstructionContext, InstructionError> {
        let _ = nesting_level;
        panic!("Legacy path invoked: instruction stack is unsupported in TRVM");
    }

    /// Gets the max height of the InstructionContext stack
    pub fn get_instruction_stack_capacity(&self) -> usize {
        panic!("Legacy path invoked: instruction stack is unsupported in TRVM");
    }

    /// Gets instruction stack height, top-level instructions are height
    /// `solana_sdk::instruction::TRANSACTION_LEVEL_STACK_HEIGHT`
    pub fn get_instruction_context_stack_height(&self) -> usize {
        panic!("Legacy path invoked: instruction stack is unsupported in TRVM");
    }

    /// Returns the current InstructionContext
    pub fn get_current_instruction_context(&self) -> Result<&InstructionContext, InstructionError> {
        panic!("Legacy path invoked: instruction stack is unsupported in TRVM");
    }

    /// Returns the InstructionContext to configure for the next invocation.
    ///
    /// The last InstructionContext is always empty and pre-reserved for the next instruction.
    pub fn get_next_instruction_context(
        &mut self,
    ) -> Result<&mut InstructionContext, InstructionError> {
        panic!("Legacy path invoked: instruction trace is unsupported in TRVM");
    }

    /// Pushes the next InstructionContext
    #[cfg(not(target_os = "solana"))]
    pub fn push(&mut self) -> Result<(), InstructionError> {
        panic!("Legacy path invoked: instruction stack/trace is unsupported in TRVM");
    }

    /// Pops the current InstructionContext
    #[cfg(not(target_os = "solana"))]
    pub fn pop(&mut self) -> Result<(), InstructionError> {
        panic!("Legacy path invoked: instruction stack/trace is unsupported in TRVM");
    }

    /// Gets the return data of the current InstructionContext or any above
    pub fn get_return_data(&self) -> (&Pubkey, &[u8]) {
        (&self.return_data.program_id, &self.return_data.data)
    }

    /// Set the return data of the current InstructionContext
    pub fn set_return_data(
        &mut self,
        program_id: Pubkey,
        data: Vec<u8>,
    ) -> Result<(), InstructionError> {
        self.return_data = TransactionReturnData { program_id, data };
        Ok(())
    }

    /// Calculates the sum of all lamports within an instruction
    #[cfg(not(target_os = "solana"))]
    fn instruction_accounts_lamport_sum(
        &self,
        instruction_context: &InstructionContext,
    ) -> Result<u128, InstructionError> {
        let mut instruction_accounts_lamport_sum: u128 = 0;
        for instruction_account_index in 0..instruction_context.get_number_of_instruction_accounts()
        {
            if instruction_context
                .is_instruction_account_duplicate(instruction_account_index)?
                .is_some()
            {
                continue; // Skip duplicate account
            }
            let index_in_transaction = instruction_context
                .get_index_of_instruction_account_in_transaction(instruction_account_index)?;
            instruction_accounts_lamport_sum = (self
                .get_account_view(index_in_transaction)?
                .lamports() as u128)
                .checked_add(instruction_accounts_lamport_sum)
                .ok_or(InstructionError::ArithmeticOverflow)?;
        }
        Ok(instruction_accounts_lamport_sum)
    }

    /// Returns the accounts resize delta
    pub fn accounts_resize_delta(&self) -> Result<i64, InstructionError> {
        Ok(self.accounts_resize_delta.load(Ordering::Relaxed))
    }
}

/// Return data at the end of a transaction
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct TransactionReturnData {
    pub program_id: Pubkey,
    pub data: Vec<u8>,
}

/// Loaded instruction shared between runtime and programs.
///
/// This context is valid for the entire duration of a (possibly cross program) instruction being processed.
#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct InstructionContext {
    nesting_level: usize,
    instruction_accounts_lamport_sum: u128,
    program_accounts: Vec<IndexOfAccount>,
    instruction_accounts: Vec<InstructionAccount>,
    instruction_data: Vec<u8>,
}

impl InstructionContext {
    /// Used together with TransactionContext::get_next_instruction_context()
    #[cfg(not(target_os = "solana"))]
    pub fn configure(
        &mut self,
        program_accounts: &[IndexOfAccount],
        instruction_accounts: &[InstructionAccount],
        instruction_data: &[u8],
    ) {
        self.program_accounts = program_accounts.to_vec();
        self.instruction_accounts = instruction_accounts.to_vec();
        self.instruction_data = instruction_data.to_vec();
    }

    /// How many Instructions were on the stack after this one was pushed
    ///
    /// That is the number of nested parent Instructions plus one (itself).
    pub fn get_stack_height(&self) -> usize {
        self.nesting_level.saturating_add(1)
    }

    /// Number of program accounts
    pub fn get_number_of_program_accounts(&self) -> IndexOfAccount {
        self.program_accounts.len() as IndexOfAccount
    }

    /// Number of accounts in this Instruction (without program accounts)
    pub fn get_number_of_instruction_accounts(&self) -> IndexOfAccount {
        self.instruction_accounts.len() as IndexOfAccount
    }

    /// Assert that enough accounts were supplied to this Instruction
    pub fn check_number_of_instruction_accounts(
        &self,
        expected_at_least: IndexOfAccount,
    ) -> Result<(), InstructionError> {
        if self.get_number_of_instruction_accounts() < expected_at_least {
            Err(InstructionError::NotEnoughAccountKeys)
        } else {
            Ok(())
        }
    }

    /// Data parameter for the programs `process_instruction` handler
    pub fn get_instruction_data(&self) -> &[u8] {
        &self.instruction_data
    }

    /// Searches for a program account by its key
    pub fn find_index_of_program_account(
        &self,
        transaction_context: &TransactionContext,
        pubkey: &Pubkey,
    ) -> Option<IndexOfAccount> {
        self.program_accounts
            .iter()
            .position(|index_in_transaction| {
                transaction_context
                    .account_keys
                    .get(*index_in_transaction as usize)
                    == Some(pubkey)
            })
            .map(|index| index as IndexOfAccount)
    }

    /// Searches for an instruction account by its key
    pub fn find_index_of_instruction_account(
        &self,
        transaction_context: &TransactionContext,
        pubkey: &Pubkey,
    ) -> Option<IndexOfAccount> {
        self.instruction_accounts
            .iter()
            .position(|instruction_account| {
                transaction_context
                    .account_keys
                    .get(instruction_account.index_in_transaction as usize)
                    == Some(pubkey)
            })
            .map(|index| index as IndexOfAccount)
    }

    /// Translates the given instruction wide program_account_index into a transaction wide index
    pub fn get_index_of_program_account_in_transaction(
        &self,
        program_account_index: IndexOfAccount,
    ) -> Result<IndexOfAccount, InstructionError> {
        Ok(*self
            .program_accounts
            .get(program_account_index as usize)
            .ok_or(InstructionError::NotEnoughAccountKeys)?)
    }

    /// Translates the given instruction wide instruction_account_index into a transaction wide index
    pub fn get_index_of_instruction_account_in_transaction(
        &self,
        instruction_account_index: IndexOfAccount,
    ) -> Result<IndexOfAccount, InstructionError> {
        Ok(self
            .instruction_accounts
            .get(instruction_account_index as usize)
            .ok_or(InstructionError::NotEnoughAccountKeys)?
            .index_in_transaction as IndexOfAccount)
    }

    /// Returns `Some(instruction_account_index)` if this is a duplicate
    /// and `None` if it is the first account with this key
    pub fn is_instruction_account_duplicate(
        &self,
        instruction_account_index: IndexOfAccount,
    ) -> Result<Option<IndexOfAccount>, InstructionError> {
        let index_in_callee = self
            .instruction_accounts
            .get(instruction_account_index as usize)
            .ok_or(InstructionError::NotEnoughAccountKeys)?
            .index_in_callee;
        Ok(if index_in_callee == instruction_account_index {
            None
        } else {
            Some(index_in_callee)
        })
    }

    /// Gets the key of the last program account of this Instruction
    pub fn get_last_program_key<'a, 'b: 'a>(
        &'a self,
        transaction_context: &'b TransactionContext,
    ) -> Result<&'b Pubkey, InstructionError> {
        self.get_index_of_program_account_in_transaction(
            self.get_number_of_program_accounts().saturating_sub(1),
        )
        .and_then(|index_in_transaction| {
            transaction_context.get_key_of_account_at_index(index_in_transaction)
        })
    }

    fn try_borrow_account<'a, 'b: 'a>(
        &'a self,
        transaction_context: &'b TransactionContext,
        index_in_transaction: IndexOfAccount,
        index_in_instruction: IndexOfAccount,
    ) -> Result<BorrowedAccount<'a>, InstructionError> {
        // Semantic check via ShadowBorrowTracker (supports QUIRK-007)
        let generation = transaction_context.borrow_tracker.write().unwrap().check_and_borrow(index_in_transaction, true)?;

        let generation = transaction_context.borrow_tracker.write().unwrap().check_and_borrow(index_in_transaction, true)?;

        let account = transaction_context.get_account_view(index_in_transaction)?;
        Ok(BorrowedAccount {
            transaction_context,
            instruction_context: self,
            index_in_transaction,
            index_in_instruction,
            account,
            generation,
        })
    }

    /// Gets the last program account of this Instruction
    pub fn try_borrow_last_program_account<'a, 'b: 'a>(
        &'a self,
        transaction_context: &'b TransactionContext,
    ) -> Result<BorrowedAccount<'a>, InstructionError> {
        let result = self.try_borrow_program_account(
            transaction_context,
            self.get_number_of_program_accounts().saturating_sub(1),
        );
        debug_assert!(result.is_ok());
        result
    }

    /// Tries to borrow a program account from this Instruction
    pub fn try_borrow_program_account<'a, 'b: 'a>(
        &'a self,
        transaction_context: &'b TransactionContext,
        program_account_index: IndexOfAccount,
    ) -> Result<BorrowedAccount<'a>, InstructionError> {
        let index_in_transaction =
            self.get_index_of_program_account_in_transaction(program_account_index)?;
        self.try_borrow_account(
            transaction_context,
            index_in_transaction,
            program_account_index,
        )
    }

    /// Gets an instruction account of this Instruction
    pub fn try_borrow_instruction_account<'a, 'b: 'a>(
        &'a self,
        transaction_context: &'b TransactionContext,
        instruction_account_index: IndexOfAccount,
    ) -> Result<BorrowedAccount<'a>, InstructionError> {
        let index_in_transaction =
            self.get_index_of_instruction_account_in_transaction(instruction_account_index)?;
        self.try_borrow_account(
            transaction_context,
            index_in_transaction,
            self.get_number_of_program_accounts()
                .saturating_add(instruction_account_index),
        )
    }

    /// Returns whether an instruction account is a signer
    pub fn is_instruction_account_signer(
        &self,
        instruction_account_index: IndexOfAccount,
    ) -> Result<bool, InstructionError> {
        Ok(self
            .instruction_accounts
            .get(instruction_account_index as usize)
            .ok_or(InstructionError::MissingAccount)?
            .is_signer)
    }

    /// Returns whether an instruction account is writable
    pub fn is_instruction_account_writable(
        &self,
        instruction_account_index: IndexOfAccount,
    ) -> Result<bool, InstructionError> {
        Ok(self
            .instruction_accounts
            .get(instruction_account_index as usize)
            .ok_or(InstructionError::MissingAccount)?
            .is_writable)
    }

    /// Calculates the set of all keys of signer instruction accounts in this Instruction
    pub fn get_signers(
        &self,
        transaction_context: &TransactionContext,
    ) -> Result<HashSet<Pubkey>, InstructionError> {
        let mut result = HashSet::new();
        for instruction_account in self.instruction_accounts.iter() {
            if instruction_account.is_signer {
                result.insert(
                    *transaction_context
                        .get_key_of_account_at_index(instruction_account.index_in_transaction)?,
                );
            }
        }
        Ok(result)
    }
}

/// Shared account borrowed from the TransactionContext and an InstructionContext.
#[derive(Debug)]
pub struct BorrowedAccount<'a> {
    transaction_context: &'a TransactionContext,
    instruction_context: &'a InstructionContext,
    index_in_transaction: IndexOfAccount,
    index_in_instruction: IndexOfAccount,
    account: AccountView,
    generation: u64,
}

impl<'a> BorrowedAccount<'a> {
    /// Returns the transaction context
    pub fn transaction_context(&self) -> &TransactionContext {
        self.transaction_context
    }

    #[inline]
    fn check_validity(&self) -> Result<(), InstructionError> {
        if self.transaction_context.borrow_tracker.read().unwrap().is_valid(self.index_in_transaction, self.generation) {
            Ok(())
        } else {
            Err(InstructionError::AccountBorrowFailed)
        }
    }

    /// Returns the index of this account (transaction wide)
    #[inline]
    pub fn get_index_in_transaction(&self) -> IndexOfAccount {
        self.index_in_transaction
    }

    /// Returns the public key of this account (transaction wide)
    #[inline]
    pub fn get_key(&self) -> &Pubkey {
        self.transaction_context
            .get_key_of_account_at_index(self.index_in_transaction)
            .unwrap()
    }

    /// Returns the owner of this account (transaction wide)
    #[inline]
    pub fn get_owner(&self) -> &Pubkey {
        self.account.owner()
    }

    /// Assignes the owner of this account (transaction wide)
    #[cfg(not(target_os = "solana"))]
    pub fn set_owner(&mut self, pubkey: &[u8]) -> Result<(), InstructionError> {
        // Only the owner can assign a new owner
        if !self.is_owned_by_current_program() {
            return Err(InstructionError::ModifiedProgramId);
        }
        // and only if the account is writable
        if !self.is_writable() {
            return Err(InstructionError::ModifiedProgramId);
        }
        // and only if the account is not executable
        if self.is_executable() {
            return Err(InstructionError::ModifiedProgramId);
        }
        // and only if the data is zero-initialized or empty
        if !is_zeroed(self.get_data()) {
            return Err(InstructionError::ModifiedProgramId);
        }
        // don't touch the account if the owner does not change
        if self.get_owner().to_bytes() == pubkey {
            return Ok(());
        }
        self.touch()?;
        self.account.copy_into_owner_from_slice(pubkey);
        Ok(())
    }

    /// Returns the number of lamports of this account (transaction wide)
    #[inline]
    pub fn get_lamports(&self) -> u64 {
        self.account.lamports()
    }

    /// Overwrites the number of lamports of this account (transaction wide)
    #[cfg(not(target_os = "solana"))]
    pub fn set_lamports(&mut self, lamports: u64) -> Result<(), InstructionError> {
        // An account not owned by the program cannot have its balance decrease
        if !self.is_owned_by_current_program() && lamports < self.get_lamports() {
            return Err(InstructionError::ExternalAccountLamportSpend);
        }
        // The balance of read-only may not change
        if !self.is_writable() {
            return Err(InstructionError::ReadonlyLamportChange);
        }
        // The balance of executable accounts may not change
        if self.is_executable() {
            return Err(InstructionError::ExecutableLamportChange);
        }
        // don't touch the account if the lamports do not change
        if self.get_lamports() == lamports {
            return Ok(());
        }
        self.touch()?;
        self.account.set_lamports(lamports);
        Ok(())
    }

    /// Adds lamports to this account (transaction wide)
    #[cfg(not(target_os = "solana"))]
    pub fn checked_add_lamports(&mut self, lamports: u64) -> Result<(), InstructionError> {
        self.set_lamports(
            self.get_lamports()
                .checked_add(lamports)
                .ok_or(InstructionError::ArithmeticOverflow)?,
        )
    }

    /// Subtracts lamports from this account (transaction wide)
    #[cfg(not(target_os = "solana"))]
    pub fn checked_sub_lamports(&mut self, lamports: u64) -> Result<(), InstructionError> {
        self.set_lamports(
            self.get_lamports()
                .checked_sub(lamports)
                .ok_or(InstructionError::ArithmeticOverflow)?,
        )
    }

    /// Returns a read-only slice of the account data (transaction wide)
    #[inline]
    pub fn get_data(&self) -> &[u8] {
        self.check_validity().expect("Account data accessed after logical invalidation (realloc in deeper frame)");
        self.account.data()
    }

    /// Returns a writable slice of the account data (transaction wide)
    #[cfg(not(target_os = "solana"))]
    pub fn get_data_mut(&mut self) -> Result<&mut [u8], InstructionError> {
        self.check_validity()?;
        self.can_data_be_changed()?;
        self.touch()?;
        self.make_data_mut();
        Ok(self.account.data_mut())
    }

    /// Returns the spare capacity of the vector backing the account data.
    ///
    /// This method should only ever be used during CPI, where after a shrinking
    /// realloc we want to zero the spare capacity.
    #[cfg(not(target_os = "solana"))]
    pub fn spare_data_capacity_mut(&mut self) -> Result<&mut [MaybeUninit<u8>], InstructionError> {
        Ok(&mut [])
    }

    /// Overwrites the account data and size (transaction wide).
    ///
    /// You should always prefer set_data_from_slice(). Calling this method is
    /// currently safe but requires some special casing during CPI when direct
    /// account mapping is enabled.
    #[cfg(all(
        not(target_os = "solana"),
        any(test, feature = "dev-context-only-utils")
    ))]
    pub fn set_data(&mut self, data: Vec<u8>) -> Result<(), InstructionError> {
        self.check_validity()?;
        self.can_data_be_resized(data.len())?;
        self.can_data_be_changed()?;
        self.touch()?;

        self.update_accounts_resize_delta(data.len())?;
        self.update_accounts_resize_delta(data.len())?;
        self.transaction_context.borrow_tracker.write().unwrap().increment_generation(self.index_in_transaction)?;
        self.account.set_data(data);
        Ok(())
    }

    /// Overwrites the account data and size (transaction wide).
    ///
    /// Call this when you have a slice of data you do not own and want to
    /// replace the account data with it.
    #[cfg(not(target_os = "solana"))]
    pub fn set_data_from_slice(&mut self, data: &[u8]) -> Result<(), InstructionError> {
        self.check_validity()?;
        self.can_data_be_resized(data.len())?;
        self.can_data_be_changed()?;
        self.touch()?;
        self.update_accounts_resize_delta(data.len())?;
        // Calling make_data_mut() here guarantees that set_data_from_slice()
        // copies in places, extending the account capacity if necessary but
        // never reducing it. This is required as the account migh be directly
        // mapped into a MemoryRegion, and therefore reducing capacity would
        // leave a hole in the vm address space. After CPI or upon program
        // termination, the runtime will zero the extra capacity.
        self.make_data_mut();
        self.transaction_context.borrow_tracker.write().unwrap().increment_generation(self.index_in_transaction)?;
        self.account.set_data_from_slice(data);

        Ok(())
    }

    /// Resizes the account data (transaction wide)
    ///
    /// Fills it with zeros at the end if is extended or truncates at the end otherwise.
    #[cfg(not(target_os = "solana"))]
    pub fn set_data_length(&mut self, new_length: usize) -> Result<(), InstructionError> {
        self.check_validity()?;
        self.can_data_be_resized(new_length)?;
        self.can_data_be_changed()?;
        // don't touch the account if the length does not change
        if self.get_data().len() == new_length {
            return Ok(());
        }
        self.touch()?;
        self.update_accounts_resize_delta(new_length)?;
        self.transaction_context.borrow_tracker.write().unwrap().increment_generation(self.index_in_transaction)?;
        self.account.resize(new_length, 0);
        Ok(())
    }

    /// Appends all elements in a slice to the account
    #[cfg(not(target_os = "solana"))]
    pub fn extend_from_slice(&mut self, data: &[u8]) -> Result<(), InstructionError> {
        let new_len = self.get_data().len().saturating_add(data.len());
        self.can_data_be_resized(new_len)?;
        self.can_data_be_changed()?;

        if data.is_empty() {
            return Ok(());
        }

        self.touch()?;
        self.update_accounts_resize_delta(new_len)?;
        // Even if extend_from_slice never reduces capacity, still realloc using
        // make_data_mut() if necessary so that we grow the account of the full
        // max realloc length in one go, avoiding smaller reallocations.
        self.make_data_mut();
        self.transaction_context.borrow_tracker.write().unwrap().increment_generation(self.index_in_transaction)?;
        self.account.extend_from_slice(data);
        Ok(())
    }

    /// Reserves capacity for at least additional more elements to be inserted
    /// in the given account. Does nothing if capacity is already sufficient.
    #[cfg(not(target_os = "solana"))]
    pub fn reserve(&mut self, additional: usize) -> Result<(), InstructionError> {
        // Note that we don't need to call can_data_be_changed() here nor
        // touch() the account. reserve() only changes the capacity of the
        // memory that holds the account but it doesn't actually change content
        // nor length of the account.
        self.make_data_mut();
        self.account.reserve(additional);

        Ok(())
    }

    /// Returns the number of bytes the account can hold without reallocating.
    #[cfg(not(target_os = "solana"))]
    pub fn capacity(&self) -> usize {
        self.account.capacity()
    }

    /// Returns whether the underlying AccountSharedData is shared.
    ///
    /// The data is shared if the account has been loaded from the accounts database and has never
    /// been written to. Writing to an account unshares it.
    ///
    /// During account serialization, if an account is shared it'll get mapped as CoW, else it'll
    /// get mapped directly as writable.
    #[cfg(not(target_os = "solana"))]
    pub fn is_shared(&self) -> bool {
        self.account.is_shared()
    }

    #[cfg(not(target_os = "solana"))]
    fn make_data_mut(&mut self) {
        // if the account is still shared, it means this is the first time we're
        // about to write into it. Make the account mutable by copying it in a
        // buffer with MAX_PERMITTED_DATA_INCREASE capacity so that if the
        // transaction reallocs, we don't have to copy the whole account data a
        // second time to fullfill the realloc.
        //
        // NOTE: The account memory region CoW code in bpf_loader::create_vm() implements the same
        // logic and must be kept in sync.
        if self.account.is_shared() {
            self.account.reserve(MAX_PERMITTED_DATA_INCREASE);
        }
    }

    /// Deserializes the account data into a state
    #[cfg(not(target_os = "solana"))]
    pub fn get_state<T: serde::de::DeserializeOwned>(&self) -> Result<T, InstructionError> {
        self.account
            .deserialize_data()
            .map_err(|_| InstructionError::InvalidAccountData)
    }

    /// Serializes a state into the account data
    #[cfg(not(target_os = "solana"))]
    pub fn set_state<T: serde::Serialize>(&mut self, state: &T) -> Result<(), InstructionError> {
        let data = self.get_data_mut()?;
        let serialized_size =
            bincode::serialized_size(state).map_err(|_| InstructionError::GenericError)?;
        if serialized_size > data.len() as u64 {
            return Err(InstructionError::AccountDataTooSmall);
        }
        bincode::serialize_into(&mut *data, state).map_err(|_| InstructionError::GenericError)?;
        Ok(())
    }

    // Returns whether or the lamports currently in the account is sufficient for rent exemption should the
    // data be resized to the given size
    #[cfg(not(target_os = "solana"))]
    pub fn is_rent_exempt_at_data_length(&self, data_length: usize) -> bool {
        self.transaction_context
            .rent
            .is_exempt(self.get_lamports(), data_length)
    }

    /// Returns whether this account is executable (transaction wide)
    #[inline]
    pub fn is_executable(&self) -> bool {
        self.account.executable()
    }

    /// Configures whether this account is executable (transaction wide)
    #[cfg(not(target_os = "solana"))]
    pub fn set_executable(&mut self, is_executable: bool) -> Result<(), InstructionError> {
        // To become executable an account must be rent exempt
        if !self
            .transaction_context
            .rent
            .is_exempt(self.get_lamports(), self.get_data().len())
        {
            return Err(InstructionError::ExecutableAccountNotRentExempt);
        }
        // Only the owner can set the executable flag
        if !self.is_owned_by_current_program() {
            return Err(InstructionError::ExecutableModified);
        }
        // and only if the account is writable
        if !self.is_writable() {
            return Err(InstructionError::ExecutableModified);
        }
        // one can not clear the executable flag
        if self.is_executable() && !is_executable {
            return Err(InstructionError::ExecutableModified);
        }
        // don't touch the account if the executable flag does not change
        if self.is_executable() == is_executable {
            return Ok(());
        }
        self.touch()?;
        self.account.set_executable(is_executable);
        Ok(())
    }

    /// Returns the rent epoch of this account (transaction wide)
    #[cfg(not(target_os = "solana"))]
    #[inline]
    pub fn get_rent_epoch(&self) -> u64 {
        self.account.rent_epoch()
    }

    /// Returns whether this account is a signer (instruction wide)
    pub fn is_signer(&self) -> bool {
        if self.index_in_instruction < self.instruction_context.get_number_of_program_accounts() {
            return false;
        }
        self.instruction_context
            .is_instruction_account_signer(
                self.index_in_instruction
                    .saturating_sub(self.instruction_context.get_number_of_program_accounts()),
            )
            .unwrap_or_default()
    }

    /// Returns whether this account is writable (instruction wide)
    pub fn is_writable(&self) -> bool {
        if self.index_in_instruction < self.instruction_context.get_number_of_program_accounts() {
            return false;
        }
        self.instruction_context
            .is_instruction_account_writable(
                self.index_in_instruction
                    .saturating_sub(self.instruction_context.get_number_of_program_accounts()),
            )
            .unwrap_or_default()
    }

    /// Returns true if the owner of this account is the current `InstructionContext`s last program (instruction wide)
    pub fn is_owned_by_current_program(&self) -> bool {
        self.instruction_context
            .get_last_program_key(self.transaction_context)
            .map(|key| key == self.get_owner())
            .unwrap_or_default()
    }

    /// Returns an error if the account data can not be mutated by the current program
    #[cfg(not(target_os = "solana"))]
    pub fn can_data_be_changed(&self) -> Result<(), InstructionError> {
        // Only non-executable accounts data can be changed
        if self.is_executable() {
            return Err(InstructionError::ExecutableDataModified);
        }
        // and only if the account is writable
        if !self.is_writable() {
            return Err(InstructionError::ReadonlyDataModified);
        }
        // and only if we are the owner
        if !self.is_owned_by_current_program() {
            return Err(InstructionError::ExternalAccountDataModified);
        }
        Ok(())
    }

    /// Returns an error if the account data can not be resized to the given length
    #[cfg(not(target_os = "solana"))]
    pub fn can_data_be_resized(&self, new_length: usize) -> Result<(), InstructionError> {
        let old_length = self.get_data().len();
        // Only the owner can change the length of the data
        if new_length != old_length && !self.is_owned_by_current_program() {
            return Err(InstructionError::AccountDataSizeChanged);
        }
        // The new length can not exceed the maximum permitted length
        if new_length > MAX_PERMITTED_DATA_LENGTH as usize {
            return Err(InstructionError::InvalidRealloc);
        }
        // The resize can not exceed the per-transaction maximum
        let length_delta = (new_length as i64).saturating_sub(old_length as i64);
        if self
            .transaction_context
            .accounts_resize_delta()?
            .saturating_add(length_delta)
            > MAX_PERMITTED_ACCOUNTS_DATA_ALLOCATIONS_PER_TRANSACTION
        {
            return Err(InstructionError::MaxAccountsDataAllocationsExceeded);
        }
        Ok(())
    }

    #[cfg(not(target_os = "solana"))]
    fn touch(&self) -> Result<(), InstructionError> {
        self.transaction_context
            .accounts()
            .touch(self.index_in_transaction)
    }

    #[cfg(not(target_os = "solana"))]
    fn update_accounts_resize_delta(&mut self, new_len: usize) -> Result<(), InstructionError> {
        let delta = (new_len as i64).saturating_sub(self.get_data().len() as i64);
        self.transaction_context
            .accounts_resize_delta
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                Some(current.saturating_add(delta))
            })
            .map_err(|_| InstructionError::GenericError)?;
        Ok(())
    }
}

/// Everything that needs to be recorded from a TransactionContext after execution
#[cfg(not(target_os = "solana"))]
pub struct ExecutionRecord {
    pub accounts: Vec<TransactionAccount>,
    pub return_data: TransactionReturnData,
    pub touched_account_count: u64,
    pub accounts_resize_delta: i64,
}

/// Used by the bank in the runtime to write back the processed accounts and recorded instructions
#[cfg(not(target_os = "solana"))]
impl From<TransactionContext> for ExecutionRecord {
    fn from(context: TransactionContext) -> Self {
        let accounts = Arc::try_unwrap(context.accounts)
            .unwrap_or_else(|accounts| (*accounts).clone());
        let touched_account_count = accounts.touched_count() as u64;
        let accounts = accounts.into_accounts();
        Self {
            accounts: Vec::from(Pin::into_inner(context.account_keys))
                .into_iter()
                .zip(accounts)
                .collect(),
            return_data: context.return_data,
            touched_account_count,
            accounts_resize_delta: context.accounts_resize_delta.load(Ordering::Relaxed),
        }
    }
}

#[cfg(not(target_os = "solana"))]
fn is_zeroed(buf: &[u8]) -> bool {
    const ZEROS_LEN: usize = 1024;
    const ZEROS: [u8; ZEROS_LEN] = [0; ZEROS_LEN];
    let mut chunks = buf.chunks_exact(ZEROS_LEN);

    #[allow(clippy::indexing_slicing)]
    {
        chunks.all(|chunk| chunk == &ZEROS[..])
            && chunks.remainder() == &ZEROS[..chunks.remainder().len()]
    }
}
