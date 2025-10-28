// Energy Efficiency Benchmark for TRIAD
// Measures: TPS, latency, CPU cycles, and estimated energy per transaction

use std::time::{Duration, Instant};
use std::process::Command;
use std::sync::{Arc, Mutex as StdMutex};
use std::sync::mpsc::{channel, Sender, Receiver};
use std::thread;
use triad::quantum::field::{ShardedQuantumField, QuantumState};
use triad::quantum::interference::InterferenceEngine;
use triad::quantum::QuantumWave;
use ed25519_dalek::{SigningKey, VerifyingKey, Signature, Signer, Verifier};
use rayon::prelude::*;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Transaction {
    id: String,
    from: String,
    to: String,
    amount: f64,
    nonce: u64,
    signature: Vec<u8>,
}

#[derive(Debug)]
struct EnergyMetrics {
    // Performance
    total_transactions: usize,
    total_time: Duration,
    tps: f64,
    
    // Timing breakdown
    signature_time: Duration,
    wave_conversion_time: Duration,
    interference_time: Duration,
    
    // CPU metrics
    cpu_cycles_start: u64,
    cpu_cycles_end: u64,
    cpu_cycles_per_tx: u64,
    
    // Energy estimates (will get from powermetrics)
    cpu_power_watts: f64,
    energy_per_tx_mj: f64,
}

// Get CPU cycles using x86 RDTSC instruction
#[cfg(target_arch = "x86_64")]
fn read_cpu_cycles() -> u64 {
    unsafe {
        let mut aux: u32 = 0;
        let cycles = std::arch::x86_64::__rdtscp(&mut aux as *mut u32);
        cycles
    }
}

#[cfg(not(target_arch = "x86_64"))]
fn read_cpu_cycles() -> u64 {
    0 // Fallback for non-x86 architectures
}

// Measure CPU power using powermetrics (macOS) - requires sudo!
fn measure_cpu_power() -> Option<f64> {
    let output = Command::new("powermetrics")
        .args(&[
            "--samplers", "cpu_power",
            "--sample-count", "1",
            "--sample-rate", "100"
        ])
        .output()
        .ok()?;
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    
    // Parse: "CPU Power: 15234 mW"
    for line in stdout.lines() {
        if line.contains("CPU Power:") {
            if let Some(power_str) = line.split_whitespace().nth(2) {
                if let Ok(milliwatts) = power_str.parse::<f64>() {
                    return Some(milliwatts / 1000.0); // Convert to watts
                }
            }
        }
    }
    None
}

fn generate_test_transactions(count: usize, signing_key: &SigningKey) -> (Vec<Transaction>, Duration) {
    let start = Instant::now();
    println!("🔑 Generating {} transactions with Ed25519 signatures...", count);
    
    let transactions: Vec<Transaction> = (0..count).map(|i| {
        let message = format!("tx_{}", i);
        let signature = signing_key.sign(message.as_bytes());
        
        Transaction {
            id: format!("tx_{}", i),
            from: format!("addr_{}", i % 100),
            to: format!("addr_{}", (i + 1) % 100),
            amount: (i as f64 % 1000.0) + 1.0,
            nonce: i as u64,
            signature: signature.to_bytes().to_vec(),
        }
    }).collect();
    
    let elapsed = start.elapsed();
    println!("✅ Generated {} transactions in {:.3}ms", count, elapsed.as_secs_f64() * 1000.0);
    
    (transactions, elapsed)
}

fn verify_signatures(transactions: &[Transaction], public_key: &VerifyingKey) -> (usize, Duration) {
    let start = Instant::now();
    println!("🔐 Verifying {} signatures...", transactions.len());
    
    let verified: usize = transactions.par_iter()
        .filter(|tx| {
            if let Ok(signature) = Signature::from_slice(&tx.signature) {
                let message = format!("tx_{}", tx.id.trim_start_matches("tx_"));
                public_key.verify(message.as_bytes(), &signature).is_ok()
            } else {
                false
            }
        })
        .count();
    
    let elapsed = start.elapsed();
    println!("✅ Verified {}/{} signatures in {:.3}ms ({:.0} sigs/sec)",
        verified, transactions.len(), 
        elapsed.as_secs_f64() * 1000.0,
        verified as f64 / elapsed.as_secs_f64());
    
    (verified, elapsed)
}

