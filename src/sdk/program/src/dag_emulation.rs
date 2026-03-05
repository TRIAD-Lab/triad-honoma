//! Blockless SVM Facade: Emulating Solana's block-centric sysvars using
//! TRIAD's Directed Acyclic Graph (DAG) Frontier and Logical Time.

use crate::{
    clock::{Clock, Slot, UnixTimestamp},
    hash::Hash,
    hash_provider::get_hash_provider,
    pubkey::Pubkey,
    sysvar::recent_blockhashes::{Entry, RecentBlockhashes},
};

/// The Frontier Hash is a deterministic identifier of the DAG's current state (the tips).
/// It serves as a facade for the Solana "Blockhash".
pub fn derive_frontier_hash(
    tip_ids: &[Hash],
    logical_time: u64,
    epoch_nonce: Hash,
) -> Hash {
    let mut sorted_tips = tip_ids.to_vec();
    sorted_tips.sort();

    let mut hasher = get_hash_provider();
    let mut data = Vec::new();
    for tip in &sorted_tips {
        data.push(tip.as_ref());
    }
    let time_bytes = logical_time.to_le_bytes();
    data.push(&time_bytes);
    data.push(epoch_nonce.as_ref());

    hasher.hashv(&data)
}

/// Maps TRIAD logical time/slot to Solana's `Clock`.
pub fn emulate_clock(
    logical_slot: Slot,
    logical_timestamp: UnixTimestamp,
    epoch: u64,
) -> Clock {
    // Solana's epoch_start_timestamp is usually constant per epoch.
    // For now, we use a simplistic model where it's 0 or anchored to the epoch.
    Clock {
        slot: logical_slot,
        epoch_start_timestamp: (epoch.saturating_mul(432000).saturating_mul(400) / 1000) as UnixTimestamp, 
        epoch,
        leader_schedule_epoch: epoch.saturating_add(1),
        unix_timestamp: logical_timestamp,
    }
}

/// Representation of a DAG Frontier state for emulating RecentBlockhashes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrontierEntry {
    pub hash: Hash,
    pub logical_slot: Slot,
}

/// Emulates `RecentBlockhashes` using a list of Frontier entries.
pub fn emulate_recent_blockhashes(
    entries: &[FrontierEntry],
) -> RecentBlockhashes {
    let mut recent_entries = Vec::with_capacity(entries.len());
    for entry in entries {
        // Fee is 0 for now as per DAG model. 
        // We map logical_slot to 'last_valid_block_height' for compatibility.
        recent_entries.push(Entry::new(&entry.hash, 0)); 
    }
    RecentBlockhashes(recent_entries)
}
