use std::sync::Arc;
use parking_lot::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel as xchan;
use rayon::prelude::*;
use tracing::{info, debug, warn};
use sha2::{Sha256, Digest};
use dashmap::DashMap;
use bytes::Bytes;
 

use crate::pipeline::config::PipelineConfig;
use crate::pipeline::metrics::StageMetrics;
use crate::quantum::consensus::{ConsensusMessage, ConsensusNode};
use crate::quantum::field::{ShardedQuantumField, WaveValidationResult};
use crate::quantum::InterferenceEngine;
use crate::tvm::{Tvm, TvmConfig, scheduler::{Scheduler, BatchItem}, state::{StateView, KeyAccess}, ExecEnvelope};
use crate::tvm::loader::{TvmProgram, load_program};
use crate::transaction::Transaction;

/// Элемент, поступающий на вход пайплайна (ингресс)
#[derive(Debug, Clone)]
pub struct IngressItem(pub ConsensusMessage);

/// Полноценный высокопроизводительный конвейер: ingress → sigverify (N) → interference → apply/consensus (DAG+TVM)
pub struct Pipeline {
    conf: PipelineConfig,
    // Стадии
    ing_tx: xchan::Sender<BatchEnvelope>,
    // Глобальное состояние
    state: Arc<StateView>,
    // Кэш программ TVM (по SHA-256 ELF).
    prog_cache: Arc<DashMap<[u8;32], Arc<TvmProgram>>>,
    // Метрики стадий
    m_ingress: Arc<Mutex<StageMetrics>>,
    m_verify: Arc<Mutex<StageMetrics>>,
    m_interf: Arc<Mutex<StageMetrics>>,
    m_apply: Arc<Mutex<StageMetrics>>,
}

