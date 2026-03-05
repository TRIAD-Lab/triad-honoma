use crate::pipeline::{Pipeline, PipelineConfig};
use async_trait::async_trait;
use uuid::Uuid;
use log::{info, debug, error, warn};
use std::sync::Arc;
use parking_lot::Mutex;

use crate::network::types::{Message, PeerInfo};
use crate::network::error::NetworkError;
use crate::network::handler::MessageHandler;
use crate::core::consensus::{ConsensusNode, ConsensusMessage};
use crate::core::consensus::batch_context::derive_block_salt;
use crate::core::consensus::batch_context::BatchContext;
use crate::core::field::CoreWave;
use crate::tvm::state::StateView;
use crate::transaction::{Transaction, SmartContract};
use crate::sharding::ShardEvent;
use crate::error_analysis::{ErrorContext, regenerate_wavefield};
use crate::semantic::SemanticAction;
use bytes::Bytes;
use crate::crypto::bls::BlsSignatureExt;

/// Квантовый обработчик сетевых сообщений
/// Интегрирует сетевой слой с квантовым консенсусом
#[derive(Clone)]
pub struct CoreNetworkHandler {
    node_id: Uuid,
    consensus_node: Arc<Mutex<ConsensusNode>>,
    state: Arc<StateView>,
    Core_waves: Arc<Mutex<Vec<CoreWave>>>,
    message_cache: Arc<Mutex<std::collections::HashMap<Uuid, Message>>>,
    interference_events: Arc<Mutex<Vec<NetworkInterferenceEvent>>>,
    pipeline: Arc<Pipeline>,
    // Isolation state: if true, this node temporarily ignores external traffic due to >1% resonance deviation
    is_isolated: Arc<Mutex<bool>>, 
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::MessageType;
    use tokio::runtime::Runtime;

    #[test]
    fn network_wave_uses_same_batch_context_as_consensus() {
        let node_id = Uuid::new_v4();
        let consensus = Arc::new(Mutex::new(ConsensusNode::new(123, 5)));
        let batch_ctx = BatchContext {
            merkle_root: [3u8; 32],
            prev_batch_hash: [7u8; 32],
            slot: 42,
            shard_id: 11,
            timestamp: 1_703_000_000,
        };
        consensus.lock().update_batch_context(batch_ctx.clone());

        let state = Arc::new(StateView::new());
        let handler = CoreNetworkHandler::new_with_state(node_id, Arc::clone(&consensus), state);

        let message = Message {
            id: Uuid::new_v4(),
            sender_id: node_id,
            recipient_id: None,
            message_type: MessageType::Error("test".to_string()),
            timestamp: 0,
            payload: Bytes::new(),
        };

        let wave_id = format!("network_wave_{}_{}", node_id, message.id);

        let rt = Runtime::new().expect("runtime");
        let network_wave = rt.block_on(handler.create_Core_wave_from_message(&message));

        let consensus_message = ConsensusMessage {
            sender_id: message.sender_id.to_string(),
            state_id: wave_id,
            raw_data: Bytes::new(),
            signature: Vec::new(),
            public_key: None,
        };
        let consensus_wave = consensus.lock().create_wave_from_message(&consensus_message);

        assert_eq!(network_wave.timestamp, consensus_wave.timestamp, "slot must match BatchContext");
        assert_eq!(network_wave.shard_id, consensus_wave.shard_id, "shard_id must match BatchContext");
        assert_eq!(network_wave.amplitude, consensus_wave.amplitude, "amplitude mismatch implies different block salt");
        assert_eq!(network_wave.phase, consensus_wave.phase, "phase mismatch implies different block salt");
        assert_eq!(network_wave.position, consensus_wave.position, "position mismatch implies different block salt");
    }
}

#[derive(Debug, Clone)]
pub struct NetworkInterferenceEvent {
    pub timestamp: std::time::Instant,
    pub wave_id: std::sync::Arc<str>,
    pub interference_strength: f64,
    pub affected_peers: Vec<Uuid>,
}