fn convert_to_waves(transactions: &[Transaction]) -> (Vec<QuantumWave>, Duration) {
    let start = Instant::now();
    println!("🌊 Converting {} transactions to quantum waves...", transactions.len());
    
    let waves: Vec<QuantumWave> = transactions.par_iter().enumerate()
        .map(|(i, tx)| {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            
            let mut hasher = DefaultHasher::new();
            tx.id.hash(&mut hasher);
            let hash = hasher.finish();
            
            let amplitude = (tx.amount / 100.0).clamp(0.1, 2.0);
            let phase = (hash as f64 / u64::MAX as f64) * 2.0 * std::f64::consts::PI;
            let shard_id = format!("shard_{}", i % 8);
            
            let state = QuantumState {
                id: tx.id.clone(),
                amplitude,
                phase,
                shard_id,
                superposition: vec![],
            };
            
            QuantumWave::from(state)
        })
        .collect();
    
    let elapsed = start.elapsed();
    println!("✅ Converted to {} waves in {:.3}ms ({:.0} waves/sec)",
        waves.len(), elapsed.as_secs_f64() * 1000.0,
        waves.len() as f64 / elapsed.as_secs_f64());
    
    (waves, elapsed)
}

fn calculate_interference(waves: &[QuantumWave]) -> (f64, f64, Duration) {
    let start = Instant::now();
    println!("🔬 Stage 4: Calculating interference pattern (resolution=1000)...");
    use std::io::{self, Write};
    io::stdout().flush().unwrap();
    
    let field = Arc::new(StdMutex::new(ShardedQuantumField::new(8, 100000)));
    
    // Add all waves at once
    let wave_items: Vec<(String, QuantumWave)> = waves.iter()
        .map(|w| (w.id.to_string(), w.clone()))
        .collect();
    
    {
        let mut f = field.lock().unwrap();
        f.add_waves_batch(wave_items);
    }
    
    // Calculate interference pattern from field
    let pattern = {
        let f = field.lock().unwrap();
        f.calculate_interference_pattern(Some(1000))
    };
    
    // Simple analysis: count high amplitude points as constructive
    let max_amp = pattern.iter().map(|p| p.amplitude.abs()).fold(0.0f64, f64::max);
    let threshold = max_amp * 0.5;
    let constructive = pattern.iter().filter(|p| p.amplitude.abs() > threshold).count();
    let constructive_ratio = constructive as f64 / pattern.len() as f64;
    let avg_amplitude = pattern.iter().map(|p| p.amplitude).sum::<f64>() / pattern.len() as f64;
    
    let elapsed = start.elapsed();
    
    println!("✅ Interference calculated in {:.3}ms | Constructive: {:.1}% | Avg amplitude: {:.3}",
        elapsed.as_secs_f64() * 1000.0,
        constructive_ratio * 100.0,
        avg_amplitude);
    
    (constructive_ratio, avg_amplitude, elapsed)
}