impl Pipeline {
    pub fn new(conf: PipelineConfig, consensus: Arc<Mutex<ConsensusNode>>, state: Arc<StateView>) -> Self {
        // Каналы между стадиями (bounded)
        let (ing_tx, ing_rx) = xchan::bounded::<BatchEnvelope>(conf.channel_capacity);
        let (ver_tx, ver_rx) = xchan::bounded::<BatchEnvelope>(conf.channel_capacity);
        let (int_tx, int_rx) = xchan::bounded::<BatchEnvelope>(conf.channel_capacity);

        let m_ingress = Arc::new(Mutex::new(StageMetrics::new(2048)));
        let m_verify  = Arc::new(Mutex::new(StageMetrics::new(2048)));
        let m_interf  = Arc::new(Mutex::new(StageMetrics::new(2048)));
        let m_apply   = Arc::new(Mutex::new(StageMetrics::new(2048)));
        let prog_cache: Arc<DashMap<[u8;32], Arc<TvmProgram>>> = Arc::new(DashMap::new());

        // Ingress: собирает сообщения в батчи, сортирует по комиссии (если есть в raw_data), отправляет дальше
        {
            let m = m_ingress.clone();
            let bs = conf.batch_size;
            let ver_tx = ver_tx.clone();
            thread::spawn(move || {
                let mut last_report = Instant::now();
                // Этот поток только подготовка батчей извне: он не читает из канала, внешний код вызывает submit()
                // Поэтому здесь только периодический репорт метрик
                loop {
                    thread::sleep(Duration::from_millis(500));
                    if Instant::now().duration_since(last_report) >= Duration::from_secs(1) {
                        {
                            let mut mm = m.lock();
                            mm.maybe_report("Ingress");
                        }
                        last_report = Instant::now();
                    }
                }
            });
        }

        // SigVerify: N воркеров, вызывает процессинг консенсуса (проверка подписи и базовая обработка)
        {
            let workers = num_cpus::get();
            for _ in 0..workers {
                let rx = ing_rx.clone();
                let tx = ver_tx.clone();
                let m = m_verify.clone();
                let consensus = Arc::clone(&consensus);
                thread::spawn(move || {
                    loop {
                        match rx.recv() {
                            Ok(mut benv) => {
                                let t0 = Instant::now();
                                // Очередь ожидания на стадии Verify
                                {
                                    let mut mm = m.lock();
                                    mm.observe_wait(t0.duration_since(benv.enqueued_at));
                                }
                                // Параллельная верификация/обработка подписей
                                // Замечание: для высокой пропускной способности минимизируем время удержания mutex
                                let (accepted, processed) = {
                                    let mut ok_cnt = 0usize;
                                    for msg in &benv.messages {
                                        let mut cons = consensus.lock();
                                        let vk = cons.public_key.clone();
                                        // Принимаем, если подпись валидна, и добавляем волну в поле
                                        if cons.process_message(msg.clone(), &vk).is_ok() { ok_cnt += 1; }
                                    }
                                    (ok_cnt, benv.messages.len())
                                };
                                let d = t0.elapsed();
                                {
                                    let mut mm = m.lock();
                                    mm.observe(d, accepted, processed);
                                    mm.maybe_report("SigVerify");
                                }
                                benv.enqueued_at = Instant::now();
                                let _ = tx.send(benv); // пропускаем дальше
                            }
                            Err(_) => break,
                        }
                    }
                });
            }
        }
        drop(ver_tx); // чтобы apply не зависал навсегда на закрытии

        // InterferenceStage: пакетно триггерит расчёт интерференции
        {
            let rx = ver_rx.clone();
            let tx = int_tx.clone();
            let m = m_interf.clone();
            let conf2 = conf.clone();
            let consensus = Arc::clone(&consensus);
            thread::spawn(move || {
                loop {
                    match rx.recv() {
                        Ok(mut benv) => {
                            let t0 = Instant::now();
                            {
                                let mut mm = m.lock();
                                mm.observe_wait(t0.duration_since(benv.enqueued_at));
                            }
                            // Пакетный пересчёт интерференции/очистка
                            {
                                let mut cons = consensus.lock();
                                let field = &mut cons.field;
                                if !field.maybe_flush_interference(conf2.interf_threshold, conf2.interf_resolution) {
                                    field.cleanup_expired_waves();
                                }
                            }
                            let d = t0.elapsed();
                            {
                                let mut mm = m.lock();
                                mm.observe(d, benv.messages.len(), benv.messages.len());
                                mm.maybe_report("Interference");
                            }
                            benv.enqueued_at = Instant::now();
                            let _ = tx.send(benv);
                        }
                        Err(_) => break,
                    }
                }
            });
        }
        drop(int_tx);

        // ApplyStage: строит DAG (упрощённо), анализирует интерференцию, вызывает TVM для программ и обновляет амплитуды
        {
            let rx = int_rx.clone();
            let m = m_apply.clone();
            let conf2 = conf.clone();
            let consensus = Arc::clone(&consensus);
            let state = Arc::clone(&state);
            let prog_cache = Arc::clone(&prog_cache);
            thread::spawn(move || {
                // Подготовка TVM окружения
                let tvm_cfg = TvmConfig::from_env();
                let resolver = crate::tvm::syscalls::build_resolver_from_config(&tvm_cfg);
                let tvm = Tvm::new(tvm_cfg.clone(), resolver);

                loop {
                    match rx.recv() {
                        Ok(benv) => {
                            let t0 = Instant::now();
                            {
                                let mut mm = m.lock();
                                mm.observe_wait(t0.duration_since(benv.enqueued_at));
                            }
                            let processed = benv.messages.len();

                            // Анализ интерференции текущего поля (inplace, без лишних аллокаций)
                            let (analysis, waves_snapshot) = {
                                let mut cons = consensus.lock();
                                let waves_snapshot = cons.field.snapshot_waves();
                                let pattern = cons.engine.calculate_interference_from_waves_inplace(&waves_snapshot);
                                let analysis = cons.engine.analyze_interference(&pattern);
                                (analysis, waves_snapshot)
                            };

                            // Простейший расчёт результата по интерференции: влияет на уровень амплитуд
                            let mut amplitude_updates: std::collections::HashMap<std::sync::Arc<str>, WaveValidationResult> =
                                std::collections::HashMap::with_capacity(waves_snapshot.len());
                            for w in &waves_snapshot {
                                // Сюда можно подключить DAG/fee-приоритизацию; для производительности опускаем лишние аллокации
                                let res = if analysis.average_amplitude > 0.0 {
                                    WaveValidationResult::Validated(analysis.average_amplitude)
                                } else if analysis.average_amplitude < 0.0 {
                                    WaveValidationResult::PartiallyValidated(analysis.average_amplitude.abs() * 0.5)
                                } else {
                                    WaveValidationResult::Rejected("neutral".into())
                                };
                                amplitude_updates.insert(std::sync::Arc::clone(&w.id), res);
                            }

                            // TVM: исполняем программные транзакции, сгруппированные по ключам
                            let items: Vec<BatchItem<Bytes>> = benv.messages.iter().map(|msg| {
                                if let Ok(env) = bincode::deserialize::<ExecEnvelope>(&msg.raw_data) {
                                    if env.is_valid() {
                                        let (access, _fee) = key_access_from_envelope(&env);
                                        return BatchItem{ id: Arc::<str>::from(msg.state_id.as_str()), payload: Bytes::from(env.code), access };
                                    }
                                }
                                let (access, _fee) = parse_key_access_and_fee(msg.raw_data.as_ref());
                                BatchItem{ id: Arc::<str>::from(msg.state_id.as_str()), payload: msg.raw_data.clone(), access }
                            }).collect();
                            let scheduled = Scheduler::schedule(items);
                            for group in scheduled.groups.into_iter() {
                                // Параллельное исполнение независимых групп
                                let results: Vec<(crate::tvm::TvmExecutionResult, crate::tvm::state::WriteBatch)> = group.par_iter().map(|it| {
                                    // Загрузка программы из payload (ELF) через кэш по SHA-256 кода
                                    let bytes_ref: &[u8] = it.payload.as_ref();
                                    let mut hasher = Sha256::new(); hasher.update(bytes_ref); let key: [u8;32] = hasher.finalize().into();
                                    let program_arc_opt: Option<Arc<TvmProgram>> = if let Some(entry) = prog_cache.get(&key) {
                                        Some(entry.value().clone())
                                    } else {
                                        match load_program(bytes_ref, &tvm.cfg, tvm.resolver.clone()) {
                                            Ok(p) => {
                                                let arc = Arc::new(p);
                                                prog_cache.insert(key, arc.clone());
                                                Some(arc)
                                            },
                                            Err(e) => {
                                                warn!(target="pipeline", "TVM load failed for {}: {}", it.id, e);
                                                None
                                            }
                                        }
                                    };
                                    if let Some(program_arc) = program_arc_opt {
                                        let (res, wb, _timers) = tvm.execute(
                                            program_arc.as_ref(),
                                            bytes_ref,
                                            &state,
                                            &it.access,
                                            analysis.average_amplitude,
                                        );
                                        (res, wb)
                                    } else {
                                        // Fallback: try TVM precompile (e.g., TRD-20 JSON ABI)
                                        if let Some((res, wb)) = crate::tvm::precompile::try_execute_precompile(bytes_ref, &state, analysis.average_amplitude) {
                                            (res, wb)
                                        } else {
                                            (crate::tvm::TvmExecutionResult{ ok:false, return_code:-2, logs: vec!["load error".to_string()] }, crate::tvm::state::WriteBatch::new())
                                        }
                                    }
                                }).collect();

                                // Коммит батчей по группе атомарно
                                for (_res, wb) in results.into_iter() {
                                    state.put_batch(&wb);
                                }
                            }

                            // Применяем обновления амплитуд в поле (одной критической секцией)
                            {
                                let mut cons = consensus.lock();
                                cons.field.update_wave_amplitudes(&amplitude_updates);
                                cons.field.remove_extinguished_waves();
                            }

                            let d = t0.elapsed();
                            {
                                let mut mm = m.lock();
                                mm.observe(d, processed, processed);
                                mm.maybe_report("Apply");
                            }
                        }
                        Err(_) => break,
                    }
                }
            });
        }

        Self { conf, ing_tx, state, prog_cache, m_ingress, m_verify, m_interf, m_apply }
    }

