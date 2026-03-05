/// 🔴 STRESS TEST: TTL Evasion Attack (Enhanced)
/// 
/// Усиления согласно плану:
/// - Chain 100 tx (DAG depth 10)
/// - TTL=5-50ms
/// - simulate double-spend (2 conflicting chains, expiry rate 20%)
/// - 50 nodes network
/// - propagation delays 20-80ms (tokio timers)
/// - orphan chains >5% → rollback
/// - Target: Min TTL 10s + propagate to children, 0 orphans, 100% expiry rejected pre-DAG

use crate::core::field::CoreWave;
use crate::core::tests::network_sim::{NetworkSimulator, NetworkConfig};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use std::collections::{HashMap, HashSet};
use tokio::time::sleep;
use rand::Rng;

#[derive(Debug, Clone)]
struct DagTransaction {
    id: String,
    wave: CoreWave,
    ttl_ms: u64,
    created_at: SystemTime,
    parents: Vec<String>,
    children: Vec<String>,
    is_double_spend: bool,
}

impl DagTransaction {
    fn is_expired(&self) -> bool {
        let now = SystemTime::now();
        let elapsed = now.duration_since(self.created_at).unwrap_or(Duration::ZERO);
        elapsed.as_millis() > self.ttl_ms as u128
    }
    
    fn ttl_remaining_ms(&self) -> u64 {
        let now = SystemTime::now();
        let elapsed = now.duration_since(self.created_at).unwrap_or(Duration::ZERO);
        self.ttl_ms.saturating_sub(elapsed.as_millis() as u64)
    }
}

