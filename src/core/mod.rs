pub mod field;
pub mod interference;
pub mod prob_ops;
pub mod consensus;
pub mod stark_prover; 
pub mod static_storage; 

#[cfg(feature = "opencl")]
pub mod opencl;

#[cfg(test)]
pub mod tests;

pub use field::{CoreField, StateVector, InterferencePoint, CoreWave};
pub use interference::{InterferenceEngine, InterferenceAnalysis};

use std::sync::Arc;
use tokio::sync::RwLock;

pub struct CoreSystem {
    field: Arc<RwLock<CoreField>>,
    interference_engine: Arc<InterferenceEngine>,
}

impl CoreSystem {
    pub fn new() -> Self {
        Self {
            field: Arc::new(RwLock::new(CoreField::new())),
            interference_engine: Arc::new(InterferenceEngine::new(1000, 0.95)),
        }
    }

    pub async fn add_state(&self, key: String, wave: CoreWave) {
        let mut field = self.field.write().await;
        field.add_wave(key, wave);
    }

    pub async fn measure(&self, key: &str) -> Option<StateVector> {
        let field = self.field.read().await;
        field.measure(key)
    }

    pub async fn get_interference_pattern(&self) -> Vec<InterferencePoint> {
        let field = self.field.read().await;
        // OPTIMIZATION: Collect pairs directly and use inplace calculation
        // This avoids cloning full CoreWave objects and fixes the iterator mismatch
        let pairs: Vec<(f64, f64)> = field.active_waves.values()
            .filter_map(|&idx| crate::core::static_storage::StaticWaveStorage::get(idx))
            .map(|w| (w.amplitude as f64, w.phase as f64))
            .collect();
        
        // Use optimized inplace calculation
        // Note: We need a mutable reference to the engine, but here we only have &self.
        // We lock the engine to get mutable access.
        let engine = &self.interference_engine;
        engine.calculate_interference_from_pairs_inplace(&pairs)
    }

    pub async fn analyze_interference(&self) -> InterferenceAnalysis {
        let pattern = self.get_interference_pattern().await;
        let engine = &self.interference_engine;
        engine.analyze_interference(&pattern)
    }
}