    /// Отдать сообщение в pipeline; оно будет собрано в батчи на ingress-стадии.
    pub fn submit(&self, msg: ConsensusMessage) {
        // Простое буферизующее поведение: мелкие батчи набираем локально и пушим по достижении batch_size
        thread_local! { static BUF: std::cell::RefCell<Vec<ConsensusMessage>> = std::cell::RefCell::new(Vec::new()); }
        let batch_size = self.conf.batch_size;
        BUF.with(|cell| {
            let mut b = cell.borrow_mut();
            b.push(msg);
            if b.len() >= batch_size {
                // Сортируем по fee (по убыванию), если его можно извлечь
                let mut out = std::mem::take(&mut *b);
                out.sort_by(|a, bmsg| {
                    let (_, fa) = parse_key_access_and_fee(&a.raw_data);
                    let (_, fb) = parse_key_access_and_fee(&bmsg.raw_data);
                    fb.partial_cmp(&fa).unwrap_or(std::cmp::Ordering::Equal)
                });
                let env = BatchEnvelope { messages: out, enqueued_at: Instant::now() };
                if let Err(e) = self.ing_tx.send(env) {
                    warn!(target="pipeline", "ingress send failed: {}", e);
                }
            }
        });
        {
            let mut mm = self.m_ingress.lock();
            mm.observe(Duration::from_millis(0), 1, 1);
        }
    }
}

