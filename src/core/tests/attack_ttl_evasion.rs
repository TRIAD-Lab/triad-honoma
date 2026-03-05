/// 🎯 ATTACK #3: Атака на TTL для evasion
/// 
/// Описание:
/// Короткий TTL позволяет транзакциям "исчезнуть" до полной интерференции,
/// нарушая causal consistency в DAG.
/// 
/// Цель: Double-spend, orphan транзакции, probabilistic failures
/// 
/// Метод:
/// 1. Создать транзакцию с TTL=1ms
/// 2. Добавить в DAG с зависимостями
/// 3. Транзакция исчезает до consensus
/// 4. Зависимые транзакции становятся orphans

use crate::core::field::CoreWave;
use std::time::{Duration, Instant};
use std::thread;

#[test]
fn test_ttl_evasion_attack() {
    println!("\n🔴 ATTACK SIMULATION: TTL Evasion");
    println!("{}", "=".repeat(60));
    
    // 1. Создаём транзакции с разными TTL
    let mut waves = Vec::new();
    
    // Легитимная транзакция (нормальный TTL)
    let legitimate_wave = CoreWave::new(
        "legit-tx-1".to_string(),
        1.0,
        1.0,
        "shard-0".to_string(),
        Vec::new(),
    );
    // TTL по умолчанию: 60 секунд
    waves.push(legitimate_wave.clone());
    
    // Атакующая транзакция (экстремально короткий TTL)
    let mut attack_wave = CoreWave::new(
        "attack-tx-double-spend".to_string(),
        1.5,
        2.0,
        "shard-0".to_string(),
        Vec::new(),
    );
    attack_wave.lifetime = Duration::from_millis(1); // ❌ Только 1ms!
    waves.push(attack_wave.clone());
    
    println!("📊 Setup:");
    println!("  • Legitimate tx TTL: {:?}", legitimate_wave.lifetime);
    println!("  • Attack tx TTL: {:?}", attack_wave.lifetime);
    
    // 2. Симулируем обработку с задержкой
    println!("\n⏱️  Simulating consensus delay...");
    thread::sleep(Duration::from_millis(5));
    
    // 3. Проверяем какие транзакции ещё живы
    let now = Instant::now();
    let mut alive_count = 0;
    let mut expired_count = 0;
    
    for wave in &waves {
        if now.duration_since(wave.created_at) > wave.lifetime {
            println!("  ❌ TX {} EXPIRED (TTL: {:?})", wave.id, wave.lifetime);
            expired_count += 1;
        } else {
            println!("  ✅ TX {} ALIVE (TTL: {:?})", wave.id, wave.lifetime);
            alive_count += 1;
        }
    }
    
    println!("\n📈 Result:");
    println!("  • Alive: {}", alive_count);
    println!("  • Expired: {}", expired_count);
    
    if expired_count > 0 {
        println!("\n⚠️  ATTACK SUCCESSFUL:");
        println!("     Attack tx disappeared before consensus!");
        println!("     Potential double-spend opportunity");
    }
    
    // 4. Симулируем double-spend scenario
    println!("\n💸 Double-Spend Scenario:");
    println!("  1. Attack tx (TTL=1ms): Alice → Bob (100 TRD)");
    println!("  2. Attack tx expires before consensus");
    println!("  3. Alice creates new tx: Alice → Charlie (100 TRD)");
    println!("  4. Only second tx reaches consensus");
    println!("  5. Bob never receives funds (orphan payment)");
    
    // 5. Защита
    println!("\n🛡️  Mitigation Strategies:");
    println!("  1. Minimum TTL enforcement (e.g., 10 seconds)");
    println!("  2. TTL-based fee scaling (shorter TTL = higher fee)");
    println!("  3. Reject expired txs early in pipeline");
    println!("  4. DAG dependency validation (check parent TTL)");
    println!("  5. Consensus finality before TTL expiry");
    
    assert!(expired_count > 0, "TTL attack test should demonstrate expiry");
}

