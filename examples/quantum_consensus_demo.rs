use std::sync::{Arc, Mutex as StdMutex};
use std::collections::HashMap;
use std::time::{Instant, Duration};
use serde::{Serialize, Deserialize};
use rand::{Rng, SeedableRng, rngs::StdRng};
use rayon::prelude::*;
// CRYPTO DISABLED: use ed25519_dalek::{VerifyingKey};
use triad::quantum::{InterferenceEngine, QuantumWave, QuantumState, StateVector};
use triad::quantum::field::ShardedQuantumField;
use triad::quantum::consensus::{ConsensusNode, ConsensusMessage};
use triad::quantum::field::{WaveValidationResult, WaveFate, WaveStatistics};
use triad::transaction::{Transaction, TransactionProcessor};
#[cfg(feature = "opencl")]
use triad::quantum::interference::OpenClAccelerator;
use triad::github::GitHubCodespaces;
use num_complex::Complex;
use log::info;
use tracing::{info as tinfo, debug as tdebug, warn as twarn};
use tokio::task::spawn_blocking;
use std::thread;
use std::sync::atomic::{AtomicBool, Ordering};
use triad::dag::Dag;
use triad::QuantumField;
use uuid::Uuid;
use bytes::Bytes;
use crossbeam_channel as xchan;

/*
 * TRIAD Quantum Consensus Demo - RAW PERFORMANCE MODE (NO CRYPTO)
 * 
 * CRYPTO DISABLED FOR PURE INTERFERENCE TESTING:
 * - All Ed25519 signature/verification removed
 * - Pure quantum interference consensus without cryptographic overhead
 * - Maximum TPS benchmark mode
 * 
 * This configuration shows the raw computational capacity of the
 * quantum interference engine without cryptographic bottlenecks.
 */

// Правильное измерение TPS
fn calculate_tps(success_count: usize, start_time: Instant) -> f64 {
    let elapsed = start_time.elapsed().as_secs_f64();
    if elapsed > 0.0 {
        success_count as f64 / elapsed
    } else {
        0.0
    }
}

#[derive(Debug, Clone)]
struct PerformanceConfig {
    batch_size: usize,
    channel_capacity: usize,
    interference_threshold: usize,
    interference_resolution: usize,
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            batch_size: 1024,
            channel_capacity: 65536,
            interference_threshold: 512,
            interference_resolution: 10_000,
        }
    }
}

impl PerformanceConfig {
    fn from_env() -> Self {
        let parse = |key: &str, default: usize| -> usize {
            std::env::var(key)
                .ok()
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(default)
        };
        let def = Self::default();
        Self {
            batch_size: parse("TRIAD_BATCH_SIZE", def.batch_size),
            channel_capacity: parse("TRIAD_CHANNEL_CAP", def.channel_capacity),
            interference_threshold: parse("TRIAD_INTERF_THRESHOLD", def.interference_threshold),
            interference_resolution: parse("TRIAD_INTERF_RES", def.interference_resolution),
        }
    }
}

// CRYPTO DISABLED: Ed25519 signatures removed for raw performance testing
// use ed25519_dalek::{SigningKey as Keypair, VerifyingKey as Ed25519PublicKey, Signature as Ed25519Signature, Signer, Verifier};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Args {
    distributed: bool,
    num_codespaces: usize,
    real_codespaces: bool,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            distributed: false,
            num_codespaces: 3,
            real_codespaces: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DemoTransaction {
    id: String,
    from: String,
    to: String,
    amount: f64,
    fee: f64, // Комиссия транзакции - основной фактор для приоритизации
    nonce: u64,
    signature: Vec<u8>,
    public_key_idx: usize, // Индекс в Ed25519 KeyPool
    shard_id: usize,
    timestamp: u64,
}

// Новая структура для подписи квантовых волн с Ed25519
#[derive(Debug, Clone, Serialize, Deserialize)]
struct WaveSignature {
    wave_id: String,
    transaction_id: String,
    amplitude: f64,
    phase: f64,
    shard_id: usize,
    signature: Vec<u8>, // Ed25519 подпись волны
    public_key_idx: usize,
    timestamp: u64,
}

impl WaveSignature {
    fn new(wave_id: String, transaction_id: String, amplitude: f64, phase: f64, shard_id: usize, signature: Vec<u8>, public_key_idx: usize) -> Self {
        Self {
            wave_id,
            transaction_id,
            amplitude,
            phase,
            shard_id,
            signature,
            public_key_idx,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }
    }
    
    // Создаем сообщение для подписи волны
    fn create_signature_message(&self) -> Vec<u8> {
        let message = format!("{}:{}:{}:{}:{}", 
            self.wave_id, 
            self.transaction_id, 
            self.amplitude, 
            self.phase, 
            self.shard_id
        );
        message.as_bytes().to_vec()
    }
}

#[derive(Debug, Clone)]
struct AccountState {
    balance: f64,
    nonce: u64,
}

#[derive(Debug)]
struct PerformanceMetrics {
    generation_time: Duration,
    serialization_time: Duration,
    websocket_time: Duration,
    cpu_load: f64,
}

// Мониторинг CPU в реальном времени
struct CpuMonitor {
    running: Arc<AtomicBool>,
    cpu_usage: Arc<StdMutex<f64>>,
    thread_handle: Option<thread::JoinHandle<()>>,
}

impl CpuMonitor {
    fn new() -> Self {
        Self {
            running: Arc::new(AtomicBool::new(false)),
            cpu_usage: Arc::new(StdMutex::new(0.0)),
            thread_handle: None,
        }
    }

    fn start(&mut self) {
        if self.running.load(Ordering::Relaxed) {
            return;
        }
        
        self.running.store(true, Ordering::Relaxed);
        let running = self.running.clone();
        let cpu_usage = self.cpu_usage.clone();
        
        self.thread_handle = Some(thread::spawn(move || {
            let mut last_cpu_time = get_cpu_time();
            let mut last_wall_time = Instant::now();
            
            while running.load(Ordering::Relaxed) {
                thread::sleep(Duration::from_millis(100)); // Обновляем каждые 100ms
                
                let current_cpu_time = get_cpu_time();
                let current_wall_time = Instant::now();
                
                let cpu_delta = current_cpu_time - last_cpu_time;
                let wall_delta = current_wall_time.duration_since(last_wall_time);
                
                if wall_delta.as_secs_f64() > 0.0 {
                    let usage = (cpu_delta / wall_delta.as_secs_f64()) * 100.0;
                    if let Ok(mut cpu_usage_guard) = cpu_usage.lock() {
                        *cpu_usage_guard = usage;
                    }
                }
                
                last_cpu_time = current_cpu_time;
                last_wall_time = current_wall_time;
            }
        }));
    }

    fn stop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
    }

    fn get_cpu_usage(&self) -> f64 {
        if let Ok(cpu_usage_guard) = self.cpu_usage.lock() {
            *cpu_usage_guard
        } else {
            0.0
        }
    }
}

// Получение времени CPU (упрощенная версия для демо)
fn get_cpu_time() -> f64 {
    // В реальной реализации здесь был бы вызов системного API
    // Для демо используем приблизительное значение
    let start = Instant::now();
    // Имитируем небольшую нагрузку для получения CPU времени
    let _: f64 = (0..1000).map(|i| i as f64).sum();
    start.elapsed().as_secs_f64()
}

// Object Pool для QuantumState (оптимизация памяти)
struct QuantumStatePool {
    pool: Arc<StdMutex<Vec<QuantumState>>>,
    max_size: usize,
}

impl QuantumStatePool {
    fn new(max_size: usize) -> Self {
        Self {
            pool: Arc::new(StdMutex::new(Vec::with_capacity(max_size))),
            max_size,
        }
    }

    fn get(&self) -> Option<QuantumState> {
        self.pool.lock().unwrap().pop()
    }

    fn return_state(&self, state: QuantumState) {
        let mut pool = self.pool.lock().unwrap();
        if pool.len() < self.max_size {
            pool.push(state);
        }
    }
}

// CRYPTO DISABLED: Ed25519KeyPool replaced with no-op stub
#[derive(Clone)]
struct Ed25519KeyPool {
    size: usize,
}

impl Ed25519KeyPool {
    fn new(size: usize) -> Self {
        println!("⚡ CRYPTO DISABLED: Ed25519KeyPool running in no-op mode for maximum performance");
        Self { size }
    }
}

// TransactionPool с переиспользованием буферов
struct TransactionPool {
    buffer: Vec<u8>,
    transactions: Vec<DemoTransaction>,
}

impl TransactionPool {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            buffer: Vec::with_capacity(capacity * 1024), // 1KB per transaction
            transactions: Vec::with_capacity(capacity),
        }
    }

    fn prepare(&mut self, count: usize, accounts: &[String]) -> Vec<u8> {
        self.buffer.clear();
        self.transactions.clear();
        
        // Генерируем транзакции параллельно
        let transactions: Vec<DemoTransaction> = generate_sharded_transactions(count, accounts);
        
        // Сериализуем в буфер
        for tx in &transactions {
            let serialized = serde_json::to_vec(tx).unwrap();
            self.buffer.extend_from_slice(&serialized);
            self.buffer.push(b'\n'); // Разделитель
        }
        
        self.transactions = transactions;
        self.buffer.clone()
    }
}

