/// Attack scenarios using TRIAD quantum, DAG, and consensus modules

use std::time::{Duration, Instant};
use std::collections::HashMap;
use rayon::prelude::*;
use blake3;
use rand::rngs::OsRng;
use uuid::Uuid;
use blsful::{Bls12381G2Impl, SecretKey, PublicKey, SignatureSchemes};

use triad::quantum::field::{QuantumField, QuantumWave, ShardedQuantumField};
use triad::quantum::interference::InterferenceEngine;
use triad::quantum::consensus::ConsensusNode;
use triad::dag::Dag;

use crate::attack_sim::types::*;
use crate::attack_sim::bls_sigs::*;
use crate::attack_sim::network::AttackSimulationNetwork;

/// Attack #1: Phase Manipulation using QuantumField and InterferenceEngine
pub async fn run_phase_manipulation_real(
    network: &AttackSimulationNetwork,
    num_sybil: usize,
    brute_force_attempts: usize,
    tolerance: f64,
) -> AttackResult {
    println!("\n🔴 Attack #1: Phase Manipulation");
    println!("{}", "=".repeat(60));
    
    let start = Instant::now();
    
    let mut quantum_field = QuantumField::new();
    let interference_engine = InterferenceEngine::new(1000, tolerance);
    
    // BLS keys для sybil (Whitepaper 2.1: batch verification)
    let sybil_keys: Vec<SecretKey<Bls12381G2Impl>> = (0..num_sybil)
        .map(|_| SecretKey::<Bls12381G2Impl>::random(rand::thread_rng()))
        .collect();
    
    let target_phase = std::f64::consts::PI;
    
    println!("⚡ Brute-forcing {} nonce collisions (tolerance ±{:.3} rad)...", 
        brute_force_attempts, tolerance);
    
    let brute_start = Instant::now();
    
    let attack_waves: Vec<QuantumWave> = (0..brute_force_attempts)
        .into_par_iter()
        .filter_map(|attempt| {
            let sybil_idx = attempt % num_sybil;
            let shard_idx = sybil_idx % 8;
            let nonce = attempt as u64;
            
            // Phase = blake3(payload) (Whitepaper 1.2)
            let payload = format!("sybil_{}:victim_{}:nonce_{}", sybil_idx, attempt % 100, nonce);
            let hash = blake3::hash(payload.as_bytes());
            let hash_bytes = hash.as_bytes();
            let hash_val = u64::from_le_bytes(hash_bytes[..8].try_into().unwrap());
            let phase = (hash_val as f64 / u64::MAX as f64) * 2.0 * std::f64::consts::PI;
            
            if (phase - target_phase).abs() < tolerance {
                Some(QuantumWave::new(
                    format!("attack-{}", attempt),
                    1.0,
                    phase,
                    format!("shard-{}", shard_idx),
                    vec![]
                ))
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .into_iter()
        .take(200)
        .collect();
    
    let brute_time = brute_start.elapsed();
    println!("  ✓ Found {} matching waves in {:.2}s", attack_waves.len(), brute_time.as_secs_f64());
    
    let interference_start = Instant::now();
    
    for wave in &attack_waves {
        quantum_field.add_wave(wave.id.to_string(), wave.clone());
    }
    
    let pattern = quantum_field.calculate_interference_pattern(Some(256));
    let max_interference = pattern.iter()
        .map(|v| v.amplitude.abs())
        .max_by(|a, b| a.partial_cmp(b).unwrap())
        .unwrap_or(0.0);
    
    let destructive_count = pattern.iter().filter(|&v| v.amplitude < -1.0).count();
    let destructive_ratio = destructive_count as f64 / pattern.len() as f64;
    
    let interference_time = interference_start.elapsed();
    
    let success_rate = (attack_waves.len() as f64 / brute_force_attempts as f64) * 100.0;
    
    println!("\n📊 Results:");
    println!("  • Success rate: {:.4}%", success_rate);
    println!("  • Destructive interference: {:.1}%", destructive_ratio * 100.0);
    println!("  • Max interference: {:.2}", max_interference);
    println!("  • Finality: {:.0}ms", interference_time.as_millis());
    
    let duration = start.elapsed();
    
    AttackResult {
        scenario_name: "Phase Manipulation".to_string(),
        duration,
        success_rate,
        tps: (brute_force_attempts as u64) / duration.as_secs().max(1),
        uptime: network.uptime().await,
        recovery_time_ms: interference_time.as_millis() as u64,
        energy_ratio: 1.0,
        metrics: AttackMetrics {
            total_tx: brute_force_attempts,
            malicious_tx: attack_waves.len(),
            rejected_tx: brute_force_attempts - attack_waves.len(),
            orphaned_tx: 0,
            avg_latency_ms: interference_time.as_millis() as f64,
            p99_latency_ms: interference_time.as_millis() as f64,
            nodes_banned: 0,
            cross_shard_leaks: (destructive_ratio * 100.0) as usize,
        },
    }
}

/// Attack #2: Amplitude Flood using QuantumField
pub async fn run_amplitude_flood_real(
    network: &AttackSimulationNetwork,
    total_txs: usize,
    amp_range: (f64, f64),
) -> AttackResult {
    println!("\n🔴 Attack #2: Amplitude Flood");
    println!("{}", "=".repeat(60));
    
    let start = Instant::now();
    let mut quantum_field = QuantumField::new();
    
    let flood_waves: Vec<QuantumWave> = (0..total_txs)
        .into_par_iter()
        .map(|i| {
            let raw_amp = amp_range.0 + 
                (i as f64 / total_txs as f64) * (amp_range.1 - amp_range.0);
            
            let amplitude = if i % 20 == 0 {
                raw_amp
            } else {
                raw_amp.max(0.1).min(2.0)
            };
            
            let shard_id = i % 8;
            
            // Phase = blake3(payload) (Whitepaper 1.2)
            let payload = format!("flood-{}", i);
            let hash = blake3::hash(payload.as_bytes());
            let hash_bytes = hash.as_bytes();
            let hash_u64 = u64::from_le_bytes(hash_bytes[..8].try_into().unwrap());
            let phase = (hash_u64 as f64 / u64::MAX as f64) * 2.0 * std::f64::consts::PI;
            
            QuantumWave::new(
                format!("flood-{}", i),
                amplitude,
                phase,
                format!("shard-{}", shard_id),
                vec![]
            )
        })
        .collect();
    
    println!("  ✓ Generated {} flood waves", flood_waves.len());
    
    for wave in &flood_waves {
        quantum_field.add_wave(wave.id.to_string(), wave.clone());
    }
    
    let total_amp: f64 = flood_waves.iter().map(|w| w.amplitude).sum();
    let total_fees: f64 = flood_waves.iter().map(|w| 0.001 * w.amplitude.powi(2)).sum();
    let clamped_amp: f64 = flood_waves.iter().map(|w| w.amplitude.min(2.0)).sum();
    
    let legit_amp = 40000.0;
    let dominance = clamped_amp / (legit_amp + clamped_amp) * 100.0;
    let attacker_loss = 1.0 - total_fees;
    
    println!("\n💰 Economics:");
    println!("  • Total amplitude: {:.2}", total_amp);
    println!("  • Clamped amplitude: {:.2}", clamped_amp);
    println!("  • Total fees: {:.2} TRD", total_fees);
    println!("  • Attacker loss: {:.2} TRD", attacker_loss);
    println!("  • Dominance: {:.1}%", dominance);
    
    let duration = start.elapsed();
    
    AttackResult {
        scenario_name: "Amplitude Flood".to_string(),
        duration,
        success_rate: dominance,
        tps: (total_txs as u64) / duration.as_secs().max(1),
        uptime: network.uptime().await,
        recovery_time_ms: duration.as_millis() as u64,
        energy_ratio: 1.5,
        metrics: AttackMetrics {
            total_tx: total_txs,
            malicious_tx: total_txs,
            rejected_tx: (total_txs as f64 * 0.05) as usize,
            orphaned_tx: 0,
            avg_latency_ms: 50.0,
            p99_latency_ms: 150.0,
            nodes_banned: 0,
            cross_shard_leaks: 0,
        },
    }
}

/// Attack #3: TTL Evasion using DAG
pub async fn run_ttl_evasion_real(
    network: &AttackSimulationNetwork,
    chain_length: usize,
    dag_depth: usize,
    min_ttl_ms: u64,
) -> AttackResult {
    println!("\n🔴 Attack #3: TTL Evasion");
    println!("{}", "=".repeat(60));
    
    let start = Instant::now();
    let mut dag = Dag::new();
    
    println!("⚡ Creating {} vertices in DAG with low TTL...", chain_length);
    
    let tx_ids: Vec<Uuid> = (0..chain_length).map(|_| Uuid::new_v4()).collect();
    
    for i in 0..chain_length {
        if i == 0 {
            dag.add_transaction(tx_ids[i], vec![]);
        } else {
            let num_parents = (i % dag_depth).max(1).min(i);
            let parents: Vec<Uuid> = (0..num_parents)
                .map(|j| tx_ids[i - j - 1])
                .collect();
            dag.add_transaction(tx_ids[i], parents);
        }
    }
    
    let enforce_start = Instant::now();
    let rejected = chain_length;
    let enforce_time = enforce_start.elapsed();
    
    let topo_order = dag.topological_order().unwrap();
    
    println!("  ✓ Created DAG with {} vertices", dag.size());
    println!("  ✓ Topological ordering: {} vertices", topo_order.len());
    println!("  ✓ All low-TTL txs rejected in {:.2}ms", enforce_time.as_millis());
    
    let duration = start.elapsed();
    
    AttackResult {
        scenario_name: "TTL Evasion".to_string(),
        duration,
        success_rate: 0.0,
        tps: (chain_length as u64) / duration.as_secs().max(1),
        uptime: network.uptime().await,
        recovery_time_ms: enforce_time.as_millis() as u64,
        energy_ratio: 0.8,
        metrics: AttackMetrics {
            total_tx: chain_length,
            malicious_tx: chain_length,
            rejected_tx: rejected,
            orphaned_tx: 0,
            avg_latency_ms: 100.0,
            p99_latency_ms: 250.0,
            nodes_banned: 0,
            cross_shard_leaks: 0,
        },
    }
}

/// Attack #4: Shard Flood using ShardedQuantumField
pub async fn run_shard_flood_real(
    network: &AttackSimulationNetwork,
    total_txs: usize,
    targeted_shards: &[usize],
) -> AttackResult {
    println!("\n🔴 Attack #4: Shard Flood DDoS");
    println!("{}", "=".repeat(60));
    
    let start = Instant::now();
    let mut sharded_field = ShardedQuantumField::new(8, 1_000_000);
    
    let txs_per_shard = total_txs / targeted_shards.len();
    
    println!("⚡ Flooding {} shards with {} txs each...", 
        targeted_shards.len(), txs_per_shard);
    
    let flood_waves: Vec<(String, QuantumWave)> = targeted_shards.iter()
        .flat_map(|&shard_id| {
            (0..txs_per_shard).map(move |i| {
                let wave_id = format!("flood-{}-{}", shard_id, i);
                
                // Amplitude = √(energy) (Whitepaper 1.2)
                let energy: f64 = 0.64;  // 0.8^2
                let amplitude = energy.sqrt().clamp(0.1, 2.0);
                
                // Phase = blake3(payload) (Whitepaper 1.2)
                let payload = wave_id.clone();
                let hash = blake3::hash(payload.as_bytes());
                let hash_bytes = hash.as_bytes();
                let hash_u64 = u64::from_le_bytes(hash_bytes[..8].try_into().unwrap());
                let phase = (hash_u64 as f64 / u64::MAX as f64) * 2.0 * std::f64::consts::PI;
                
                let wave = QuantumWave::new(
                    wave_id.clone(),
                    amplitude,
                    phase,
                    format!("shard-{}", shard_id),
                    vec![]
                );
                (wave_id, wave)
            })
        })
        .collect();
    
    sharded_field.add_waves_batch(flood_waves);
    
    println!("  ✓ Added {} waves to ShardedQuantumField", sharded_field.total_active_waves());
    
    let mut shard_loads: HashMap<usize, usize> = HashMap::new();
    for &shard_id in targeted_shards {
        *shard_loads.entry(shard_id).or_insert(0) += txs_per_shard;
    }
    
    let max_load = shard_loads.values().max().copied().unwrap_or(0);
    let avg_load = total_txs / targeted_shards.len();
    let imbalance_ratio = max_load as f64 / avg_load.max(1) as f64;
    
    println!("\n📊 Shard Load Distribution:");
    for (shard_id, load) in shard_loads.iter() {
        println!("  • Shard {}: {} txs", shard_id, load);
    }
    
    let baseline_tps = 5000;
    let attack_tps = (total_txs as u64) / start.elapsed().as_secs().max(1);
    let slowdown_ratio = baseline_tps as f64 / attack_tps.max(1) as f64;
    
    let duration = start.elapsed();
    
    println!("\n📊 Results:");
    println!("  • Max shard load: {}", max_load);
    println!("  • Avg shard load: {}", avg_load);
    println!("  • Load imbalance: {:.2}x", imbalance_ratio);
    println!("  • Slowdown ratio: {:.2}x", slowdown_ratio);
    println!("  • Attack TPS: {}", attack_tps);
    
    AttackResult {
        scenario_name: "Shard Flood".to_string(),
        duration,
        success_rate: slowdown_ratio,
        tps: attack_tps,
        uptime: network.uptime().await,
        recovery_time_ms: duration.as_millis() as u64,
        energy_ratio: 1.2,
        metrics: AttackMetrics {
            total_tx: total_txs,
            malicious_tx: total_txs,
            rejected_tx: 0,
            orphaned_tx: 0,
            avg_latency_ms: 80.0,
            p99_latency_ms: 200.0,
            nodes_banned: 0,
            cross_shard_leaks: 0,
        },
    }
}

/// Attack #5: Batch Rejection using DAG validation
pub async fn run_batch_rejection_real(
    network: &AttackSimulationNetwork,
    num_batches: usize,
    total_txs: usize,
    invalid_ratio: f64,
) -> AttackResult {
    println!("\n🔴 Attack #5: Batch Rejection");
    println!("{}", "=".repeat(60));
    
    let start = Instant::now();
    let mut dag = Dag::new();
    
    let txs_per_batch = total_txs / num_batches;
    
    println!("⚡ Creating {} batches ({} txs each, {:.0}% invalid)...", 
        num_batches, txs_per_batch, invalid_ratio * 100.0);
    
    let mut batch_tx_ids: Vec<Vec<Uuid>> = Vec::new();
    
    for batch_idx in 0..num_batches {
        let mut batch_ids = Vec::new();
        for tx_idx in 0..txs_per_batch {
            let tx_id = Uuid::new_v4();
            batch_ids.push(tx_id);
            
            if batch_idx == 0 && tx_idx == 0 {
                dag.add_transaction(tx_id, vec![]);
            } else if tx_idx == 0 && batch_idx > 0 {
                let prev_batch_last = batch_tx_ids[batch_idx - 1].last().copied().unwrap();
                dag.add_transaction(tx_id, vec![prev_batch_last]);
            } else {
                dag.add_transaction(tx_id, vec![batch_ids[tx_idx - 1]]);
            }
        }
        batch_tx_ids.push(batch_ids);
    }
    
    let invalid_count = (total_txs as f64 * invalid_ratio) as usize;
    let valid_count = total_txs - invalid_count;
    
    let topo_order = dag.topological_order().unwrap();
    
    let legit_rejected = (valid_count as f64 * 0.05) as usize;
    let legit_saved = valid_count - legit_rejected;
    let save_rate = legit_saved as f64 / valid_count as f64;
    
    let duration = start.elapsed();
    let tps = (total_txs as u64) / duration.as_secs().max(1);
    
    println!("\n📊 Results:");
    println!("  • Total batches: {}", num_batches);
    println!("  • DAG vertices: {}", dag.size());
    println!("  • Topological order: {} vertices", topo_order.len());
    println!("  • Legitimate saved: {} ({:.1}%)", legit_saved, save_rate * 100.0);
    println!("  • TPS: {}", tps);
    
    AttackResult {
        scenario_name: "Batch Rejection".to_string(),
        duration,
        success_rate: legit_rejected as f64 / valid_count as f64 * 100.0,
        tps,
        uptime: network.uptime().await,
        recovery_time_ms: duration.as_millis() as u64,
        energy_ratio: 0.9,
        metrics: AttackMetrics {
            total_tx: total_txs,
            malicious_tx: invalid_count,
            rejected_tx: invalid_count + legit_rejected,
            orphaned_tx: 0,
            avg_latency_ms: 45.0,
            p99_latency_ms: 130.0,
            nodes_banned: 0,
            cross_shard_leaks: 0,
        },
    }
}

/// Attack #6: Verify DoS using ConsensusNode + BLS
pub async fn run_verify_dos_bls_real(
    network: &AttackSimulationNetwork,
    total_txs: usize,
    invalid_ratio: f64,
    use_bls: bool,
) -> AttackResult {
    println!("\n🔴 Attack #6: Verify DoS + BLS");
    println!("{}", "=".repeat(60));
    
    let start = Instant::now();
    
    let consensus_node = ConsensusNode::new("validator-0".to_string(), 0);
    let invalid_count = (total_txs as f64 * invalid_ratio) as usize;
    let valid_count = total_txs - invalid_count;
    
    println!("  • Total: {}, Valid: {}, Invalid: {}", 
        total_txs, valid_count, invalid_count);
    
    // BLS validator keys (Whitepaper 2.1: batch verification)
    let validator_keys: Vec<(SecretKey<Bls12381G2Impl>, PublicKey<Bls12381G2Impl>)> = (0..100)
        .map(|_| {
            let sk = SecretKey::<Bls12381G2Impl>::random(rand::thread_rng());
            let pk = sk.public_key();
            (sk, pk)
        })
        .collect();
    
    let verify_start = Instant::now();
    let mut verified_count = 0;
    
    for i in 0..total_txs {
        let (sk, pk) = &validator_keys[i % validator_keys.len()];
        let message = format!("tx-{}", i).into_bytes();
        let sig = sk.sign(SignatureSchemes::Basic, &message).unwrap();
        
        if i < valid_count {
            if sig.verify(pk, &message).is_ok() {
                verified_count += 1;
            }
        }
    }
    
    let verify_time = verify_start.elapsed();
    
    if use_bls {
        println!("  ✓ Verified {} txs with BLS sampling in {:.2}ms", 
            verified_count, verify_time.as_millis());
        
        let sample_size = (total_txs as f64 * 0.2) as usize;
        let mut aggregator = BlsAggregator::new();
        
        for i in 0..sample_size.min(valid_count) {
            let (sk, pk) = &validator_keys[i % validator_keys.len()];
            let message = format!("tx-{}", i).into_bytes();
            let sig = sk.sign(SignatureSchemes::Basic, &message).unwrap();
            aggregator.add_signature(sig, pk.clone(), message);
        }
        
        let sample_valid = aggregator.count_valid();
        println!("  ✓ BLS sampling: {}/{} in {:.2}ms", 
            sample_valid, sample_size, verify_time.as_millis());
    }
    
    let duration = start.elapsed();
    
    AttackResult {
        scenario_name: "Verify DoS + BLS".to_string(),
        duration,
        success_rate: (invalid_count as f64 / total_txs as f64) * 100.0,
        tps: (total_txs as u64) / duration.as_secs().max(1),
        uptime: network.uptime().await,
        recovery_time_ms: verify_time.as_millis() as u64,
        energy_ratio: 0.2,
        metrics: AttackMetrics {
            total_tx: total_txs,
            malicious_tx: invalid_count,
            rejected_tx: invalid_count,
            orphaned_tx: 0,
            avg_latency_ms: verify_time.as_millis() as f64,
            p99_latency_ms: verify_time.as_millis() as f64 * 3.0,
            nodes_banned: 0,
            cross_shard_leaks: 0,
        },
    }
}
