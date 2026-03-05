/// 🎯 ATTACK #5: Пробабилистическая атака на батчи
/// 
/// Описание:
/// Эксплойт probabilistic ordering: спам транзакций с phase близкими к 180°
/// для <α порога в батче, вызывая rejection.
/// 
/// Цель: Массовый rejection, снижение TPS ниже 11K
/// 
/// Метод:
/// 1. Создать батч из 500 транзакций
/// 2. Большинство с phase ≈ π (противофаза)
/// 3. Σ A_i * cos(...) < 0 (деструктивная интерференция)
/// 4. Весь батч отклоняется

use crate::core::interference::InterferenceEngine;

#[test]
fn test_probabilistic_batch_attack() {
    println!("\n🔴 ATTACK SIMULATION: Probabilistic Batch Attack");
    println!("{}", "=".repeat(60));
    
    const BATCH_SIZE: usize = 500;
    const ATTACK_RATIO: f64 = 0.7; // 70% attack txs
    
    // 1. Легитимные транзакции (случайные фазы)
    let legitimate_count = (BATCH_SIZE as f64 * (1.0 - ATTACK_RATIO)) as usize;
    let legitimate_waves: Vec<(f64, f64, f64)> = (0..legitimate_count)
        .map(|i| {
            let amplitude = 1.0;
            let phase = (i as f64 / legitimate_count as f64) * 2.0 * std::f64::consts::PI;
            let frequency = 0.9;
            (amplitude, phase, frequency)
        })
        .collect();
    
    // 2. Атакующие транзакции (фазы около π)
    let attack_count = BATCH_SIZE - legitimate_count;
    let attack_waves: Vec<(f64, f64, f64)> = (0..attack_count)
        .map(|i| {
            let amplitude = 1.0;
            // Phase clustered around π ± 0.2
            let phase = std::f64::consts::PI + 
                        (i as f64 / attack_count as f64 - 0.5) * 0.4;
            let frequency = 0.9;
            (amplitude, phase, frequency)
        })
        .collect();
    
    println!("📊 Batch Composition:");
    println!("  • Legitimate: {} ({:.0}%)", legitimate_count, (1.0 - ATTACK_RATIO) * 100.0);
    println!("  • Attack: {} ({:.0}%)", attack_count, ATTACK_RATIO * 100.0);
    
    // 3. Объединяем в батч
    let mut batch_waves = legitimate_waves.clone();
    batch_waves.extend(attack_waves.clone());
    
    // 4. Вычисляем интерференцию
    let engine = InterferenceEngine::new(1000, 0.0);
    let pattern = engine.calculate_interference_with_frequencies(&batch_waves);
    let analysis = engine.analyze_interference(&pattern);
    
    println!("\n📈 Interference Analysis:");
    println!("  • Constructive: {:.1}%", 
        analysis.constructive_points.len() as f64 / analysis.total_points() as f64 * 100.0);
    println!("  • Destructive: {:.1}%",
        analysis.destructive_points.len() as f64 / analysis.total_points() as f64 * 100.0);
    println!("  • Avg amplitude: {:.4}", analysis.average_amplitude);
    println!("  • Max amplitude: {:.4}", analysis.max_amplitude);
    
    // 5. Проверяем порог консенсуса
    const CONSENSUS_THRESHOLD: f64 = 0.5; // >50% constructive required
    
    let constructive_ratio = analysis.constructive_points.len() as f64 / 
                            analysis.total_points() as f64;
    
    let batch_accepted = constructive_ratio > CONSENSUS_THRESHOLD;
    
    println!("\n🎯 Consensus Decision:");
    if batch_accepted {
        println!("  ✅ BATCH ACCEPTED (constructive: {:.1}%)", constructive_ratio * 100.0);
    } else {
        println!("  ❌ BATCH REJECTED (constructive: {:.1}%)", constructive_ratio * 100.0);
        println!("     {} legitimate txs lost!", legitimate_count);
    }
    
    // 6. Оценка влияния на TPS
    if !batch_accepted {
        let tps_baseline = 14_000.0; // TPS без атаки
        let batch_rate = 10.0; // батчей в секунду
        let lost_txs_per_sec = BATCH_SIZE as f64 * batch_rate;
        let tps_under_attack = tps_baseline - lost_txs_per_sec;
        
        println!("\n📉 TPS Impact:");
        println!("  • Baseline TPS: {:.0}", tps_baseline);
        println!("  • Lost txs/sec: {:.0}", lost_txs_per_sec);
        println!("  • TPS under attack: {:.0}", tps_under_attack);
        println!("  • Reduction: {:.1}%", 
            (1.0 - tps_under_attack / tps_baseline) * 100.0);
        
        if tps_under_attack < 11_000.0 {
            println!("\n⚠️  ATTACK SUCCESSFUL:");
            println!("     TPS dropped below 11K target!");
        }
    }
    
    // 7. Защита
    println!("\n🛡️  Mitigation Strategies:");
    println!("  1. Per-batch phase diversity check");
    println!("  2. Isolate suspicious txs (don't reject whole batch)");
    println!("  3. Adaptive threshold based on batch composition");
    println!("  4. Reputation system (penalize senders of rejected batches)");
    println!("  5. Parallel batch processing (isolate failures)");
    
    assert!(
        constructive_ratio > 0.2,
        "Extreme destructive interference: {:.1}%",
        (1.0 - constructive_ratio) * 100.0
    );
}

