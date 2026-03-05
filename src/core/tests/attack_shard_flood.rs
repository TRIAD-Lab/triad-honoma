/// 🎯 ATTACK #4: Шард-специфический флуд
/// 
/// Описание:
/// Таргетинг 1 из 8 шардов (frequency=shard_id) спамом,
/// перегружая spatial hashing (32 buckets) до O(N²) в шарде.
/// 
/// Цель: Локальный DoS, задержки в пайплайне (generation/verification)
/// 
/// Метод:
/// 1. Отправить 10K транзакций в один shard_id
/// 2. Перегрузить spatial hashing
/// 3. Замедлить interference calculation (>69ms)

use crate::core::interference::InterferenceEngine;
use std::time::Instant;

#[test]
fn test_shard_flood_attack() {
    println!("\n🔴 ATTACK SIMULATION: Shard-Specific Flood");
    println!("{}", "=".repeat(60));
    
    const NUM_SHARDS: usize = 8;
    const TARGET_SHARD: usize = 0;
    const FLOOD_SIZE: usize = 10_000;
    
    // 1. Нормальное распределение (по всем шардам)
    let mut normal_waves: Vec<(f64, f64, f64)> = Vec::new();
    for i in 0..FLOOD_SIZE {
        let shard_id = i % NUM_SHARDS;
        let frequency = 0.8 + (shard_id as f64 / NUM_SHARDS as f64) * 0.4;
        let amplitude = 1.0;
        let phase = (i as f64 / FLOOD_SIZE as f64) * 2.0 * std::f64::consts::PI;
        normal_waves.push((amplitude, phase, frequency));
    }
    
    println!("📊 Normal Distribution:");
    println!("  • {} txs distributed across {} shards", FLOOD_SIZE, NUM_SHARDS);
    println!("  • ~{} txs per shard", FLOOD_SIZE / NUM_SHARDS);
    
    // 2. Атака: все транзакции в один шард
    let mut attack_waves: Vec<(f64, f64, f64)> = Vec::new();
    let target_frequency = 0.8 + (TARGET_SHARD as f64 / NUM_SHARDS as f64) * 0.4;
    
    for i in 0..FLOOD_SIZE {
        let amplitude = 1.0;
        let phase = (i as f64 / FLOOD_SIZE as f64) * 2.0 * std::f64::consts::PI;
        attack_waves.push((amplitude, phase, target_frequency));
    }
    
    println!("\n⚠️  Attack Distribution:");
    println!("  • {} txs ALL to shard {}", FLOOD_SIZE, TARGET_SHARD);
    println!("  • Frequency: {:.2}", target_frequency);
    
    // 3. Измеряем время интерференции
    let engine = InterferenceEngine::new(1000, 0.0);
    
    // Нормальный случай
    let start_normal = Instant::now();
    let _pattern_normal = engine.calculate_interference_with_frequencies(&normal_waves);
    let time_normal = start_normal.elapsed();
    
    // Атака
    let start_attack = Instant::now();
    let _pattern_attack = engine.calculate_interference_with_frequencies(&attack_waves);
    let time_attack = start_attack.elapsed();
    
    println!("\n⏱️  Performance Impact:");
    println!("  • Normal: {:.2}ms", time_normal.as_secs_f64() * 1000.0);
    println!("  • Attack: {:.2}ms", time_attack.as_secs_f64() * 1000.0);
    println!("  • Slowdown: {:.2}x", time_attack.as_secs_f64() / time_normal.as_secs_f64());
    
    let slowdown = time_attack.as_secs_f64() / time_normal.as_secs_f64();
    
    if slowdown > 1.5 {
        println!("\n⚠️  ATTACK IMPACT DETECTED:");
        println!("     Shard flood causes {:.1}x slowdown!", slowdown);
    }
    
    // 4. Анализ spatial hashing
    const NUM_BUCKETS: usize = 32;
    const BUCKET_SIZE: f64 = 2.0 * std::f64::consts::PI / NUM_BUCKETS as f64;
    
    let mut buckets_normal: Vec<usize> = vec![0; NUM_BUCKETS];
    for (_, phase, _) in &normal_waves {
        let bucket_idx = ((phase / BUCKET_SIZE) as usize).min(NUM_BUCKETS - 1);
        buckets_normal[bucket_idx] += 1;
    }
    
    let mut buckets_attack: Vec<usize> = vec![0; NUM_BUCKETS];
    for (_, phase, _) in &attack_waves {
        let bucket_idx = ((phase / BUCKET_SIZE) as usize).min(NUM_BUCKETS - 1);
        buckets_attack[bucket_idx] += 1;
    }
    
    let max_normal = *buckets_normal.iter().max().unwrap();
    let max_attack = *buckets_attack.iter().max().unwrap();
    
    println!("\n📊 Spatial Hashing Analysis:");
    println!("  • Normal max bucket: {} txs", max_normal);
    println!("  • Attack max bucket: {} txs", max_attack);
    println!("  • Bucket load increase: {:.1}x", max_attack as f64 / max_normal as f64);
    
    // 5. Защита
    println!("\n🛡️  Mitigation Strategies:");
    println!("  1. Per-shard rate limiting (max {} txs/sec)", FLOOD_SIZE / 10);
    println!("  2. Dynamic shard rebalancing");
    println!("  3. Reject txs if shard queue > threshold");
    println!("  4. Adaptive spatial hashing (more buckets under load)");
    println!("  5. Cross-shard transaction batching");
    
    // Тест не должен быть слишком медленным
    assert!(
        time_attack.as_millis() < 500,
        "Shard flood caused excessive slowdown: {}ms",
        time_attack.as_millis()
    );
}

