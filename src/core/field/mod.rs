pub mod sharded;
pub mod storage;
pub mod legacy;

pub use sharded::ShardedCoreField;
pub use storage::{SpaceTimePoint, StateVector, InterferencePoint, WaveStatistics, WaveHealth, WaveHealthStatus, WaveValidationResult, WaveFate};
pub use legacy::CoreField;

use std::sync::Arc;
use blake3::Hasher;
use std::time::{Instant, Duration};

/// Speed of light in the simulation (normalized). 
pub const C: f64 = 1.0;

#[repr(C)]
#[derive(Debug, Clone)]
pub struct CoreWave {
    pub id: Arc<str>,
    
    // Minkowski 4D Wave fields (Performance optimized: f32)
    pub amplitude: f32,       // α · log₁₀(1 + amount)
    pub phase: f32,           // Blake3 derived [0, 2π)
    pub position: [f32; 3],   // 3-torus position
    pub timestamp: u32,       // Slot number
    
    pub shard_id: u8,
    pub shard_index: usize, // Cached for routing
    
    // Legacy fields (Uncommented for compatibility with existing logic)
    // pub frequency: f64, 
    // pub wave_vector: [f64; 3],
    // pub origin: SpaceTimePoint,
    pub created_at: Instant,
    pub lifetime: Duration,
    // pub superposition: Vec<StateVector>,
    
    pub signature: Vec<u8>, // Placeholder for Schnorr
}

impl CoreWave {


    /// Create a new CoreWave (compatibility constructor)
    pub fn new(
        id: String,
        amplitude: f64,
        phase: f64,
        shard_id: u8,
        _superposition: Vec<crate::core::field::StateVector>,
    ) -> Self {
        Self {
            id: Arc::from(id),
            amplitude: amplitude as f32,
            phase: phase as f32,
            position: [0.0, 0.0, 0.0], // Default position
            timestamp: 0, // Default timestamp
            shard_id,
            shard_index: 0,
            signature: Vec::new(),
            created_at: Instant::now(),
            lifetime: Duration::from_secs(60),
        }
    }

    /// Create wave from transaction parameters (Minkowski spec)
    #[inline]
    pub fn from_transaction(
        id: &str,
        amount: u64,
        block_salt: &[u8; 32],
        tx_data: &[u8],
        slot: u32,
        spatial_scale: f32,
        shard_id: u8,
    ) -> Self {
        const ALPHA: f32 = 1.0;
        
        // Amplitude: α · log₁₀(1 + amount)
        let amplitude = ALPHA * (1.0 + amount as f32).log10();
        
        // Hash generation
        let mut hasher = Hasher::new();
        hasher.update(block_salt);
        hasher.update(tx_data);
        let hash = *hasher.finalize().as_bytes();
        
        // Phase: 2π · frac(hash[0..8])
        let val_phase = u64::from_le_bytes(hash[0..8].try_into().unwrap());
        let phase = (val_phase as f64 / u64::MAX as f64) as f32 * 2.0 * std::f32::consts::PI as f32;
        
        // Position: map hash[8..32] to [0, L)³
        let x_bytes = u64::from_le_bytes(hash[8..16].try_into().unwrap());
        let y_bytes = u64::from_le_bytes(hash[16..24].try_into().unwrap());
        let z_bytes = u64::from_le_bytes(hash[24..32].try_into().unwrap());
        
        let position = [
            ((x_bytes as f64 / u64::MAX as f64) as f32) * spatial_scale,
            ((y_bytes as f64 / u64::MAX as f64) as f32) * spatial_scale,
            ((z_bytes as f64 / u64::MAX as f64) as f32) * spatial_scale,
        ];

        Self {
            id: Arc::from(id),
            amplitude,
            phase,
            position,
            timestamp: slot,
            shard_id,
            shard_index: 0, // Should be set by sharding logic
            signature: Vec::new(),
            created_at: Instant::now(),
            lifetime: Duration::from_secs(60), // Default lifetime
        }
    }

    /// Check causal connection (Minkowski interval Δs² ≤ 0)
    #[inline]
    pub fn is_causal(&self, observer_pos: [f32; 3], observer_time: u32, c: f32) -> bool {
        let dt = (observer_time as i64 - self.timestamp as i64) as f32;
        if dt <= 0.0 { return false; } // Only past affects future
        
        let dx = observer_pos[0] - self.position[0];
        let dy = observer_pos[1] - self.position[1];
        let dz = observer_pos[2] - self.position[2];
        
        let dr_sq = dx*dx + dy*dy + dz*dz;
        let c_dt_sq = c*c * dt*dt;
        
        dr_sq <= c_dt_sq
    }
    
    #[inline]
    pub fn energy(&self) -> f32 {
        self.amplitude * self.amplitude
    }
}


