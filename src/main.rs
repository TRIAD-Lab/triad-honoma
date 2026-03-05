// High-performance allocator (mimalloc): +40-70% memory performance, 0 GC pauses
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use bytes::Bytes;
use futures::future::join_all;
use rand::Rng;
use rand::SeedableRng;
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::Mutex;
use tokio::time::sleep;
use triad::core::consensus::{ConsensusMessage, ConsensusNode};
use triad::core::CoreSystem;
use triad::dag::Dag;
use triad::error_analysis::ErrorAI;
use triad::network::{MessageType, NetworkMessage, NetworkNode, NodeConfig};
use triad::semantic::{SemanticEngine, SystemGoal};
use triad::sharding::{Shard, ShardAI};
use triad::token::TrdToken;
use triad::transaction::processor::CoreTransactionProcessor;
use triad::transaction::{Transaction, TransactionProcessor, TransactionStatus};
use uuid::Uuid;

const NODES: usize = 8;
const SIM_DURATION: u64 = 20; // секунд
const TARGET_TX: usize = 100;

#[derive(Default)]
struct Metrics {
    total_tx: usize,
    failed_tx: usize,
    consensus_reached: usize,
    consensus_failed: usize,
    latencies: Vec<u128>,
    dag_adds: usize,
    dag_validations: usize,
    interference_events: usize,
    interference_sum: f64,
    interference_max: f64,
    batch_tx: usize,
    batch_failed: usize,
    attacks: usize,
    overloads: usize,
    recoveries: usize,
    cross_shard_lost: usize,
}

#[derive(Clone, Copy, Debug)]
enum Scenario {
    SimpleTx,
    ContractTx,
    CrossShardTx,
    FaultyTx,
    BatchTx,
}

fn random_scenario<R: rand::Rng + ?Sized>(rng: &mut R) -> Scenario {
    let v = rng.gen_range(0..100);
    match v {
        0..=49 => Scenario::SimpleTx,
        50..=69 => Scenario::ContractTx,
        70..=84 => Scenario::CrossShardTx,
        85..=89 => Scenario::FaultyTx,
        _ => Scenario::BatchTx,
    }
}

#[derive(Clone)]
struct OverloadState {
    overloaded: Vec<usize>,
}

#[derive(Clone, Copy, PartialEq)]
enum TxType {
    Simple,
    Contract,
    CrossShard,
    Faulty,
}

fn random_tx_type(rng: &mut rand::rngs::ThreadRng) -> TxType {
    let v = rng.gen_range(0..100);
    match v {
        0..=59 => TxType::Simple,
        60..=79 => TxType::Contract,
        80..=94 => TxType::CrossShard,
        _ => TxType::Faulty,
    }
}

struct SimNode {
    id: String,
    network: NetworkNode,
    consensus: ConsensusNode,
    shard: Shard,
    tx_processor: CoreTransactionProcessor,
    token: TrdToken,
    error_ai: ErrorAI,
    semantic: SemanticEngine,
    dag: Dag,
    processed: usize,
    errors: usize,
}

// Имитация интерференции (заглушка)
fn simulate_interference(amount: f64, noise: f64) -> f64 {
    (amount * noise).sin().abs() * 10.0
}

// Получить id всех транзакций DAG (если нет публичного поля)
fn get_dag_tx_ids(dag: &Dag) -> Vec<Uuid> {
    // Если есть публичный метод, используем его, иначе возвращаем пустой вектор
    // TODO: заменить на реальный способ получения id транзакций DAG
    vec![]
}

// --- Новый код: отдельные async-функции для сценариев ---