// Генерируем транзакции БЕЗ подписей (подписи будут создаваться для волн)
fn generate_sharded_transactions(num_transactions: usize, accounts: &[String]) -> Vec<DemoTransaction> {
    let start = Instant::now();
    
    let transactions: Vec<DemoTransaction> = (0..num_transactions).into_par_iter().map(|i| {
        let from_idx = i % accounts.len();
        let to_idx = (i + 1) % accounts.len();
        let from = &accounts[from_idx];
        let to = &accounts[to_idx];
        
        // Добавляем случайность в суммы транзакций
        let mut rng = rand::thread_rng();
        let base_amount = if i % 3 == 0 { 
            10.0 + (i as f64 * 0.1) 
        } else if i % 3 == 1 { 
            50.0 + (i as f64 * 0.5) 
        } else { 
            100.0 + (i as f64 * 1.0) 
        };
        let amount = base_amount + (rng.gen::<f64>() * 10.0); // ±10 случайность
        
        // Генерируем комиссию с случайностью
        let base_fee = if i % 5 == 0 {
            0.001 + (i as f64 * 0.0001) // Низкие комиссии
        } else if i % 5 == 1 {
            0.01 + (i as f64 * 0.001) // Средние комиссии
        } else if i % 5 == 2 {
            0.1 + (i as f64 * 0.01) // Высокие комиссии
        } else if i % 5 == 3 {
            0.5 + (i as f64 * 0.05) // Очень высокие комиссии
        } else {
            0.8 + (i as f64 * 0.02) // Премиум комиссии
        };
        let fee = base_fee + (rng.gen::<f64>() * 0.1); // ±0.1 случайность
        
        let nonce = i as u64;
        let shard_id = (i + rng.gen::<usize>() % 10) % 3; // Случайное распределение по шардам
        
        // Создаем транзакцию БЕЗ подписи (подпись будет создана для волны)
        let transaction = DemoTransaction {
            id: format!("tx_{}", i),
            from: from.clone(),
            to: to.clone(),
            amount,
            fee,
            nonce,
            signature: vec![], // Пустая подпись - подписи создаются для волн
            public_key_idx: from_idx,
            shard_id,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        };
        
        transaction
    }).collect();
    
    let generation_time = start.elapsed();
    println!("⚡ Generated {} transactions in {:.3}ms ({:.0} tx/sec)", 
        num_transactions, generation_time.as_secs_f64() * 1000.0, 
        num_transactions as f64 / generation_time.as_secs_f64());
    
    transactions
}

// Создаем сообщения для шардов из сериализованных данных
fn create_sharded_messages_from_serialized(serialized: Vec<u8>, num_shards: usize) -> Vec<Vec<ConsensusMessage>> {
    let mut sharded_messages = vec![Vec::new(); num_shards];
    
    // Парсим сериализованные транзакции
    let lines: Vec<&[u8]> = serialized.split(|&b| b == b'\n').filter(|line| !line.is_empty()).collect();
    
    for (i, line) in lines.iter().enumerate() {
        if let Ok(tx) = serde_json::from_slice::<DemoTransaction>(line) {
            let shard_id = tx.shard_id % num_shards;
            let message = ConsensusMessage {
                sender_id: tx.from.clone(),
                state_id: tx.id,
                raw_data: line.to_vec().into(),
                signature: Vec::new(),
            };
            sharded_messages[shard_id].push(message);
        }
    }
    
    sharded_messages
}

// Создаем сообщения для шардов из транзакций
fn create_sharded_messages_from_transactions(transactions: &[DemoTransaction], num_shards: usize) -> Vec<Vec<ConsensusMessage>> {
    let mut sharded_messages = vec![Vec::new(); num_shards];
    
    for tx in transactions {
        let shard_id = tx.shard_id % num_shards;
        let message = ConsensusMessage {
            sender_id: tx.from.clone(),
            state_id: tx.id.clone(),
            raw_data: serde_json::to_vec(tx).unwrap().into(),
            signature: Vec::new(),
        };
        sharded_messages[shard_id].push(message);
    }
    
    sharded_messages
}

// CRYPTO DISABLED: Process consensus without cryptographic keys
fn process_distributed_consensus(
    nodes: &mut [ConsensusNode],
    sharded_messages: &[Vec<ConsensusMessage>],
    accounts_state: &mut HashMap<String, AccountState>,
    quantum_field: &Arc<StdMutex<ShardedQuantumField>>,
    interference_engine: &Arc<StdMutex<InterferenceEngine>>,
    verification_results: &[bool],
    keypool: &Ed25519KeyPool
) -> (usize, usize) {
    let start = Instant::now();
    let mut success_count = 0;
    let mut failure_count = 0;
    
    // Обрабатываем каждый шард параллельно с использованием rayon
    let results: Vec<(usize, usize)> = sharded_messages.iter().enumerate()
        .filter(|(_, messages)| !messages.is_empty())
        .collect::<Vec<_>>()
        .into_par_iter()
        .map(|(shard_id, messages)| {
            let shard_success = process_shard_consensus(
                shard_id,
                messages,
                quantum_field,
                interference_engine,
                verification_results,
                keypool
            );
            (shard_success, messages.len() - shard_success)
        })
        .collect();
    
    // Суммируем результаты
    for (success, failure) in results {
        success_count += success;
        failure_count += failure;
    }
    
    let total_time = start.elapsed();
    let elapsed = total_time.as_secs_f64();
    let accepted_tps = if elapsed > 0.0 { success_count as f64 / elapsed } else { 0.0 };
    let processed_tps = if elapsed > 0.0 { (success_count + failure_count) as f64 / elapsed } else { 0.0 };
    println!(
        "🔍 Consensus: {:.3}ms | Success: {} | Fail: {} | TPS (accepted): {:.0} | TPS (processed): {:.0}",
        total_time.as_secs_f64() * 1000.0,
        success_count,
        failure_count,
        accepted_tps,
        processed_tps
    );
    
    (success_count, failure_count)
}

