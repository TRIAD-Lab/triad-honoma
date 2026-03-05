/// Реалистичная симуляция P2P сети для стресс-тестирования атак
/// 
/// Features:
/// - 50-200 nodes (Rayon threads)
/// - Network delays 10-100ms (tokio timers)
/// - Packet loss 1-5% (random drop)
/// - Gossipsub broadcast simulation
/// - Topology via petgraph DAG

use std::time::{Duration, Instant};
use std::collections::HashMap;
use tokio::time::sleep;
use rand::Rng;
use rayon::prelude::*;

#[derive(Debug, Clone)]
pub struct NetworkNode {
    pub id: usize,
    pub is_malicious: bool,
    pub reputation: f64, // 0.0-1.0
    pub connected_peers: Vec<usize>,
}

#[derive(Debug, Clone)]
pub struct NetworkConfig {
    pub num_nodes: usize,
    pub malicious_ratio: f64,
    pub base_delay_ms: u64,
    pub delay_variance_ms: u64,
    pub packet_loss_rate: f64,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            num_nodes: 50,
            malicious_ratio: 0.1,
            base_delay_ms: 30,
            delay_variance_ms: 50,
            packet_loss_rate: 0.02,
        }
    }
}

pub struct NetworkSimulator {
    pub nodes: Vec<NetworkNode>,
    pub config: NetworkConfig,
    topology: Vec<Vec<usize>>, // adjacency list
}

impl NetworkSimulator {
    pub fn new(config: NetworkConfig) -> Self {
        let mut rng = rand::thread_rng();
        let num_malicious = (config.num_nodes as f64 * config.malicious_ratio) as usize;
        
        let nodes: Vec<NetworkNode> = (0..config.num_nodes)
            .map(|id| {
                let is_malicious = id < num_malicious;
                NetworkNode {
                    id,
                    is_malicious,
                    reputation: if is_malicious { 0.5 } else { 1.0 },
                    connected_peers: Vec::new(),
                }
            })
            .collect();
        
        // Create random topology (каждая нода связана с 5-10 соседями)
        let mut topology = vec![Vec::new(); config.num_nodes];
        for i in 0..config.num_nodes {
            let num_peers = rng.gen_range(5..10);
            for _ in 0..num_peers {
                let peer_id = rng.gen_range(0..config.num_nodes);
                if peer_id != i && !topology[i].contains(&peer_id) {
                    topology[i].push(peer_id);
                    topology[peer_id].push(i); // bidirectional
                }
            }
        }
        
        Self { nodes, config, topology }
    }
    
    /// Симулирует broadcast транзакции через gossip протокол
    pub async fn broadcast_transaction(&self, from_node: usize, tx_data: &[u8]) -> BroadcastResult {
        let start = Instant::now();
        let mut rng = rand::thread_rng();
        
        let mut delivered = Vec::new();
        let mut dropped = Vec::new();
        
        // Gossip к соседям
        for &peer_id in &self.topology[from_node] {
            // Packet loss
            if rng.r#gen::<f64>() < self.config.packet_loss_rate {
                dropped.push(peer_id);
                continue;
            }
            
            // Network delay
            let delay = Duration::from_millis(
                self.config.base_delay_ms + rng.gen_range(0..self.config.delay_variance_ms)
            );
            sleep(delay).await;
            
            delivered.push(peer_id);
        }
        
