use crate::{
    clock::Clock,
    epoch_schedule::EpochSchedule,
    rent::Rent,
    sysvar::slot_hashes::SlotHashes,
};
use std::sync::Arc;

/// WaveContext is the new "lightweight Bank" in TRVM.
/// It carries all invariants for a single Wave of execution.
#[derive(Debug, Clone)]
pub struct WaveContext {
    /// Unique identifier for this Wave.
    pub wave_id: Arc<str>,
    /// Logical time or execution slot in the TRIAD DAG pipeline.
    pub execution_slot: u64,
    /// Maximum compute units (CU) allowed for entire wave.
    pub cu_limit: u64,
    
    // Sysvars cached for fast access within the wave
    pub rent: Rent,
    pub clock: Clock,
    pub epoch_schedule: EpochSchedule,
    pub slot_hashes: SlotHashes,
}

impl WaveContext {
    /// Create a new WaveContext for a specific slot.
    pub fn new(wave_id: &str, slot: u64) -> Self {
        Self {
            wave_id: Arc::from(wave_id),
            execution_slot: slot,
            cu_limit: 100_000_000, // Default wave limit
            rent: Rent::default(),
            clock: Clock::default(),
            epoch_schedule: EpochSchedule::without_warmup(),
            slot_hashes: SlotHashes::default(),
        }
    }

    /// Update clock timestamp for the wave.
    pub fn with_clock(mut self, clock: Clock) -> Self {
        self.clock = clock;
        self
    }

    /// Update rent sysvar.
    pub fn with_rent(mut self, rent: Rent) -> Self {
        self.rent = rent;
        self
    }

    /// Set compute budget for the wave.
    pub fn with_cu_limit(mut self, limit: u64) -> Self {
        self.cu_limit = limit;
        self
    }
}
