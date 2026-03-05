use super::{Transaction, TransactionProcessor, TransactionStatus};
use crate::core::field::{CoreField, StateVector, CoreWave, WaveValidationResult};
use crate::core::prob_ops::{ProbabilisticOperation, OperationOutcome};
use crate::core::interference::InterferenceEngine;
use crate::core::consensus::{ConsensusNode, BatchContextHandle};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use lazy_static::lazy_static;
use rayon::prelude::*;

use num_complex::Complex;
use super::{SmartContract, SmartContractProcessor, ContractStatus};

// use crate::crypto::bls::{self, SecretKey, BlsSecretKeyExt};

use rand_core::OsRng;
use serde_json;

lazy_static! {
    static ref TRANSACTION_CACHE: Mutex<HashMap<String, Transaction>> = Mutex::new(HashMap::new());
    static ref Core_FIELD: Arc<Mutex<CoreField>> = Arc::new(Mutex::new(CoreField::new()));
}

/// Реализация обработчика транзакций с квантово-вдохновленным подходом
pub struct CoreTransactionProcessor {
    min_amount: f64,
    max_amount: f64,
    interference_engine: Arc<InterferenceEngine>,
}

impl CoreTransactionProcessor {
    pub fn new(min_amount: f64, max_amount: f64) -> Self {
        Self {
            min_amount,
            max_amount,
            interference_engine: Arc::new(InterferenceEngine::new(1000, 0.95)),
        }
    }

    /// Создает подписанную квантовую волну из транзакции
    pub fn create_signed_wave_from_transaction(
        &self,
        transaction: &Transaction,
        _secret_key: &() // &SecretKey - removed BLS dependency
    ) -> CoreWave {
        // Legacy method - kept for compatibility
        // Just create wave without signing for now
        self.create_signed_wave_from_transaction_schnorr(transaction, &mut ConsensusNode::new(0, 0))
    }
    
    /// PHASE 2: Creates signed Core wave from transaction using Schnorr
    pub fn create_signed_wave_from_transaction_schnorr(
        &self,
        transaction: &Transaction,
        consensus_node: &ConsensusNode
    ) -> CoreWave {
        let ctx = consensus_node.batch_context.load();
        let block_salt = crate::core::consensus::batch_context::derive_block_salt(&ctx, &transaction.id.to_string());

        let mut wave = CoreWave::from_transaction(
            &transaction.id.to_string(),
            transaction.amount as u64,
            &block_salt,
            &transaction.data,
            ctx.slot as u32,
            1000.0,
            ctx.shard_id,
        );

        let sig = consensus_node.sign_message(&transaction.data);
        wave.signature = sig.to_bytes().to_vec();

        wave
    }

    /// Рассчитывает амплитуду волны на основе транзакции
    fn calculate_transaction_amplitude(&self, transaction: &Transaction) -> f64 {
        // Амплитуда зависит от суммы и размера данных
        let amount_factor = (transaction.amount / 1000.0).clamp(0.1, 1.0);
        let data_factor = (transaction.data.len() as f64 / 100.0).clamp(0.1, 1.0);
        (amount_factor + data_factor) / 2.0
    }

    /// Рассчитывает фазу волны на основе транзакции
    fn calculate_transaction_phase(&self, transaction: &Transaction) -> f64 {
        // Фаза зависит от времени и отправителя
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let phase = (timestamp % 1000) as f64 * 0.001 * 2.0 * std::f64::consts::PI;
        
        // Добавляем случайность на основе отправителя
        let sender_hash = transaction.sender.len() as u64;
        let additional_phase = (sender_hash % 1000) as f64 * 0.001 * 2.0 * std::f64::consts::PI;
        
        (phase + additional_phase) % (2.0 * std::f64::consts::PI)
    }