        BroadcastResult {
            delivered_count: delivered.len(),
            dropped_count: dropped.len(),
            latency: start.elapsed(),
            reached_nodes: delivered,
        }
    }
    
    /// Параллельная верификация на всех нодах
    pub fn parallel_verify(&self, tx_data: &[u8], is_valid: bool) -> VerificationResult {
        let start = Instant::now();
        
        // Simulate Ristretto verification cost: ~40k CPU cycles (faster than Ed25519)
        let verification_cycles = 40_000_u64;
        
        let results: Vec<(usize, bool, Duration)> = self.nodes.par_iter()
            .map(|node| {
                let node_start = Instant::now();
                
                // Simulate verification work
                std::thread::sleep(Duration::from_nanos(verification_cycles / 1000));
                
                let verified = is_valid && node.reputation > 0.8;
                (node.id, verified, node_start.elapsed())
            })
            .collect();
        
        let valid_count = results.iter().filter(|(_, v, _)| *v).count();
        let avg_latency = results.iter().map(|(_, _, d)| d.as_micros()).sum::<u128>() / results.len() as u128;
        
        VerificationResult {
            total_nodes: self.config.num_nodes,
            valid_count,
            invalid_count: self.config.num_nodes - valid_count,
            avg_latency_us: avg_latency as u64,
            total_time: start.elapsed(),
            total_cycles: verification_cycles * self.config.num_nodes as u64,
        }
    }
    
    /// Update node reputation based on behavior
    pub fn update_reputation(&mut self, node_id: usize, delta: f64) {
        if let Some(node) = self.nodes.get_mut(node_id) {
            node.reputation = (node.reputation + delta).clamp(0.0, 1.0);
        }
    }
    
    /// Ban nodes with low reputation
    pub fn ban_low_reputation_nodes(&mut self, threshold: f64) -> Vec<usize> {
        let mut banned = Vec::new();
        for node in &mut self.nodes {
            if node.reputation < threshold {
                node.reputation = 0.0;
                banned.push(node.id);
            }
        }
        banned
    }
}

#[derive(Debug)]
pub struct BroadcastResult {
    pub delivered_count: usize,
    pub dropped_count: usize,
    pub latency: Duration,
    pub reached_nodes: Vec<usize>,
}

#[derive(Debug)]
pub struct VerificationResult {
    pub total_nodes: usize,
    pub valid_count: usize,
    pub invalid_count: usize,
    pub avg_latency_us: u64,
    pub total_time: Duration,
    pub total_cycles: u64,
}

/// Метрики производительности сети
#[derive(Debug, Default)]
pub struct NetworkMetrics {
    pub total_transactions: usize,
    pub successful_broadcasts: usize,
    pub failed_broadcasts: usize,
    pub avg_latency_ms: f64,
    pub tps: f64,
    pub energy_cycles: u64,
    pub uptime_percent: f64,
}

impl NetworkMetrics {
    pub fn calculate(
        total_tx: usize,
        successful: usize,
        total_time: Duration,
        total_cycles: u64,
    ) -> Self {
        let tps = total_tx as f64 / total_time.as_secs_f64();
        let uptime = (successful as f64 / total_tx as f64) * 100.0;
        
        Self {
            total_transactions: total_tx,
            successful_broadcasts: successful,
            failed_broadcasts: total_tx - successful,
            avg_latency_ms: total_time.as_secs_f64() * 1000.0 / total_tx as f64,
            tps,
            energy_cycles: total_cycles,
            uptime_percent: uptime,
        }
    }
    
    pub fn print(&self) {
        println!("\n📊 Network Performance Metrics:");
        println!("  • Total transactions: {}", self.total_transactions);
        println!("  • Successful: {} ({:.1}%)", self.successful_broadcasts, self.uptime_percent);
        println!("  • Failed: {}", self.failed_broadcasts);
        println!("  • Average latency: {:.2}ms", self.avg_latency_ms);
        println!("  • TPS: {:.0}", self.tps);
        println!("  • Energy cost: {} cycles ({:.1}M cycles/tx)", 
            self.energy_cycles, 
            self.energy_cycles as f64 / self.total_transactions as f64 / 1_000_000.0
        );
        println!("  • Uptime: {:.2}%", self.uptime_percent);
    }
    
    pub fn assert_performance(&self, min_tps: f64, max_latency_ms: f64, min_uptime: f64) {
        assert!(
            self.tps >= min_tps,
            "TPS too low: {:.0} < {:.0}",
            self.tps, min_tps
        );
        
        assert!(
            self.avg_latency_ms <= max_latency_ms,
            "Latency too high: {:.2}ms > {:.2}ms",
            self.avg_latency_ms, max_latency_ms
        );
        
        assert!(
            self.uptime_percent >= min_uptime,
            "Uptime too low: {:.2}% < {:.2}%",
            self.uptime_percent, min_uptime
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_network_simulator() {
        let config = NetworkConfig {
            num_nodes: 10,
            malicious_ratio: 0.2,
            base_delay_ms: 10,
            delay_variance_ms: 20,
            packet_loss_rate: 0.05,
        };
        
        let sim = NetworkSimulator::new(config);
        assert_eq!(sim.nodes.len(), 10);
        
        let result = sim.broadcast_transaction(0, b"test_tx").await;
        assert!(result.delivered_count > 0);
    }
}