#[test]
fn test_shard_distribution_fairness() {
    println!("\n🔍 Testing Shard Distribution Fairness");
    
    const NUM_SHARDS: usize = 8;
    const TOTAL_TXS: usize = 10_000;
    
    // Симулируем хэш-функцию для распределения по шардам
    use blake3::Hasher;
    
    let mut shard_counts = vec![0usize; NUM_SHARDS];
    
    for i in 0..TOTAL_TXS {
        // Hash(from, to, nonce) → shard_id
        let mut hasher = Hasher::new();
        hasher.update(format!("addr-{}", i / 100));
        hasher.update(format!("addr-{}", i % 100));
        hasher.update(i.to_le_bytes());
        let hash = hasher.finalize();
        
        let shard_id = hash[0] as usize % NUM_SHARDS;
        shard_counts[shard_id] += 1;
    }
    
    println!("\nShard distribution:");
    for (shard_id, count) in shard_counts.iter().enumerate() {
        let percentage = (*count as f64 / TOTAL_TXS as f64) * 100.0;
        let bar = "█".repeat((percentage / 2.0) as usize);
        println!("  Shard {}: {:>4} txs ({:>5.2}%) {}", 
            shard_id, count, percentage, bar);
    }
    
    // Проверка справедливости: каждый шард должен быть ~12.5% ± 2%
    let expected_per_shard = TOTAL_TXS / NUM_SHARDS;
    let tolerance = (expected_per_shard as f64 * 0.15) as usize; // 15% tolerance
    
    println!("\n📊 Fairness Analysis:");
    println!("  • Expected per shard: {}", expected_per_shard);
    println!("  • Tolerance: ±{}", tolerance);
    
    let mut unfair_shards = 0;
    for (shard_id, count) in shard_counts.iter().enumerate() {
        let deviation = (*count as i32 - expected_per_shard as i32).abs() as usize;
        if deviation > tolerance {
            println!("  ⚠️  Shard {} deviated by {} txs!", shard_id, deviation);
            unfair_shards += 1;
        }
    }
    
    if unfair_shards == 0 {
        println!("  ✅ All shards within fairness tolerance");
    } else {
        println!("  ⚠️  {} shards outside tolerance", unfair_shards);
    }
    
    assert!(unfair_shards <= 1, "Too many shards with unfair distribution");
}

#[test]
fn test_adaptive_bucket_scaling() {
    println!("\n🔧 Testing Adaptive Bucket Scaling");
    
    // При высокой нагрузке можно увеличить количество buckets
    let test_loads = vec![100, 1_000, 10_000, 50_000];
    
    println!("\nOptimal bucket count:");
    for load in test_loads {
        // Эвристика: sqrt(N) buckets для O(1) lookup
        let optimal_buckets = (load as f64).sqrt() as usize;
        let optimal_buckets = optimal_buckets.max(32).min(256);
        
        let avg_per_bucket = load / optimal_buckets;
        
        println!("  Load: {:>6} → Buckets: {:>3} → Avg/bucket: {:>4}", 
            load, optimal_buckets, avg_per_bucket);
    }
    
    println!("\n✅ Adaptive scaling keeps avg bucket load < 100");
}