impl CoreNetworkHandler {
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
            Core_waves: Arc::new(Mutex::new(Vec::new())),
            message_cache: Arc::new(Mutex::new(std::collections::HashMap::new())),
            interference_events: Arc::new(Mutex::new(Vec::new())),
            pipeline: Arc::new(pipe),
            is_isolated: Arc::new(Mutex::new(false)),
        }
    }

    /// Создает ПОДПИСАННУЮ квантовую волну из сетевого сообщения (использует ключ узла консенсуса)
    async fn create_Core_wave_from_message(&self, message: &Message) -> CoreWave {
        let wave_id = format!("network_wave_{}_{}", self.node_id, message.id);

        let consensus = self.consensus_node.lock();
        let ctx = consensus.batch_context.load();
        let block_salt = derive_block_salt(&ctx, &wave_id);
        let slot = ctx.slot as u32;
        let shard_id = ctx.shard_id;
        drop(consensus);

        CoreWave::from_transaction(
            &wave_id,
            0,
            &block_salt,
            &[],
            slot,
            1000.0,
            shard_id,
        )
    }

    /// Обрабатывает квантовую интерференцию в сети
    async fn process_network_interference(&self, wave: &CoreWave, peers: &[PeerInfo]) {
        let interference_event = NetworkInterferenceEvent {
            timestamp: std::time::Instant::now(),
            wave_id: wave.id.clone(),
            interference_strength: wave.amplitude as f64,
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
        let mut waves = self.Core_waves.lock();
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

        // Собираем подписанные волны и верифицируем батчом (Schnorr)
        let mut items: Vec<(CoreWave, curve25519_dalek::ristretto::RistrettoPoint)> = Vec::with_capacity(messages.len());

        // Берем публичный ключ локального узла (PHASE 2: Schnorr)
        let pk = {
            let consensus = self.consensus_node.lock();
            consensus.keypair.public
        };

        for msg in &messages {
            let wave = self.create_Core_wave_from_message(msg).await;
            items.push((wave, pk));
        }

        // Подготовим ссылки для батч-верификации
        let wave_refs: Vec<(&CoreWave, curve25519_dalek::ristretto::RistrettoPoint)> = items.iter().map(|(w, k)| (w, *k)).collect();

        let mut consensus = self.consensus_node.lock();
        // FIXME: verify_waves_batch and add_signed_wave removed - update to new API
        // For now, just add waves without verification
        for (wave, _k) in items.into_iter() {
            consensus.field.add_wave(wave.id.to_string(), wave);
        }
    }

    /// Получает статистику квантовой сети
    pub async fn get_Core_network_stats(&self) -> CoreNetworkStats {
        let waves = self.Core_waves.lock();
        let events = self.interference_events.lock();
        let cache = self.message_cache.lock();

        CoreNetworkStats {
            active_waves: waves.len(),
            interference_events: events.len(),
            cached_messages: cache.len(),
            total_wave_amplitude: waves.iter().map(|w| w.amplitude as f64).sum(),
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
pub fn build_global_core_handler(
    node_id: Uuid,
    consensus_node: Arc<Mutex<ConsensusNode>>,
) -> Box<dyn crate::network::handler::MessageHandler + Send + Sync> {
    let state = Arc::new(StateView::new());
    let handler = CoreNetworkHandler::new_with_state(node_id, consensus_node, state);
    Box::new(handler)
}

#[derive(Debug, Clone)]
pub struct CoreNetworkStats {
    pub active_waves: usize,
    pub interference_events: usize,
    pub cached_messages: usize,
    pub total_wave_amplitude: f64,
}

#[async_trait]
impl MessageHandler for CoreNetworkHandler {
    async fn handle_message(&mut self, from: Uuid, message: Message) -> Result<(), NetworkError> {
        info!("[CoreNetworkHandler] Received message from {}: type={:?}, id={}", from, message.message_type, message.id);

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
        info!("[CoreNetworkHandler] Новый peer подключен: {} ({})", peer_info.name, peer_info.id);

        // Формируем сервисное консенсус-сообщение и отправляем в pipeline
        // PeerInfo не реализует Serialize, используем переносимый формат
        let spi = crate::network::types::SerializablePeerInfo::from(&peer_info);
        let payload = serde_json::to_vec(&spi).unwrap_or_default();
        let consensus_message = ConsensusMessage {
            sender_id: peer_info.id.to_string(),
            state_id: format!("peer_connected_{}", peer_info.id),
            raw_data: Bytes::from(payload),
            signature: Vec::new(),
            public_key: None, // Service message, no pubkey needed
        };
        self.pipeline.submit(consensus_message);

        Ok(())
    }

    async fn handle_peer_disconnected(&mut self, peer_id: Uuid) -> Result<(), NetworkError> {
        info!("[CoreNetworkHandler] Peer отключился: {}", peer_id);

        // Отправляем сервисное сообщение в pipeline
        let consensus_message = ConsensusMessage {
            sender_id: peer_id.to_string(),
            state_id: format!("peer_disconnected_{}", peer_id),
            raw_data: Bytes::new(),
            signature: Vec::new(),
            public_key: None, // Service message, no pubkey needed
        };
        self.pipeline.submit(consensus_message);

        Ok(())
    }
}

impl CoreNetworkHandler {
    async fn handle_transaction_message(&self, from: Uuid, transaction: Transaction) -> Result<(), NetworkError> {
        debug!("Обработка транзакции от узла {}: {}", from, transaction.id);
        
        // FIXME: Legacy CoreState removed - transaction now goes directly to pipeline
        // The pipeline will create CoreWave via ConsensusNode

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
            public_key: None, // TODO: Should include sender's pubkey in production
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
        
        // FIXME: Legacy CoreState removed
        // Smart contract processing should go through pipeline

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


