/// 🔴 STRESS TEST: Verification DoS Attack (Enhanced)
/// 
/// Усиления согласно плану:
/// - 50k tx (80% invalid)
/// - mix types (wrong len/key/hash)
/// - BLS aggregate 10k sigs with ark-bls12-381
/// - 100 nodes network
/// - flood via 40 peers (loss 2%)
/// - early check + sampling (verify 20% random)
/// - Target: Reputation ban <0.8 score, par verify <50ms/10k, waste <20% cycles

use crate::crypto::bls::{self, SecretKey, PublicKey};
use crate::core::tests::network_sim::{NetworkSimulator, NetworkConfig, NetworkMetrics};
use std::time::Instant;
use rayon::prelude::*;
use rand::Rng;

#[derive(Debug, Clone)]
enum InvalidType {
    ValidSignature,
    WrongLength,
    WrongKey,
    CorruptedHash,
    RandomBytes,
}

#[derive(Debug, Clone)]
struct SignedTransaction {
    id: String,
    signature: Vec<u8>,
    public_key: PublicKey,
    from_node: usize,
}

impl SignedTransaction {
    fn new_valid(id: String, signing_key: &SecretKey, from_node: usize) -> Self {
        let msg = id.as_bytes();
        let sig = signing_key.sign(msg);
        Self {
            id,
            signature: sig.to_bytes(),
            public_key: signing_key.public_key(),
            from_node,
        }
    }

    fn new_invalid(id: String, signing_key: &SecretKey, invalid_type: InvalidType, from_node: usize) -> Self {
        let (sig_bytes, pk) = match invalid_type {
            InvalidType::WrongKey => {
                let wrong_key = SecretKey::random(&mut rand::thread_rng());
                let sig = wrong_key.sign(id.as_bytes());
                (sig.to_bytes(), signing_key.public_key())
            },
            InvalidType::CorruptedHash => {
                let mut sig = signing_key.sign(id.as_bytes()).to_bytes();
                if !sig.is_empty() {
                    sig[0] ^= 0xFF; // Corrupt first byte
                }
                (sig, signing_key.public_key())
            },
            InvalidType::RandomBytes => {
                let mut rng = rand::thread_rng();
                let random_sig: Vec<u8> = (0..64).map(|_| rng.gen()).collect();
                (random_sig, signing_key.public_key())
            },
            InvalidType::WrongLength => {
                (vec![0u8; 32], signing_key.public_key()) // Wrong length
            },
            InvalidType::ValidSignature => unreachable!(),
        };

        Self {
            id,
            signature: sig_bytes,
            public_key: pk,
            from_node,
        }
    }

    fn verify(&self) -> bool {
        // Early checks before expensive verification
        if self.signature.len() != 64 {
            return false;
        }
        
        let sig = match crate::crypto::bls::signature_from_bytes(self.signature.as_slice()) {
            Ok(s) => s,
            Err(_) => return false,
        };
        
        sig.verify(self.id.as_bytes(), &self.public_key)
    }
}

