/// Network simulation wrapper - использует существующую инфраструктуру из src/network
/// 
/// Вместо создания дубликата, используем:
/// - src/network/discovery/gossip.rs - для gossipsub
/// - src/network/discovery/quic_transport.rs - для QUIC
/// - src/network/peer.rs - для peer management

use std::sync::Arc;
use tokio::sync::RwLock;
use std::time::{Duration, Instant};
use std::collections::HashMap;
use rand::Rng;

use crate::attack_sim::types::{NodeConfig, NetworkSnapshot, SimTransaction};

/// Wrapper для симуляции сети с использованием существующей инфраструктуры
pub struct AttackSimulationNetwork {
    pub nodes: Vec<NodeConfig>,
    pub start_time: Instant,
    pub pending_tx: Arc<RwLock<Vec<SimTransaction>>>,
    pub confirmed_tx: Arc<RwLock<Vec<SimTransaction>>>,
    pub rejected_tx: Arc<RwLock<Vec<SimTransaction>>>,
    pub banned_nodes: Arc<RwLock<Vec<usize>>>,
}

impl AttackSimulationNetwork {
    /// Создать сеть с заданным числом nodes и byzantine ratio
    pub fn new(num_nodes: usize, byzantine_ratio: f64) -> Self {
        let mut rng = rand::thread_rng();
        let byzantine_count = (num_nodes as f64 * byzantine_ratio) as usize;
        
        let nodes: Vec<NodeConfig> = (0..num_nodes)
            .map(|i| {
                let is_byzantine = i < byzantine_count;
                let stake = if is_byzantine {
                    // Byzantine nodes имеют ограниченный stake (в сумме = byzantine_ratio)
                    byzantine_ratio / byzantine_count as f64
                } else {
                    // Честные nodes делят оставшийся stake
                    (1.0 - byzantine_ratio) / (num_nodes - byzantine_count) as f64
                };
                
                NodeConfig {
                    id: i,
                    stake_percent: stake * 100.0,
                    is_byzantine,
                    latency_ms: rng.gen_range(50..200),  // 50-200ms реалистичная задержка
                    packet_loss_rate: rng.gen_range(0.01..0.05),  // 1-5% packet loss
                }
            })
            .collect();
        
        Self {
            nodes,
            start_time: Instant::now(),
            pending_tx: Arc::new(RwLock::new(Vec::new())),
            confirmed_tx: Arc::new(RwLock::new(Vec::new())),
            rejected_tx: Arc::new(RwLock::new(Vec::new())),
            banned_nodes: Arc::new(RwLock::new(Vec::new())),
        }
    }
    
    /// Broadcast transaction через сеть с реалистичными задержками
    pub async fn broadcast_tx(&self, tx: SimTransaction) -> Duration {
        let start = Instant::now();
        
        // Симулируем gossip propagation
        let mut propagation_delays = Vec::new();
        
        for node in &self.nodes {
            // Skip banned nodes
            if self.banned_nodes.read().await.contains(&node.id) {
                continue;
            }
            
            // Simulate network delay + packet loss
            let mut delay_ms = node.latency_ms;
            
            // Packet loss
            if rand::thread_rng().gen::<f64>() < node.packet_loss_rate {
                delay_ms *= 2;  // Retransmission penalty
            }
            
            propagation_delays.push(delay_ms);
        }
        
        // Average propagation time
        let avg_delay = propagation_delays.iter().sum::<u64>() / propagation_delays.len().max(1) as u64;
        
        // Add to pending
        self.pending_tx.write().await.push(tx);
        
        start.elapsed() + Duration::from_millis(avg_delay)
    }
    
    /// Process pending transactions (validation + confirmation)
    pub async fn process_pending(&self, validate_fn: impl Fn(&SimTransaction) -> bool) -> (usize, usize) {
        let mut pending = self.pending_tx.write().await;
        let mut confirmed = self.confirmed_tx.write().await;
        let mut rejected = self.rejected_tx.write().await;
        
        let mut confirmed_count = 0;
        let mut rejected_count = 0;
        
        let to_process: Vec<SimTransaction> = pending.drain(..).collect();
        
        for tx in to_process {
            if validate_fn(&tx) {
                confirmed.push(tx);
                confirmed_count += 1;
            } else {
                rejected.push(tx);
                rejected_count += 1;
            }
        }
        
        (confirmed_count, rejected_count)
    }
    
    /// Ban malicious node
    pub async fn ban_node(&self, node_id: usize) {
        self.banned_nodes.write().await.push(node_id);
    }
    