    /// Назначает шард для транзакции
    fn assign_shard(&self, transaction: &Transaction) -> u8 {
        use crate::sharding::assign_shard;
        // Use const_routing via global assign_shard
        let mut hash = [0u8; 32];
        let tx_hash = blake3::hash(transaction.id.as_bytes());
        hash.copy_from_slice(tx_hash.as_bytes());
        assign_shard(&hash)
    }

    /// Обрабатывает транзакцию через квантовую валидацию
    pub fn process_transaction_Core(
        &self,
        transaction: &Transaction,
        consensus_node: &mut ConsensusNode
    ) -> TransactionResult {
        // 1. Создаем подписанную волну (PHASE 2: pass whole consensus node)
        let wave = self.create_signed_wave_from_transaction_schnorr(transaction, consensus_node);
        
        // 2. Валидируем волну через интерференцию
        let validation_result = consensus_node.validate_wave_through_interference(&wave);
        
        // 3. Применяем результат к транзакции
        match validation_result {
            WaveValidationResult::Validated(amplitude) => {
                TransactionResult::Success {
                    transaction_id: transaction.id,
                    Core_amplitude: amplitude,
                    validation_method: "Interference consensus".to_string(),
                }
            },
            WaveValidationResult::PartiallyValidated(amplitude) => {
                TransactionResult::PartialSuccess {
                    transaction_id: transaction.id,
                    Core_amplitude: amplitude,
                    reason: "Partial interference validation".to_string(),
                }
            },
            WaveValidationResult::Rejected(reason) => {
                TransactionResult::Failure {
                    transaction_id: transaction.id,
                    reason: format!("Core interference rejection: {}", reason),
                }
            }
        }
    }

    /// Обрабатывает батч транзакций с новой системой валидации
    pub fn process_batch_Core(
        &self,
        transactions: &[Transaction],
        consensus_nodes: &mut [ConsensusNode]
    ) -> Vec<TransactionResult> {
        let mut results = Vec::new();
        
        for (i, transaction) in transactions.iter().enumerate() {
            // Распределяем транзакции по узлам консенсуса
            let node_index = i % consensus_nodes.len();
            let consensus_node = &mut consensus_nodes[node_index];
            
            let result = self.process_transaction_Core(transaction, consensus_node);
            results.push(result);
        }
        
        results
    }

    /// Обновляет состояние волн в квантовом поле на основе результатов валидации
    pub fn update_wave_states(
        &self,
        validation_results: &[TransactionResult],
        _consensus_nodes: &mut [ConsensusNode]
    ) -> Result<(), String> {
        let mut field = Core_FIELD.lock().unwrap();
        
        for result in validation_results {
            match result {
                TransactionResult::Success { transaction_id, Core_amplitude: _, .. } => {
                    // Находим волну и усиливаем её
                    if let Some(&idx) = field.active_waves.get(&transaction_id.to_string()) {
                        if let Some(wave) = crate::core::static_storage::StaticWaveStorage::get_mut(idx) {
                            wave.amplitude *= 1.5;
                            // wave.lifetime = std::time::Duration::from_secs(600);
                        }
                    }
                },
                TransactionResult::PartialSuccess { transaction_id, Core_amplitude: _, .. } => {
                    // Частично ослабляем волну
                    if let Some(&idx) = field.active_waves.get(&transaction_id.to_string()) {
                        if let Some(wave) = crate::core::static_storage::StaticWaveStorage::get_mut(idx) {
                            wave.amplitude *= 0.7;
                            // wave.lifetime = std::time::Duration::from_secs(300);
                        }
                    }
                },
                TransactionResult::Failure { transaction_id, .. } => {
                    // Гасим волну
                    if let Some(&idx) = field.active_waves.get(&transaction_id.to_string()) {
                        if let Some(wave) = crate::core::static_storage::StaticWaveStorage::get_mut(idx) {
                            wave.amplitude *= 0.1;
                            // wave.lifetime = std::time::Duration::from_millis(50);
                        }
                    }
                }
            }
        }
        
        // Удаляем полностью погашенные волны
        // field.remove_extinguished_waves();
        
        Ok(())
    }