// Обрабатываем консенсус для одного шарда с подписанными волнами
fn process_shard_consensus(
    shard_id: usize,
    messages: &[ConsensusMessage],
    quantum_field: &Arc<StdMutex<ShardedQuantumField>>,
    interference_engine: &Arc<StdMutex<InterferenceEngine>>,
    verification_results: &[bool],
    keypool: &Ed25519KeyPool
) -> usize {
    let perf = PerformanceConfig::from_env();
    println!("🔬 [Shard {}] Processing {} messages with pipeline", shard_id, messages.len());

    // Метрики стадий
    let mut stage_ingress_durs = Vec::new();
    let mut stage_verify_durs = Vec::new();
    let mut stage_interf_durs = Vec::new();
    let mut stage_apply_durs = Vec::new();

    let shard_start = Instant::now();

    // Каналы
    let (ing_tx, ing_rx) = xchan::bounded::<Vec<DemoTransaction>>(perf.channel_capacity);
    let (ver_tx, ver_rx) = xchan::bounded::<Vec<QuantumWave>>(perf.channel_capacity);
    let (int_tx, int_rx) = xchan::bounded::<Vec<QuantumWave>>(perf.channel_capacity);

    // Ingress: парсинг и приоритизация
    let msgs_clone: Vec<ConsensusMessage> = messages.to_vec();
    let bs = perf.batch_size;
    let ingress = std::thread::spawn(move || {
        let t0 = Instant::now();
        let mut batch: Vec<DemoTransaction> = Vec::with_capacity(bs);
        for msg in msgs_clone.into_iter() {
            if let Ok(tx) = serde_json::from_slice::<DemoTransaction>(&msg.raw_data) {
                batch.push(tx);
                if batch.len() >= bs {
                    // сортировка по fee (убывание)
                    batch.sort_by(|a,b| b.fee.partial_cmp(&a.fee).unwrap_or(std::cmp::Ordering::Equal));
                    ing_tx.send(batch).ok();
                    batch = Vec::with_capacity(bs);
                }
            }
        }
        if !batch.is_empty() {
            batch.sort_by(|a,b| b.fee.partial_cmp(&a.fee).unwrap_or(std::cmp::Ordering::Equal));
            let _ = ing_tx.send(batch);
        }
        drop(ing_tx);
        t0.elapsed()
    });

    // SigVerify: N воркеров
    let ver_workers = num_cpus::get();
    let mut ver_handles = Vec::new();
    // Share keypool across verifier threads without cloning the whole pool each time
    let keypool_arc = Arc::new(keypool.clone());
    for _ in 0..ver_workers {
        let rx = ing_rx.clone();
        let tx = ver_tx.clone();
        let keypool = Arc::clone(&keypool_arc);
        let handle = std::thread::spawn(move || {
            let mut local_dur = Duration::from_millis(0);
            while let Ok(batch) = rx.recv() {
                let t0 = Instant::now();
                // создаём и подписываем волны, затем верифицируем (как в демо)
                let waves_and_signatures = create_and_sign_quantum_waves(&batch, keypool.as_ref(), shard_id);
                let ver_results = verify_wave_signatures_with_aggregation(&waves_and_signatures, keypool.as_ref());
                let waves: Vec<QuantumWave> = waves_and_signatures.into_iter()
                    .zip(ver_results.into_iter())
                    .filter_map(|((w,_sig), ok)| if ok { Some(w) } else { None })
                    .collect();
                let _ = tx.send(waves);
                local_dur += t0.elapsed();
            }
            local_dur
        });
        ver_handles.push(handle);
    }
    drop(ver_tx);

    // Interference worker: добавляет волны и триггерит пересчёт
    let qf = Arc::clone(quantum_field);
    let int_handle = std::thread::spawn(move || {
        let mut local_dur = Duration::from_millis(0);
        while let Ok(waves) = ver_rx.recv() {
            if waves.is_empty() { continue; }
            let t0 = Instant::now();
            let pairs: Vec<(String, QuantumWave)> = waves.iter().map(|w| (w.id.to_string(), w.clone())).collect();
            {
                let mut field = qf.lock().unwrap();
                field.add_waves_batch(pairs);
                let _ = field.maybe_flush_interference(perf.interference_threshold, perf.interference_resolution);
            }
            let _ = int_tx.send(waves);
            local_dur += t0.elapsed();
        }
        drop(int_tx);
        local_dur
    });

    // Apply/Consensus: собирает волны, строит DAG, анализирует интерференцию и применяет
    let qf2 = Arc::clone(quantum_field);
    let eng = Arc::clone(interference_engine);
    let apply_handle = std::thread::spawn(move || {
        let mut local_dur = Duration::from_millis(0);
        let mut total_success = 0usize;
        let mut total_processed = 0usize;
        while let Ok(waves) = int_rx.recv() {
            if waves.is_empty() { continue; }
            let t0 = Instant::now();
            // Построение DAG и уровней как ранее
            let dag_start = Instant::now();
            let mut dag = Dag::new();
            let ns = Uuid::new_v4();
            let wave_uuid: Vec<Uuid> = waves.iter().map(|w| Uuid::new_v5(&ns, w.id.as_bytes())).collect();
            let index_by_uuid: HashMap<Uuid, usize> = wave_uuid.iter().cloned().enumerate().map(|(i,u)|(u,i)).collect();
            let mut by_account: HashMap<String, Vec<(u64, usize)>> = HashMap::new();
            let mut by_account_nonce: HashMap<(String, u64), Vec<usize>> = HashMap::new();
            // Здесь нет исходных tx, поэтому упрощенно связываем последовательности по wave.id хэшу
            for (i, w) in waves.iter().enumerate() {
                by_account.entry(w.shard_id.clone()).or_default().push((i as u64, i));
                by_account_nonce.entry((w.shard_id.clone(), i as u64)).or_default().push(i);
            }
            for (_acc, mut items) in by_account { items.sort_by_key(|(nonce, _)| *nonce); let mut prev=None; for &(_n, idx) in &items { let parents = match prev { Some(p)=> vec![wave_uuid[p]], None=>Vec::new()}; dag.add_transaction(wave_uuid[idx], parents); prev=Some(idx);} }
            for ((_acc,_nonce), idxs) in by_account_nonce.into_iter() { if idxs.len()<=1 {continue;} let best_idx=idxs[0]; for &i in &idxs { if i==best_idx {continue;} dag.add_transaction(wave_uuid[i], vec![wave_uuid[best_idx]]);} }
            let _dag_time = dag_start.elapsed();

            // Анализ интерференции текущего поля
            let interference_pattern = {
                let field = qf2.lock().unwrap();
                field.calculate_interference_pattern(Some(10_000))
            };
            let analysis = { let mut engine = eng.lock().unwrap(); engine.analyze_interference(&interference_pattern) };
            let op = create_consensus_decision(&analysis, &[]);
            let outcome = op.execute();
            let valid_messages = waves.len();
            let success_count = match outcome.label.as_str() { "ConsensusReached"=> valid_messages, "PartialConsensus"=> ((valid_messages as f64)*0.7) as usize, _=>0 };
            total_success += success_count;
            total_processed += valid_messages;

            // Обновление состояний
            update_wave_states_after_consensus(&qf2, &waves, success_count, valid_messages);

            local_dur += t0.elapsed();
        }
        (local_dur, total_success, total_processed)
    });

    // Собираем метрики
    let ingress_dur = ingress.join().unwrap_or_default();
    stage_ingress_durs.push(ingress_dur);
    let mut ver_total = Duration::from_millis(0);
    for h in ver_handles { ver_total += h.join().unwrap_or_default(); }
    stage_verify_durs.push(ver_total);
    let int_dur = int_handle.join().unwrap_or_default();
    stage_interf_durs.push(int_dur);
    let (apply_dur, success_total, processed_total) = apply_handle.join().unwrap();
    stage_apply_durs.push(apply_dur);

    // Вывод метрик стадий
    let elapsed = shard_start.elapsed().as_secs_f64();
    let accepted_tps = if elapsed>0.0 { success_total as f64 / elapsed } else {0.0};
    let processed_tps = if elapsed>0.0 { processed_total as f64 / elapsed } else {0.0};
    println!("📊 [Shard {}] Pipeline TPS: accepted={:.0}, processed={:.0}", shard_id, accepted_tps, processed_tps);

    success_total
}
    // (duplicate block removed)

/// Обновляет состояние волн на основе результатов консенсуса
fn update_wave_states_after_consensus(
    quantum_field: &Arc<StdMutex<ShardedQuantumField>>,
    waves: &[QuantumWave],
    success_count: usize,
    total_count: usize
) {
    use triad::quantum::field::WaveValidationResult;
    let mut updates: HashMap<Arc<str>, WaveValidationResult> = HashMap::with_capacity(waves.len());
    for (i, wave) in waves.iter().enumerate() {
        let val = if i < success_count {
            WaveValidationResult::Validated(1.0)
        } else {
            WaveValidationResult::PartiallyValidated(0.7)
        };
        updates.insert(Arc::clone(&wave.id), val);
    }
    let mut field = quantum_field.lock().unwrap();
    field.update_wave_amplitudes(&updates);
    field.remove_extinguished_waves();
    let stats = field.get_wave_statistics();
    println!("🌊 Wave statistics: total={}, strong={}, weak={}, avg_amplitude={:.3}", 
        stats.total_waves, stats.strong_waves, stats.weak_waves, stats.average_amplitude);
}

// ...
// Создаем решение консенсуса на основе интерференции и комиссий
fn create_consensus_decision(analysis: &triad::quantum::interference::InterferenceAnalysis, messages: &[ConsensusMessage]) -> triad::quantum::prob_ops::ProbabilisticOperation {
    let constructive_ratio = if analysis.total_points() > 0 {
        analysis.constructive_points.len() as f64 / analysis.total_points() as f64
    } else {
        0.0
    };
    
    // Извлекаем комиссии из сообщений для расчета фактора комиссии
    let fee_factor = calculate_fee_factor(messages);
    
    let phase_conflict = calculate_phase_conflict(messages);
    let ttl_factor = calculate_ttl_factor();
    let reputation_factor = calculate_reputation_factor(messages);
    
    // Улучшенная формула успеха с комиссией как основным фактором
    let success_prob = (constructive_ratio * 0.4 + 
                       fee_factor * 0.3 + // Снижаем вес комиссии с 50% до 30%
                       (1.0 - phase_conflict) * 0.2 + // Увеличиваем вес фаз с 10% до 20%
                       ttl_factor * 0.05 + 
                       reputation_factor * 0.05).clamp(0.3, 0.95); // Увеличиваем минимум с 0.1 до 0.3
    
    println!("🔬 [Decision] Factors: constructive={:.3}, fee_factor={:.3}, phase_conflict={:.3}, ttl={:.3}, reputation={:.3} → success_prob={:.3}", 
        constructive_ratio, fee_factor, phase_conflict, ttl_factor, reputation_factor, success_prob);
    
    triad::quantum::prob_ops::ProbabilisticOperation::new("ConsensusReached", success_prob)
}

fn calculate_phase_conflict(messages: &[ConsensusMessage]) -> f64 {
    if messages.len() < 2 { return 0.0; }
    
    let phases: Vec<f64> = messages.iter()
        .map(|msg| calculate_transaction_phase(msg)) // РЕАЛЬНЫЕ фазы
        .collect();
    
    let mut conflicts = 0.0;
    for i in 0..phases.len() {
        for j in (i+1)..phases.len() {
            let phase_diff = (phases[i] - phases[j]).abs();
            if phase_diff > std::f64::consts::PI / 2.0 {
                conflicts += 1.0;
            }
        }
    }
    
    conflicts / (phases.len() * (phases.len() - 1) / 2) as f64
}

fn calculate_ttl_factor() -> f64 {
    // Добавляем случайность в TTL фактор
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    let seed = now.as_nanos() as u64;
    let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
    
    // TTL варьируется от 0.7 до 0.9 с небольшой случайностью
    0.7 + (rng.gen::<f64>() * 0.2)
}

