use crate::pipeline::{Pipeline, PipelineConfig};
use async_trait::async_trait;
use uuid::Uuid;
use log::{info, debug, error, warn};
use std::sync::Arc;
use parking_lot::Mutex;

use crate::network::types::{Message, PeerInfo};
use crate::network::error::NetworkError;
use crate::network::handler::MessageHandler;
use crate::quantum::consensus::{ConsensusNode, ConsensusMessage};
use crate::quantum::field::{QuantumWave, QuantumState};
use crate::tvm::state::StateView;
use crate::transaction::{Transaction, SmartContract};
use crate::sharding::ShardEvent;
use crate::error_analysis::{ErrorContext, regenerate_wavefield};
use crate::semantic::SemanticAction;
use bytes::Bytes;

/// Квантовый обработчик сетевых сообщений
/// Интегрирует сетевой слой с квантовым консенсусом
#[derive(Clone)]
pub struct QuantumNetworkHandler {
    node_id: Uuid,
    consensus_node: Arc<Mutex<ConsensusNode>>,
    state: Arc<StateView>,
    quantum_waves: Arc<Mutex<Vec<QuantumWave>>>,
    message_cache: Arc<Mutex<std::collections::HashMap<Uuid, Message>>>,
    interference_events: Arc<Mutex<Vec<NetworkInterferenceEvent>>>,
    pipeline: Arc<Pipeline>,
    // Isolation state: if true, this node temporarily ignores external traffic due to >1% resonance deviation
    is_isolated: Arc<Mutex<bool>>, 
}

#[derive(Debug, Clone)]
pub struct NetworkInterferenceEvent {
    pub timestamp: std::time::Instant,
    pub wave_id: std::sync::Arc<str>,
    pub interference_strength: f64,
    pub affected_peers: Vec<Uuid>,
}

impl QuantumNetworkHandler {
    /// Старый конструктор (обратная совместимость): создаёт новый StateView по умолчанию
    pub fn new(
        node_id: Uuid,
        consensus_node: Arc<Mutex<ConsensusNode>>,
    ) -> Self {
        let state = Arc::new(StateView::new());
        Self::new_with_state(node_id, consensus_node, state)
    }

    /// Новый конструктор: принимает глобальное состояние узла
    pub fn new_with_state(
        node_id: Uuid,
        consensus_node: Arc<Mutex<ConsensusNode>>,
        state: Arc<StateView>,
    ) -> Self {
        let pipe = Pipeline::new(PipelineConfig::from_env(), Arc::clone(&consensus_node), Arc::clone(&state));
        Self {
            node_id,
            consensus_node,
            state,
            quantum_waves: Arc::new(Mutex::new(Vec::new())),
            message_cache: Arc::new(Mutex::new(std::collections::HashMap::new())),
            interference_events: Arc::new(Mutex::new(Vec::new())),
            pipeline: Arc::new(pipe),
            is_isolated: Arc::new(Mutex::new(false)),
        }
    }

    /// Создает ПОДПИСАННУЮ квантовую волну из сетевого сообщения (использует ключ узла консенсуса)
    async fn create_quantum_wave_from_message(&self, message: &Message) -> QuantumWave {
        let wave_id = format!("network_wave_{}_{}", self.node_id, message.id);
        
        // Создаем суперпозицию на основе типа сообщения
        let superposition = match &message.message_type {
            crate::network::MessageType::Transaction(_) => {
                vec![crate::quantum::field::StateVector {
                    value: num_complex::Complex::new(1.0, 0.0),
                    probability: 0.8,
                }]
            },
            crate::network::MessageType::Consensus(_) => {
                vec![crate::quantum::field::StateVector {
                    value: num_complex::Complex::new(0.0, 1.0),
                    probability: 0.9,
                }]
            },
            _ => {
                vec![crate::quantum::field::StateVector {
                    value: num_complex::Complex::new(0.5, 0.5),
                    probability: 0.5,
                }]
            }
        };

        // Подписываем волну ключом консенсус-узла
        let mut consensus = self.consensus_node.lock();
        QuantumWave::new_signed(
            wave_id,
            1.0, // амплитуда
            0.0, // фаза
            "network".to_string(),
            superposition,
            &consensus.secret_key,
        )
    }