    fn create_Core_wave(&self, transaction: &Transaction) -> CoreWave {
        let ctx = self.default_batch_context().load();
        let block_salt = crate::core::consensus::batch_context::derive_block_salt(&ctx, &transaction.id.to_string());
        CoreWave::from_transaction(
            &transaction.id.to_string(),
            transaction.amount as u64,
            &block_salt,
            &transaction.data,
            ctx.slot as u32,
            1000.0,
            ctx.shard_id,
        )
    }

    fn create_probabilistic_operation(&self, transaction: &Transaction) -> ProbabilisticOperation {
        ProbabilisticOperation {
            description: format!("Transaction processing for {}", transaction.id),
            outcomes: vec![
                OperationOutcome {
                    label: "Success".to_string(),
                    probability: 0.8,
                    value: Some(serde_json::json!({
                        "status": "completed",
                        "transaction_id": transaction.id.to_string()
                    })),
                },
                OperationOutcome {
                    label: "Failed".to_string(),
                    probability: 0.2,
                    value: Some(serde_json::json!({
                        "status": "failed",
                        "error": "Transaction processing failed"
                    })),
                },
            ],
        }
    }

    pub fn create_Core_state_from_any(&self, data: &serde_json::Value) -> CoreWave {
        // Универсальный конструктор квантового состояния из сериализованной сущности
        let id = data.get("id").map(|v| v.to_string()).unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let amplitude = data.get("amplitude").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
        let phase = data.get("phase").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
        let shard_id = data.get("shard_id").and_then(|v| v.as_u64()).unwrap_or(0) as u8;
        
        CoreWave {
            id: id.into(),
            amplitude,
            phase,
            position: [0.0, 0.0, 0.0],
            timestamp: 0,
            shard_id,
            shard_index: 0,
            signature: vec![],
            created_at: std::time::Instant::now(),
            lifetime: std::time::Duration::from_secs(60),
        }
    }

    pub fn batch_process(&self, transactions: &mut [Transaction]) -> Vec<Result<(), String>> {
        transactions.par_iter_mut().map(|tx| self.process(tx)).collect()
    }

    // Удалена старая BLS-агрегация подписей. Для Ed25519 можно реализовать батч-верификацию на уровне волн/сообщений.

    pub fn generate_zk_proof(&self, _tx: &Transaction) -> Result<Vec<u8>, String> {
        // TODO: Реализовать генерацию ZK-доказательства
        Ok(vec![])
    }
}

impl TransactionProcessor for CoreTransactionProcessor {
    /// Обрабатывает одиночную транзакцию (для совместимости)
    fn process(&self, transaction: &mut Transaction) -> Result<(), String> {
        // Для одиночной транзакции создаём батч из одного элемента
        let mut batch = vec![transaction.clone()];
        let result = self.process_batch(&mut batch);
        
        // Обновляем оригинальную транзакцию
        if let Ok(()) = result {
            *transaction = batch[0].clone();
        }
        
        result
    }

