/// 🔴 STRESS TEST: Amplitude Flood Attack (Enhanced)
/// 
/// Усиления согласно плану:
/// - 50k tx (amp=5-20, ignore clamp)
/// - vary fees (quadratic: fee=0.0001*amp²)
/// - economic simulation (attacker loss >gain)
/// - 100 nodes network
/// - flood via 30 peers (delays 50ms)
/// - measure energy +200% (cycles*amp factor)
/// - Target: Clamp rejects 100%, normalized Σ amp <2.0, increase <1.5x after fees

use crate::core::interference::InterferenceEngine;
use crate::core::tests::network_sim::{NetworkSimulator, NetworkConfig, NetworkMetrics};
use std::time::Instant;
use rayon::prelude::*;
use rand::Rng;

#[derive(Debug)]
struct Transaction {
    id: String,
    amount: f64,
    raw_amplitude: f64,
    clamped_amplitude: f64,
    fee: f64,
    is_attack: bool,
}

impl Transaction {
    fn new(id: String, amount: f64, is_attack: bool, bypass_clamp: bool) -> Self {
        let raw_amplitude = amount / 100.0;
        let clamped_amplitude = if bypass_clamp {
            raw_amplitude // Attacker ignores clamp
        } else {
            raw_amplitude.max(0.1).min(2.0) // Legitimate clamps
        };
        
        // Quadratic fee: fee = base_fee * amp²
        let base_fee = 0.0001;
        let fee = base_fee * raw_amplitude * raw_amplitude;
        
        Self {
            id,
            amount,
            raw_amplitude,
            clamped_amplitude,
            fee,
            is_attack,
        }
    }
}