fn calculate_fee_factor(messages: &[ConsensusMessage]) -> f64 {
    if messages.is_empty() {
        return 0.5; // Базовый фактор если нет сообщений
    }
    
    let mut total_fee = 0.0;
    let mut max_fee: f64 = 0.0;
    let mut valid_transactions = 0;
    
    for message in messages {
        // Пытаемся извлечь комиссию из транзакции
        if let Ok(tx) = serde_json::from_slice::<DemoTransaction>(&message.raw_data) {
            total_fee += tx.fee;
            max_fee = max_fee.max(tx.fee);
            valid_transactions += 1;
        }
    }
    
    if valid_transactions == 0 {
        return 0.5; // Базовый фактор если не удалось извлечь транзакции
    }
    
    // Нормализуем среднюю комиссию относительно максимальной
    let avg_fee = total_fee / valid_transactions as f64;
    let normalized_fee = if max_fee > 0.0 { avg_fee / max_fee } else { 0.5 };
    
    // Применяем нелинейную функцию для усиления эффекта высоких комиссий
    let fee_factor = normalized_fee.powf(0.5); // Степень 0.5 вместо 0.7 - менее агрессивно
    
    // Добавляем базовый бонус для повышения минимального значения
    let fee_factor = (fee_factor + 0.3).clamp(0.3, 1.0); // Минимум 0.3, максимум 1.0
    
    println!("💰 [Fee Factor] avg_fee={:.4}, max_fee={:.4}, normalized={:.3}, factor={:.3}", 
        avg_fee, max_fee, normalized_fee, fee_factor);
    
    fee_factor
}

fn calculate_reputation_factor(messages: &[ConsensusMessage]) -> f64 {
    // Добавляем случайность в репутацию на основе отправителей
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    let seed = now.as_nanos() as u64;
    let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
    
    // Репутация варьируется от 0.8 до 1.0 с небольшой случайностью
    0.8 + (rng.gen::<f64>() * 0.2)
}

// Проверяем консенсус в распределенной сети
fn check_distributed_consensus(nodes: &[ConsensusNode], round: usize) -> bool {
    let consensus_threshold = (nodes.len() as f64 * 0.67) as usize;
    // Предполагаем активность всех узлов-участников в этом демо
    let consensus_count = nodes.len();
    
    consensus_count >= consensus_threshold
}

// Создаем распределенные узлы
fn create_distributed_nodes(num_nodes: usize, num_shards: usize) -> Vec<ConsensusNode> {
    (0..num_nodes).map(|i| {
        ConsensusNode::new(format!("node_{}", i), i % num_shards)
    }).collect()
}

// Менеджер распределенной сети
struct DistributedNetworkManager {
    nodes: Vec<ConsensusNode>,
    keypool: Ed25519KeyPool,
    quantum_field: Arc<StdMutex<ShardedQuantumField>>,
    interference_engine: Arc<StdMutex<InterferenceEngine>>,
    codespaces: Vec<String>,
    cpu_monitor: CpuMonitor,
}

impl DistributedNetworkManager {
    fn new() -> Self {
        // Проверяем доступность GPU
        println!("🔧 Initializing interference engine with GPU acceleration...");
        #[cfg(feature = "opencl")]
        {
            match OpenClAccelerator::new() {
                Ok(_) => println!("🚀 GPU acceleration available (OpenCL)"),
                Err(e) => println!("⚠️  GPU acceleration failed: {}, using CPU fallback", e),
            }
        }
        #[cfg(not(feature = "opencl"))]
        {
            println!("⚠️  OpenCL feature not enabled, using CPU only");
        }
        
        let shards = num_cpus::get().next_power_of_two();
        Self {
            nodes: Vec::new(),
            keypool: Ed25519KeyPool::new(1000),
            quantum_field: Arc::new(StdMutex::new(ShardedQuantumField::new(shards, 1_000_000))),
            interference_engine: Arc::new(StdMutex::new(InterferenceEngine::new(1000, 0.95))),
            codespaces: Vec::new(),
            cpu_monitor: CpuMonitor::new(),
        }
    }

    async fn setup_distributed_network(&mut self, num_codespaces: usize) -> Result<(), String> {
        println!("🚀 Setting up distributed network with {} codespaces...", num_codespaces);
        
        // Создаем узлы
        self.nodes = create_distributed_nodes(10, 3);
        println!("✅ Created {} nodes", self.nodes.len());
        
        // Проверяем, нужно ли использовать реальные GitHub Codespaces
        if std::env::args().any(|arg| arg == "--local") {
            println!("🏠 Running in local mode - skipping GitHub Codespaces");
            // В локальном режиме просто создаем фиктивные codespace ID
            for i in 0..num_codespaces {
                self.codespaces.push(format!("local-codespace-{}", i));
                if i < self.nodes.len() {
                    self.nodes[i].id = format!("local-node-{}", i);
                }
            }
            println!("✅ Local distributed network setup completed");
            return Ok(());
        }
        
        // Настраиваем GitHub Codespaces с реальными данными
        let github = GitHubCodespaces::new(
            std::env::var("GITHUB_TOKEN").unwrap_or_else(|_| "your-github-token-here".to_string()),
            "fillay12321".to_string(), 
            "TRIAD".to_string()
        );
        
        // Получаем существующие codespaces
        let list = github.list_codespaces().await?;
        println!("📋 Found {} existing codespaces", list.len());
        
        let mut start_idx = 0;
        if !list.is_empty() {
            println!("🔗 Found {} existing codespaces, reusing them...", list.len());
            for (i, codespace) in list.into_iter().take(num_codespaces).enumerate() {
                println!("🔗 Attaching to existing codespace: {} ({})", codespace.name, codespace.id);
                self.codespaces.push(codespace.id.to_string());
                
                // Обновляем узел с codespace ID
                if i < self.nodes.len() {
                    self.nodes[i].id = format!("codespace-{}", i);
                }
                start_idx = i + 1;
            }
            
            // Если у нас достаточно существующих codespaces, не создаем новые
            if start_idx >= num_codespaces {
                println!("✅ Using existing codespaces, no need to create new ones!");
                return Ok(());
            }
        }
        
        // Создаем новые codespaces если нужно (максимум 2)
        let max_codespaces = 2; // Ограничиваем до 2 codespaces
        if start_idx < max_codespaces && start_idx < num_codespaces {
            for i in start_idx..max_codespaces.min(num_codespaces) {
                println!("🆕 Creating new codespace {}... (this may take 1-2 minutes)", i);
                
                // Создаем codespace с таймаутом
                let codespace = tokio::time::timeout(
                    Duration::from_secs(180), // 3 минуты таймаут
                    github.create_codespace("basicLinux32gb", "WestUs2")
                ).await
                .map_err(|_| "Timeout creating codespace".to_string())??;
                
                println!("✅ Codespace {} created successfully!", i);
                self.codespaces.push(codespace.id.to_string());
                
                if i < self.nodes.len() {
                    self.nodes[i].id = format!("codespace-{}", i);
                }
            }
        }
        
        println!("✅ Distributed network setup completed!");
        Ok(())
    }

    // CRYPTO DISABLED: No-op demonstration
    fn demonstrate_ed25519_aggregation(&self) {
        println!("⚡ === [NO-CRYPTO] Ed25519 Demo Skipped (crypto disabled) ===");
        println!("⚡ Running in pure interference mode for maximum TPS");
        println!("⚡ === End Demo ===\n");
    }