    /// Обрабатывает батч транзакций с настоящей интерференцией
    fn process_batch(&self, transactions: &mut [Transaction]) -> Result<(), String> {
        if transactions.is_empty() {
            return Ok(());
        }
        
        // 1. Создаём волны для всех транзакций с разными амплитудами и фазами
        let waves: Vec<CoreWave> = transactions
            .iter()
            .map(|tx| self.create_wave_from_transaction(tx))
            .collect();
        
        // 2. Добавляем все волны в квантовое поле
        {
            let mut field = Core_FIELD.lock().unwrap();
            for wave in &waves {
                field.add_wave(wave.id.to_string(), wave.clone());
            }
        }
        
        // 3. Рассчитываем интерференцию для ВСЕХ волн вместе
        // OPTIMIZATION: Use inplace calculation (54M waves/sec)
        let engine = &self.interference_engine;
        // Cast f32 to f64 for InterferenceEngine
        let pairs: Vec<(f64, f64)> = waves.iter().map(|w| (w.amplitude as f64, w.phase as f64)).collect();
        let pattern = engine.calculate_interference_from_pairs_inplace(&pairs);
        let analysis = engine.analyze_interference(&pattern);
        
        // 4. Создаём вероятностную операцию НА ОСНОВЕ паттерна интерференции
        let op = self.create_probabilistic_decision(&analysis, transactions);
        let outcome = op.execute();
        
        // 5. Принимаем решение на основе результата вероятностного коллапса
        match outcome.label.as_str() {
            "ConsensusReached" => {
                // Конструктивная интерференция: применяем все транзакции
                for tx in transactions {
                    tx.update_status(TransactionStatus::Completed);
                }
                Ok(())
            },
            "ConflictDetected" => {
                // Деструктивная интерференция: отклоняем все транзакции
                for tx in transactions {
                    tx.update_status(TransactionStatus::Failed);
                }
                Err("Деструктивная интерференция: консенсус не достигнут".into())
            },
            "PartialConsensus" => {
                // Частичный консенсус: применяем часть транзакций на основе интерференции
                let consensus_ratio = outcome.value.as_ref()
                    .and_then(|v| v.get("consensus_ratio").and_then(|r| r.as_f64()))
                    .unwrap_or(0.5);
                
                let success_count = (transactions.len() as f64 * consensus_ratio) as usize;
                let success_count = success_count.clamp(0, transactions.len());
                
                for (i, tx) in transactions.iter_mut().enumerate() {
                    if i < success_count {
                        tx.update_status(TransactionStatus::Completed);
                    } else {
                        tx.update_status(TransactionStatus::Failed);
                    }
                }
                Ok(())
            },
            _ => Err("Неопределённый результат консенсуса".into()),
        }
    }

    /// Создаёт волну с амплитудой и фазой, зависящими от контекста транзакции
    fn create_wave_from_transaction(&self, tx: &Transaction) -> CoreWave {
        let ctx = self.default_batch_context().load();
        let block_salt = crate::core::consensus::batch_context::derive_block_salt(&ctx, &tx.id.to_string());
        CoreWave::from_transaction(
            &tx.id.to_string(),
            tx.amount as u64,
            &block_salt,
            &tx.data,
            ctx.slot as u32,
            1000.0,
            ctx.shard_id,
        )
    }

    fn default_batch_context(&self) -> BatchContextHandle {
        BatchContextHandle::new()
    }

    /// Создаёт вероятностную операцию НА ОСНОВЕ паттерна интерференции
    fn create_probabilistic_decision(&self, analysis: &crate::core::interference::InterferenceAnalysis, transactions: &[Transaction]) -> ProbabilisticOperation {
        // 1. Анализируем качество интерференции
        let constructive_ratio = if analysis.total_points() > 0 {
            analysis.constructive_count as f64 / analysis.total_points() as f64
        } else {
            0.0
        };
        
        let avg_amplitude = analysis.average_amplitude;
        
        // 2. Анализируем качество транзакций
        let transaction_quality = self.calculate_transaction_quality(transactions);
        
        // 3. Анализируем TTL волн
        let ttl_factor = self.calculate_ttl_factor();
        
        // 4. Анализируем репутацию отправителей
        let reputation_factor = self.calculate_sender_reputation(transactions);
        
        // 5. Рассчитываем итоговую вероятность успеха на основе физики
        let mut success_prob = 0.0;
        
        // Конструктивная интерференция повышает вероятность
        success_prob += constructive_ratio * 0.35;
        
        // Амплитуда влияет на вероятность
        success_prob += (avg_amplitude + 1.0) * 0.25; // Нормализуем [-1, 1] -> [0, 2]
        
        // Качество транзакций влияет на вероятность
        success_prob += transaction_quality * 0.2;
        
        // TTL влияет на вероятность
        success_prob += ttl_factor * 0.1;
        
        // Репутация отправителей влияет на вероятность
        success_prob += reputation_factor * 0.1;
        
        // Ограничиваем вероятность
        let success_prob = success_prob.clamp(0.0, 1.0);
        
        // 6. Создаём операцию с рассчитанной вероятностью
        ProbabilisticOperation::new("transaction_consensus", success_prob)
    }