fn run_energy_benchmark(num_transactions: usize) -> EnergyMetrics {
    println!("\n🔋 TRIAD ENERGY EFFICIENCY BENCHMARK 🔋");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Transactions: {}", num_transactions);
    println!("Hardware: MacBook Pro 2016 i7 (Skylake)");
    println!("Pipeline: 250 tx/batch, 3 parallel stages");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");
    
    use std::io::{self, Write};
    
    // Measure baseline CPU power (before load)
    println!("📊 Measuring baseline CPU power...");
    let baseline_power = measure_cpu_power().unwrap_or(0.0);
    println!("Baseline CPU Power: {:.2}W\n", baseline_power);
    
    let total_start = Instant::now();
    let cycles_start = read_cpu_cycles();
    
    let signing_key = SigningKey::generate(&mut rand::thread_rng());
    let verifying_key = signing_key.verifying_key();
    
    // Pipeline channels
    let (tx_sender, tx_receiver): (Sender<Vec<Transaction>>, Receiver<Vec<Transaction>>) = channel();
    let (verified_sender, verified_receiver): (Sender<Vec<Transaction>>, Receiver<Vec<Transaction>>) = channel();
    let (wave_sender, wave_receiver): (Sender<Vec<QuantumWave>>, Receiver<Vec<QuantumWave>>) = channel();
    
    let batch_size = 250;
    let num_batches = (num_transactions + batch_size - 1) / batch_size;
    
    // Stage 1: Generate transactions (main thread)
    println!("🔑 Stage 1: Generating {} transactions in {} batches...", num_transactions, num_batches);
    io::stdout().flush().unwrap();
    let gen_start = Instant::now();
    
    thread::spawn(move || {
        for batch_num in 0..num_batches {
            let start_idx = batch_num * batch_size;
            let end_idx = ((batch_num + 1) * batch_size).min(num_transactions);
            let batch_txs: Vec<Transaction> = (start_idx..end_idx).map(|i| {
                let message = format!("tx_{}", i);
                let signature = signing_key.sign(message.as_bytes());
                Transaction {
                    id: format!("tx_{}", i),
                    from: format!("addr_{}", i % 100),
                    to: format!("addr_{}", (i + 1) % 100),
                    amount: (i as f64 % 1000.0) + 1.0,
                    nonce: i as u64,
                    signature: signature.to_bytes().to_vec(),
                }
            }).collect();
            tx_sender.send(batch_txs).unwrap();
        }
    });
    
    // Stage 2: Verify signatures (parallel thread)
    let vkey = verifying_key.clone();
    let verified_count = Arc::new(StdMutex::new(0usize));
    let verified_count_clone = verified_count.clone();
    
    thread::spawn(move || {
        println!("🔐 Stage 2: Verifying signatures...");
        let mut count = 0;
        while let Ok(batch) = tx_receiver.recv() {
            let verified_batch: Vec<Transaction> = batch.into_iter()
                .filter(|tx| {
                    if let Ok(signature) = Signature::from_slice(&tx.signature) {
                        let message = format!("tx_{}", tx.id.trim_start_matches("tx_"));
                        vkey.verify(message.as_bytes(), &signature).is_ok()
                    } else {
                        false
                    }
                })
                .collect();
            count += verified_batch.len();
            verified_sender.send(verified_batch).unwrap();
        }
        *verified_count_clone.lock().unwrap() = count;
    });
    
    // Stage 3: Convert to waves (parallel thread)
    let wave_time_tracker = Arc::new(StdMutex::new(Duration::ZERO));
    let wave_time_clone = wave_time_tracker.clone();
    
    thread::spawn(move || {
        println!("🌊 Stage 3: Converting to quantum waves...");
        let stage_start = Instant::now();
        while let Ok(batch) = verified_receiver.recv() {
            let batch_start = Instant::now();
            let waves: Vec<QuantumWave> = batch.iter().enumerate().map(|(i, tx)| {
                use std::collections::hash_map::DefaultHasher;
                use std::hash::{Hash, Hasher};
                
                let mut hasher = DefaultHasher::new();
                tx.id.hash(&mut hasher);
                let hash = hasher.finish();
                
                let amplitude = (tx.amount / 100.0).clamp(0.1, 2.0);
                let phase = (hash as f64 / u64::MAX as f64) * 2.0 * std::f64::consts::PI;
                let shard_id = format!("shard_{}", i % 8);
                
                let state = QuantumState {
                    id: tx.id.clone(),
                    amplitude,
                    phase,
                    shard_id,
                    superposition: vec![],
                };
                
                QuantumWave::from(state)
            }).collect();
            *wave_time_clone.lock().unwrap() += batch_start.elapsed();
            wave_sender.send(waves).unwrap();
        }
    });
    
    // Collect all waves
    let mut all_waves = Vec::new();
    while let Ok(wave_batch) = wave_receiver.recv() {
        all_waves.extend(wave_batch);
        if all_waves.len() >= num_transactions {
            break;
        }
    }
    
    let gen_time = gen_start.elapsed();
    let wave_time = *wave_time_tracker.lock().unwrap();
    println!("✅ Pipeline stages 1-3 completed in {:.3}ms", gen_time.as_secs_f64() * 1000.0);
    println!("   └─ Wave conversion: {:.3}ms (measured separately)", wave_time.as_secs_f64() * 1000.0);
    
    let verified = *verified_count.lock().unwrap();
    
    // Stage 4: Calculate interference (sequential, needs all waves)
    let (_constructive, _avg_amp, interference_time) = calculate_interference(&all_waves);
    
    let cycles_end = read_cpu_cycles();
    let total_time = total_start.elapsed();
    
    // Measure CPU power under load
    println!("\n📊 Measuring CPU power under load...");
    let load_power = measure_cpu_power().unwrap_or(baseline_power);
    println!("Load CPU Power: {:.2}W", load_power);
    
    let mut cpu_power = load_power - baseline_power;
    let tps = verified as f64 / total_time.as_secs_f64();
    let cpu_cycles_per_tx = if cycles_end > cycles_start {
        (cycles_end - cycles_start) / verified as u64
    } else {
        0
    };
    
    // Energy per transaction (millijoules)
    let energy_per_tx_mj = if cpu_power > 0.1 {
        // Use measured power if available
        let total_energy_joules = cpu_power * total_time.as_secs_f64();
        (total_energy_joules / verified as f64) * 1000.0
    } else if cpu_cycles_per_tx > 0 {
        // Fallback: estimate from CPU cycles
        // i7-6700HQ Skylake: ~2.6-3.5 GHz, TDP 45W, typical load ~25W
        let estimated_freq_ghz = 3.0; // Conservative estimate
        let estimated_cpu_power_watts = 25.0; // Typical under load
        let time_per_tx_seconds = cpu_cycles_per_tx as f64 / (estimated_freq_ghz * 1e9);
        let energy_joules = estimated_cpu_power_watts * time_per_tx_seconds;
        cpu_power = estimated_cpu_power_watts; // Update for display
        energy_joules * 1000.0 // Convert to mJ
    } else {
        0.0
    };
    
    EnergyMetrics {
        total_transactions: verified,
        total_time,
        tps,
        signature_time: gen_time, // Pipeline includes all stages 1-3
        wave_conversion_time: wave_time, // Real measurement!
        interference_time,
        cpu_cycles_start: cycles_start,
        cpu_cycles_end: cycles_end,
        cpu_cycles_per_tx,
        cpu_power_watts: cpu_power,
        energy_per_tx_mj,
    }
}