    /// Обрабатывает квантовую интерференцию в сети
    async fn process_network_interference(&self, wave: &QuantumWave, peers: &[PeerInfo]) {
        let interference_event = NetworkInterferenceEvent {
            timestamp: std::time::Instant::now(),
            wave_id: wave.id.clone(),
            interference_strength: wave.amplitude,
            affected_peers: peers.iter().map(|p| p.id).collect(),
        };

        {
            let mut events = self.interference_events.lock();
            events.push(interference_event.clone());
            
            // Ограничиваем количество событий
            if events.len() > 100 {
                events.remove(0);
            }
        }

        debug!("Квантовая интерференция в сети: волна={}, сила={}, затронуто узлов={}", 
            wave.id, interference_event.interference_strength, peers.len());
    }

    /// Очищает устаревшие квантовые волны
    async fn cleanup_expired_waves(&self) {
        let mut waves = self.quantum_waves.lock();
        let now = std::time::Instant::now();
        
        waves.retain(|wave| {
            now.duration_since(wave.created_at) < wave.lifetime
        });
    }

    /// Пакетная обработка сообщений из кэша с батч-верификацией подписей (zebra_batch)
    async fn process_cached_messages_batch(&self) {
        // Снимаем снимок кэша, чтобы минимизировать время блокировки
        let messages: Vec<Message> = {
            let cache = self.message_cache.lock();
            cache.values().cloned().collect()
        };

        if messages.is_empty() { return; }

        // Собираем подписанные волны и верифицируем батчом
        let mut items: Vec<(QuantumWave, ed25519_dalek::VerifyingKey)> = Vec::with_capacity(messages.len());

        // Берем публик ключ локального узла (демо)
        let vk = {
            let consensus = self.consensus_node.lock();
            consensus.public_key.clone()
        };

        for msg in &messages {
            let wave = self.create_quantum_wave_from_message(msg).await;
            items.push((wave, vk.clone()));
        }

        // Подготовим ссылки для батч-верификации
        let wave_refs: Vec<(&QuantumWave, ed25519_dalek::VerifyingKey)> = items.iter().map(|(w, k)| (w, k.clone())).collect();

        let mut consensus = self.consensus_node.lock();
        if consensus.field.verify_waves_batch(&wave_refs) {
            // Добавляем все волны в поле
            for (wave, _k) in items.into_iter() {
                let _ = consensus.field.add_signed_wave(wave.id.to_string(), wave, &vk);
            }
        } else {
            warn!("Batch verification failed for {} cached messages", messages.len());
        }
    }

    /// Получает статистику квантовой сети
    pub async fn get_quantum_network_stats(&self) -> QuantumNetworkStats {
        let waves = self.quantum_waves.lock();
        let events = self.interference_events.lock();
        let cache = self.message_cache.lock();

        QuantumNetworkStats {
            active_waves: waves.len(),
            interference_events: events.len(),
            cached_messages: cache.len(),
            total_wave_amplitude: waves.iter().map(|w| w.amplitude).sum(),
        }
    }

    /// Computes the local average interference amplitude snapshot for resonance comparison
    fn local_average_amplitude(&self) -> f64 {
        let mut cons = self.consensus_node.lock();
        let waves = cons.field.snapshot_waves();
        if waves.is_empty() { return 0.0; }
        let pattern = cons.engine.calculate_interference_from_waves_inplace(&waves);
        let analysis = cons.engine.analyze_interference(&pattern);
        analysis.average_amplitude
    }

    /// Returns true if node is currently isolated
    fn is_currently_isolated(&self) -> bool { *self.is_isolated.lock() }

    /// Set isolation state with logging
    fn set_isolation(&self, iso: bool, reason: &str) {
        let mut guard = self.is_isolated.lock();
        if *guard != iso {
            let was_iso = *guard;
            *guard = iso;
            if iso {
                warn!("[Resonance] Node isolated due to: {}", reason);
            } else {
                info!("[Resonance] Node restored to normal operation");
                // On recovery, trigger decentralized wavefield regeneration using recent interference events
                self.regenerate_from_trust();
            }
        }
    }

