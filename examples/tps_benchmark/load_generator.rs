use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;
use rayon::prelude::*;
use std::sync::Arc;
/// Load Generator for TPS benchmarking
///
/// Generates realistic transaction load with configurable TPS
/// Uses real TRIAD Transaction and ConsensusMessage types
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use triad::crypto::bls::{BlsSecretKeyExt, BlsSignerExt, PublicKey, SecretKey, secret_to_public};

use triad::quantum::consensus::ConsensusMessage;
use triad::transaction::{Transaction, payload::TransactionPayload};

/// Load generator configuration
#[derive(Debug, Clone)]
pub struct LoadConfig {
    pub target_tps: u64,
    pub duration: Duration,
    pub warmup_duration: Duration,
    pub num_senders: usize,
}

impl Default for LoadConfig {
    fn default() -> Self {
        Self {
            target_tps: 100_000,
            duration: Duration::from_secs(60),
            warmup_duration: Duration::from_secs(0), // No warmup needed
            num_senders: 100,
        }
    }
}

/// Load generator
pub struct LoadGenerator {
    config: LoadConfig,
    senders: Vec<SecretKey>,
    tx_counter: Arc<RwLock<u64>>,
    precomputed_msgs: Arc<RwLock<Vec<ConsensusMessage>>>,
}

