use rand::Rng;
use std::collections::HashMap;
use std::sync::Arc;
/// Realistic Network Simulator with latency, packet loss, and QUIC transport
///
/// Includes:
/// - Network delays (50-200ms)
/// - Packet loss (1-5%)
/// - QUIC transport optimization
/// - Gossipsub propagation
use std::time::Duration;
use tokio::sync::RwLock;

/// Network configuration
#[derive(Debug, Clone)]
pub struct NetworkConfig {
    pub num_nodes: usize,
    pub min_latency_ms: u64,
    pub max_latency_ms: u64,
    pub packet_loss_rate: f64,
    pub quic_enabled: bool,
    pub gossip_fanout: usize,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            num_nodes: 200,
            min_latency_ms: 50,
            max_latency_ms: 200,
            packet_loss_rate: 0.03, // 3%
            quic_enabled: true,
            gossip_fanout: 5,
        }
    }
}

/// Network simulator
pub struct NetworkSimulator {
    config: NetworkConfig,
    node_latencies: Vec<u64>,
    node_packet_loss: Vec<f64>,
    message_counter: Arc<RwLock<u64>>,
    bytes_transferred: Arc<RwLock<u64>>,
}

impl NetworkSimulator {
    pub fn new(config: NetworkConfig) -> Self {
        let mut rng = rand::thread_rng();

        // Generate realistic latencies for each node
        let node_latencies: Vec<u64> = (0..config.num_nodes)
            .map(|_| rng.gen_range(config.min_latency_ms..=config.max_latency_ms))
            .collect();

        // Generate packet loss rates for each node
        let node_packet_loss: Vec<f64> = (0..config.num_nodes)
            .map(|_| rng.gen_range(0.01..=config.packet_loss_rate))
            .collect();

        Self {
            config,
            node_latencies,
            node_packet_loss,
            message_counter: Arc::new(RwLock::new(0)),
            bytes_transferred: Arc::new(RwLock::new(0)),
        }
    }

    /// Simulate network delay for a specific node
    pub fn simulate_delay(&self, node_id: usize) -> Duration {
        let base_latency = self.node_latencies[node_id % self.node_latencies.len()];

        // QUIC is 20-30% faster than TCP
        let actual_latency = if self.config.quic_enabled {
            let speedup = rand::thread_rng().gen_range(0.7..=0.8);
            (base_latency as f64 * speedup) as u64
        } else {
            base_latency
        };

        Duration::from_millis(actual_latency)
    }

    /// Check if packet should be lost
    pub fn should_drop_packet(&self, node_id: usize) -> bool {
        let loss_rate = self.node_packet_loss[node_id % self.node_packet_loss.len()];
        rand::thread_rng().r#gen::<f64>() < loss_rate
    }

    /// Simulate gossipsub propagation delay
    pub async fn gossip_propagation_delay(&self, message_size: usize) -> Duration {
        // Calculate number of hops needed
        let hops = (self.config.num_nodes as f64)
            .log(self.config.gossip_fanout as f64)
            .ceil() as u32;

        // Average latency
        let avg_latency = (self.config.min_latency_ms + self.config.max_latency_ms) / 2;

        // Total delay = avg_latency * hops
        let base_delay = Duration::from_millis(avg_latency * hops as u64);

        // Add bandwidth delay (assume 100 Mbps)
        let bandwidth_delay_us = (message_size * 8) as u64 / 100; // microseconds
        let bandwidth_delay = Duration::from_micros(bandwidth_delay_us);

        // Track bytes
        *self.bytes_transferred.write().await += message_size as u64;
        *self.message_counter.write().await += 1;

        base_delay + bandwidth_delay
    }

    /// Get network statistics
    pub async fn stats(&self) -> NetworkStats {
        NetworkStats {
            total_messages: *self.message_counter.read().await,
            total_bytes: *self.bytes_transferred.read().await,
            avg_latency_ms: (self.config.min_latency_ms + self.config.max_latency_ms) / 2,
            packet_loss_rate: self.config.packet_loss_rate,
        }
    }
}

#[derive(Debug, Clone)]
pub struct NetworkStats {
    pub total_messages: u64,
    pub total_bytes: u64,
    pub avg_latency_ms: u64,
    pub packet_loss_rate: f64,
}