    /// Рассчитывает качество транзакций
    fn calculate_transaction_quality(&self, transactions: &[Transaction]) -> f64 {
        let mut total_quality = 0.0;
        
        for tx in transactions {
            let mut quality = 0.0;
            
            // Качество зависит от суммы (больше = лучше, но с ограничениями)
            if tx.amount >= self.min_amount && tx.amount <= self.max_amount {
                quality += 0.4;
            } else if tx.amount > 0.0 {
                quality += 0.2; // Частично валидная
            }
            
            // Качество зависит от валидности
            if tx.is_valid() {
                quality += 0.3;
            }
            
            // Качество зависит от размера данных
            let data_factor = (tx.data.len() as f64 / 1000.0).clamp(0.1, 1.0);
            quality += data_factor * 0.3;
            
            total_quality += quality;
        }
        
        if !transactions.is_empty() {
            total_quality / transactions.len() as f64
        } else {
            0.0
        }
    }

    /// Рассчитывает фактор TTL (время жизни волн)
    fn calculate_ttl_factor(&self) -> f64 {
        let field = Core_FIELD.lock().unwrap();
        let now = std::time::Instant::now();
        let mut total_ttl_factor = 0.0;
        let mut wave_count = 0;
        
        for &idx in field.active_waves.values() {
            if let Some(wave) = crate::core::static_storage::StaticWaveStorage::get(idx) {
                // Minkowski: use timestamp (slot) instead of created_at/lifetime
                // For now, return 1.0 as placeholder
                let factor = 1.0;
                
                total_ttl_factor += factor;
                wave_count += 1;
            }
        }
        
        if wave_count > 0 {
            total_ttl_factor / wave_count as f64
        } else {
            0.0
        }
    }

    /// Рассчитывает фактор репутации отправителей
    fn calculate_sender_reputation(&self, transactions: &[Transaction]) -> f64 {
        let mut total_reputation = 0.0;
        
        for tx in transactions {
            let mut reputation = 0.0;
            
            // Репутация зависит от длины отправителя
            let sender_factor = if tx.sender.len() > 10 { 1.0 } else { 0.5 };
            reputation += sender_factor * 0.4;
            
            // Репутация зависит от суммы (больше = выше репутация, но с ограничениями)
            let amount_factor = if tx.amount >= self.min_amount && tx.amount <= self.max_amount {
                1.0
            } else if tx.amount > 0.0 {
                0.5
            } else {
                0.0
            };
            reputation += amount_factor * 0.3;
            
            // Репутация зависит от размера данных
            let data_factor = (tx.data.len() as f64 / 1000.0).clamp(0.1, 1.0);
            reputation += data_factor * 0.3;
            
            total_reputation += reputation;
        }
        
        if !transactions.is_empty() {
            total_reputation / transactions.len() as f64
        } else {
            0.0
        }
    }