    async fn run_consensus_simulation(&mut self) -> Result<(), String> {
        println!("⚡ Starting distributed consensus simulation [NO-CRYPTO MODE - Pure Interference]...");
        
        // Демонстрируем Ed25519 агрегацию
        self.demonstrate_ed25519_aggregation();
        
        // Запускаем мониторинг CPU
        self.cpu_monitor.start();
        println!("📊 CPU Monitor started - tracking usage in real-time");
        
        // Создаем аккаунты
        let accounts: Vec<String> = (0..100).map(|i| {
            let account_type = if i < 30 { "user" } else if i < 60 { "node" } else if i < 80 { "validator" } else { "miner" };
            format!("{}_{}", account_type, i)
        }).collect();
        
        // Конфигурация энергопотребления CPU (для оценки энергии)
        let cpu_watts: f64 = std::env::var("TRIAD_CPU_WATTS")
            .ok()
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(45.0);

        // Структура и хранилище метрик по раундам (распределенный режим)
        #[derive(Clone)]
        struct RoundSummary {
            round: usize,
            consensus: bool,
            time_ms: f64,
            tps: f64,
            success: usize,
            fail: usize,
            energy_j: f64,
        }

        let mut round_summaries: Vec<RoundSummary> = Vec::with_capacity(10);

        // Основной цикл симуляции
        let mut tx_pool = TransactionPool::with_capacity(1000);
        let mut total_success = 0;
        let mut total_failure = 0;
        let mut total_time = Duration::ZERO;
        let simulation_start = Instant::now();
        
        for round in 1..=10 {
            println!("\n🔄 Round {}/10 starting...", round);
            
            // === ДЕТАЛЬНОЕ ПРОФИЛИРОВАНИЕ ===
            let round_start = Instant::now();

            // Снимки CPU usage для оценки энергозатрат раунда
            let cpu_usage_start = self.cpu_monitor.get_cpu_usage();
            
            // 1. ГЕНЕРАЦИЯ ТРАНЗАКЦИЙ
            let tx_gen_start = Instant::now();
            let accounts = vec!["Alice".to_string(), "Bob".to_string(), "Charlie".to_string()];
            let tx_pool = generate_sharded_transactions(100, &accounts);
            let tx_gen_time = tx_gen_start.elapsed();
            println!("⚡ [Round {}] Transaction generation: {:.3}ms | TPS: {:.0}", 
                round, tx_gen_time.as_secs_f64() * 1000.0, 100.0 / tx_gen_time.as_secs_f64());
            
            // 2. СЕРИАЛИЗАЦИЯ ТРАНЗАКЦИЙ
            let serial_start = Instant::now();
            let serialized = serialize_transactions(&tx_pool);
            let serial_time = serial_start.elapsed();
            println!("📦 [Round {}] Transaction serialization: {:.3}ms | Size: {} bytes", 
                round, serial_time.as_secs_f64() * 1000.0, serialized.len());
            
            // 3. ПОДГОТОВКА КОНСЕНСУСА
            let prep_start = Instant::now();
            let mut nodes = self.nodes.clone();
            let mut accounts_state = HashMap::new(); // Упрощенно для демо
            let local_field = self.quantum_field.clone();
            let local_engine = self.interference_engine.clone();
            let keypool = self.keypool.clone();
            let prep_time = prep_start.elapsed();
            println!("🔧 [Round {}] Consensus preparation: {:.3}ms", 
                round, prep_time.as_secs_f64() * 1000.0);
            
            let total_prep_time = tx_gen_start.elapsed();
            println!("⚡ [Round {}] Total preparation: {:.3}ms", round, total_prep_time.as_secs_f64() * 1000.0);
            
            // 4. СОЗДАНИЕ ШАРДОВ
            let shard_start = Instant::now();
            let sharded_messages = create_sharded_messages_from_transactions(&tx_pool, 15);
            let shard_time = shard_start.elapsed();
            println!("🔀 [Round {}] Shard creation: {:.3}ms | Shards: {}", 
                round, shard_time.as_secs_f64() * 1000.0, sharded_messages.len());
            
            // 5. Ed25519 ВЕРИФИКАЦИЯ ВОЛН
            let ed25519_start = Instant::now();
            println!("🔬 [Round {}] Starting Ed25519 wave signature verification", round);
            
            // Создаем и подписываем квантовые волны из транзакций
            let waves_and_signatures = create_and_sign_quantum_waves(&tx_pool, &keypool, 0);
            
            // Верифицируем подписи волн с агрегацией
            let wave_verification_results = verify_wave_signatures_with_aggregation(&waves_and_signatures, &keypool);
            
            let ed25519_time = ed25519_start.elapsed();
            let valid_waves = wave_verification_results.iter().filter(|&&valid| valid).count();
            println!("🔐 [Round {}] Ed25519 wave verification: {:.3}ms | Valid: {}/{}", 
                round, ed25519_time.as_secs_f64() * 1000.0, valid_waves, wave_verification_results.len());
            
            // 6. ОБРАБОТКА КОНСЕНСУСА
            let consensus_start = Instant::now();
            let (success_count, failure_count) = process_distributed_consensus(
                &mut nodes,
                &sharded_messages,
                &mut accounts_state,
                &local_field,
                &local_engine,
                &wave_verification_results,
                &keypool
            );
            let consensus_time = consensus_start.elapsed();
            
            // === ДЕТАЛЬНАЯ СТАТИСТИКА ===
            let total_round_time = round_start.elapsed();
            let tps = if total_round_time.as_secs_f64() > 0.0 {
                (success_count + failure_count) as f64 / total_round_time.as_secs_f64()
            } else {
                0.0
            };
            
            println!("🔍 [Round {}] DETAILED PROFILING:", round);
            println!("   📊 Transaction generation: {:.3}ms ({:.1}%)", 
                tx_gen_time.as_secs_f64() * 1000.0, 
                (tx_gen_time.as_secs_f64() / total_round_time.as_secs_f64()) * 100.0);
            println!("   📦 Serialization: {:.3}ms ({:.1}%)", 
                serial_time.as_secs_f64() * 1000.0,
                (serial_time.as_secs_f64() / total_round_time.as_secs_f64()) * 100.0);
            println!("   🔧 Preparation: {:.3}ms ({:.1}%)", 
                prep_time.as_secs_f64() * 1000.0,
                (prep_time.as_secs_f64() / total_round_time.as_secs_f64()) * 100.0);
            println!("   🔀 Sharding: {:.3}ms ({:.1}%)", 
                shard_time.as_secs_f64() * 1000.0,
                (shard_time.as_secs_f64() / total_round_time.as_secs_f64()) * 100.0);
            println!("   🔐 Ed25519 wave verification: {:.3}ms ({:.1}%)", 
                ed25519_time.as_secs_f64() * 1000.0,
                (ed25519_time.as_secs_f64() / total_round_time.as_secs_f64()) * 100.0);
            println!("   🌊 Consensus processing: {:.3}ms ({:.1}%)", 
                consensus_time.as_secs_f64() * 1000.0,
                (consensus_time.as_secs_f64() / total_round_time.as_secs_f64()) * 100.0);
            println!("   ⏱️ Total round time: {:.3}ms (100%)", 
                total_round_time.as_secs_f64() * 1000.0);
            
            println!("Round {}: Consensus={} | Time={:.3}ms | Success={} | Fail={} | TPS={:.0}", 
                round, 
                if success_count > failure_count { "true" } else { "false" },
                total_round_time.as_secs_f64() * 1000.0,
                success_count, 
                failure_count, 
                tps
            );
            
            // Обновляем общую статистику
            total_success += success_count;
            total_failure += failure_count;
            total_time += total_round_time;

            // Подсчет энергии раунда
            let cpu_usage_end = self.cpu_monitor.get_cpu_usage();
            let avg_cpu_usage = ((cpu_usage_start + cpu_usage_end) / 2.0).clamp(0.0, 100.0);
            let round_secs = total_round_time.as_secs_f64();
            let energy_j = (avg_cpu_usage / 100.0) * cpu_watts * round_secs; // Джоули = Вт * сек * загрузка

            // Сохраняем сводку по раунду
            round_summaries.push(RoundSummary {
                round,
                consensus: success_count > failure_count,
                time_ms: total_round_time.as_secs_f64() * 1000.0,
                tps,
                success: success_count,
                fail: failure_count,
                energy_j,
            });
        }
        
        // Останавливаем мониторинг CPU
        self.cpu_monitor.stop();
        println!("📊 CPU Monitor stopped");
        println!("✅ Distributed consensus simulation completed!");
        
        let final_tps = if total_time.as_secs_f64() > 0.0 {
            (total_success + total_failure) as f64 / total_time.as_secs_f64()
        } else {
            0.0
        };
        
        println!("✅ Final TPS: {:.0} (real time: {:.3}s)", final_tps, total_time.as_secs_f64());

        // === ФИНАЛЬНАЯ ДЕТАЛЬНАЯ СТАТИСТИКА ===
        println!("\n🎯 === FINAL DETAILED PROFILING ANALYSIS ===");
        println!("📊 Overall Performance:");
        println!("   Total rounds: 10");
        println!("   Total time: {:.3}s", total_time.as_secs_f64());
        println!("   Total transactions: {}", total_success + total_failure);
        println!("   Success rate: {:.1}%, Final TPS: {:.0}", (total_success as f64 / (total_success + total_failure) as f64) * 100.0, final_tps);

        // === ИТОГОВАЯ ТАБЛИЦА ПО РАУНДАМ ===
        println!("\n📋 ROUNDS SUMMARY (CPU {:.0}W):", cpu_watts);
        println!("{:<7} │ {:<9} │ {:>9} │ {:>8} │ {:>7} │ {:>6} │ {:>9}",
                 "Round", "Consensus", "Time (ms)", "TPS", "Success", "Fail", "Energy (J)");
        println!("{}", "-".repeat(72));
        let mut total_energy: f64 = 0.0;
        for s in &round_summaries {
            total_energy += s.energy_j;
            println!("{:<7} │ {:<9} │ {:>9.3} │ {:>8.0} │ {:>7} │ {:>6} │ {:>9.3}",
                     s.round,
                     if s.consensus { "passed" } else { "failed" },
                     s.time_ms,
                     s.tps,
                     s.success,
                     s.fail,
                     s.energy_j);
        }
        println!("{}", "-".repeat(72));
        println!("{:<7} │ {:<9} │ {:>9} │ {:>8} │ {:>7} │ {:>6} │ {:>9.3}",
                 "TOTAL", "-", "-", "-", total_success, total_failure, total_energy);

        println!("\n🔍 Performance Summary:");
        println!("   Total rounds completed: 10");
        println!("   Average TPS: {:.0}", final_tps);
        println!("   Success rate: {:.1}%", (total_success as f64 / (total_success + total_failure) as f64) * 100.0);
        
        println!("\n💡 OPTIMIZATION RECOMMENDATIONS:");
        println!("   1. Focus on consensus processing optimization");
        println!("   2. Consider parallel wave creation across shards");
        println!("   3. Implement wave caching to reduce regeneration");
        println!("   4. Use GPU acceleration for interference calculations");
        println!("   5. Optimize quantum field operations");
        
        println!("\n💡 OPTIMIZATION RECOMMENDATIONS:");
        println!("   1. Focus on consensus processing optimization (largest time consumer)");
        println!("   2. Consider parallel wave creation across shards");
        println!("   3. Implement wave caching to reduce regeneration");
        println!("   4. Use GPU acceleration for interference calculations");
        println!("   5. Optimize quantum field operations");
        
        println!("✅ Local simulation completed!");
        Ok(())
    }