#[derive(Debug, Clone)]
struct BatchEnvelope {
    messages: Vec<ConsensusMessage>,
    enqueued_at: Instant,
}

// ==== Вспомогательные функции парсинга raw_data для KeyAccess и fee ====

fn parse_key_access_and_fee(raw: &[u8]) -> (KeyAccess, f64) {
    // Пытаемся распарсить ExecEnvelope (bincode)
    if let Ok(env) = bincode::deserialize::<ExecEnvelope>(raw) {
        if env.is_valid() {
            let (ka, fee) = key_access_from_envelope(&env);
            return (ka, fee);
        }
    }
    // Пытаемся распарсить bincode<Transaction>
    if let Ok(tx) = bincode::deserialize::<Transaction>(raw) {
        let mut acc = KeyAccess::new();
        // Ключи состояния как bytes: acct/<sender>, acct/<receiver>
        let s_key = format!("acct/{}", tx.sender).into_bytes();
        let r_key = format!("acct/{}", tx.receiver).into_bytes();
        // Читаем оба, пишем оба (баланс списывается у sender, начисляется receiver)
        acc.read_keys.insert(s_key.clone());
        acc.read_keys.insert(r_key.clone());
        acc.write_keys.insert(s_key);
        acc.write_keys.insert(r_key);
        // fee: пробуем вытащить из tx.data как JSON {"fee": <f64>} или как первые 8 байт f64 LE
        let fee = extract_fee_from_data(&tx.data);
        return (acc, fee);
    }
    // Иначе пробуем JSON с полями {from,to,fee}
    if let Ok(v) = serde_json::from_slice::<serde_json::Value>(raw) {
        let mut acc = KeyAccess::new();
        if let (Some(from), Some(to)) = (v.get("from").and_then(|x| x.as_str()), v.get("to").and_then(|x| x.as_str())) {
            let s_key = format!("acct/{}", from).into_bytes();
            let r_key = format!("acct/{}", to).into_bytes();
            acc.read_keys.insert(s_key.clone());
            acc.read_keys.insert(r_key.clone());
            acc.write_keys.insert(s_key);
            acc.write_keys.insert(r_key);
        }
        let fee = v.get("fee").and_then(|x| x.as_f64()).unwrap_or(0.0);
        return (acc, fee);
    }
    (KeyAccess::new(), 0.0)
}

fn extract_fee_from_data(data: &[u8]) -> f64 {
    // Попытка 1: JSON в data
    if let Ok(v) = serde_json::from_slice::<serde_json::Value>(data) {
        if let Some(f) = v.get("fee").and_then(|x| x.as_f64()) { return f; }
    }
    // Попытка 2: первые 8 байт f64 little-endian
    if data.len() >= 8 {
        let mut arr = [0u8;8]; arr.copy_from_slice(&data[..8]);
        return f64::from_le_bytes(arr);
    }
    0.0
}

fn key_access_from_envelope(env: &ExecEnvelope) -> (KeyAccess, f64) {
    let mut acc = KeyAccess::new();
    for r in &env.meta.reads {
        if r.contains('/') { acc.read_keys.insert(r.as_bytes().to_vec()); }
        else { acc.read_keys.insert(format!("acct/{}", r).into_bytes()); }
    }
    for w in &env.meta.writes {
        if w.contains('/') { acc.write_keys.insert(w.as_bytes().to_vec()); }
        else { acc.write_keys.insert(format!("acct/{}", w).into_bytes()); }
    }
    (acc, env.meta.fee)
}