    /// Builds a trust map from recent network interference events and triggers regeneration
    fn regenerate_from_trust(&self) {
        // Build trust scores from last up to 100 events (amplitude normalized to [0,1])
        let events = self.interference_events.lock();
        if events.is_empty() { return; }
        let n = events.len();
        let start = if n > 100 { n - 100 } else { 0 };
        let slice = &events[start..];
        let mut max_amp = 0.0f64;
        for e in slice.iter() { max_amp = max_amp.max(e.interference_strength.abs()); }
        let denom = if max_amp < 1e-12 { 1.0 } else { max_amp };
        let mut trust: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
        for e in slice.iter() {
            let score = (e.interference_strength.abs() / denom).clamp(0.0, 1.0);
            trust.entry(e.wave_id.to_string()).and_modify(|s| { *s = (*s + score) * 0.5; }).or_insert(score);
        }
        drop(events);

        let updated = {
            let mut cons = self.consensus_node.lock();
            regenerate_wavefield(&mut *cons, &trust)
        };
        info!("[Regeneration] Applied wavefield regeneration updates: {} waves", updated);
    }
}

/// Удобная фабрика для инициализации обработчика с ГЛОБАЛЬНЫМ состоянием,
/// возвращает объект, готовый к установке в `Network::new(..., handler)`.
pub fn build_global_quantum_handler(
    node_id: Uuid,
    consensus_node: Arc<Mutex<ConsensusNode>>,
) -> Box<dyn crate::network::handler::MessageHandler + Send + Sync> {
    let state = Arc::new(StateView::new());
    let handler = QuantumNetworkHandler::new_with_state(node_id, consensus_node, state);
    Box::new(handler)
}

#[derive(Debug, Clone)]
pub struct QuantumNetworkStats {
    pub active_waves: usize,
    pub interference_events: usize,
    pub cached_messages: usize,
    pub total_wave_amplitude: f64,
}

#[async_trait]
impl MessageHandler for QuantumNetworkHandler {
    async fn handle_message(&mut self, from: Uuid, message: Message) -> Result<(), NetworkError> {
        info!("[QuantumNetworkHandler] Received message from {}: type={:?}, id={}", from, message.message_type, message.id);

        // Кэшируем исходное сетевое сообщение (для диагностики)
        {
            let mut cache = self.message_cache.lock();
            cache.insert(message.id, message.clone());
            
            // Ограничиваем размер кэша
            if cache.len() > 1000 {
                cache.clear();
            }
        }

        // Триггерим пакетную обработку, когда в кэше наберется минимум 16 сообщений
        if { self.message_cache.lock().len() } >= 16 {
            self.process_cached_messages_batch().await;
        }

        // Обрабатываем сообщение в зависимости от типа: направляем в Pipeline (Ingress)
        match message.message_type {
            crate::network::MessageType::Transaction(transaction) => {
                if self.is_currently_isolated() {
                    warn!("[Isolation] Dropping transaction from {} while isolated", from);
                } else {
                    self.handle_transaction_message(from, transaction).await?;
                }
            },
            crate::network::MessageType::Consensus(consensus_msg) => {
                if self.is_currently_isolated() {
                    warn!("[Isolation] Dropping consensus message from {} while isolated", from);
                } else {
                    self.handle_consensus_message(from, consensus_msg).await?;
                }
            },
            crate::network::MessageType::SmartContract(contract) => {
                if self.is_currently_isolated() {
                    warn!("[Isolation] Dropping smart contract from {} while isolated", from);
                } else {
                    self.handle_smart_contract_message(from, contract).await?;
                }
            },
            // Resonance synchronization: compare peer amplitude with local, isolate if deviation > 1%
            crate::network::MessageType::ResonanceMetrics { amp, ts: _ } => {
                let local = self.local_average_amplitude();
                let denom = if local.abs() < 1e-12 { 1.0 } else { local.abs() };
                let dev = ((amp - local).abs() / denom).min(1.0);
                if dev > 0.01 {
                    self.set_isolation(true, &format!("resonance deviation {:.4}", dev));
                } else {
                    self.set_isolation(false, "within 1% deviation");
                }
            },
            crate::network::MessageType::IsolationNotice { reason } => {
                self.set_isolation(true, &format!("peer issued isolation notice: {}", reason));
            },
            crate::network::MessageType::ShardEvent(event) => {
                if self.is_currently_isolated() {
                    warn!("[Isolation] Dropping shard event from {} while isolated", from);
                } else {
                    self.handle_shard_event_message(from, event).await?;
                }
            },
            crate::network::MessageType::ErrorEvent(error_ctx) => {
                self.handle_error_event_message(from, error_ctx).await?;
            },
            crate::network::MessageType::SemanticAction(action) => {
                self.handle_semantic_action_message(from, action).await?;
            },
            crate::network::MessageType::Error(error_str) => {
                warn!("Received error from {}: {}", from, error_str);
            },
        }

        // Очищаем устаревшие волны
        self.cleanup_expired_waves().await;

        Ok(())
    }