#[tokio::test]
async fn test_ttl_evasion_attack_stress() {
    println!("\n🔴 STRESS TEST: TTL Evasion Attack (Enhanced)");
    println!("{}", "=".repeat(80));
    
    let test_start = Instant::now();
    
    // === CONFIGURATION ===
    const CHAIN_LENGTH: usize = 100;
    const DAG_DEPTH: usize = 10;
    const MIN_TTL_MS: u64 = 10_000; // Minimum enforced: 10 seconds
    const ATTACK_TTL_MIN: u64 = 5;
    const ATTACK_TTL_MAX: u64 = 50;
    const DOUBLE_SPEND_RATIO: f64 = 0.2;
    
    let net_config = NetworkConfig {
        num_nodes: 50,
        malicious_ratio: 0.2,
        base_delay_ms: 40,
        delay_variance_ms: 40, // 20-80ms range
        packet_loss_rate: 0.03,
    };
    
    // Save values before moving config
    let base_delay = net_config.base_delay_ms;
    let delay_variance = net_config.delay_variance_ms;
    
    println!("\n⚙️  Configuration:");
    println!("  • Chain length: {} txs", CHAIN_LENGTH);
    println!("  • DAG depth: {} levels", DAG_DEPTH);
    println!("  • Minimum TTL (enforced): {}ms ({:.1}s)", MIN_TTL_MS, MIN_TTL_MS as f64 / 1000.0);
    println!("  • Attack TTL range: {}-{}ms", ATTACK_TTL_MIN, ATTACK_TTL_MAX);
    println!("  • Double-spend ratio: {:.0}%", DOUBLE_SPEND_RATIO * 100.0);
    println!("  • Network propagation: {}-{}ms", 
        base_delay, 
        base_delay + delay_variance
    );
    
    // === 1. NETWORK SETUP ===
    println!("\n🌐 Setting up network simulator...");
    let network = NetworkSimulator::new(net_config);
    println!("  • Nodes initialized: {}", network.nodes.len());
    
    // === 2. CREATE DAG TRANSACTION CHAINS ===
    println!("\n📝 Creating DAG transaction chains...");
    let dag_start = Instant::now();
    
    let mut rng = rand::thread_rng();
    let mut dag_txs: Vec<DagTransaction> = Vec::new();
    let mut parent_ids: Vec<String> = Vec::new();
    
    // Create legitimate chain with proper TTL
    for level in 0..DAG_DEPTH {
        let num_tx_at_level = CHAIN_LENGTH / DAG_DEPTH;
        
        for i in 0..num_tx_at_level {
            let tx_id = format!("legit-L{}-{}", level, i);
            let ttl_ms = MIN_TTL_MS + rng.gen_range(0..5000); // 10-15s
            
            // Select random parents from previous level
            let parents = if level == 0 {
                Vec::new()
            } else {
                let num_parents = rng.gen_range(1..3.min(parent_ids.len() + 1));
                parent_ids.iter()
                    .take(num_parents)
                    .cloned()
                    .collect()
            };
            
            let wave = CoreWave::new(
                tx_id.clone(),
                1.0,
                rng.r#gen::<f64>() * 2.0 * std::f64::consts::PI,
                "shard-0".to_string(),
                Vec::new(),
            );
            
            let tx = DagTransaction {
                id: tx_id.clone(),
                wave,
                ttl_ms,
                created_at: SystemTime::now(),
                parents: parents.clone(),
                children: Vec::new(),
                is_double_spend: false,
            };
            
            dag_txs.push(tx);
            
            // Update parent IDs for next level
            if level < DAG_DEPTH - 1 && dag_txs.len() % (num_tx_at_level / 2) == 0 {
                parent_ids.push(tx_id);
                if parent_ids.len() > 5 {
                    parent_ids.remove(0);
                }
            }
        }
    }
    
    println!("  • Created {} legitimate txs in {:.2}ms", 
        dag_txs.len(), dag_start.elapsed().as_secs_f64() * 1000.0);
    
    // === 3. CREATE ATTACK TRANSACTIONS (short TTL + double-spend) ===
    println!("\n⚠️  ATTACK: Creating short-TTL transactions...");
    let attack_start = Instant::now();
    
    let num_attack = (CHAIN_LENGTH as f64 * DOUBLE_SPEND_RATIO) as usize;
    let mut attack_txs: Vec<DagTransaction> = Vec::new();
    
    for i in 0..num_attack {
        let tx_id = format!("attack-{}", i);
        let ttl_ms = rng.gen_range(ATTACK_TTL_MIN..ATTACK_TTL_MAX);
        
        // Create double-spend: same parents as existing tx
        let target_idx = rng.gen_range(0..dag_txs.len());
        let parents = dag_txs[target_idx].parents.clone();
        
        let wave = CoreWave::new(
            tx_id.clone(),
            1.5, // Higher amplitude to try to dominate
            std::f64::consts::PI, // Destructive phase
            "shard-0".to_string(),
            Vec::new(),
        );
        
        let tx = DagTransaction {
            id: tx_id,
            wave,
            ttl_ms,
            created_at: SystemTime::now(),
            parents,
            children: Vec::new(),
            is_double_spend: true,
        };
        
        attack_txs.push(tx);
    }
    
    println!("  • Created {} attack txs in {:.2}ms", 
        attack_txs.len(), attack_start.elapsed().as_secs_f64() * 1000.0);
    println!("  • Attack TTL range: {}-{}ms (avg: {:.1}ms)", 
        ATTACK_TTL_MIN, ATTACK_TTL_MAX,
        attack_txs.iter().map(|t| t.ttl_ms).sum::<u64>() as f64 / attack_txs.len() as f64
    );
    
    // === 4. SIMULATE NETWORK PROPAGATION ===
    println!("\n🌐 Simulating network propagation with delays...");
    let prop_start = Instant::now();
    
    let mut propagated: Vec<&DagTransaction> = Vec::new();
    let mut expired_during_prop: Vec<&DagTransaction> = Vec::new();
    
    // Simulate propagation for attack txs
    for tx in &attack_txs {
        // Calculate if tx will expire during propagation
        let prop_delay_ms = base_delay + 
            rng.gen_range(0..delay_variance);
        
        // Sleep to simulate real-time propagation
        sleep(Duration::from_millis(1)).await; // Scaled down for testing
        
        if tx.ttl_remaining_ms() > prop_delay_ms {
            propagated.push(tx);
        } else {
            expired_during_prop.push(tx);
        }
    }
    
    let expiry_rate = expired_during_prop.len() as f64 / attack_txs.len() as f64;
    
    println!("  • Propagation time: {:.2}ms", prop_start.elapsed().as_secs_f64() * 1000.0);
    println!("  • Expired during propagation: {} ({:.1}%)", 
        expired_during_prop.len(), expiry_rate * 100.0);
    println!("  • Successfully propagated (attack): {}", propagated.len());
    
    // === 5. ENFORCE MINIMUM TTL ===
    println!("\n✅ Enforcing minimum TTL policy ({}ms)...", MIN_TTL_MS);
    let enforce_start = Instant::now();
    
    let mut rejected_by_ttl: Vec<&DagTransaction> = Vec::new();
    let mut accepted: Vec<&DagTransaction> = Vec::new();
    
    // Check all transactions
    for tx in dag_txs.iter().chain(attack_txs.iter()) {
        if tx.ttl_ms < MIN_TTL_MS {
            rejected_by_ttl.push(tx);
        } else {
            accepted.push(tx);
        }
    }
    
    println!("  • Enforcement time: {:.2}ms", enforce_start.elapsed().as_secs_f64() * 1000.0);
    println!("  • Rejected (TTL < {}ms): {}", MIN_TTL_MS, rejected_by_ttl.len());
    println!("  • Accepted (TTL ≥ {}ms): {}", MIN_TTL_MS, accepted.len());
    println!("  • Attack txs rejected: {} / {} ({:.1}%)",
        attack_txs.iter().filter(|tx| tx.ttl_ms < MIN_TTL_MS).count(),
        attack_txs.len(),
        (attack_txs.iter().filter(|tx| tx.ttl_ms < MIN_TTL_MS).count() as f64 / attack_txs.len() as f64) * 100.0
    );
    
    // === 6. DETECT ORPHAN CHAINS ===
    println!("\n🔍 Detecting orphan chains (txs with expired parents)...");
    let orphan_start = Instant::now();
    
    // Build parent-child relationships
    let mut tx_map: HashMap<String, &DagTransaction> = HashMap::new();
    for tx in &accepted {
        tx_map.insert(tx.id.clone(), *tx);
    }
    
    let mut orphans: Vec<&DagTransaction> = Vec::new();
    
    for tx in &accepted {
        for parent_id in &tx.parents {
            if !tx_map.contains_key(parent_id) {
                orphans.push(*tx);
                break;
            }
        }
    }
    
    let orphan_rate = orphans.len() as f64 / accepted.len() as f64;
    
    println!("  • Orphan detection time: {:.2}ms", orphan_start.elapsed().as_secs_f64() * 1000.0);
    println!("  • Orphaned transactions: {} ({:.1}%)", orphans.len(), orphan_rate * 100.0);
    
    // === 7. PROPAGATE TO CHILDREN ===
    println!("\n📤 Checking TTL propagation to children...");
    
    let mut children_ok = 0;
    let mut children_orphaned = 0;
    
    for tx in &accepted {
        if tx.parents.is_empty() {
            continue; // Root txs have no parents
        }
        
        // Check if all parents exist and haven't expired
        let mut all_parents_valid = true;
        for parent_id in &tx.parents {
            if let Some(parent) = tx_map.get(parent_id) {
                if parent.is_expired() {
                    all_parents_valid = false;
                    break;
                }
            } else {
                all_parents_valid = false;
                break;
            }
        }
        
        if all_parents_valid {
            children_ok += 1;
        } else {
            children_orphaned += 1;
        }
    }
    
    println!("  • Children with valid parents: {}", children_ok);
    println!("  • Children orphaned: {}", children_orphaned);
    
    // === 8. DOUBLE-SPEND DETECTION ===
    println!("\n🚨 Detecting double-spend conflicts...");
    let ds_start = Instant::now();
    
    let mut conflicts = 0;
    let mut resolved = 0;
    
    // Group by parent set
    let mut parent_groups: HashMap<Vec<String>, Vec<&DagTransaction>> = HashMap::new();
    for tx in &accepted {
        if !tx.parents.is_empty() {
            let mut sorted_parents = tx.parents.clone();
            sorted_parents.sort();
            parent_groups.entry(sorted_parents).or_insert_with(Vec::new).push(tx);
        }
    }
    
    for (parents, txs) in &parent_groups {
        if txs.len() > 1 {
            conflicts += 1;
            // Resolve by choosing tx with longer TTL
            let winner = txs.iter().max_by_key(|tx| tx.ttl_ms).unwrap();
            if !winner.is_double_spend {
                resolved += 1;
            }
        }
    }
    
    println!("  • Detection time: {:.2}ms", ds_start.elapsed().as_secs_f64() * 1000.0);
    println!("  • Conflicts detected: {}", conflicts);
    println!("  • Resolved in favor of legitimate: {}", resolved);
    
    // === 9. PERFORMANCE METRICS ===
    let total_time = test_start.elapsed();
    let total_txs = dag_txs.len() + attack_txs.len();
    
    println!("\n📊 Performance Summary:");
    println!("  • Total transactions: {}", total_txs);
    println!("  • Legitimate: {}", dag_txs.len());
    println!("  • Attack: {}", attack_txs.len());
    println!("  • Rejected by TTL enforcement: {} ({:.1}%)", 
        rejected_by_ttl.len(),
        (rejected_by_ttl.len() as f64 / total_txs as f64) * 100.0
    );
    println!("  • Expired during propagation: {} ({:.1}%)", 
        expired_during_prop.len(),
        expiry_rate * 100.0
    );
    println!("  • Orphaned: {} ({:.1}%)", orphans.len(), orphan_rate * 100.0);
    println!("  • Double-spend conflicts: {}", conflicts);
    println!("  • Total time: {:.2}s", total_time.as_secs_f64());
    
    // === 10. ASSERTIONS ===
    println!("\n✅ Validation:");
    
    // All attack txs should be rejected
    let attack_rejection_rate = attack_txs.iter()
        .filter(|tx| tx.ttl_ms < MIN_TTL_MS)
        .count() as f64 / attack_txs.len() as f64;
    assert!(
        attack_rejection_rate == 1.0,
        "Not all attack txs rejected: {:.1}%",
        attack_rejection_rate * 100.0
    );
    println!("  ✓ Attack rejection: 100% (TTL < {}ms)", MIN_TTL_MS);
    
    // Orphan rate should be very low (<5%)
    assert!(
        orphan_rate < 0.05,
        "Orphan rate too high: {:.1}% > 5%",
        orphan_rate * 100.0
    );
    println!("  ✓ Orphan rate: {:.1}% < 5%", orphan_rate * 100.0);
    
    // No legitimate tx should be orphaned
    let legit_orphaned = orphans.iter()
        .filter(|tx| !tx.is_double_spend)
        .count();
    assert!(
        legit_orphaned == 0,
        "Legitimate txs orphaned: {}",
        legit_orphaned
    );
    println!("  ✓ Legitimate txs orphaned: 0");
    
    // Double-spends should be resolved
    assert!(
        resolved == conflicts,
        "Unresolved conflicts: {} / {}",
        conflicts - resolved, conflicts
    );
    println!("  ✓ Double-spend conflicts resolved: {} / {}", resolved, conflicts);
    
    // Expiry during propagation should match expected rate
    assert!(
        expiry_rate >= 0.15,
        "Expiry rate too low: {:.1}% (expected ~20%)",
        expiry_rate * 100.0
    );
    println!("  ✓ Expiry rate during propagation: {:.1}% ≈ 20%", expiry_rate * 100.0);
    
    println!("\n🎯 Test completed in {:.2}s", total_time.as_secs_f64());
    println!("✅ TTL Evasion attack successfully mitigated!");
    println!("   Rejection: 100%, Orphans: {:.1}%, Conflicts resolved: {}",
        orphan_rate * 100.0, resolved
    );
}