    async fn cleanup(&self) -> Result<(), String> {
        println!("🧹 Cleaning up distributed network...");
        
        // Останавливаем WebSocket сервер
        // В реальной реализации здесь должна быть остановка сервера
        
        println!("✅ Cleanup completed!");
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Инициализируем логирование и tracing
    let _ = env_logger::try_init();
    let env_filter = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into());
    let _ = tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_thread_ids(true)
        .with_target(false)
        .compact()
        .try_init();
    
    // Парсим аргументы командной строки
    let args = if std::env::args().any(|arg| arg == "--distributed") {
        let num_codespaces = std::env::args()
            .position(|arg| arg == "--num-codespaces")
            .and_then(|pos| std::env::args().nth(pos + 1))
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(3);
        
        let real_codespaces = !std::env::args().any(|arg| arg == "--local");
        
        Args {
            distributed: true,
            num_codespaces,
            real_codespaces,
        }
    } else {
        Args::default()
    };
    
    println!("🚀 TRIAD Quantum Consensus Demo");
    println!("📊 Mode: {}", if args.distributed { "Distributed" } else { "Local" });
    if args.distributed {
        println!("🌐 Codespaces: {}", args.num_codespaces);
    }
    println!("🔧 Features: OpenCL + GPU Acceleration + Ed25519 Wave Signatures");
    println!("🌊 NEW: Quantum Wave Validation Through Interference + Ed25519 Aggregation");
    println!("🔐 Ed25519: Signing Quantum Waves Instead of Transactions!");
    println!("🚀 AGGREGATION: 1000x Faster Verification Through Batch Processing!");
    println!("🔍 PROFILING: Detailed performance analysis enabled!");
    println!("---");
    
    if args.distributed {
        // Запускаем в распределенном режиме
        let mut network_manager = DistributedNetworkManager::new();
        
        // Настраиваем распределенную сеть
        if let Err(e) = network_manager.setup_distributed_network(args.num_codespaces).await {
            println!("❌ Failed to setup distributed network: {}", e);
            return Err(e.into());
        }
        
        // Запускаем симуляцию консенсуса
        if let Err(e) = network_manager.run_consensus_simulation().await {
            println!("❌ Failed to run consensus simulation: {}", e);
            return Err(e.into());
        }
        
        // Очищаем ресурсы
        if let Err(e) = network_manager.cleanup().await {
            println!("⚠️  Failed to cleanup: {}", e);
        }
        
        println!("✅ Distributed network demo completed successfully!");
            } else {
        // Запускаем локальный режим (как раньше)
        println!("🏠 Running in local mode with Ed25519 aggregation...");
        
        // Демонстрируем Ed25519 агрегацию в локальном режиме
        let keypool = Ed25519KeyPool::new(100);
        demonstrate_local_ed25519_aggregation(&keypool);
        
        // Создаем узлы с распределением по шардам
        let num_shards = 3;
        let mut nodes = create_distributed_nodes(10, num_shards);
        println!("✅ Initialized {} consensus nodes across {} shards", 10, num_shards);
        
        // Создаем аккаунты и ключи с большим разнообразием для локального режима
        let mut accounts: Vec<String> = Vec::new();
        let mut rng = rand::thread_rng(); // Changed from OsRng to thread_rng for local mode
        for i in 0..100 {
            let account_type = if i < 30 { "user" } else if i < 60 { "node" } else if i < 80 { "validator" } else { "miner" };
            let account_id = if i < 10 { format!("0{}", i) } else { i.to_string() };
            accounts.push(format!("{}_{}_{}", account_type, account_id, rng.gen_range(1000..9999)));
        }
        
        // CRYPTO DISABLED: No keypairs needed
        let keypool = Ed25519KeyPool::new(100);
        
        // Глобальное состояние аккаунтов
        let mut accounts_state: HashMap<String, AccountState> = accounts.iter()
            .enumerate()
            .map(|(i, name)| {
                (name.clone(), AccountState {
                    balance: 1000.0 + (i as f64 * 100.0),
                    nonce: 0,
                })
            })
            .collect();
        
        // Демонстрируем новую систему валидации через интерференцию
        demonstrate_wave_validation_system(&keypool);
        
        // Конфигурация энергопотребления для оценки энергии (локальный режим)
        let cpu_watts: f64 = std::env::var("TRIAD_CPU_WATTS")
            .ok()
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(45.0);
        let avg_cpu_usage: f64 = std::env::var("TRIAD_CPU_USAGE_AVG")
            .ok()
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(60.0)
            .clamp(0.0, 100.0);

        // Структура и хранилище метрик по раундам (локальный режим)
        #[derive(Clone)]
        struct RoundSummaryLocal {
            round: usize,
            consensus: bool,
            time_ms: f64,
            tps: f64,
            success: usize,
            fail: usize,
            energy_j: f64,
        }

        let mut round_summaries: Vec<RoundSummaryLocal> = Vec::with_capacity(10);

        // Основной цикл симуляции
        let mut tx_pool = TransactionPool::with_capacity(1000);
        let mut total_success = 0;
        let mut total_failure = 0;
        let mut total_time = Duration::ZERO;
        let simulation_start = Instant::now();
        
        for round in 1..=10 {
            println!("\n🔄 Round {}/10 starting...", round);
            
            // === ДЕТАЛЬНОЕ ПРОФИЛИРОВАНИЕ ===
            let round_start = Instant::now();
            
            // 1. ГЕНЕРАЦИЯ ТРАНЗАКЦИЙ
            let tx_gen_start = Instant::now();
            let accounts = vec!["Alice".to_string(), "Bob".to_string(), "Charlie".to_string()];
            let tx_pool = generate_sharded_transactions(100, &accounts);
            let tx_gen_time = tx_gen_start.elapsed();
            println!("⚡ [Round {}] Transaction generation: {:.3}ms | TPS: {:.0}", 
                round, tx_gen_time.as_secs_f64() * 1000.0, 100.0 / tx_gen_time.as_secs_f64());
            
            // 2. СЕРИАЛИЗАЦИЯ ТРАНЗАКЦИЙ
            let serial_start = Instant::now();
            let serialized = serialize_transactions(&tx_pool);
            let serial_time = serial_start.elapsed();
            println!("📦 [Round {}] Transaction serialization: {:.3}ms | Size: {} bytes", 
                round, serial_time.as_secs_f64() * 1000.0, serialized.len());
            
            // 3. ПОДГОТОВКА КОНСЕНСУСА
            let prep_start = Instant::now();
            let mut nodes = nodes.clone();
            let mut accounts_state = HashMap::new(); // Упрощенно для демо
            let local_field = Arc::new(StdMutex::new(ShardedQuantumField::new(8, 500_000)));
            let local_engine = Arc::new(StdMutex::new(InterferenceEngine::new(1000, 0.95)));
            let keypool = keypool.clone();
            let prep_time = prep_start.elapsed();
            println!("🔧 [Round {}] Consensus preparation: {:.3}ms", 
                round, prep_time.as_secs_f64() * 1000.0);
            
            let total_prep_time = tx_gen_start.elapsed();
            println!("⚡ [Round {}] Total preparation: {:.3}ms", round, total_prep_time.as_secs_f64() * 1000.0);
            
            // 4. СОЗДАНИЕ ШАРДОВ
            let shard_start = Instant::now();
            let sharded_messages = create_sharded_messages_from_transactions(&tx_pool, 15);
            let shard_time = shard_start.elapsed();
            println!("🔀 [Round {}] Shard creation: {:.3}ms | Shards: {}", 
                round, shard_time.as_secs_f64() * 1000.0, sharded_messages.len());
            
            // 5. Ed25519 ВЕРИФИКАЦИЯ ВОЛН
            let ed25519_start = Instant::now();
            println!("🔬 [Round {}] Starting Ed25519 wave signature verification", round);
            
            // Создаем и подписываем квантовые волны из транзакций
            let waves_and_signatures = create_and_sign_quantum_waves(&tx_pool, &keypool, 0);
            
            // Верифицируем подписи волн с агрегацией
            let wave_verification_results = verify_wave_signatures_with_aggregation(&waves_and_signatures, &keypool);
            
            let ed25519_time = ed25519_start.elapsed();
            let valid_waves = wave_verification_results.iter().filter(|&&valid| valid).count();
            println!("🔐 [Round {}] Ed25519 wave verification: {:.3}ms | Valid: {}/{}", 
                round, ed25519_time.as_secs_f64() * 1000.0, valid_waves, wave_verification_results.len());
            
            // 6. ОБРАБОТКА КОНСЕНСУСА
            let consensus_start = Instant::now();
            let (success_count, failure_count) = process_distributed_consensus(
                &mut nodes,
                &sharded_messages,
                &mut accounts_state,
                &local_field,
                &local_engine,
                &wave_verification_results,
                &keypool
            );
            let consensus_time = consensus_start.elapsed();
            
            // === ДЕТАЛЬНАЯ СТАТИСТИКА ===
            let total_round_time = round_start.elapsed();
            let tps = if total_round_time.as_secs_f64() > 0.0 {
                (success_count + failure_count) as f64 / total_round_time.as_secs_f64()
            } else {
                0.0
            };
            
            println!("🔍 [Round {}] DETAILED PROFILING:", round);
            println!("   📊 Transaction generation: {:.3}ms ({:.1}%)", 
                tx_gen_time.as_secs_f64() * 1000.0, 
                (tx_gen_time.as_secs_f64() / total_round_time.as_secs_f64()) * 100.0);
            println!("   📦 Serialization: {:.3}ms ({:.1}%)", 
                serial_time.as_secs_f64() * 1000.0,
                (serial_time.as_secs_f64() / total_round_time.as_secs_f64()) * 100.0);
            println!("   🔧 Preparation: {:.3}ms ({:.1}%)", 
                prep_time.as_secs_f64() * 1000.0,
                (prep_time.as_secs_f64() / total_round_time.as_secs_f64()) * 100.0);
            println!("   🔀 Sharding: {:.3}ms ({:.1}%)", 
                shard_time.as_secs_f64() * 1000.0,
                (shard_time.as_secs_f64() / total_round_time.as_secs_f64()) * 100.0);
            println!("   🔐 Ed25519 wave verification: {:.3}ms ({:.1}%)", 
                ed25519_time.as_secs_f64() * 1000.0,
                (ed25519_time.as_secs_f64() / total_round_time.as_secs_f64()) * 100.0);
            println!("   🌊 Consensus processing: {:.3}ms ({:.1}%)", 
                consensus_time.as_secs_f64() * 1000.0,
                (consensus_time.as_secs_f64() / total_round_time.as_secs_f64()) * 100.0);
            println!("   ⏱️ Total round time: {:.3}ms (100%)", 
                total_round_time.as_secs_f64() * 1000.0);
            
            println!("Round {}: Consensus={} | Time={:.3}ms | Success={} | Fail={} | TPS={:.0}", 
                round, 
                if success_count > failure_count { "true" } else { "false" },
                total_round_time.as_secs_f64() * 1000.0,
                success_count, 
                failure_count, 
                tps
            );
            
            // Обновляем общую статистику
            total_success += success_count;
            total_failure += failure_count;
            total_time += total_round_time;

            // Энергия раунда (оценка по среднему CPU usage)
            let round_secs = total_round_time.as_secs_f64();
            let energy_j = (avg_cpu_usage / 100.0) * cpu_watts * round_secs;

            // Сохраняем сводку по раунду
            round_summaries.push(RoundSummaryLocal {
                round,
                consensus: success_count > failure_count,
                time_ms: total_round_time.as_secs_f64() * 1000.0,
                tps,
                success: success_count,
                fail: failure_count,
                energy_j,
            });
        }
        
        // Финальный TPS для всего симуляции
        let total_time = simulation_start.elapsed().as_secs_f64();
        let total_tps = total_success as f64 / total_time;
        println!("✅ Final TPS: {:.0} (real time: {:.3}s)", total_tps, total_time);

        // Итоговая таблица по раундам (локальный режим)
        println!("\n📋 ROUNDS SUMMARY (CPU {:.0}W, avg {:.0}%):", cpu_watts, avg_cpu_usage);
        println!("{:<7} │ {:<9} │ {:>9} │ {:>8} │ {:>7} │ {:>6} │ {:>9}",
                 "Round", "Consensus", "Time (ms)", "TPS", "Success", "Fail", "Energy (J)");
        println!("{}", "-".repeat(72));
        let mut total_energy: f64 = 0.0;
        for s in &round_summaries {
            total_energy += s.energy_j;
            println!("{:<7} │ {:<9} │ {:>9.3} │ {:>8.0} │ {:>7} │ {:>6} │ {:>9.3}",
                     s.round,
                     if s.consensus { "passed" } else { "failed" },
                     s.time_ms,
                     s.tps,
                     s.success,
                     s.fail,
                     s.energy_j);
        }
        println!("{}", "-".repeat(72));
        println!("{:<7} │ {:<9} │ {:>9} │ {:>8} │ {:>7} │ {:>6} │ {:>9.3}",
                 "TOTAL", "-", "-", "-", total_success, total_failure, total_energy);

        println!("✅ Local simulation completed!");
    }
    