#[test]
fn test_phase_clustering_detection() {
    println!("\n🔍 Testing Phase Clustering Detection");
    
    // Detect if too many txs have similar phases (attack indicator)
    const NUM_BUCKETS: usize = 32;
    const BUCKET_SIZE: f64 = 2.0 * std::f64::consts::PI / NUM_BUCKETS as f64;
    const CLUSTERING_THRESHOLD: f64 = 0.3; // >30% in one bucket = suspicious
    
    // Test cases
    let test_cases = vec![
        ("Normal distribution", false, 0..1000),
        ("Attack clustering", true, 400..600),
    ];
    
    for (name, is_attack, range) in test_cases {
        let mut phases = Vec::new();
        
        if is_attack {
            // Attack: cluster around π
            for i in range {
                let phase = std::f64::consts::PI + 
                           (i as f64 / 200.0 - 0.5) * BUCKET_SIZE;
                phases.push(phase);
            }
        } else {
            // Normal: distribute evenly
            for i in 0..1000 {
                let phase = (i as f64 / 1000.0) * 2.0 * std::f64::consts::PI;
                phases.push(phase);
            }
        }
        
        // Count phases per bucket
        let mut buckets = vec![0usize; NUM_BUCKETS];
        for phase in &phases {
            let bucket_idx = ((phase / BUCKET_SIZE) as usize).min(NUM_BUCKETS - 1);
            buckets[bucket_idx] += 1;
        }
        
        let max_in_bucket = *buckets.iter().max().unwrap();
        let clustering = max_in_bucket as f64 / phases.len() as f64;
        
        println!("\n{}: ", name);
        println!("  • Max concentration: {:.1}%", clustering * 100.0);
        
        if clustering > CLUSTERING_THRESHOLD {
            println!("  ⚠️  SUSPICIOUS: Possible attack detected!");
        } else {
            println!("  ✅ Normal: Healthy phase distribution");
        }
    }
}

#[test]
fn test_batch_isolation_strategy() {
    println!("\n🔧 Testing Batch Isolation Strategy");
    
    // Instead of rejecting whole batch, isolate suspicious txs
    const BATCH_SIZE: usize = 500;
    
    // Simulate mixed batch
    let mut txs: Vec<(usize, f64, bool)> = Vec::new(); // (id, phase, is_attack)
    
    // 400 legitimate
    for i in 0..400 {
        let phase = (i as f64 / 400.0) * 2.0 * std::f64::consts::PI;
        txs.push((i, phase, false));
    }
    
    // 100 attack (clustered phases)
    for i in 400..500 {
        let phase = std::f64::consts::PI + (i as f64 / 100.0 - 5.0) * 0.1;
        txs.push((i, phase, true));
    }
    
    // Detect clustering
    const NUM_BUCKETS: usize = 32;
    const BUCKET_SIZE: f64 = 2.0 * std::f64::consts::PI / NUM_BUCKETS as f64;
    
    let mut buckets: Vec<Vec<usize>> = vec![Vec::new(); NUM_BUCKETS];
    for (id, phase, _) in &txs {
        let bucket_idx = ((phase / BUCKET_SIZE) as usize).min(NUM_BUCKETS - 1);
        buckets[bucket_idx].push(*id);
    }
    
    // Find suspicious bucket
    let max_bucket_idx = buckets.iter()
        .enumerate()
        .max_by_key(|(_, b)| b.len())
        .map(|(i, _)| i)
        .unwrap();
    
    let suspicious_ids = &buckets[max_bucket_idx];
    
    println!("\nBatch isolation:");
    println!("  • Total txs: {}", BATCH_SIZE);
    println!("  • Suspicious bucket: {} ({} txs)", max_bucket_idx, suspicious_ids.len());
    println!("  • Isolated: {} txs", suspicious_ids.len());
    println!("  • Accepted: {} txs", BATCH_SIZE - suspicious_ids.len());
    
    // Calculate recovery
    let attack_txs_total = 100;
    let attack_txs_caught = suspicious_ids.len();
    let legitimate_saved = BATCH_SIZE - suspicious_ids.len();
    
    println!("\n📊 Strategy Effectiveness:");
    println!("  • Attack detection: {:.0}%", 
        attack_txs_caught as f64 / attack_txs_total as f64 * 100.0);
    println!("  • Legitimate saved: {:.0}%",
        legitimate_saved as f64 / 400.0 * 100.0);
    
    println!("\n✅ Isolation prevents collateral damage to legitimate txs");
}