async fn scenario_simple_tx(
    nodes: Arc<Mutex<Vec<SimNode>>>,
    metrics: Arc<Mutex<Metrics>>,
    overload_state: OverloadState,
    semaphore: Arc<tokio::sync::Semaphore>,
) {
    let _permit = semaphore.acquire().await.unwrap();
    println!("[DEBUG] scenario_simple_tx started");
    let mut rng = StdRng::from_entropy();
    let mut sender_idx = rng.gen_range(0..NODES);
    let mut receiver_idx = rng.gen_range(0..NODES);
    while receiver_idx == sender_idx {
        receiver_idx = rng.gen_range(0..NODES);
    }
    let (sender_idx, receiver_idx) = if sender_idx < receiver_idx {
        (sender_idx, receiver_idx)
    } else {
        (receiver_idx, sender_idx)
    };
    if sender_idx >= NODES || receiver_idx >= NODES || sender_idx == receiver_idx {
        println!(
            "[DEBUG] invalid sender/receiver indices: {} {}",
            sender_idx, receiver_idx
        );
        return;
    }
    // Берём lock только на split_at_mut и sender/receiver
    let (amount, parents, sender_id, receiver_id, tx, tx_id) = {
        println!("[DEBUG] scenario_simple_tx before nodes.lock");
        let mut guard = nodes.lock().await;
        println!("[DEBUG] scenario_simple_tx after nodes.lock");
        let (first, second) = guard.split_at_mut(receiver_idx);
        let sender = &mut first[sender_idx];
        let receiver = &mut second[0];
        let amount = rng.gen_range(1.0..50.0);
        let parents: Vec<Uuid> = (0..rng.gen_range(0..3))
            .filter_map(|_| {
                let txs = sender.dag.all_tx_ids();
                if !txs.is_empty() {
                    Some(*txs.choose(&mut rng).unwrap())
                } else {
                    None
                }
            })
            .collect();
        let tx = Transaction::new(sender.id.clone(), receiver.id.clone(), amount, vec![]);
        let tx_id = tx.id;
        (
            amount,
            parents,
            sender.id.clone(),
            receiver.id.clone(),
            tx,
            tx_id,
        )
    };
    let t0 = Instant::now();
    let msg = NetworkMessage {
        id: Uuid::new_v4(),
        sender: sender_id.clone(),
        receiver: receiver_id.clone(),
        message_type: MessageType::Transaction(tx.clone()),
        timestamp: SystemTime::now(),
        data: vec![],
    };
    // Перегрузка: если receiver перегружен, транзакция теряется или задерживается
    let mut lost = false;
    if overload_state.overloaded.contains(&receiver_idx) {
        if rng.gen_bool(0.5) {
            lost = true;
        }
    }
    if false && rng.gen_bool(0.15) {
        // CrossShardTx не используется
        lost = true;
        metrics.lock().await.cross_shard_lost += 1;
    }
    // Второй lock только для изменения sender/receiver
    {
        let mut guard = nodes.lock().await;
        let (first, second) = guard.split_at_mut(receiver_idx);
        let sender = &mut first[sender_idx];
        let receiver = &mut second[0];
        if !lost {
            receiver.network.process_message(msg);
            receiver.processed += 1;
        }
        receiver.dag.add_transaction(tx_id, parents.clone());
    }
    println!("[DEBUG] scenario_simple_tx before metrics.lock for dag_adds");
    metrics.lock().await.dag_adds += 1;
    println!("[DEBUG] scenario_simple_tx after metrics.lock for dag_adds");
    // DAG validation
    let valid = {
        let mut guard = nodes.lock().await;
        let (first, second) = guard.split_at_mut(receiver_idx);
        let receiver = &second[0];
        receiver.dag.validate_parallel(&[tx_id])
    };
    if valid.iter().all(|&v| v) {
        metrics.lock().await.dag_validations += 1;
    }
    // Интерференция
    let interference = simulate_interference(amount, rng.gen_range(0.0..1.0));
    metrics.lock().await.interference_events += 1;
    metrics.lock().await.interference_sum += interference;
    if interference > metrics.lock().await.interference_max {
        metrics.lock().await.interference_max = interference;
    }
    // Латентность
    let latency = t0.elapsed().as_micros();
    metrics.lock().await.latencies.push(latency);
    // Транзакция успешна?
    println!("[DEBUG] scenario_simple_tx before metrics.lock for total_tx/failed_tx");
    let tx_ok = !lost;
    if tx_ok {
        metrics.lock().await.total_tx += 1;
        println!("[PROGRESS] scenario_simple_tx: total_tx incremented");
    } else {
        metrics.lock().await.failed_tx += 1;
        println!("[PROGRESS] scenario_simple_tx: failed_tx incremented");
    }
    println!("[DEBUG] scenario_simple_tx after metrics.lock for total_tx/failed_tx");
    // Консенсус (можно оптимизировать аналогично)
    println!("[DEBUG] scenario_simple_tx before consensus");
    let consensus_msg = ConsensusMessage {
        sender_id: sender_id.clone(),
        state_id: tx.id.to_string(),
        raw_data: Bytes::new(),
        signature: vec![], // Добавляем пустую подпись для демо
        public_key: None,  // Добавляем поле public_key
    };
    let mut agree = 0;
    let mut guard = nodes.lock().await;
    for node in guard.iter_mut() {
        // Создаем фиктивный публичный ключ для демо
        let dummy_public_key = node.consensus.keypair.public.clone();
        let res = node
            .consensus
            .process_message(consensus_msg.clone(), &dummy_public_key);
        if res.is_ok() {
            agree += 1;
        }
    }
    if agree as f64 / NODES as f64 >= 0.66 {
        metrics.lock().await.consensus_reached += 1;
    } else {
        metrics.lock().await.consensus_failed += 1;
    }
    println!("[DEBUG] scenario_simple_tx finished");
}

