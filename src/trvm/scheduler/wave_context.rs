use std::sync::Arc;

use crate::sdk::{
    feature_set::FeatureSet,
    hash::Hash,
    transaction_context::{IndexOfAccount, TransactionAccount},
};
use solana_sdk::{
    clock::Clock,
    epoch_schedule::EpochSchedule,
    message::SanitizedMessage,
    rent::Rent,
    sysvar::slot_hashes::SlotHashes,
};

#[derive(Debug, Clone)]
pub struct WaveTransactionContext {
    pub tx_id: u128,
    pub accounts: Vec<TransactionAccount>,
    pub message: SanitizedMessage,
    pub program_indices: Vec<IndexOfAccount>,
}

/// Immutable wave-scoped snapshot passed into the execution kernel.
/// Contains sysvars, limits, preloaded accounts, and message metadata.
#[derive(Debug, Clone)]
pub struct WaveContext {
    pub wave_id: Arc<str>,
    pub clock: Clock,
    pub rent: Rent,
    pub epoch_schedule: EpochSchedule,
    pub slot_hashes: SlotHashes,
    pub blockhash: Hash,
    pub feature_set: Arc<FeatureSet>,
    pub cu_limit: u64,
    pub lamports_per_signature: u64,
    pub transactions: Vec<WaveTransactionContext>,
}

impl WaveContext {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        wave_id: Arc<str>,
        clock: Clock,
        rent: Rent,
        epoch_schedule: EpochSchedule,
        slot_hashes: SlotHashes,
        blockhash: Hash,
        feature_set: Arc<FeatureSet>,
        cu_limit: u64,
        lamports_per_signature: u64,
        transactions: Vec<WaveTransactionContext>,
    ) -> Self {
        Self {
            wave_id,
            clock,
            rent,
            epoch_schedule,
            slot_hashes,
            blockhash,
            feature_set,
            cu_limit,
            lamports_per_signature,
            transactions,
        }
    }
}
