use parking_lot::RwLock;
/// Benchmark Node - integrates all TRIAD components
///
/// Uses:
/// - ConsensusNode (BLS signatures, Interference Engine)
/// - ShardedQuantumField (Minkowski spacetime)
/// - Const Routing (O(1) sharding)
use std::sync::Arc;
use tokio::sync::mpsc;
use triad::crypto::bls::{self, PublicKey, SecretKey};
use triad::quantum::consensus::{ConsensusMessage, ConsensusNode};
use triad::quantum::field::ShardedQuantumField;
use triad::quantum::interference::InterferenceEngine;
use triad::sharding::const_routing;

use crate::tps_benchmark::metrics::NodeMetrics;

/// Benchmark node configuration
#[derive(Debug, Clone)]
pub struct BenchmarkNodeConfig {
    pub node_id: usize,
    pub shard_id: usize,
    pub num_shards: usize,
    pub max_waves: usize,
}

impl Default for BenchmarkNodeConfig {
    fn default() -> Self {
        Self {
            node_id: 0,
            shard_id: 0,
            num_shards: 8,
            max_waves: 1_000_000,
        }
    }
}

/// Benchmark node with full TRIAD integration
#[derive(Clone)]
pub struct BenchmarkNode {
    pub id: usize,
    pub config: BenchmarkNodeConfig,

    // TRIAD components
    pub consensus: ConsensusNode,

    // Metrics
    pub metrics: Arc<RwLock<NodeMetrics>>,

    // Message queue
    msg_sender: mpsc::Sender<ConsensusMessage>,
    msg_receiver: Arc<tokio::sync::RwLock<mpsc::Receiver<ConsensusMessage>>>,
}

impl BenchmarkNode {
    pub fn new(config: BenchmarkNodeConfig) -> Self {
        // Create ConsensusNode (Phase 3 optimized)
        let consensus = ConsensusNode::new(config.node_id as u128, config.shard_id as u8);

        // Create message queue
        let (msg_sender, msg_receiver) = mpsc::channel(50000); // Large buffer for high throughput

        let node_id = config.node_id;

        Self {
            id: node_id,
            config,
            consensus,
            metrics: Arc::new(RwLock::new(NodeMetrics::new(node_id))),
            msg_sender,
            msg_receiver: Arc::new(tokio::sync::RwLock::new(msg_receiver)),
        }
    }

    pub fn reset_metrics(&self) {
        self.metrics.write().reset();
    }

    /// Get message sender
    pub fn msg_sender(&self) -> mpsc::Sender<ConsensusMessage> {
        self.msg_sender.clone()
    }

    /// Process message queue using real ConsensusNode pipeline
    pub async fn process_queue(&mut self, max_batch: usize) -> (usize, usize) {
        // Extract messages from queue
        let mut msgs_to_process = Vec::with_capacity(max_batch);
        {
            let mut receiver = self.msg_receiver.write().await;
            for _ in 0..max_batch {
                match receiver.try_recv() {
                    Ok(msg) => msgs_to_process.push(msg),
                    Err(_) => break,
                }
            }
        }

        let msg_count = msgs_to_process.len();

        if msg_count == 0 {
            return (0, 0);
        }

        // DEBUG: Log queue extraction
        if msg_count > 0 && self.id % 50 == 0 {
            println!(
                "  🔍 [Node {}] Extracted {} msgs from queue",
                self.id, msg_count
            );
        }

        // OPTIMIZATION: Process directly without spawn_blocking overhead
        let start_batch = std::time::Instant::now();

        // Use the optimized batch processing pipeline
        let (processed, rejected) = match self.consensus.process_batch(msgs_to_process) {
            Ok(_) => {
                let processed = msg_count;
                let rejected = 0;
                (processed, rejected)
            }
            Err(_e) => {
                // Batch failed (e.g. signature verification or destructive interference)
                let rejected = msg_count;
                let processed = 0;
                (processed, rejected)
            }
        };

        // Update Metrics
        let batch_latency = start_batch.elapsed();
        {
            let mut m = self.metrics.write();
            m.processed_tx += processed as u64;
            m.rejected_tx += rejected as u64;
            // Accumulate weighted latency: batch_time * num_txs
            // This allows calculating the true average latency per transaction
            m.total_latency += batch_latency * (processed as u32);
        }

        (processed, rejected)
    }
}