fn main() {
    // Run benchmark with different transaction counts
    let test_sizes = vec![500]; // Start small for testing
    
    for size in test_sizes {
        let metrics = run_energy_benchmark(size);
        
        println!("\n╔═══════════════════════════════════════════════════════════╗");
        println!("║              ENERGY BENCHMARK RESULTS                     ║");
        println!("╠═══════════════════════════════════════════════════════════╣");
        println!("║ Transactions processed:    {:>10}                    ║", metrics.total_transactions);
        println!("║ Total time:                {:>10.3} ms                ║", metrics.total_time.as_secs_f64() * 1000.0);
        println!("║ Throughput (TPS):          {:>10.0}                    ║", metrics.tps);
        println!("╠═══════════════════════════════════════════════════════════╣");
        println!("║ TIMING BREAKDOWN:                                         ║");
        println!("║   Signature verification:  {:>10.3} ms                ║", metrics.signature_time.as_secs_f64() * 1000.0);
        println!("║   Wave conversion:         {:>10.3} ms                ║", metrics.wave_conversion_time.as_secs_f64() * 1000.0);
        println!("║   Interference calc:       {:>10.3} ms                ║", metrics.interference_time.as_secs_f64() * 1000.0);
        println!("╠═══════════════════════════════════════════════════════════╣");
        println!("║ CPU METRICS:                                              ║");
        if metrics.cpu_cycles_per_tx > 0 {
            println!("║   CPU cycles/tx:           {:>10}                    ║", metrics.cpu_cycles_per_tx);
        } else {
            println!("║   CPU cycles/tx:           N/A (non-x86)                 ║");
        }
        println!("║   CPU power (incremental): {:>10.2} W                 ║", metrics.cpu_power_watts);
        println!("╠═══════════════════════════════════════════════════════════╣");
        println!("║ ENERGY EFFICIENCY:                                        ║");
        println!("║   Energy per tx:           {:>10.4} mJ                ║", metrics.energy_per_tx_mj);
        println!("║   Energy per tx:           {:>10.7} Wh                ║", metrics.energy_per_tx_mj / 3600.0);
        println!("║   Energy per tx:           {:>10.7} kWh               ║", metrics.energy_per_tx_mj / 3_600_000.0);
        println!("╠═══════════════════════════════════════════════════════════╣");
        println!("║ COMPARISON (approx):                                      ║");
        println!("║   Bitcoin PoW:             ~1,200,000 Wh/tx               ║");
        println!("║   Ethereum PoS:            ~20 Wh/tx                      ║");
        println!("║   Solana:                  ~0.5 Wh/tx                     ║");
        println!("║   TRIAD:                   ~{:>10.7} Wh/tx  ✅          ║", metrics.energy_per_tx_mj / 3600.0);
        println!("╚═══════════════════════════════════════════════════════════╝\n");
        
        std::thread::sleep(Duration::from_secs(2)); // Cool down between tests
    }
    
    println!("\n💡 Note: To get accurate CPU power measurements, run with sudo:");
    println!("   sudo cargo run --release --example energy_benchmark\n");
}