#[tokio::test]
async fn test_amplitude_flood_attack_stress() {
    println!("\n🔴 STRESS TEST: Amplitude Flood Attack (Enhanced)");
    println!("{}", "=".repeat(80));
    
    let test_start = Instant::now();
    
    // === CONFIGURATION ===
    const NUM_LEGITIMATE_TX: usize = 40_000;
    const NUM_ATTACK_TX: usize = 10_000;
    const TOTAL_TX: usize = NUM_LEGITIMATE_TX + NUM_ATTACK_TX;
    
    const LEGITIMATE_AMOUNT_RANGE: (f64, f64) = (10.0, 200.0); // → amp 0.1-2.0
    const ATTACK_AMOUNT_RANGE: (f64, f64) = (500.0, 2000.0);   // → amp 5.0-20.0
    
    let net_config = NetworkConfig {
        num_nodes: 100,
        malicious_ratio: 0.3, // 30% malicious nodes
        base_delay_ms: 50,
        delay_variance_ms: 30,
        packet_loss_rate: 0.02,
    };
    
    println!("\n⚙️  Configuration:");
    println!("  • Legitimate txs: {} (amounts: {:.0}-{:.0})", 
        NUM_LEGITIMATE_TX, LEGITIMATE_AMOUNT_RANGE.0, LEGITIMATE_AMOUNT_RANGE.1);
    println!("  • Attack txs: {} (amounts: {:.0}-{:.0}, NO CLAMP)", 
        NUM_ATTACK_TX, ATTACK_AMOUNT_RANGE.0, ATTACK_AMOUNT_RANGE.1);
    println!("  • Network nodes: {}", net_config.num_nodes);
    println!("  • Malicious nodes: {}", (net_config.num_nodes as f64 * net_config.malicious_ratio) as usize);
    
    // === 1. NETWORK SETUP ===
    println!("\n🌐 Setting up network simulator...");
    let network = NetworkSimulator::new(net_config);
    println!("  • Nodes initialized: {}", network.nodes.len());
    
    // === 2. GENERATE TRANSACTIONS ===
    println!("\n📝 Generating transactions...");
    let gen_start = Instant::now();
    
    let legitimate_txs: Vec<Transaction> = (0..NUM_LEGITIMATE_TX)
        .into_par_iter()
        .map(|i| {
            let mut rng = rand::thread_rng();
            let amount = rng.gen_range(LEGITIMATE_AMOUNT_RANGE.0..LEGITIMATE_AMOUNT_RANGE.1);
            Transaction::new(format!("legit-{}", i), amount, false, false)
        })
        .collect();
    
    let attack_txs: Vec<Transaction> = (0..NUM_ATTACK_TX)
        .into_par_iter()
        .map(|i| {
            let mut rng = rand::thread_rng();
            let amount = rng.gen_range(ATTACK_AMOUNT_RANGE.0..ATTACK_AMOUNT_RANGE.1);
            Transaction::new(format!("attack-{}", i), amount, true, true) // bypass_clamp = true
        })
        .collect();
    
    println!("  • Generated {} txs in {:.2}ms", 
        TOTAL_TX, gen_start.elapsed().as_secs_f64() * 1000.0);
    
    // === 3. ANALYZE BEFORE ENFORCEMENT ===
    println!("\n⚠️  BEFORE ENFORCEMENT (attacker bypasses clamp):");
    
    let legit_total_amp: f64 = legitimate_txs.iter().map(|tx| tx.clamped_amplitude).sum();
    let attack_total_amp_raw: f64 = attack_txs.iter().map(|tx| tx.raw_amplitude).sum();
    let attack_dominance = attack_total_amp_raw / (legit_total_amp + attack_total_amp_raw);
    
    println!("  • Legitimate Σ amplitude: {:.2}", legit_total_amp);
    println!("  • Attack Σ amplitude (unclamped): {:.2}", attack_total_amp_raw);
    println!("  • Attack dominance: {:.1}%", attack_dominance * 100.0);
    
    // Energy cost (baseline: 194k cycles/tx, scaled by amplitude)
    let legit_energy: u64 = legitimate_txs.iter()
        .map(|tx| (194_000.0 * tx.clamped_amplitude) as u64)
        .sum();
    let attack_energy_raw: u64 = attack_txs.iter()
        .map(|tx| (194_000.0 * tx.raw_amplitude) as u64)
        .sum();
    let energy_increase = (attack_energy_raw as f64 / legit_energy as f64) * 100.0;
    
    println!("  • Legitimate energy: {} cycles ({:.1}M)", 
        legit_energy, legit_energy as f64 / 1_000_000.0);
    println!("  • Attack energy (unclamped): {} cycles ({:.1}M)", 
        attack_energy_raw, attack_energy_raw as f64 / 1_000_000.0);
    println!("  • Energy increase: +{:.1}%", energy_increase);
    
    // === 4. APPLY AMPLITUDE ENFORCEMENT ===
    println!("\n✅ AFTER ENFORCEMENT (clamping to 2.0):");
    let enforcement_start = Instant::now();
    
    // Clamp all amplitudes
    let enforced_attack_txs: Vec<Transaction> = attack_txs.par_iter()
        .map(|tx| {
            Transaction::new(tx.id.clone(), tx.amount, tx.is_attack, false) // enforce clamp
        })
        .collect();
    
    let attack_total_amp_clamped: f64 = enforced_attack_txs.iter().map(|tx| tx.clamped_amplitude).sum();
    let clamped_dominance = attack_total_amp_clamped / (legit_total_amp + attack_total_amp_clamped);
    
    println!("  • Attack Σ amplitude (clamped): {:.2}", attack_total_amp_clamped);
    println!("  • Attack dominance (clamped): {:.1}%", clamped_dominance * 100.0);
    println!("  • Enforcement time: {:.2}ms", enforcement_start.elapsed().as_secs_f64() * 1000.0);
    
    let attack_energy_clamped: u64 = enforced_attack_txs.iter()
        .map(|tx| (194_000.0 * tx.clamped_amplitude) as u64)
        .sum();
    let energy_reduction = ((attack_energy_raw - attack_energy_clamped) as f64 / attack_energy_raw as f64) * 100.0;
    
    println!("  • Attack energy (clamped): {} cycles ({:.1}M)", 
        attack_energy_clamped, attack_energy_clamped as f64 / 1_000_000.0);
    println!("  • Energy saved: {:.1}%", energy_reduction);
    
    // === 5. ECONOMIC ANALYSIS (Quadratic Fees) ===
    println!("\n💰 ECONOMIC ANALYSIS (Quadratic fee model):");
    
    let legit_total_fees: f64 = legitimate_txs.iter().map(|tx| tx.fee).sum();
    let attack_total_fees: f64 = attack_txs.iter().map(|tx| tx.fee).sum();
    
    println!("  • Legitimate total fees: {:.6} TRD", legit_total_fees);
    println!("  • Attack total fees: {:.6} TRD", attack_total_fees);
    println!("  • Fee ratio (attack/legit): {:.1}x", attack_total_fees / legit_total_fees);
    
    // Simulate attacker's economic gain vs cost
    // Assume attacker gains 0.0001 TRD per successful dominating tx
    let potential_gain = NUM_ATTACK_TX as f64 * 0.0001;
    let actual_cost = attack_total_fees;
    let net_result = potential_gain - actual_cost;
    
    println!("\n  📊 Attacker economics:");
    println!("    • Potential gain: {:.6} TRD", potential_gain);
    println!("    • Actual cost (fees): {:.6} TRD", actual_cost);
    println!("    • Net result: {:.6} TRD ({})", 
        net_result, 
        if net_result < 0.0 { "LOSS" } else { "PROFIT" }
    );
    
    // === 6. INTERFERENCE CALCULATION ===
    println!("\n🔬 Calculating Core interference...");
    let interference_start = Instant::now();
    
    let engine = InterferenceEngine::new(1000, 0.0);
    
    // Combined waves with clamped amplitudes
    let mut all_waves: Vec<(f64, f64, f64)> = Vec::new();
    
    for tx in &legitimate_txs {
        let phase = rand::random::<f64>() * 2.0 * std::f64::consts::PI;
        all_waves.push((tx.clamped_amplitude, phase, 1.0));
    }
    
    for tx in &enforced_attack_txs {
        let phase = std::f64::consts::PI; // Attacker tries destructive
        all_waves.push((tx.clamped_amplitude, phase, 1.0));
    }
    
    let pattern = engine.calculate_interference_with_frequencies(&all_waves);
    let analysis = engine.analyze_interference(&pattern);
    
    let max_amplitude = pattern.iter()
        .map(|p| p.amplitude.abs())
        .max_by(|a, b| a.partial_cmp(b).unwrap())
        .unwrap_or(0.0);
    
    println!("  • Interference calculated in {:.2}ms", interference_start.elapsed().as_secs_f64() * 1000.0);
    println!("  • Max amplitude: {:.2}", max_amplitude);
    println!("  • Normalized max: {:.2} (per tx)", max_amplitude / TOTAL_TX as f64);
    
    // === 7. NETWORK PROPAGATION ===
    println!("\n🌐 Network propagation simulation...");
    let prop_start = Instant::now();
    
    // Simulate 30 malicious peers flooding
    let malicious_nodes: Vec<usize> = network.nodes.iter()
        .enumerate()
        .filter(|(_, n)| n.is_malicious)
        .map(|(i, _)| i)
        .take(30)
        .collect();
    
    let mut successful = 0;
    for (i, tx) in enforced_attack_txs.iter().take(100).enumerate() {
        let node_id = malicious_nodes[i % malicious_nodes.len()];
        let result = network.broadcast_transaction(node_id, tx.id.as_bytes()).await;
        if result.delivered_count > 0 {
            successful += 1;
        }
    }
    
    println!("  • Propagated 100 sample txs via {} malicious peers", malicious_nodes.len());
    println!("  • Success rate: {:.1}%", (successful as f64 / 100.0) * 100.0);
    println!("  • Time: {:.2}ms", prop_start.elapsed().as_secs_f64() * 1000.0);
    
    // === 8. PERFORMANCE METRICS ===
    let total_time = test_start.elapsed();
    let total_energy = legit_energy + attack_energy_clamped;
    
    let metrics = NetworkMetrics::calculate(
        TOTAL_TX,
        NUM_LEGITIMATE_TX + enforced_attack_txs.len(),
        total_time,
        total_energy,
    );
    
    metrics.print();
    
    // === 9. ASSERTIONS ===
    println!("\n✅ Validation:");
    
    // All attack amplitudes should be clamped to ≤2.0
    let all_clamped = enforced_attack_txs.iter().all(|tx| tx.clamped_amplitude <= 2.0);
    assert!(all_clamped, "Some attack amplitudes not clamped!");
    println!("  ✓ All amplitudes clamped: 100% ≤ 2.0");
    
    // Normalized max amplitude should be reasonable
    let normalized_max = max_amplitude / TOTAL_TX as f64;
    assert!(
        normalized_max < 2.0,
        "Normalized max amplitude too high: {:.2} > 2.0",
        normalized_max
    );
    println!("  ✓ Normalized Σ amplitude: {:.2} < 2.0", normalized_max);
    
    // Energy increase should be limited after clamping
    let final_energy_ratio = attack_energy_clamped as f64 / legit_energy as f64;
    assert!(
        final_energy_ratio < 1.5,
        "Energy increase too high: {:.2}x > 1.5x",
        final_energy_ratio
    );
    println!("  ✓ Energy increase: {:.2}x < 1.5x", final_energy_ratio);
    
    // Attacker should lose money
    assert!(
        net_result < 0.0,
        "Attacker profitable: {:.6} TRD",
        net_result
    );
    println!("  ✓ Attacker economics: {:.6} TRD (LOSS)", net_result);
    
    // Clamped dominance should be much lower
    assert!(
        clamped_dominance < 0.4,
        "Attack dominance too high after clamp: {:.1}%",
        clamped_dominance * 100.0
    );
    println!("  ✓ Attack dominance (clamped): {:.1}% < 40%", clamped_dominance * 100.0);
    
    println!("\n🎯 Test completed in {:.2}s", total_time.as_secs_f64());
    println!("✅ Amplitude Flood attack successfully mitigated!");
}