impl LoadGenerator {
    pub fn new(config: LoadConfig) -> Self {
        // Generate BLS secret keys using ChaCha20Rng (thread-safe, deterministic)
        // We simulate 100,000 users, but only generate keys for active senders to save RAM
        // In a real benchmark, we'd pre-generate these to disk, but here we do it on fly or just use a smaller set of keys
        // cycled through 100k "identities" (sender_id strings).
        // To keep it simple but realistic:
        // We will generate 1000 real keys and cycle them, BUT we will generate 100,000 unique sender_id strings.
        // The node verifies signature against the public key provided (or looked up).
        // For this benchmark, we assume the node accepts the signature if it validates against the provided pubkey.

        let num_keys = 1000; // Keep 1000 real keys for signing speed
        let senders: Vec<_> = (0..num_keys)
            .map(|i| {
                let seed = i as u64;
                let mut rng = ChaCha20Rng::seed_from_u64(seed);
                SecretKey::random(&mut rng)
            })
            .collect();

        Self {
            config,
            senders,
            tx_counter: Arc::new(RwLock::new(0)),
            precomputed_msgs: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Pre-generate transactions to avoid runtime signing bottleneck
    pub async fn pregenerate_transactions(&self, total_count: usize) {
        println!(
            "  ⏳ Pre-generating {} realistic transactions...",
            total_count
        );
        println!("     - 5,000 active users (realistic cache hit rates)");
        println!("     - Zipf distribution (Hot DEX pools vs Cold wallets)");
        println!("     - Jupiter V6 complex payloads (8-14 CPI simulation)");

        let start = Instant::now();
        let senders = self.senders.clone();
        let msgs_lock = self.precomputed_msgs.clone();

        // Offload heavy CPU work (signing) to blocking thread pool
        tokio::task::spawn_blocking(move || {
            let batch_size = 5000;
            let num_batches = (total_count + batch_size - 1) / batch_size;

            // Hot accounts (DEX pools, Programs) - 100 accounts receive 60% of traffic
            let hot_accounts: Vec<String> = (0..100)
                .map(|i| format!("Program-Jupiter-V6-Pool-{}", i))
                .collect();

            let all_msgs: Vec<ConsensusMessage> = (0..num_batches)
                .into_par_iter()
                .flat_map(|batch_idx| {
                    let start_idx = batch_idx * batch_size;
                    let end_idx = std::cmp::min(start_idx + batch_size, total_count);

                    // Per-thread RNG
                    let mut rng = rand::thread_rng();

                    (start_idx..end_idx)
                        .map(|i| {
                            // 1. Select Sender (simulate 5k active users making multiple txs each)
                            // REALISTIC: Each user makes ~10 transactions on average (50k txs / 5k users)
                            // This allows cache to be effective (high hit rate after warmup)
                            let key_idx = i % senders.len();
                            let virtual_user_idx = i % 5_000; // Reduced from 100k for realistic cache testing
                            let sender_addr = format!("User-Wallet-{:06}", virtual_user_idx);

                            let secret = &senders[key_idx];

                            // 2. Select Receiver (Zipf-like distribution)
                            let receiver_addr = if rand::Rng::gen_bool(&mut rng, 0.6) {
                                let idx = rand::Rng::gen_range(&mut rng, 0..hot_accounts.len());
                                hot_accounts[idx].clone()
                            } else {
                                format!(
                                    "User-Wallet-{:06}",
                                    rand::Rng::gen_range(&mut rng, 0..5_000)
                                )
                            };

                            // 3. Generate realistic payload
                            let payload = TransactionPayload::generate_random();
                            let payload_bytes = payload.to_bytes();

                            // 4. Create real Transaction
                            let tx = Transaction::new(
                                sender_addr.clone(),
                                receiver_addr,
                                rand::Rng::gen_range(&mut rng, 0.1..1000.0),
                                payload_bytes.clone(),
                            );

                            // 5. Serialize & Sign
                            let tx_bytes = tx.to_bytes().unwrap();
                            let signature = secret.sign_msg(&tx_bytes);

                            // 6. Wrap in ConsensusMessage
                            // For benchmark: always include pubkey
                            // In production: only first message would include it
                            ConsensusMessage {
                                sender_id: sender_addr,
                                state_id: tx.id.to_string(),
                                raw_data: tx_bytes.into(),
                                signature: signature.to_bytes().to_vec(),
                                public_key: Some(secret.public_key().to_bytes().to_vec()),
                            }
                        })
                        .collect::<Vec<_>>()
                })
                .collect();

            let mut buffer = msgs_lock.blocking_write();
            *buffer = all_msgs;
        })
        .await
        .unwrap();

        println!(
            "  ✓ Pre-generation complete in {:.2}s",
            start.elapsed().as_secs_f64()
        );
    }

    /// Get a batch of pre-generated messages
    pub async fn get_batch(&self, batch_size: usize) -> Vec<ConsensusMessage> {
        let mut counter = self.tx_counter.write().await;
        let start_idx = *counter as usize;
        let msgs = self.precomputed_msgs.read().await;

        if start_idx >= msgs.len() {
            return Vec::new();
        }

        let end_idx = std::cmp::min(start_idx + batch_size, msgs.len());
        *counter = end_idx as u64;

        msgs[start_idx..end_idx].to_vec()
    }

    /// Calculate transactions per batch based on target TPS
    pub fn batch_size(&self, interval: Duration) -> usize {
        let tps = self.config.target_tps as f64;
        let interval_secs = interval.as_secs_f64();
        (tps * interval_secs) as usize
    }

    /// Get total transactions generated
    pub async fn total_generated(&self) -> u64 {
        *self.tx_counter.read().await
    }
}

/// High-Precision TPS controller
pub struct TpsController {
    target_tps: u64,
    last_batch_time: Instant,
    batch_interval: Duration,
}

impl TpsController {
    pub fn new(target_tps: u64) -> Self {
        // Use smaller interval for smoother flow (1ms)
        let batch_interval = Duration::from_millis(1);

        Self {
            target_tps,
            last_batch_time: Instant::now(),
            batch_interval,
        }
    }

    /// Spin-wait for high precision timing
    pub async fn wait_for_next_batch(&mut self) {
        let target_time = self.last_batch_time + self.batch_interval;

        // Spin-wait loop for microsecond precision
        while Instant::now() < target_time {
            std::hint::spin_loop();
        }

        self.last_batch_time = Instant::now();
    }

    /// Calculate batch size for this interval
    pub fn batch_size(&self) -> usize {
        let tps = self.target_tps as f64;
        let interval_secs = self.batch_interval.as_secs_f64();
        // Always return at least 1 if TPS > 0
        std::cmp::max(1, (tps * interval_secs) as usize)
    }
}