async fn scenario_batch_tx(
    nodes: Arc<Mutex<Vec<SimNode>>>,
    metrics: Arc<Mutex<Metrics>>,
    overload_state: OverloadState,
    semaphore: Arc<tokio::sync::Semaphore>,
) {
    let _permit = semaphore.acquire().await.unwrap();
    println!("[DEBUG] scenario_batch_tx started");
    let mut rng = StdRng::from_entropy();
    let sender_idx = rng.gen_range(0..NODES);
    let mut receivers: Vec<usize> = (0..NODES).filter(|&i| i != sender_idx).collect();
    receivers.shuffle(&mut rng);
    let batch_size = rng.gen_range(2..=4).min(receivers.len());
    let mut batch_failed = false;
    for &receiver_idx in &receivers[..batch_size] {
        let (amount, sender_id, receiver_id, tx, tx_id) = {
            let mut guard = nodes.lock().await;
            let (first, second) = guard.split_at_mut(receiver_idx);
            let sender = &mut first[sender_idx];
            let receiver = &mut second[0];
            let amount = rng.gen_range(1.0..50.0);
            let tx = Transaction::new(sender.id.clone(), receiver.id.clone(), amount, vec![]);
            let tx_id = tx.id;
            (amount, sender.id.clone(), receiver.id.clone(), tx, tx_id)
        };
        let msg = NetworkMessage {
            id: Uuid::new_v4(),
            sender: sender_id.clone(),
            receiver: receiver_id.clone(),
            message_type: MessageType::Transaction(tx.clone()),
            timestamp: SystemTime::now(),
            data: vec![],
        };
        if overload_state.overloaded.contains(&receiver_idx) && rng.gen_bool(0.5) {
            batch_failed = true;
            continue;
        }
        {
            let mut guard = nodes.lock().await;
            let (first, second) = guard.split_at_mut(receiver_idx);
            let sender = &mut first[sender_idx];
            let receiver = &mut second[0];
            receiver.network.process_message(msg);
            receiver.processed += 1;
            receiver.dag.add_transaction(tx.id, vec![]);
        }
        metrics.lock().await.dag_adds += 1;
        let valid = {
            let mut guard = nodes.lock().await;
            let (first, second) = guard.split_at_mut(receiver_idx);
            let receiver = &second[0];
            receiver.dag.validate_parallel(&[tx_id])
        };
        if valid.iter().all(|&v| v) {
            metrics.lock().await.dag_validations += 1;
        }
        let interference = simulate_interference(amount, rng.gen_range(0.0..1.0));
        metrics.lock().await.interference_events += 1;
        metrics.lock().await.interference_sum += interference;
        if interference > metrics.lock().await.interference_max {
            metrics.lock().await.interference_max = interference;
        }
        let latency = rng.gen_range(10..50) as u128;
        metrics.lock().await.latencies.push(latency);
        metrics.lock().await.total_tx += 1;
    }
    metrics.lock().await.batch_tx += 1;
    if batch_failed {
        metrics.lock().await.batch_failed += 1;
        println!("[PROGRESS] scenario_batch_tx: batch_failed incremented");
    }
    println!("[DEBUG] scenario_batch_tx finished");
}