    async fn handle_peer_connected(&mut self, peer_info: PeerInfo) -> Result<(), NetworkError> {
        info!("[QuantumNetworkHandler] Новый peer подключен: {} ({})", peer_info.name, peer_info.id);

        // Формируем сервисное консенсус-сообщение и отправляем в pipeline
        // PeerInfo не реализует Serialize, используем переносимый формат
        let spi = crate::network::types::SerializablePeerInfo::from(&peer_info);
        let payload = serde_json::to_vec(&spi).unwrap_or_default();
        let consensus_message = ConsensusMessage {
            sender_id: peer_info.id.to_string(),
            state_id: format!("peer_connected_{}", peer_info.id),
            raw_data: Bytes::from(payload),
            signature: Vec::new(),
        };
        self.pipeline.submit(consensus_message);

        Ok(())
    }

    async fn handle_peer_disconnected(&mut self, peer_id: Uuid) -> Result<(), NetworkError> {
        info!("[QuantumNetworkHandler] Peer отключился: {}", peer_id);

        // Отправляем сервисное сообщение в pipeline
        let consensus_message = ConsensusMessage {
            sender_id: peer_id.to_string(),
            state_id: format!("peer_disconnected_{}", peer_id),
            raw_data: Bytes::new(),
            signature: Vec::new(),
        };
        self.pipeline.submit(consensus_message);

        Ok(())
    }
}

impl QuantumNetworkHandler {
    async fn handle_transaction_message(&self, from: Uuid, transaction: Transaction) -> Result<(), NetworkError> {
        debug!("Обработка транзакции от узла {}: {}", from, transaction.id);
        
        // Создаем квантовое состояние из транзакции
        let quantum_state = QuantumState::new(
            transaction.id.to_string(),
            transaction.amount as f64,
            0.0,
            "transaction".to_string(),
            vec![crate::quantum::field::StateVector {
                value: num_complex::Complex::new(transaction.amount as f64, 0.0),
                probability: 1.0,
            }],
        );

        // Обрабатываем через консенсус: подписываем полезную нагрузку
        let raw = bincode::serialize(&transaction).unwrap_or_default();
        let signature = {
            let consensus = self.consensus_node.lock();
            let sig = consensus.sign_message(&raw);
            sig.to_bytes().to_vec()
        };
        let consensus_message = ConsensusMessage {
            sender_id: from.to_string(),
            state_id: transaction.id.to_string(),
            raw_data: Bytes::from(raw),
            signature,
        };

        // Передаём в конвейер для дальнейшей обработки
        self.pipeline.submit(consensus_message);

        Ok(())
    }

    async fn handle_consensus_message(&self, from: Uuid, consensus_msg: ConsensusMessage) -> Result<(), NetworkError> {
        debug!("Обработка консенсусного сообщения от узла {}: {}", from, consensus_msg.state_id);
        // Передаём в конвейер для дальнейшей обработки
        self.pipeline.submit(consensus_msg);

        Ok(())
    }

    async fn handle_smart_contract_message(&self, from: Uuid, contract: SmartContract) -> Result<(), NetworkError> {
        debug!("Обработка смарт-контракта от узла {}: {}", from, contract.id);
        
        // Создаем квантовое состояние из контракта
        let quantum_state = QuantumState::new(
            contract.id.to_string(),
            1.0,
            0.0,
            "smart_contract".to_string(),
            vec![crate::quantum::field::StateVector {
                value: num_complex::Complex::new(1.0, 0.0),
                probability: 1.0,
            }],
        );

        Ok(())
    }

    async fn handle_shard_event_message(&self, from: Uuid, event: ShardEvent) -> Result<(), NetworkError> {
        debug!("Обработка события шарда от узла {}: {:?}", from, event);
        Ok(())
    }

    async fn handle_error_event_message(&self, from: Uuid, error_ctx: ErrorContext) -> Result<(), NetworkError> {
        debug!("Обработка ошибки от узла {}: {}", from, error_ctx.description);
        Ok(())
    }

    async fn handle_semantic_action_message(&self, from: Uuid, action: SemanticAction) -> Result<(), NetworkError> {
        debug!("Обработка семантического действия от узла {}: {:?}", from, action);
        Ok(())
    }
} 