    /// Валидирует транзакцию через интерференцию с другими активными волнами
    fn verify(&self, transaction: &Transaction) -> bool {
        // Создаём волну для текущей транзакции
        let wave = self.create_wave_from_transaction(transaction);
        
        // Добавляем в поле
        {
            let mut field = Core_FIELD.lock().unwrap();
            field.add_wave(wave.id.to_string(), wave.clone());
        }
        
        // Получаем ВСЕ активные волны из поля
        let all_waves: Vec<CoreWave> = {
            let field = Core_FIELD.lock().unwrap();
            field.snapshot_waves()
        };
        
        if all_waves.len() < 2 {
            return true; // Нужно минимум 2 волны для интерференции
        }
        
        // Рассчитываем интерференцию между ВСЕМИ волнами
        // OPTIMIZATION: Use inplace calculation (54M waves/sec)
        let engine = &self.interference_engine;
        // Cast f32 to f64 for InterferenceEngine
        let pairs: Vec<(f64, f64)> = all_waves.iter().map(|w| (w.amplitude as f64, w.phase as f64)).collect();
        let pattern = engine.calculate_interference_from_pairs_inplace(&pairs);
        let analysis = engine.analyze_interference(&pattern);
                
        // Создаём вероятностную операцию НА ОСНОВЕ интерференции
        let op = self.create_verification_decision(&analysis, transaction);
        let outcome = op.execute();
        
        // Принимаем решение на основе результата вероятностного коллапса
        outcome.label == "ConsensusReached"
    }

    /// Создаёт вероятностную операцию для верификации НА ОСНОВЕ интерференции
    fn create_verification_decision(&self, analysis: &crate::core::interference::InterferenceAnalysis, transaction: &Transaction) -> ProbabilisticOperation {
        // 1. Анализируем качество интерференции
        let constructive_ratio = if analysis.total_points() > 0 {
            analysis.constructive_count as f64 / analysis.total_points() as f64
        } else {
            0.0
        };
        
        let avg_amplitude = analysis.average_amplitude;
        
        // 2. Анализируем качество транзакции
        let transaction_quality = if transaction.is_valid() && 
            transaction.amount >= self.min_amount && 
            transaction.amount <= self.max_amount {
            1.0
        } else if transaction.amount > 0.0 {
            0.5
        } else {
            0.0
        };
        
        // 3. Рассчитываем вероятность успеха на основе физики
        let mut success_prob = 0.0;
        
        // Конструктивная интерференция повышает вероятность
        success_prob += constructive_ratio * 0.4;
        
        // Амплитуда влияет на вероятность
        success_prob += (avg_amplitude + 1.0) * 0.3; // Нормализуем [-1, 1] -> [0, 2]
        
        // Качество транзакции влияет на вероятность
        success_prob += transaction_quality * 0.3;
        
        // Ограничиваем вероятность
        let success_prob = success_prob.clamp(0.0, 1.0);
        
        // 4. Создаём операцию с рассчитанной вероятностью
        ProbabilisticOperation::new("verification", success_prob)
    }
}

/// Результат квантовой валидации транзакции
#[derive(Debug, Clone)]
pub enum TransactionResult {
    Success {
        transaction_id: uuid::Uuid,
        Core_amplitude: f64,
        validation_method: String,
    },
    PartialSuccess {
        transaction_id: uuid::Uuid,
        Core_amplitude: f64,
        reason: String,
    },
    Failure {
        transaction_id: uuid::Uuid,
        reason: String,
    },
}

pub struct CoreSmartContractProcessor;

impl SmartContractProcessor for CoreSmartContractProcessor {
    fn deploy(&self, contract: &mut SmartContract) -> Result<(), String> {
        // Простейшая логика: если код не пустой, считаем деплой успешным
        if contract.code.is_empty() {
            contract.status = ContractStatus::Failed;
            return Err("Пустой код контракта".to_string());
        }
        contract.status = ContractStatus::Deployed;
        Ok(())
    }

    fn execute(&self, contract: &mut SmartContract, input: Vec<u8>) -> Result<Vec<u8>, String> {
        // Простейшая логика: если контракт деплоен, выполнение успешно
        if !matches!(contract.status, ContractStatus::Deployed) {
            contract.status = ContractStatus::Aborted;
            return Err("Контракт не деплоен".to_string());
        }
        contract.status = ContractStatus::Executed;
        // Просто возвращаем входные данные как результат (заглушка)
        Ok(input)
    }

    fn verify(&self, contract: &SmartContract) -> bool {
        // Простейшая валидация: код не пустой
        !contract.code.is_empty()
    }
} 