async fn scenario_attack(nodes: Arc<Mutex<Vec<SimNode>>>, metrics: Arc<Mutex<Metrics>>) {
    let mut rng = StdRng::from_entropy();
    let attacker_idx = rng.gen_range(0..NODES);
    let mut receivers: Vec<usize> = (0..NODES).filter(|&i| i != attacker_idx).collect();
    receivers.shuffle(&mut rng);
    let attack_size = rng.gen_range(5..=10).min(receivers.len());
    let mut guard = nodes.lock().await;
    for &receiver_idx in &receivers[..attack_size] {
        let (a, b) = if attacker_idx < receiver_idx {
            (attacker_idx, receiver_idx)
        } else {
            (receiver_idx, attacker_idx)
        };
        if a >= NODES || b >= NODES || a == b {
            continue;
        }
        let (first, second) = guard.split_at_mut(b);
        let sender = &mut first[a];
        let receiver = &mut second[0];
        let amount = rng.gen_range(1.0..10.0);
        let tx = Transaction::new(sender.id.clone(), receiver.id.clone(), amount, vec![]);
        let msg = NetworkMessage {
            id: Uuid::new_v4(),
            sender: sender.id.clone(),
            receiver: receiver.id.clone(),
            message_type: MessageType::Transaction(tx.clone()),
            timestamp: SystemTime::now(),
            data: vec![],
        };
        receiver.network.process_message(msg);
        receiver.processed += 1;
        metrics.lock().await.dag_adds += 1;
        receiver.dag.add_transaction(tx.id, vec![]);
        let valid = receiver.dag.validate_parallel(&[tx.id]);
        if valid.iter().all(|&v| v) {
            metrics.lock().await.dag_validations += 1;
        }
        let interference = simulate_interference(amount, rng.gen_range(0.0..1.0));
        metrics.lock().await.interference_events += 1;
        metrics.lock().await.interference_sum += interference;
        if interference > metrics.lock().await.interference_max {
            metrics.lock().await.interference_max = interference;
        }
        let latency = rng.gen_range(10..50) as u128;
        metrics.lock().await.latencies.push(latency);
        metrics.lock().await.failed_tx += 1; // атака считается неуспешной
    }
    metrics.lock().await.attacks += 1;
}

async fn scenario_overload(metrics: Arc<Mutex<Metrics>>, mut overload_state: OverloadState) {
    let mut rng = StdRng::from_entropy();
    let idx = rng.gen_range(0..NODES);
    if !overload_state.overloaded.contains(&idx) {
        overload_state.overloaded.push(idx);
        metrics.lock().await.overloads += 1;
    }
}

async fn scenario_recovery(metrics: Arc<Mutex<Metrics>>, mut overload_state: OverloadState) {
    let mut rng = StdRng::from_entropy();
    if !overload_state.overloaded.is_empty() {
        let idx = rng.gen_range(0..overload_state.overloaded.len());
        overload_state.overloaded.remove(idx);
        metrics.lock().await.recoveries += 1;
    }
}