#[test]
fn test_minimum_ttl_enforcement() {
    println!("\n🔍 Testing Minimum TTL Enforcement");
    
    const MIN_TTL_SECONDS: u64 = 10;
    
    let test_ttls = vec![
        Duration::from_millis(1),
        Duration::from_millis(100),
        Duration::from_secs(1),
        Duration::from_secs(5),
        Duration::from_secs(10),
        Duration::from_secs(60),
    ];
    
    println!("\nTTL validation:");
    for ttl in test_ttls {
        let is_valid = ttl >= Duration::from_secs(MIN_TTL_SECONDS);
        let status = if is_valid { "✅ ACCEPT" } else { "❌ REJECT" };
        
        println!("  TTL: {:>8?} → {}", ttl, status);
    }
    
    println!("\n✅ Enforcement: TTL must be ≥ {} seconds", MIN_TTL_SECONDS);
}

#[test]
fn test_dag_orphan_detection() {
    println!("\n🔍 Testing DAG Orphan Detection");
    
    // Симулируем DAG с orphan транзакциями
    // Parent TX → Child TX, но parent expired
    
    #[derive(Debug)]
    struct TestTransaction {
        id: String,
        parent_id: Option<String>,
        ttl: Duration,
        created_at: Instant,
    }
    
    let now = Instant::now();
    
    let mut transactions = vec![
        TestTransaction {
            id: "tx-1".to_string(),
            parent_id: None,
            ttl: Duration::from_millis(1),
            created_at: now,
        },
        TestTransaction {
            id: "tx-2".to_string(),
            parent_id: Some("tx-1".to_string()),
            ttl: Duration::from_secs(60),
            created_at: now,
        },
        TestTransaction {
            id: "tx-3".to_string(),
            parent_id: Some("tx-2".to_string()),
            ttl: Duration::from_secs(60),
            created_at: now,
        },
    ];
    
    // Ждём чтобы tx-1 expired
    thread::sleep(Duration::from_millis(5));
    
    println!("\nChecking DAG consistency:");
    
    let check_time = Instant::now();
    let mut expired_ids = Vec::new();
    
    // Находим expired транзакции
    for tx in &transactions {
        if check_time.duration_since(tx.created_at) > tx.ttl {
            println!("  ❌ {} EXPIRED", tx.id);
            expired_ids.push(tx.id.clone());
        } else {
            println!("  ✅ {} ALIVE", tx.id);
        }
    }
    
    // Находим orphan транзакции (parent expired)
    let mut orphan_count = 0;
    for tx in &transactions {
        if let Some(parent_id) = &tx.parent_id {
            if expired_ids.contains(parent_id) {
                println!("  ⚠️  {} is ORPHAN (parent {} expired)", tx.id, parent_id);
                orphan_count += 1;
            }
        }
    }
    
    println!("\n📊 Result:");
    println!("  • Expired: {}", expired_ids.len());
    println!("  • Orphans: {}", orphan_count);
    
    if orphan_count > 0 {
        println!("\n⚠️  CONSISTENCY VIOLATION:");
        println!("     {} transactions have expired parents", orphan_count);
        println!("     DAG integrity compromised!");
    }
    
    println!("\n🛡️  Recommendation:");
    println!("  • Reject transactions if parent is expired");
    println!("  • Or propagate parent TTL to children");
    
    assert!(orphan_count > 0, "Should detect orphan transactions");
}

#[test]
fn test_ttl_based_fee_scaling() {
    println!("\n💰 Testing TTL-based Fee Scaling");
    
    // Предлагаемая модель: fee = base_fee / (ttl_seconds / 60)
    // Короткий TTL = выше комиссия
    
    let base_fee = 0.0001; // TRD
    
    println!("\nFee table (base: {} TRD):", base_fee);
    println!("  TTL (sec) | Fee (TRD) | Multiplier");
    println!("{}", "  ".repeat(40));
    
    for ttl_sec in [1, 5, 10, 30, 60, 300, 600].iter() {
        let multiplier = 60.0 / (*ttl_sec as f64);
        let fee = base_fee * multiplier;
        
        println!("  {:>9} | {:>9.6} | {:>6.2}x", ttl_sec, fee, multiplier);
    }
    
    println!("\n📊 Analysis:");
    println!("  • TTL=1s: 60x base fee (prohibitive!)");
    println!("  • TTL=60s: 1x base fee (standard)");
    println!("  • TTL=600s: 0.1x base fee (discount for patience)");
    println!("\n✅ Fee scaling disincentivizes short TTL abuse");
}