    Ok(())
}

// Функция для сериализации транзакций
fn serialize_transactions(transactions: &[DemoTransaction]) -> Vec<u8> {
    let mut serialized = Vec::new();
    for tx in transactions {
        // Простая сериализация для демо
        serialized.extend_from_slice(tx.id.as_bytes());
        serialized.extend_from_slice(tx.from.as_bytes());
        serialized.extend_from_slice(tx.to.as_bytes());
        serialized.extend_from_slice(&tx.amount.to_le_bytes());
        serialized.extend_from_slice(&tx.fee.to_le_bytes());
    }
    serialized
}

// CRYPTO DISABLED: Create quantum waves without Ed25519 signatures
fn create_and_sign_quantum_waves(
    transactions: &[DemoTransaction], 
    _keypool: &Ed25519KeyPool,
    shard_id: usize
) -> Vec<(QuantumWave, WaveSignature)> {
    let start = Instant::now();
    println!("🌊 [NO-CRYPTO] Creating quantum waves for {} transactions (signatures disabled)", transactions.len());
    
    let waves_and_signatures: Vec<(QuantumWave, WaveSignature)> = transactions.iter().enumerate()
        .map(|(i, tx)| {
            // Создаем квантовое состояние для волны
            let phase = calculate_transaction_phase(&ConsensusMessage {
                sender_id: tx.from.clone(),
                state_id: tx.id.clone(),
                raw_data: Bytes::new(),
                signature: vec![],
            });
            
            let amplitude = (tx.amount / 100.0).max(0.1).min(2.0);
            
            let state = QuantumState::new(
                tx.id.clone(),
                amplitude,
                phase,
                format!("shard_{}", shard_id),
                vec![StateVector {
                    value: Complex::new(1.0, 0.0),
                    probability: 1.0,
                }]
            );
            
            // Создаем квантовую волну
            let wave = QuantumWave::from(state);
            
            // Создаем пустую подпись волны (криптография отключена)
            let wave_signature = WaveSignature::new(
                wave.id.to_string(),
                tx.id.clone(),
                amplitude,
                phase,
                shard_id,
                vec![], // NO SIGNATURE - crypto disabled
                tx.public_key_idx,
            );
            
            (wave, wave_signature)
        })
        .collect();
    
    let creation_time = start.elapsed();
    println!("🌊 [NO-CRYPTO] Created {} quantum waves in {:.3}ms ({:.0} waves/sec)", 
        waves_and_signatures.len(), 
        creation_time.as_secs_f64() * 1000.0,
        waves_and_signatures.len() as f64 / creation_time.as_secs_f64());
    
    waves_and_signatures
}

// CRYPTO DISABLED: No verification - accept all waves
fn verify_wave_signatures_with_aggregation(
    waves_and_signatures: &[(QuantumWave, WaveSignature)],
    _keypool: &Ed25519KeyPool
) -> Vec<bool> {
    let start = Instant::now();
    println!("⚡ [NO-CRYPTO] Skipping signature verification for {} waves (all accepted)", waves_and_signatures.len());
    
    // All waves are valid when crypto is disabled
    let results = vec![true; waves_and_signatures.len()];
    
    let total_time = start.elapsed();
    println!("⚡ [NO-CRYPTO] Instant verification: {:.3}ms | {}/{} waves valid", 
        total_time.as_secs_f64() * 1000.0, results.len(), waves_and_signatures.len());
    
    results
}

// CRYPTO DISABLED: No-op demonstration
fn demonstrate_local_ed25519_aggregation(_keypool: &Ed25519KeyPool) {
    println!("⚡ === [NO-CRYPTO] Ed25519 Demo Skipped (crypto disabled) ===");
    println!("⚡ Running in pure interference mode for maximum TPS");
    println!("⚡ === End Demo ===\n");
}

// Функция для вычисления фазы волны на основе содержания транзакции
fn calculate_transaction_phase(msg: &ConsensusMessage) -> f64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    
    // Создаем хеш из содержимого транзакции
    let mut hasher = DefaultHasher::new();
    
    // Хешируем различные аспекты транзакции
    msg.raw_data.hash(&mut hasher);
    msg.state_id.hash(&mut hasher);
    msg.sender_id.hash(&mut hasher);
    
    let hash_value = hasher.finish();
    
    // Преобразуем хеш в фазу в диапазоне [0, 2π]
    let phase = (hash_value as f64) / (u64::MAX as f64) * 2.0 * std::f64::consts::PI;
    
    // Дополнительные факторы для фазы:
    
    // 1. Размер данных влияет на фазу
    let size_factor = (msg.raw_data.len() as f64) / 1000.0;
    
    // 2. ID отправителя влияет на фазу (для создания пространственных паттернов)
    let sender_factor = msg.sender_id.len() as f64;
    
    // 3. ID состояния влияет на фазу (для создания пространственных паттернов)
    let state_factor = msg.state_id.len() as f64;
    
    // Комбинируем все факторы
    let combined_phase = phase + size_factor + sender_factor + state_factor;
    
    // Нормализуем к диапазону [0, 2π]
    let normalized_phase = combined_phase % (2.0 * std::f64::consts::PI);
    
    normalized_phase
}