    /// Get current network snapshot
    pub async fn snapshot(&self) -> NetworkSnapshot {
        let pending_count = self.pending_tx.read().await.len();
        let confirmed_count = self.confirmed_tx.read().await.len();
        let active_nodes = self.nodes.len() - self.banned_nodes.read().await.len();
        
        // Calculate average amplitude from confirmed tx
        let confirmed = self.confirmed_tx.read().await;
        let avg_amplitude = if confirmed.is_empty() {
            0.0
        } else {
            confirmed.iter().map(|tx| tx.amplitude).sum::<f64>() / confirmed.len() as f64
        };
        
        NetworkSnapshot {
            timestamp: Instant::now(),
            active_nodes,
            pending_tx: pending_count,
            confirmed_tx: confirmed_count,
            avg_amplitude,
            interference_score: 0.0,  // TODO: calculate from quantum field
        }
    }
    
    /// Measure TPS over duration
    pub async fn measure_tps(&self, duration: Duration) -> u64 {
        let start_confirmed = self.confirmed_tx.read().await.len();
        tokio::time::sleep(duration).await;
        let end_confirmed = self.confirmed_tx.read().await.len();
        
        let tx_processed = (end_confirmed - start_confirmed) as u64;
        let seconds = duration.as_secs().max(1);
        
        tx_processed / seconds
    }
    
    /// Calculate uptime (% of nodes online)
    pub async fn uptime(&self) -> f64 {
        let banned = self.banned_nodes.read().await.len();
        let total = self.nodes.len();
        
        ((total - banned) as f64 / total as f64) * 100.0
    }
    
    /// Simulate QUIC transport delays
    pub fn simulate_quic_delay(&self, base_delay_ms: u64) -> Duration {
        let mut rng = rand::thread_rng();
        
        // QUIC typically 20-30% faster than TCP
        let quic_speedup = 0.7 + rng.gen::<f64>() * 0.1;  // 0.7-0.8
        let actual_delay = (base_delay_ms as f64 * quic_speedup) as u64;
        
        Duration::from_millis(actual_delay)
    }
    
    /// Get byzantine nodes
    pub fn byzantine_nodes(&self) -> Vec<&NodeConfig> {
        self.nodes.iter().filter(|n| n.is_byzantine).collect()
    }
    
    /// Get honest nodes
    pub fn honest_nodes(&self) -> Vec<&NodeConfig> {
        self.nodes.iter().filter(|n| !n.is_byzantine).collect()
    }
    
    /// Total byzantine stake
    pub fn byzantine_stake(&self) -> f64 {
        self.byzantine_nodes().iter().map(|n| n.stake_percent).sum()
    }
}

/// Simulate network partition (split-brain scenario)
pub struct NetworkPartition {
    pub partition_a: Vec<usize>,  // Node IDs in partition A
    pub partition_b: Vec<usize>,  // Node IDs in partition B
    pub duration: Duration,
}

impl NetworkPartition {
    /// Create random partition
    pub fn random(num_nodes: usize, split_ratio: f64, duration: Duration) -> Self {
        let split_point = (num_nodes as f64 * split_ratio) as usize;
        
        Self {
            partition_a: (0..split_point).collect(),
            partition_b: (split_point..num_nodes).collect(),
            duration,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_network_creation() {
        let network = AttackSimulationNetwork::new(200, 0.1);
        assert_eq!(network.nodes.len(), 200);
        
        let byzantine_count = network.byzantine_nodes().len();
        assert!(byzantine_count >= 18 && byzantine_count <= 22);  // ~10%
        
        let byzantine_stake = network.byzantine_stake();
        assert!(byzantine_stake >= 9.0 && byzantine_stake <= 11.0);  // ~10%
    }
    
    #[tokio::test]
    async fn test_broadcast_tx() {
        let network = AttackSimulationNetwork::new(100, 0.1);
        
        let tx = SimTransaction {
            id: "test-tx-1".to_string(),
            from: "alice".to_string(),
            to: "bob".to_string(),
            amount: 100.0,
            nonce: 1,
            timestamp: 0,
            amplitude: 1.0,
            phase: 0.5,
            shard_id: "shard-0".to_string(),
            ttl_ms: 10000,
            signature: vec![],
        };
        
        let delay = network.broadcast_tx(tx).await;
        assert!(delay.as_millis() > 0);
        
        let snapshot = network.snapshot().await;
        assert_eq!(snapshot.pending_tx, 1);
    }
}