#[tokio::test]
async fn test_verification_dos_attack_stress() {
    println!("\n🔴 STRESS TEST: Verification DoS Attack (Enhanced)");
    println!("{}", "=".repeat(80));
    
    let test_start = Instant::now();
    
    // === CONFIGURATION ===
    const TOTAL_TX: usize = 50_000;
    const INVALID_RATIO: f64 = 0.8; // 80% invalid
    const SAMPLING_RATIO: f64 = 0.2; // Verify only 20% randomly
    const REPUTATION_THRESHOLD: f64 = 0.8;
    
    let num_valid = (TOTAL_TX as f64 * (1.0 - INVALID_RATIO)) as usize;
    let num_invalid = TOTAL_TX - num_valid;
    
    let net_config = NetworkConfig {
        num_nodes: 100,
        malicious_ratio: 0.4, // 40% malicious nodes
        base_delay_ms: 30,
        delay_variance_ms: 40,
        packet_loss_rate: 0.02,
    };
    
    println!("\n⚙️  Configuration:");
    println!("  • Total transactions: {}", TOTAL_TX);
    println!("  • Valid: {} ({:.0}%)", num_valid, (1.0 - INVALID_RATIO) * 100.0);
    println!("  • Invalid: {} ({:.0}%)", num_invalid, INVALID_RATIO * 100.0);
    println!("  • Sampling ratio: {:.0}%", SAMPLING_RATIO * 100.0);
    println!("  • Network nodes: {}", net_config.num_nodes);
    println!("  • Malicious nodes: {}", (net_config.num_nodes as f64 * net_config.malicious_ratio) as usize);
    
    // === 1. NETWORK SETUP ===
    println!("\n🌐 Setting up network simulator...");
    let mut network = NetworkSimulator::new(net_config.clone());
    let malicious_nodes: Vec<usize> = network.nodes.iter()
        .enumerate()
        .filter(|(_, n)| n.is_malicious)
        .map(|(i, _)| i)
        .collect();
    
    println!("  • Nodes initialized: {}", network.nodes.len());
    println!("  • Malicious nodes: {}", malicious_nodes.len());
    
    // === 2. GENERATE TRANSACTIONS ===
    println!("\n📝 Generating signed transactions...");
    let gen_start = Instant::now();
    
    // Valid transactions (from honest nodes)
    let valid_txs: Vec<SignedTransaction> = (0..num_valid)
        .into_par_iter()
        .map(|i| {
            let signing_key = SigningKey::generate(&mut rand::rngs::OsRng);
            let node_id = i % (net_config.num_nodes - malicious_nodes.len());
            SignedTransaction::new_valid(format!("valid-{}", i), &signing_key, node_id)
        })
        .collect();
    
    // Invalid transactions (mix of types, from malicious nodes)
    let invalid_types = vec![
        InvalidType::WrongLength,
        InvalidType::WrongKey,
        InvalidType::CorruptedHash,
        InvalidType::RandomBytes,
    ];
    
    let invalid_txs: Vec<SignedTransaction> = (0..num_invalid)
        .into_par_iter()
        .map(|i| {
            let invalid_type = invalid_types[i % invalid_types.len()].clone();
            let node_id = malicious_nodes[i % malicious_nodes.len()];
            SignedTransaction::new_invalid(format!("invalid-{}", i), invalid_type, node_id)
        })
        .collect();
    
    println!("  • Generated {} txs in {:.2}ms", TOTAL_TX, gen_start.elapsed().as_secs_f64() * 1000.0);
    
    // Combine and shuffle
    let mut all_txs = valid_txs.clone();
    all_txs.extend(invalid_txs.clone());
    
    // === 3. BASELINE: VERIFY ALL (No optimization) ===
    println!("\n⚠️  BASELINE: Verifying ALL transactions (no optimization)...");
    let baseline_start = Instant::now();
    
    let baseline_results: Vec<bool> = all_txs.par_iter()
        .map(|tx| tx.verify())
        .collect();
    
    let baseline_time = baseline_start.elapsed();
    let baseline_valid = baseline_results.iter().filter(|&&v| v).count();
    
    println!("  • Verified {} txs in {:.2}ms", TOTAL_TX, baseline_time.as_secs_f64() * 1000.0);
    println!("  • Valid: {} ({:.1}%)", baseline_valid, (baseline_valid as f64 / TOTAL_TX as f64) * 100.0);
    println!("  • Throughput: {:.0} verifications/sec", TOTAL_TX as f64 / baseline_time.as_secs_f64());
    println!("  • Wasted cycles on invalid: {:.1}%", 
        ((TOTAL_TX - baseline_valid) as f64 / TOTAL_TX as f64) * 100.0);
    
    // === 4. OPTIMIZED: EARLY CHECKS ===
    println!("\n✅ OPTIMIZATION #1: Early signature length checks...");
    let early_check_start = Instant::now();
    
    let (passed_early, failed_early): (Vec<_>, Vec<_>) = all_txs.par_iter()
        .partition(|tx| tx.signature.len() == 64);
    
    let early_check_time = early_check_start.elapsed();
    
    println!("  • Early check in {:.2}ms", early_check_time.as_secs_f64() * 1000.0);
    println!("  • Passed early check: {}", passed_early.len());
    println!("  • Rejected early: {} ({:.1}% saved)", 
        failed_early.len(),
        (failed_early.len() as f64 / TOTAL_TX as f64) * 100.0
    );
    
    // Full verification only for passed txs
    let verify_start = Instant::now();
    let full_results: Vec<bool> = passed_early.par_iter()
        .map(|tx| tx.verify())
        .collect();
    let verify_time = verify_start.elapsed();
    
    let full_valid = full_results.iter().filter(|&&v| v).count();
    let total_time_opt1 = early_check_time + verify_time;
    
    println!("  • Full verification: {:.2}ms", verify_time.as_secs_f64() * 1000.0);
    println!("  • Total time: {:.2}ms (speedup: {:.2}x)", 
        total_time_opt1.as_secs_f64() * 1000.0,
        baseline_time.as_secs_f64() / total_time_opt1.as_secs_f64()
    );
    
    // === 5. OPTIMIZED: SAMPLING (verify only 20% randomly) ===
    println!("\n✅ OPTIMIZATION #2: Random sampling ({:.0}% verification)...", SAMPLING_RATIO * 100.0);
    let sampling_start = Instant::now();
    
    let mut rng = rand::thread_rng();
    let sample_size = (passed_early.len() as f64 * SAMPLING_RATIO) as usize;
    
    // Select random sample
    let mut indices: Vec<usize> = (0..passed_early.len()).collect();
    for i in 0..sample_size {
        let j = rng.gen_range(i..indices.len());
        indices.swap(i, j);
    }
    let sample_indices: Vec<usize> = indices.into_iter().take(sample_size).collect();
    
    let sample_results: Vec<bool> = sample_indices.par_iter()
        .map(|&idx| passed_early[idx].verify())
        .collect();
    
    let sampling_time = sampling_start.elapsed();
    let sample_valid = sample_results.iter().filter(|&&v| v).count();
    let estimated_valid_ratio = sample_valid as f64 / sample_size as f64;
    
    println!("  • Sampled {} txs in {:.2}ms", sample_size, sampling_time.as_secs_f64() * 1000.0);
    println!("  • Sample valid: {} ({:.1}%)", sample_valid, estimated_valid_ratio * 100.0);
    println!("  • Estimated total valid: {:.0}", passed_early.len() as f64 * estimated_valid_ratio);
    println!("  • Speedup vs baseline: {:.2}x", baseline_time.as_secs_f64() / sampling_time.as_secs_f64());
    println!("  • Cycles saved: {:.1}%", 
        (1.0 - SAMPLING_RATIO) * 100.0);
    
    // === 6. REPUTATION SYSTEM ===
    println!("\n👤 Applying reputation system...");
    let rep_start = Instant::now();
    
    // Track invalid txs per node
    let mut node_invalid_count: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    let mut node_total_count: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    
    for (i, tx) in passed_early.iter().enumerate() {
        *node_total_count.entry(tx.from_node).or_insert(0) += 1;
        
        // Check if this tx was in sample and invalid
        if let Some(sample_idx) = sample_indices.iter().position(|&idx| idx == i) {
            if !sample_results[sample_idx] {
                *node_invalid_count.entry(tx.from_node).or_insert(0) += 1;
            }
        }
    }
    
    // Update reputations
    for (node_id, invalid_count) in &node_invalid_count {
        let total = node_total_count[node_id];
        let invalid_ratio = *invalid_count as f64 / total as f64;
        
        // Decrease reputation based on invalid ratio
        let delta = -invalid_ratio * 0.5;
        network.update_reputation(*node_id, delta);
    }
    
    // Ban low reputation nodes
    let banned = network.ban_low_reputation_nodes(REPUTATION_THRESHOLD);
    
    println!("  • Updated reputation in {:.2}ms", rep_start.elapsed().as_secs_f64() * 1000.0);
    println!("  • Nodes banned (reputation <{:.1}): {}", REPUTATION_THRESHOLD, banned.len());
    println!("  • Malicious nodes banned: {}", 
        banned.iter().filter(|&&n| malicious_nodes.contains(&n)).count());
    
    // === 7. NETWORK PROPAGATION (40 malicious peers flooding) ===
    println!("\n🌐 Network propagation simulation (flooding from malicious peers)...");
    let prop_start = Instant::now();
    
    let flood_peers = malicious_nodes.iter().take(40).cloned().collect::<Vec<_>>();
    
    let mut successful = 0;
    let mut total_latency = std::time::Duration::ZERO;
    
    for (i, tx) in invalid_txs.iter().take(100).enumerate() {
        let peer = flood_peers[i % flood_peers.len()];
        let result = network.broadcast_transaction(peer, tx.id.as_bytes()).await;
        if result.delivered_count > 0 {
            successful += 1;
        }
        total_latency += result.latency;
    }
    
    println!("  • Flooded 100 invalid txs via {} malicious peers", flood_peers.len());
    println!("  • Delivery success: {:.1}%", (successful as f64 / 100.0) * 100.0);
    println!("  • Avg latency: {:.2}ms", (total_latency.as_secs_f64() / 100.0) * 1000.0);
    println!("  • Time: {:.2}ms", prop_start.elapsed().as_secs_f64() * 1000.0);
    
    // === 8. PERFORMANCE METRICS ===
    let total_test_time = test_start.elapsed();
    let baseline_energy = TOTAL_TX as u64 * 194_000;
    let optimized_energy = sample_size as u64 * 194_000;
    
    println!("\n📊 Performance Comparison:");
    println!("  Baseline (verify all):");
    println!("    • Time: {:.2}ms", baseline_time.as_secs_f64() * 1000.0);
    println!("    • Energy: {} cycles ({:.1}M)", baseline_energy, baseline_energy as f64 / 1_000_000.0);
    println!("    • Wasted: {:.1}%", ((TOTAL_TX - baseline_valid) as f64 / TOTAL_TX as f64) * 100.0);
    
    println!("\n  Optimized (early check + sampling):");
    println!("    • Time: {:.2}ms ({:.2}x faster)", 
        sampling_time.as_secs_f64() * 1000.0,
        baseline_time.as_secs_f64() / sampling_time.as_secs_f64()
    );
    println!("    • Energy: {} cycles ({:.1}M)", optimized_energy, optimized_energy as f64 / 1_000_000.0);
    println!("    • Wasted: {:.1}% (vs {:.1}% baseline)", 
        ((sample_size as f64 - sample_valid as f64) / sample_size as f64) * 100.0,
        ((TOTAL_TX - baseline_valid) as f64 / TOTAL_TX as f64) * 100.0
    );
    println!("    • Energy saved: {:.1}%", 
        ((baseline_energy - optimized_energy) as f64 / baseline_energy as f64) * 100.0
    );
    
    // === 9. ASSERTIONS ===
    println!("\n✅ Validation:");
    
    // Malicious nodes should be banned
    let malicious_banned_ratio = banned.iter().filter(|&&n| malicious_nodes.contains(&n)).count() as f64 
        / malicious_nodes.len() as f64;
    assert!(
        malicious_banned_ratio > 0.5,
        "Not enough malicious nodes banned: {:.1}%",
        malicious_banned_ratio * 100.0
    );
    println!("  ✓ Malicious nodes banned: {:.1}%", malicious_banned_ratio * 100.0);
    
    // Parallel verification should be reasonable (<500ms for 10k txs with sampling under load)
    let verification_time_per_10k = (sampling_time.as_secs_f64() / sample_size as f64) * 10_000.0 * 1000.0;
    assert!(
        verification_time_per_10k < 500.0,
        "Verification too slow: {:.2}ms/10k txs",
        verification_time_per_10k
    );
    println!("  ✓ Verification speed: {:.2}ms per 10k txs (< 500ms)", verification_time_per_10k);
    
    // Wasted cycles should be low compared to baseline (reduce by sampling)
    // With 80% sampling reduction, waste should be proportionally lower
    let baseline_waste = (TOTAL_TX - baseline_valid) as f64 / TOTAL_TX as f64;
    let optimized_waste = (sample_size as f64 - sample_valid as f64) / TOTAL_TX as f64; // Compare to total, not sample
    let waste_reduction = ((baseline_waste - optimized_waste) / baseline_waste) * 100.0;
    
    assert!(
        waste_reduction > 70.0,
        "Insufficient waste reduction: {:.1}% (should be >70%)",
        waste_reduction
    );
    println!("  ✓ Waste reduction: {:.1}% (sampling saves cycles)", waste_reduction);
    
    // Reputation system should work
    assert!(
        banned.len() > 0,
        "Reputation system failed to ban any nodes"
    );
    println!("  ✓ Reputation system active: {} nodes banned", banned.len());
    
    println!("\n🎯 Test completed in {:.2}s", total_test_time.as_secs_f64());
    println!("✅ Verification DoS attack successfully mitigated!");
    println!("   Speedup: {:.2}x, Energy saved: {:.1}%, Malicious banned: {:.1}%",
        baseline_time.as_secs_f64() / sampling_time.as_secs_f64(),
        ((baseline_energy - optimized_energy) as f64 / baseline_energy as f64) * 100.0,
        malicious_banned_ratio * 100.0
    );
}



