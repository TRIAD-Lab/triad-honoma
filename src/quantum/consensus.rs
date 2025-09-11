use crate::quantum::{InterferenceEngine, QuantumWave, QuantumState, StateVector};
use crate::quantum::field::ShardedQuantumField;
use crate::quantum::prob_ops::ProbabilisticOperation;
use crate::transaction::processor::QuantumTransactionProcessor;
use ed25519_dalek::{SigningKey, VerifyingKey, Signature, Signer, Verifier};
use bytes::Bytes;
use rand::{rngs::StdRng, SeedableRng};
use num_complex::Complex;
use rayon::prelude::*;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone)]
pub struct ConsensusNode {
    pub id: String,
    pub shard_id: usize,
    pub field: ShardedQuantumField,
    pub engine: InterferenceEngine,
    pub secret_key: SigningKey,
    pub public_key: VerifyingKey,
    cache: Arc<Mutex<HashMap<String, bool>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusMessage {
    pub sender_id: String,
    pub state_id: String,
    pub raw_data: Bytes,
    pub signature: Vec<u8>,
}

#[derive(Debug, Clone, Default)]
pub struct ConsensusState {
    pub current_round: u64,
    pub votes: Vec<ConsensusVote>,
    pub threshold: f64,
    pub status: ConsensusStatus,
}

#[derive(Debug, Clone)]
pub struct ConsensusVote {
    pub node_id: String,
    pub vote: bool,
    pub timestamp: std::time::SystemTime,
}

#[derive(Debug, Clone)]
pub enum ConsensusStatus {
    Pending,
    Committed,
    Aborted,
}

impl Default for ConsensusStatus {
    fn default() -> Self {
        Self::Pending
    }
}

impl ConsensusNode {
    pub fn new(id: String, shard_id: usize) -> Self {
        let mut rng: StdRng = StdRng::from_entropy();
        let secret_key = SigningKey::generate(&mut rng);
        let public_key = secret_key.verifying_key();
        let shards = num_cpus::get().next_power_of_two();
        Self {
            id,
            shard_id,
            field: ShardedQuantumField::new(shards, 1_000_000),
            engine: InterferenceEngine::new(1000, 0.95),
            secret_key,
            public_key,
            cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn sign_message(&self, raw_data: &[u8]) -> Signature {
        self.secret_key.sign(raw_data)
    }

    pub fn verify_message(public_key: &VerifyingKey, signature_bytes: &[u8], raw_data: &[u8]) -> bool {
        match Signature::from_slice(signature_bytes) {
            Ok(sig) => public_key.verify(raw_data, &sig).is_ok(),
            Err(_) => false,
        }
    }

    pub fn process_message(&mut self, message: ConsensusMessage, sender_public_key: &VerifyingKey) -> Result<QuantumState, String> {
        // First, verify the signature
        if !Self::verify_message(sender_public_key, &message.signature, message.raw_data.as_ref()) {
            return Err("Invalid signature".to_string());
        }

        let start = std::time::Instant::now();
        // Check cache
        if let Ok(cache) = self.cache.lock() {
            if cache.contains_key(&message.state_id) {
                let wave = self.field.get_wave(&message.state_id)?;
                let state = QuantumState::from(wave);
                let duration = start.elapsed();
                println!("Message processing time (cache): {:?}", duration);
                return Ok(state);
            }
        }
        // Create a probabilistic operation for data processing
        let op = ProbabilisticOperation {
            description: "Data processing".to_string(),
            outcomes: vec![
                crate::quantum::prob_ops::OperationOutcome {
                    label: "Processed".to_string(),
                    probability: 0.8,
                    value: Some(serde_json::json!({
                        "processed": true,
                        "data": message.raw_data
                    })),
                },
                crate::quantum::prob_ops::OperationOutcome {
                    label: "Failed".to_string(),
                    probability: 0.2,
                    value: Some(serde_json::json!({
                        "processed": false,
                        "error": "Processing failed"
                    })),
                },
            ],
        };
        // Perform probabilistic processing
        let outcome = op.execute();
        // Create a quantum state based on the processing result
        let state = QuantumState {
            id: message.state_id.clone(),
            shard_id: self.shard_id.to_string(),
            amplitude: 1.0,
            phase: 0.0,
            superposition: vec![
                StateVector {
                    value: Complex::new(message.raw_data.len() as f64, 0.0),
                    probability: if outcome.label == "Processed" { 0.8 } else { 0.2 },
                },
            ],
        };
        // Add state to the field
        let wave = QuantumWave::new(
            message.state_id.clone(),
            state.amplitude,
            state.phase,
            state.shard_id.clone(),
            state.superposition.clone(),
        );
        self.field.add_wave(message.state_id.clone(), wave);
        // Save to cache
        if let Ok(mut cache) = self.cache.lock() {
            cache.insert(message.state_id, true);
        }
        let duration = start.elapsed();
        println!("Message processing time: {:?}", duration);
        Ok(state)
    }

    /// Проверяет интерференцию между ВСЕМИ активными волнами (не с искусственными!)
    pub fn check_interference(&self, state_id: &str) -> Result<bool, String> {
        let _wave = self.field.get_wave(state_id)?;
        
        // Получаем ВСЕ активные волны из поля для расчёта интерференции
        let all_waves: Vec<QuantumWave> = self.field.snapshot_waves();
        
        if all_waves.len() < 2 {
            return Ok(true); // Нужно минимум 2 волны для интерференции
        }
        
        // Рассчитываем интерференцию между ВСЕМИ волнами
        let states: Vec<QuantumState> = all_waves.iter().map(|w| w.into()).collect();
        let pattern = self.engine.calculate_interference_pattern(&states);
        let analysis = self.engine.analyze_interference(&pattern);
        
        // Проверяем качество интерференции
        let has_constructive = analysis.average_amplitude > 0.1; // Порог для конструктивной интерференции
        let has_destructive = analysis.average_amplitude < -0.1; // Порог для деструктивной интерференции
        
        // Консенсус достигнут, если есть и конструктивная, и деструктивная интерференция
        Ok(has_constructive && has_destructive)
    }

    /// Обрабатывает батч сообщений с настоящей интерференцией
    pub fn process_batch(&mut self, messages: Vec<ConsensusMessage>) -> Result<(), String> {
        if messages.is_empty() {
            return Ok(());
        }
        
        // 1. Создаём волны для всех сообщений с разными амплитудами и фазами
        let waves: Vec<QuantumWave> = messages
            .iter()
            .map(|msg| {
                let state = self.create_state_from_message(msg);
                QuantumWave::from(state)
            })
            .collect();
        
        // 2. Добавляем все волны в квантовое поле
        for wave in &waves {
            self.field.add_wave(wave.id.to_string(), wave.clone());
        }
        
        // 3. Рассчитываем интерференцию для ВСЕХ волн вместе
        let states: Vec<QuantumState> = waves.iter().map(|w| w.into()).collect();
        let pattern = self.engine.calculate_interference_pattern(&states);
        let analysis = self.engine.analyze_interference(&pattern);
        
        // 4. Создаём вероятностную операцию НА ОСНОВЕ паттерна интерференции
        let op = self.create_probabilistic_decision(&analysis, &messages);
        let outcome = op.execute();
        
        // 5. Принимаем решение на основе результата вероятностного коллапса
        match outcome.label.as_str() {
            "ConsensusReached" => Ok(()), // Консенсус достигнут
            "ConflictDetected" => Err("Деструктивная интерференция: консенсус не достигнут".into()),
            "PartialConsensus" => {
                // Частичный консенсус - применяем только часть транзакций
                let success_count = (messages.len() as f64 * outcome.value.as_ref()
                    .and_then(|v| v.get("consensus_ratio").and_then(|r| r.as_f64()))
                    .unwrap_or(0.5)) as usize;
                
                let _success_count = success_count.clamp(0, messages.len());
                // Здесь можно добавить логику для частичного применения
                Ok(())
            },
            _ => Err("Неопределённый результат консенсуса".into()),
        }
    }

    /// Создаёт вероятностную операцию НА ОСНОВЕ паттерна интерференции
    fn create_probabilistic_decision(&self, analysis: &crate::quantum::interference::InterferenceAnalysis, messages: &[ConsensusMessage]) -> ProbabilisticOperation {
        // 1. Анализируем качество интерференции
        let constructive_ratio = if analysis.total_points() > 0 {
            analysis.constructive_points.len() as f64 / analysis.total_points() as f64
        } else {
            0.0
        };
        
        let avg_amplitude = analysis.average_amplitude;
        
        // 2. Анализируем фазы волн (противофаза = конфликт)
        let phase_conflict = self.calculate_phase_conflict();
        
        // 3. Анализируем TTL волн (устаревшие = низкая вероятность)
        let ttl_factor = self.calculate_ttl_factor();
        
        // 4. Анализируем репутацию узлов
        let reputation_factor = self.calculate_reputation_factor(messages);
        
        // 5. Рассчитываем итоговую вероятность успеха на основе физики
        let mut success_prob = 0.0;
        
        // Конструктивная интерференция повышает вероятность
        success_prob += constructive_ratio * 0.4;
        
        // Амплитуда влияет на вероятность
        success_prob += (avg_amplitude + 1.0) * 0.2; // Нормализуем [-1, 1] -> [0, 2]
        
        // Фазы влияют на вероятность
        success_prob += (1.0 - phase_conflict) * 0.2;
        
        // TTL влияет на вероятность
        success_prob += ttl_factor * 0.1;
        
        // Репутация влияет на вероятность
        success_prob += reputation_factor * 0.1;
        
        // Ограничиваем вероятность
        let success_prob = success_prob.clamp(0.0, 1.0);
        
        // 6. Создаём операцию с рассчитанной вероятностью
        ProbabilisticOperation::new("consensus_decision", success_prob)
    }

    /// Рассчитывает конфликт фаз между волнами
    fn calculate_phase_conflict(&self) -> f64 {
        let snapshot = self.field.snapshot_waves();
        let waves: Vec<&QuantumWave> = snapshot.iter().collect();
        if waves.len() < 2 {
            return 0.0;
        }
        
        let mut total_conflict = 0.0;
        let mut comparisons = 0;
        
        for i in 0..waves.len() {
            for j in (i + 1)..waves.len() {
                let phase_diff = (waves[i].phase - waves[j].phase).abs();
                // Противофаза (π) = максимальный конфликт
                let conflict = (phase_diff - std::f64::consts::PI).abs() / std::f64::consts::PI;
                total_conflict += conflict;
                comparisons += 1;
            }
        }
        
        if comparisons > 0 {
            total_conflict / comparisons as f64
        } else {
            0.0
        }
    }

    /// Рассчитывает фактор TTL (время жизни волн)
    fn calculate_ttl_factor(&self) -> f64 {
        let now = std::time::Instant::now();
        let mut total_ttl_factor = 0.0;
        let mut wave_count = 0;
        
        for wave in self.field.snapshot_waves().iter() {
            let age = now.duration_since(wave.created_at);
            let ttl_ratio = age.as_secs_f64() / wave.lifetime.as_secs_f64();
            
            // Чем старше волна, тем ниже фактор
            let factor = if ttl_ratio < 0.5 {
                1.0 // Молодая волна
            } else if ttl_ratio < 0.8 {
                0.5 // Средняя волна
            } else {
                0.0 // Устаревшая волна
            };
            
            total_ttl_factor += factor;
            wave_count += 1;
        }
        
        if wave_count > 0 {
            total_ttl_factor / wave_count as f64
        } else {
            0.0
        }
    }

    /// Рассчитывает фактор репутации узлов
    fn calculate_reputation_factor(&self, messages: &[ConsensusMessage]) -> f64 {
        // Простая модель репутации на основе размера данных и отправителя
        let mut total_reputation = 0.0;
        
        for msg in messages {
            // Репутация зависит от размера данных (больше данных = выше репутация)
            let data_factor = (msg.raw_data.len() as f64 / 1000.0).clamp(0.1, 1.0);
            
            // Репутация зависит от отправителя (можно расширить)
            let sender_factor = if msg.sender_id.len() > 10 { 1.0 } else { 0.5 };
            
            let reputation = (data_factor + sender_factor) / 2.0;
            total_reputation += reputation;
        }
        
        if !messages.is_empty() {
            total_reputation / messages.len() as f64
        } else {
            0.0
        }
    }

    /// Создаёт состояние с амплитудой и фазой, зависящими от контекста сообщения
    fn create_state_from_message(&self, msg: &ConsensusMessage) -> QuantumState {
        // Амплитуда зависит от размера данных
        let amplitude = (msg.raw_data.len() as f64 / 1000.0).clamp(0.1, 1.0);
        
        // Фаза зависит от времени и содержимого
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let phase = (timestamp % 1000) as f64 * 0.001 * 2.0 * std::f64::consts::PI;
        
        // Шард зависит от отправителя
        let shard_id = format!("shard-{}", msg.sender_id.len() % 3);
        
        QuantumState {
            id: msg.state_id.clone(),
            shard_id,
            amplitude,
            phase,
            superposition: vec![StateVector {
                value: Complex::new(msg.raw_data.len() as f64, 0.0),
                probability: 1.0,
            }],
        }
    }

    /// Проверяет интерференцию для батча (настоящая проверка, не по одной!)
    pub fn check_interference_batch(&self, state_ids: &[String]) -> Result<Vec<bool>, String> {
        if state_ids.is_empty() {
            return Ok(vec![]);
        }
        
        // Получаем ВСЕ активные волны из поля
        let all_waves: Vec<QuantumWave> = self.field.snapshot_waves();
        
        if all_waves.len() < 2 {
            return Ok(vec![true; state_ids.len()]); // Нужно минимум 2 волны
        }
        
        // Рассчитываем общую интерференцию для всех волн
        let states: Vec<QuantumState> = all_waves.iter().map(|w| w.into()).collect();
        let pattern = self.engine.calculate_interference_pattern(&states);
        let analysis = self.engine.analyze_interference(&pattern);
        
        // Применяем результат интерференции ко всем транзакциям
        let consensus_reached = analysis.average_amplitude > 0.0;
        Ok(vec![consensus_reached; state_ids.len()])
    }

    pub fn validate_entity(&mut self, entity: &serde_json::Value) -> bool {
        // Universal validation of any entity via quantum interference
        let processor = QuantumTransactionProcessor::new(0.0, 1000000.0);
        let state = processor.create_quantum_state_from_any(entity);
        let pattern = self.engine.calculate_interference_pattern(&[state]);
        let analysis = self.engine.analyze_interference(&pattern);
        !analysis.constructive_points.is_empty()
    }

    pub fn process_dag_message(&mut self, msg: ConsensusMessage) {
        // Handle DAG with interference
        // FIXME: Need a way to get the public key for the sender
        // For now, we can't verify, so we just process
        // self.process_message(msg, &sender_public_key).unwrap_or_else(|e| {
        //     error!("Failed to process DAG message: {}", e);
        //     QuantumState::default()
        // });
    }

    /// Обрабатывает батч ПОДПИСАННЫХ квантовых волн с их публичными ключами отправителей
    /// Выполняет батч-верификацию подписей через `QuantumField::verify_waves_batch()`,
    /// профилирует время, затем применяет физическую валидацию (интерференция)
    /// и настраивает жизненный цикл (амплитуда/TTL), очищая устаревшие волны.
    pub fn process_waves_batch(
        &mut self,
        mut items: Vec<(QuantumWave, VerifyingKey)>,
    ) -> Result<usize, String> {
        if items.is_empty() {
            return Ok(0);
        }

        // Профилируем только часть верификации подписей
        let t_verify_start = std::time::Instant::now();

        // Подготавливаем ссылки для батч-верификации
        // ВАЖНО: держим собственный вектор волн, чтобы ссылки были валидны на время вызова
        let waves: Vec<QuantumWave> = items.iter().map(|(w, _)| w.clone()).collect();
        let refs: Vec<(&QuantumWave, VerifyingKey)> = items
            .iter()
            .enumerate()
            .map(|(i, (_w, vk))| (&waves[i], *vk))
            .collect();

        let all_valid = self.field.verify_waves_batch(&refs);
        let verify_elapsed = t_verify_start.elapsed();

        #[cfg(feature = "zebra_batch")]
        println!(
            "Batch verify (zebra_batch=ON): items={}, time_ms={}",
            refs.len(),
            verify_elapsed.as_millis()
        );
        #[cfg(not(feature = "zebra_batch"))]
        println!(
            "Batch verify (zebra_batch=OFF, fallback parallel): items={}, time_ms={}",
            refs.len(),
            verify_elapsed.as_millis()
        );

        if !all_valid {
            return Err("Batch signature verification failed".to_string());
        }

        // Добавляем валидные волны в поле (повторная проверка допустима для надёжности)
        let mut accepted = 0usize;
        for (wave, vk) in items.drain(..) {
            match self.field.add_signed_wave(wave.id.to_string(), wave.clone(), &vk) {
                Ok(_) => accepted += 1,
                Err(e) => {
                    // Если повторная проверка не прошла (например, TTL), пропускаем
                    eprintln!("Skip wave: {} due to {}", wave.id, e);
                }
            }
        }

        // Физическая валидация через интерференцию и настройка жизненного цикла
        // Для упрощения применяем решение по каждой волне отдельно (метод сам считает интерференцию со всеми)
        use crate::quantum::field::WaveValidationResult;
        let mut results: HashMap<Arc<str>, WaveValidationResult> = HashMap::new();
        for w in self.field.snapshot_waves().into_iter() {
            let verdict = self.validate_wave_through_interference(&w);
            results.insert(Arc::clone(&w.id), verdict);
        }

        // Применяем изменения амплитуд/TTL в соответствии с результатами
        self.field.update_wave_amplitudes(&results);

        // Очистка: удаляем погашенные и истекшие волны
        self.field.remove_extinguished_waves();
        self.field.cleanup_expired_waves();

        Ok(accepted)
    }

    /// Удобный враппер: принимает ссылки на волны/ключи (например из внешнего кэша),
    /// копирует их и вызывает `process_waves_batch`.
    pub fn process_waves_batch_refs(
        &mut self,
        items: &[(&QuantumWave, &VerifyingKey)],
    ) -> Result<usize, String> {
        let owned: Vec<(QuantumWave, VerifyingKey)> = items
            .iter()
            .map(|(w, vk)| ((*w).clone(), **vk))
            .collect();
        self.process_waves_batch(owned)
    }

    // pub fn process_messages_parallel(nodes: &mut [ConsensusNode], message: ConsensusMessage) -> Result<(), String> {
    //     nodes.par_iter_mut().for_each(|node| {
    //         // FIXME: Need public key here
    //         node.process_message(message.clone()).unwrap_or_else(|e| {
    //             error!("Failed to process message: {}", e);
    //             QuantumState::default()
    //         });
    //     });
    //     Ok(())
    // }

    pub fn check_interference_parallel(nodes: &[ConsensusNode], state_id: &str) -> Result<bool, String> {
        let results: Vec<bool> = nodes.par_iter()
            .map(|node| node.check_interference(state_id).unwrap_or(false))
            .collect();
        
        Ok(results.iter().all(|&x| x))
    }

    /// Валидирует волну через интерференцию с другими волнами
    pub fn validate_wave_through_interference(&self, wave: &QuantumWave) -> crate::quantum::field::WaveValidationResult {
        // 1. Проверяем подпись волны
        if !wave.verify_signature(&self.public_key) {
            return crate::quantum::field::WaveValidationResult::Rejected("Invalid signature".to_string());
        }
        
        // 2. Добавляем волну во временное поле для анализа интерференции
        // 3. Рассчитываем интерференцию ВСЕХ волн вместе + добавленная
        let extra = vec![wave.clone()];
        let pattern = self.field.calculate_interference_with_extra(&extra, 10_000);
        let analysis = self.engine.analyze_interference(&pattern);
        
        // 4. Принимаем решение на основе интерференции
        match self.decide_wave_fate(&analysis, wave) {
            WaveFate::Amplify => crate::quantum::field::WaveValidationResult::Validated(analysis.average_amplitude),
            WaveFate::Attenuate => crate::quantum::field::WaveValidationResult::PartiallyValidated(analysis.average_amplitude * 0.5),
            WaveFate::Cancel => crate::quantum::field::WaveValidationResult::Rejected("Destructive interference".to_string()),
        }
    }
    
    /// Решает судьбу волны на основе интерференции
    fn decide_wave_fate(&self, analysis: &crate::quantum::interference::InterferenceAnalysis, wave: &QuantumWave) -> WaveFate {
        let constructive_ratio = if analysis.total_points() > 0 {
            analysis.constructive_points.len() as f64 / analysis.total_points() as f64
        } else {
            0.0
        };
        
        let destructive_ratio = if analysis.total_points() > 0 {
            analysis.destructive_points.len() as f64 / analysis.total_points() as f64
        } else {
            0.0
        };
        
        // Анализируем фазу волны относительно других
        let phase_conflict = self.calculate_phase_conflict_with_wave(wave);
        
        // Принимаем решение:
        if constructive_ratio > 0.7 && phase_conflict < 0.3 {
            WaveFate::Amplify // Усиливаем волну
        } else if destructive_ratio > 0.6 || phase_conflict > 0.7 {
            WaveFate::Cancel // Гасим волну
        } else {
            WaveFate::Attenuate // Частично ослабляем
        }
    }

    /// Рассчитывает конфликт фаз между волнами
    fn calculate_phase_conflict_with_wave(&self, target_wave: &QuantumWave) -> f64 {
        let snapshot = self.field.snapshot_waves();
        let waves: Vec<&QuantumWave> = snapshot.iter().collect();
        if waves.len() < 2 {
            return 0.0;
        }
        
        let mut total_conflict = 0.0;
        let mut comparisons = 0;
        
        for wave in waves {
            if wave.id != target_wave.id {
                let phase_diff = (wave.phase - target_wave.phase).abs();
                // Противофаза (π) = максимальный конфликт
                let conflict = (phase_diff - std::f64::consts::PI).abs() / std::f64::consts::PI;
                total_conflict += conflict;
                comparisons += 1;
            }
        }
        
        if comparisons > 0 {
            total_conflict / comparisons as f64
        } else {
            0.0
        }
    }
}

// Добавляем enum для судьбы волны
#[derive(Debug)]
pub enum WaveFate {
    Amplify,    // Усилить волну
    Attenuate,  // Ослабить волну
    Cancel,     // Погасить волну
}

// Add DAG integration
pub struct DagConsensus {
    // DAG-specific fields
}