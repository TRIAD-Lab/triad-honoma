use core_affinity::{CoreId, get_core_ids, set_for_current};
use log::{info, warn};

/// Adaptive CPU core allocation strategy
#[derive(Debug, Clone)]
pub struct CoreAllocation {
    /// Cores dedicated to interference engine (Core computations)
    pub interference_cores: Vec<CoreId>,
    /// Cores dedicated to QUIC network stack
    pub network_cores: Vec<CoreId>,
    /// Cores dedicated to consensus
    pub consensus_cores: Vec<CoreId>,
    /// Total available cores
    pub total_cores: usize,
}

impl CoreAllocation {
    /// Auto-detect and allocate cores based on available CPU count
    pub fn auto_allocate() -> Self {
        let core_ids = get_core_ids().unwrap_or_default();
        let total = core_ids.len();
        
        info!("Detected {} CPU cores, allocating adaptively...", total);
        
        match total {
            0..=1 => {
                // Single core: everything shares
                warn!("Only {} core(s) available, no isolation possible", total);
                Self {
                    interference_cores: core_ids.clone(),
                    network_cores: core_ids.clone(),
                    consensus_cores: core_ids,
                    total_cores: total,
                }
            }
            2 => {
                // 2 cores: 1 for interference + consensus, 1 for network
                info!("2-core config: [0] Interference+Consensus, [1] Network");
                Self {
                    interference_cores: vec![core_ids[0]],
                    network_cores: vec![core_ids[1]],
                    consensus_cores: vec![core_ids[0]],
                    total_cores: 2,
                }
            }
            3 => {
                // 3 cores: 2 for interference, 1 for network
                info!("3-core config: [0,1] Interference, [2] Network");
                Self {
                    interference_cores: vec![core_ids[0], core_ids[1]],
                    network_cores: vec![core_ids[2]],
                    consensus_cores: vec![core_ids[0]],
                    total_cores: 3,
                }
            }
            4 => {
                // 4 cores: 3 for interference, 1 for network (as user requested)
                info!("4-core config: [0,1,2] Interference, [3] Network");
                Self {
                    interference_cores: vec![core_ids[0], core_ids[1], core_ids[2]],
                    network_cores: vec![core_ids[3]],
                    consensus_cores: vec![core_ids[0]],
                    total_cores: 4,
                }
            }
            5..=7 => {
                // 5-7 cores: 2/3 for interference, 1/3 for network, 1 for consensus
                let network_count = total / 3;
                let consensus_count = 1;
                let interference_count = total - network_count - consensus_count;
                
                let mut interference = Vec::new();
                let mut network = Vec::new();
                let consensus = vec![core_ids[0]]; // Consensus on core 0
                
                for (i, core) in core_ids.iter().enumerate().skip(1) {
                    if interference.len() < interference_count {
                        interference.push(*core);
                    } else {
                        network.push(*core);
                    }
                }
                
                info!("{}-core config: Interference={}, Network={}, Consensus=1", 
                      total, interference_count, network_count);
                
                Self {
                    interference_cores: interference,
                    network_cores: network,
                    consensus_cores: consensus,
                    total_cores: total,
                }
            }
            8..=15 => {
                // 8-15 cores: 50% interference, 25% network, 25% consensus
                let network_count = total / 4;
                let consensus_count = total / 4;
                let interference_count = total - network_count - consensus_count;
                
                let mut interference = Vec::new();
                let mut network = Vec::new();
                let mut consensus = Vec::new();
                
                for (i, core) in core_ids.iter().enumerate() {
                    if interference.len() < interference_count {
                        interference.push(*core);
                    } else if network.len() < network_count {
                        network.push(*core);
                    } else {
                        consensus.push(*core);
                    }
                }
                
                info!("{}-core config: Interference={}, Network={}, Consensus={}", 
                      total, interference_count, network_count, consensus_count);
                
                Self {
                    interference_cores: interference,
                    network_cores: network,
                    consensus_cores: consensus,
                    total_cores: total,
                }
            }
            _ => {
                // 16+ cores: 60% interference, 20% network, 20% consensus
                let network_count = total / 5;
                let consensus_count = total / 5;
                let interference_count = total - network_count - consensus_count;
                
                let mut interference = Vec::new();
                let mut network = Vec::new();
                let mut consensus = Vec::new();
                
                for (i, core) in core_ids.iter().enumerate() {
                    if interference.len() < interference_count {
                        interference.push(*core);
                    } else if network.len() < network_count {
                        network.push(*core);
                    } else {
                        consensus.push(*core);
                    }
                }
                
                info!("{}-core config: Interference={}, Network={}, Consensus={}", 
                      total, interference_count, network_count, consensus_count);
                
                Self {
                    interference_cores: interference,
                    network_cores: network,
                    consensus_cores: consensus,
                    total_cores: total,
                }
            }
        }
    }
    
    /// Pin current thread to interference cores
    pub fn pin_to_interference(&self) -> Result<(), String> {
        if let Some(&core) = self.interference_cores.first() {
            if set_for_current(core) {
                info!("Thread pinned to interference core {:?}", core);
                Ok(())
            } else {
                Err("Failed to pin thread to interference core".to_string())
            }
        } else {
            Err("No interference cores available".to_string())
        }
    }
    
    /// Pin current thread to network cores
    pub fn pin_to_network(&self) -> Result<(), String> {
        if let Some(&core) = self.network_cores.first() {
            if set_for_current(core) {
                info!("Thread pinned to network core {:?}", core);
                Ok(())
            } else {
                Err("Failed to pin thread to network core".to_string())
            }
        } else {
            Err("No network cores available".to_string())
        }
    }
    
    /// Pin current thread to consensus cores
    pub fn pin_to_consensus(&self) -> Result<(), String> {
        if let Some(&core) = self.consensus_cores.first() {
            if set_for_current(core) {
                info!("Thread pinned to consensus core {:?}", core);
                Ok(())
            } else {
                Err("Failed to pin thread to consensus core".to_string())
            }
        } else {
            Err("No consensus cores available".to_string())
        }
    }
    
    /// Pin current thread to specific core index within interference pool
    pub fn pin_to_interference_core(&self, index: usize) -> Result<(), String> {
        if let Some(&core) = self.interference_cores.get(index) {
            if set_for_current(core) {
                info!("Thread pinned to interference core {} ({:?})", index, core);
                Ok(())
            } else {
                Err(format!("Failed to pin to interference core {}", index))
            }
        } else {
            Err(format!("Interference core {} not available", index))
        }
    }
    
    /// Get allocation strategy summary
    pub fn summary(&self) -> String {
        format!(
            "CPU Allocation: {} total cores | Interference: {} | Network: {} | Consensus: {}",
            self.total_cores,
            self.interference_cores.len(),
            self.network_cores.len(),
            self.consensus_cores.len()
        )
    }
}

/// Global core allocation instance
static CORE_ALLOCATION: std::sync::OnceLock<CoreAllocation> = std::sync::OnceLock::new();

/// Initialize global core allocation
pub fn init_core_allocation() -> &'static CoreAllocation {
    CORE_ALLOCATION.get_or_init(|| {
        let allocation = CoreAllocation::auto_allocate();
        info!("{}", allocation.summary());
        allocation
    })
}

/// Get global core allocation
pub fn get_core_allocation() -> &'static CoreAllocation {
    CORE_ALLOCATION.get().expect("Core allocation not initialized. Call init_core_allocation() first.")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_core_allocation() {
        let allocation = CoreAllocation::auto_allocate();
        println!("{}", allocation.summary());
        assert!(allocation.total_cores > 0);
    }
}