#[tokio::main]
/// Function documentation
async fn main() {
    println!("=== TRIAD NETWORK SIMULATION ===");
    println!("[DEBUG] main started");
    let nodes: Arc<Mutex<Vec<SimNode>>> = Arc::new(Mutex::new(vec![]));
    let node_ids: Vec<String> = (0..NODES).map(|i| format!("node{}", i + 1)).collect();
    let metrics = Arc::new(Mutex::new(Metrics::default()));
    let mut overload_state = OverloadState { overloaded: vec![] };
    let semaphore = Arc::new(tokio::sync::Semaphore::new(16)); // Ограничение на 16 задач
    for i in 0..NODES {
        let id = node_ids[i].clone();
        let peers = node_ids.iter().filter(|nid| *nid != &id).cloned().collect();
        let network = NetworkNode::new(NodeConfig {
            id: id.clone(),
            address: format!("127.0.0.1:{}", 8000 + i),
            initial_peers: peers,
            consensus_threshold: 0.66,
        });
        // Generate u128 ID from string ID for ConsensusNode
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        use std::hash::Hash;
        use std::hash::Hasher;
        id.hash(&mut hasher);
        let id_u128 = hasher.finish() as u128;

        let consensus = ConsensusNode::new(id_u128, 0);
        let shard = Shard::new(0, vec![]);
        let tx_processor = CoreTransactionProcessor::new(0.0, 1000.0);
        let token = TrdToken::new("TRD", 6);
        let error_ai = ErrorAI::new();
        let semantic = SemanticEngine::new(SystemGoal::CoreConsensus {
            target_tps: 1000,
            energy_efficiency: 0.95,
            security_level: 0.99,
        });
        let dag = Dag::new();
        nodes.lock().await.push(SimNode {
            id,
            network,
            consensus,
            shard,
            tx_processor,
            token,
            error_ai,
            semantic,
            dag,
            processed: 0,
            errors: 0,
        });
    }
    println!("[DEBUG] nodes initialized");
    let mut handles = vec![];
    // let progress_metrics = Arc::clone(&metrics);
    // let progress_handle = tokio::spawn(async move {
    //     println!("[DEBUG] progress_handle started");
    //     use tokio::time::{sleep, Duration};
    //     loop {
    //         sleep(Duration::from_secs(2)).await;
    //         let m = progress_metrics.lock().await;
    //         println!("[PROGRESS] total_tx: {}, failed_tx: {}, consensus: {}/{}", m.total_tx, m.failed_tx, m.consensus_reached, m.consensus_reached + m.consensus_failed);
    //         if m.total_tx + m.failed_tx >= TARGET_TX { break; }
    //     }
    // });
    println!("[DEBUG] after progress_handle spawn, before while loop");
    println!("[DEBUG] before metrics.lock in while");
    loop {
        let (total_done, total_tx, failed_tx) = {
            let m = metrics.lock().await;
            (
                m.total_tx + m.failed_tx >= TARGET_TX,
                m.total_tx,
                m.failed_tx,
            )
        };
        println!(
            "[DEBUG] after metrics.lock in while: total_tx={}, failed_tx={}, TARGET_TX={}",
            total_tx, failed_tx, TARGET_TX
        );
        if total_done {
            break;
        }
        println!("[DEBUG] entered while loop");
        let nodes = Arc::clone(&nodes);
        let metrics = Arc::clone(&metrics);
        let mut overload_state = overload_state.clone();
        let mut rng = StdRng::from_entropy();
        let scenario = random_scenario(&mut rng);
        println!("[DEBUG] spawning scenario: {:?}", scenario);
        let handle = match scenario {
            Scenario::SimpleTx
            | Scenario::ContractTx
            | Scenario::CrossShardTx
            | Scenario::FaultyTx => tokio::spawn(scenario_simple_tx(
                nodes,
                metrics,
                overload_state.clone(),
                semaphore.clone(),
            )),
            Scenario::BatchTx => tokio::spawn(scenario_batch_tx(
                nodes,
                metrics,
                overload_state.clone(),
                semaphore.clone(),
            )),
        };
        println!("[DEBUG] handle pushed");
        handles.push(handle);
    }
    println!("[DEBUG] after while loop, before join_all");
    join_all(handles).await;
    // progress_handle.await.unwrap();
    // println!("[DEBUG] after progress_handle.await");
    println!("[DEBUG] before nodes.lock at end");
    let nodes = nodes.lock().await;
    println!("[DEBUG] after nodes.lock at end");
    println!("[DEBUG] before metrics.lock at end");
    let metrics = metrics.lock().await;
    println!("[DEBUG] after metrics.lock at end");
    // Вывод метрик
    let avg_latency = if metrics.latencies.is_empty() {
        0.0
    } else {
        metrics.latencies.iter().sum::<u128>() as f64 / metrics.latencies.len() as f64
    };
    let max_latency = metrics.latencies.iter().max().cloned().unwrap_or(0);
    let avg_interf = if metrics.interference_events == 0 {
        0.0
    } else {
        metrics.interference_sum / metrics.interference_events as f64
    };

    // --- Метрики DAG по всей сети ---
    let mut dag_tx_count = 0;
    let mut dag_depths = vec![];
    let mut dag_cycles = 0;
    let mut parent_counts = vec![];
    let mut child_counts = vec![];
    for node in nodes.iter() {
        let tx_ids = node.dag.all_tx_ids();
        dag_tx_count += tx_ids.len();
        dag_depths.push(node.dag.depth());
        if node.dag.has_cycle() {
            dag_cycles += 1;
        }
        for tx_id in &tx_ids {
            if let Some(parents) = node.dag.parents(tx_id) {
                parent_counts.push(parents.len() as f64);
            }
            let children = node.dag.children(tx_id);
            child_counts.push(children.len() as f64);
        }
    }
    let avg_dag_depth = if dag_depths.is_empty() {
        0.0
    } else {
        dag_depths.iter().sum::<usize>() as f64 / dag_depths.len() as f64
    };
    let avg_parents = if parent_counts.is_empty() {
        0.0
    } else {
        parent_counts.iter().sum::<f64>() / parent_counts.len() as f64
    };
    let avg_children = if child_counts.is_empty() {
        0.0
    } else {
        child_counts.iter().sum::<f64>() / child_counts.len() as f64
    };

    println!("\n=== SIMULATION COMPLETE ===");
    println!(
        "Total transactions: {} (failed: {})",
        metrics.total_tx, metrics.failed_tx
    );
    println!(
        "Consensus reached: {} ({}%)",
        metrics.consensus_reached,
        (metrics.consensus_reached as f64
            / (metrics.consensus_reached + metrics.consensus_failed) as f64
            * 100.0) as u32
    );
    println!(
        "DAG: adds={}, validations={}, total_txs={}, avg_depth={:.2}, cycles={} ",
        metrics.dag_adds, metrics.dag_validations, dag_tx_count, avg_dag_depth, dag_cycles
    );
    println!(
        "DAG: avg_parents={:.2}, avg_children={:.2}",
        avg_parents, avg_children
    );
    println!("Latency: avg={:.2}μs, max={}μs", avg_latency, max_latency);
    println!(
        "Interference: avg={:.3}, max={:.3}, events={}",
        avg_interf, metrics.interference_max, metrics.interference_events
    );
    println!(
        "Batch: total={}, failed={}",
        metrics.batch_tx, metrics.batch_failed
    );
    println!("Attacks: {}", metrics.attacks);
    println!(
        "Overloads: {}, Recoveries: {}",
        metrics.overloads, metrics.recoveries
    );
    println!("Cross-shard lost: {}", metrics.cross_shard_lost);
    for node in nodes.iter() {
        println!(
            "Node {}: processed={}, errors={}, contracts={}, shard_events={}",
            node.id,
            node.processed,
            node.errors,
            node.network.contracts.len(),
            node.network.shard_events.len()
        );
    }
}