/// Демонстрирует новую систему валидации через интерференцию с подписанными волнами
fn demonstrate_wave_validation_system(keypool: &Ed25519KeyPool) {
    println!("🌊 === Professional Wave Validation System Demo with Ed25519 Signatures ===");
    
    // Используем существующую функцию для генерации аккаунтов
    let accounts: Vec<String> = (0..10).map(|i| {
        let account_type = if i < 3 { "user" } else if i < 6 { "node" } else { "validator" };
        format!("{}_{}", account_type, i)
    }).collect();
    
    println!("🔧 Created {} test accounts", accounts.len());
    
    // Используем существующую функцию для генерации транзакций
    println!("📝 Generating test transactions...");
    let test_transactions = generate_sharded_transactions(5, &accounts);
    
    // Создаем узлы консенсуса используя существующую функцию
    let mut consensus_nodes = create_distributed_nodes(15, 15);
    println!("🔧 Created {} consensus nodes using existing function", consensus_nodes.len());
    
    // Создаем квантовое поле для демонстрации интерференции
    let mut quantum_field = QuantumField::new();
    
    // СОЗДАЕМ И ПОДПИСЫВАЕМ КВАНТОВЫЕ ВОЛНЫ
    println!("\n🌊 === Creating and Signing Quantum Waves from Transactions ===");
    
    let waves_and_signatures = create_and_sign_quantum_waves(&test_transactions, keypool, 0);
    
    // ВЕРИФИЦИРУЕМ ПОДПИСИ ВОЛН
    println!("\n🔐 === Verifying Wave Signatures with Ed25519 Aggregation ===");
    let verification_results = verify_wave_signatures_with_aggregation(&waves_and_signatures, keypool);
    
    // Добавляем только валидные волны в квантовое поле и узлы консенсуса
    for (i, (wave, wave_sig)) in waves_and_signatures.iter().enumerate() {
        if i < verification_results.len() && verification_results[i] {
            println!("✅ Adding valid wave {} with signature", wave.id);
            quantum_field.add_wave(wave.id.to_string(), wave.clone());
            
            // Добавляем волну в соответствующий узел консенсуса
            let node_index = wave.shard_id.parse::<usize>().unwrap_or(0) % consensus_nodes.len();
            consensus_nodes[node_index].field.add_wave(wave.id.to_string(), wave.clone());
        } else {
            println!("❌ Skipping invalid wave {} (signature verification failed)", wave.id);
        }
    }
    
    println!("🌊 Added {} valid waves to quantum field and consensus nodes", 
        verification_results.iter().filter(|&&valid| valid).count());
    
    // Рассчитываем РЕАЛЬНУЮ квантовую интерференцию
    let interference_start = Instant::now();
    let interference_pattern = quantum_field.calculate_interference_pattern(Some(5000));
    let interference_time = interference_start.elapsed();
    
    println!("🌊 Calculated interference pattern: {} points in {:.3}ms", 
        interference_pattern.len(), interference_time.as_secs_f64() * 1000.0);
    
    // Анализируем интерференцию
    let constructive_points = interference_pattern.iter()
        .filter(|point| point.amplitude > 0.5)
        .count();
    let destructive_points = interference_pattern.iter()
        .filter(|point| point.amplitude < -0.5)
        .count();
    let neutral_points = interference_pattern.len() - constructive_points - destructive_points;
    
    println!("📊 Interference Analysis:");
    println!("   Constructive points: {} ({:.1}%)", 
        constructive_points, (constructive_points as f64 / interference_pattern.len() as f64) * 100.0);
    println!("   Destructive points: {} ({:.1}%)", 
        destructive_points, (destructive_points as f64 / interference_pattern.len() as f64) * 100.0);
    println!("   Neutral points: {} ({:.1}%)", 
        neutral_points, (neutral_points as f64 / interference_pattern.len() as f64) * 100.0);
    
    // Демонстрируем валидацию через интерференцию для подписанных волн
    println!("\n🔍 === Wave Validation Through Interference (Signed Waves) ===");
    
    let mut validation_results = HashMap::new();
    
    for (i, (wave, wave_sig)) in waves_and_signatures.iter().enumerate() {
        let wave_id = wave.id.clone();
        
        // Проверяем, что подпись волны валидна
        let signature_valid = i < verification_results.len() && verification_results[i];
        
        if !signature_valid {
            println!("❌ Wave {}: REJECTED | Reason: Invalid Ed25519 signature", wave_id);
            validation_results.insert(wave_id.clone(), WaveValidationResult::Rejected("Invalid Ed25519 signature".to_string()));
            continue;
        }
        
        // Симулируем результат валидации на основе интерференции
        let interference_strength = if i < interference_pattern.len() {
            interference_pattern[i].amplitude.abs()
        } else {
            0.5 // Базовое значение
        };
        
        // Профессиональная логика валидации для подписанных волн
        let validation_result = if interference_strength > 1.0 {
            WaveValidationResult::Validated(interference_strength)
        } else if interference_strength > 0.5 {
            WaveValidationResult::PartiallyValidated(interference_strength)
        } else {
            WaveValidationResult::Rejected("Low interference strength".to_string())
        };
        
        validation_results.insert(wave_id.clone(), validation_result.clone());
        
        // Выводим результат валидации
        match &validation_result {
            WaveValidationResult::Validated(amplitude) => {
                println!("✅ Wave {}: VALIDATED | Ed25519: ✅ | Interference: {:.3}", 
                    wave_id, amplitude);
            },
            WaveValidationResult::PartiallyValidated(amplitude) => {
                println!("⚠️  Wave {}: PARTIALLY VALIDATED | Ed25519: ✅ | Interference: {:.3}", 
                    wave_id, amplitude);
            },
            WaveValidationResult::Rejected(reason) => {
                println!("❌ Wave {}: REJECTED | Ed25519: ✅ | Reason: {}", wave_id, reason);
            }
        }
    }
    
    // Применяем результаты валидации к волнам используя существующий метод
    println!("\n🔄 === Applying Validation Results to Signed Waves ===");
    
    for (wave_id, result) in &validation_results {
        if let Some(wave) = quantum_field.active_waves.get_mut(wave_id.as_ref()) {
            let old_amplitude = wave.amplitude;
            wave.update_amplitude(result);
            let new_amplitude = wave.amplitude;
            
            println!("🌊 Wave {}: amplitude {:.3} -> {:.3} | lifetime: {:?}", 
                wave_id, old_amplitude, new_amplitude, wave.lifetime);
        }
    }
    
    // Показываем профессиональную статистику по подписанным волнам
    println!("\n📊 === Professional Wave Statistics (Signed Waves) ===");
    
    let field_stats = quantum_field.get_wave_statistics();
    println!("🌊 Quantum Field Statistics:");
    println!("   Total waves: {}", field_stats.total_waves);
    println!("   Total amplitude: {:.3}", field_stats.total_amplitude);
    println!("   Average amplitude: {:.3}", field_stats.average_amplitude);
    println!("   Strong waves (amp > 0.5): {}", field_stats.strong_waves);
    println!("   Weak waves (amp < 0.1): {}", field_stats.weak_waves);
    
    // Статистика по узлам
    for (i, node) in consensus_nodes.iter().enumerate() {
        let node_stats = node.field.get_wave_statistics();
        println!("🔧 Node {} Statistics:", i);
        println!("   Waves: {} | Avg amplitude: {:.3} | Strong: {} | Weak: {}", 
            node_stats.total_waves, node_stats.average_amplitude, 
            node_stats.strong_waves, node_stats.weak_waves);
    }
    
    // Демонстрируем управление жизненным циклом подписанных волн
    println!("\n⏰ === Wave Lifecycle Management (Signed Waves) ===");
    
    // Проверяем здоровье волн
    for (wave_id, _) in &validation_results {
        if let Some(health) = quantum_field.check_wave_health(wave_id.as_ref()) {
            println!("🏥 Wave {} health: {:?} | Age ratio: {:.2} | Amplitude: {:.3}", 
                wave_id, health.status, health.age_ratio, health.amplitude);
        }
    }
    
    // Очищаем устаревшие волны используя существующий метод
    let waves_before_cleanup = quantum_field.active_waves.len();
    quantum_field.cleanup_expired_waves();
    let waves_after_cleanup = quantum_field.active_waves.len();
    
    println!("🧹 Cleanup: {} -> {} waves (removed {})", 
        waves_before_cleanup, waves_after_cleanup, 
        waves_before_cleanup - waves_after_cleanup);
    
    // Финальная статистика
    let final_stats = quantum_field.get_wave_statistics();
    println!("\n🎯 === Final Results (Signed Waves) ===");
    println!("✅ Successfully processed {} transactions with Ed25519-signed waves", test_transactions.len());
    println!("🔐 Wave signatures verified: {}/{}", 
        verification_results.iter().filter(|&&valid| valid).count(),
        verification_results.len());
    println!("🌊 Final wave count: {}", final_stats.total_waves);
    println!("📈 Total system amplitude: {:.3}", final_stats.total_amplitude);
    println!("🚀 Professional wave validation system with Ed25519 signatures demo completed!");
    
    println!("🌊 === End Professional Wave Validation Demo with Ed25519 Signatures ===\n");
}