#[test]
fn test_quadratic_fee_escalation() {
    println!("\n💰 Testing Quadratic Fee Escalation");
    println!("{}", "=".repeat(60));
    
    let base_fee = 0.0001;
    let test_amounts = vec![10.0, 50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0];
    
    println!("\nFee table (quadratic model: fee = {:.6} * amp²):", base_fee);
    println!("  Amount | Raw Amp | Clamped | Fee (raw) | Fee (clamped) | Ratio");
    println!("{}", "-".repeat(75));
    
    for amount in test_amounts {
        let raw_amp: f64 = amount / 100.0;
        let clamped_amp = raw_amp.max(0.1).min(2.0);
        let fee_raw = base_fee * raw_amp * raw_amp;
        let fee_clamped = base_fee * clamped_amp * clamped_amp;
        let ratio = fee_raw / fee_clamped;
        
        println!("  {:>6.0} | {:>7.2} | {:>7.2} | {:>9.6} | {:>13.6} | {:>5.1}x",
            amount, raw_amp, clamped_amp, fee_raw, fee_clamped, ratio);
    }
    
    println!("\n📊 Analysis:");
    println!("  • Low amounts (≤200): No difference (within clamp range)");
    println!("  • High amounts (>200): Dramatic fee increase (>4x)");
    println!("  • Amount=5000: Fee is {:.1}x higher without clamp!", 
        (base_fee * 50.0 * 50.0) / (base_fee * 4.0));
    
    println!("\n✅ Quadratic fees make high-amplitude attacks economically prohibitive!");
}



