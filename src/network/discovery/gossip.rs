//! Gossip protocol implementation with fan-out 3-7
//! Efficient message propagation with exponential spread

use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use uuid::Uuid;
use log::{debug, info, warn};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant, SystemTime};
use serde::{Serialize, Deserialize};
use rand::Rng;
use dashmap::{DashMap, DashSet};
use parking_lot::Mutex;

use crate::network::error::NetworkError;
use crate::network::types::{NetworkEvent, PeerInfo};
use crate::network::peer::PeerManager;

/// Gossip message with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GossipMessage {
    /// Unique message ID
    pub id: Uuid,
    /// Sender peer ID
    pub sender_id: Uuid,
    /// Message type
    pub message_type: GossipMessageType,
    /// Message payload
    pub payload: Vec<u8>,
    /// Timestamp when message was created
    pub timestamp: u64,
    /// TTL (Time To Live) - how many hops message can travel
    pub ttl: u8,
    /// Current hop count
    pub hop_count: u8,
    /// Message signature for authenticity
    pub signature: Option<Vec<u8>>,
    /// Gossip topic
    pub topic: String,
}

/// Types of gossip messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GossipMessageType {
    /// Transaction broadcast
    Transaction,
    /// Block announcement
    Block,
    /// State update
    StateUpdate,
    /// Peer discovery
    PeerDiscovery,
    /// Heartbeat
    Heartbeat,
    /// Custom message
    Custom(String),
}

/// Gossip protocol configuration
#[derive(Debug, Clone)]
pub struct GossipConfig {
    /// Fan-out factor (how many peers to forward to)
    pub fan_out: u8,
    /// Message TTL (max hops)
    pub max_ttl: u8,
    /// Heartbeat interval
    pub heartbeat_interval: Duration,
    /// Message cache TTL
    pub message_cache_ttl: Duration,
    /// Maximum message size
    pub max_message_size: usize,
    /// Enable message deduplication
    pub enable_dedup: bool,
    /// Enable message validation
    pub enable_validation: bool,
}

impl Default for GossipConfig {
    fn default() -> Self {
        Self {
            fan_out: 5, // Fan-out 5 (between 3-7 as recommended)
            max_ttl: 10,
            heartbeat_interval: Duration::from_secs(30),
            message_cache_ttl: Duration::from_secs(300), // 5 minutes
            max_message_size: 1024 * 1024, // 1MB
            enable_dedup: true,
            enable_validation: true,
        }
    }
}

/// Gossip protocol service
#[derive(Debug)]
pub struct GossipService {
    /// Current node ID
    node_id: Uuid,
    /// Configuration
    config: GossipConfig,
    /// Event sender
    event_sender: mpsc::Sender<NetworkEvent>,
    /// Peer manager
    peer_manager: Arc<RwLock<PeerManager>>,
    /// Message cache for deduplication (lock-free reads)
    message_cache: Arc<DashMap<Uuid, CachedMessage>>,
    /// Topic subscriptions (lock-free reads)
    subscriptions: Arc<DashSet<String>>,
    /// Gossip statistics
    stats: Arc<Mutex<GossipStats>>,
    /// Running flag
    running: bool,
}

/// Cached message for deduplication
#[derive(Debug, Clone)]
struct CachedMessage {
    /// Message
    message: GossipMessage,
    /// When message was cached
    cached_at: Instant,
    /// Peers that have seen this message
    seen_by: HashSet<Uuid>,
}

/// Gossip protocol statistics
#[derive(Debug, Default, Clone)]
struct GossipStats {
    /// Total messages received
    messages_received: u64,
    /// Total messages forwarded
    messages_forwarded: u64,
    /// Total messages dropped
    messages_dropped: u64,
    /// Cache hit rate
    cache_hit_rate: f32,
    /// Average propagation time
    avg_propagation_time_ms: f32,
    /// Active topics
    active_topics: usize,
}

impl GossipService {
    /// Creates new gossip service
    pub fn new(
        node_id: Uuid,
        config: GossipConfig,
        event_sender: mpsc::Sender<NetworkEvent>,
        peer_manager: Arc<RwLock<PeerManager>>,
    ) -> Self {
        Self {
            node_id,
            config,
            event_sender,
            peer_manager,
            message_cache: Arc::new(DashMap::new()),
            subscriptions: Arc::new(DashSet::new()),
            stats: Arc::new(Mutex::new(GossipStats::default())),
            running: false,
        }
    }
    
    /// Starts gossip service
    pub async fn start(&mut self) -> Result<(), NetworkError> {
        if self.running {
            return Err(NetworkError::ServiceAlreadyStarted);
        }
        
        info!("🚀 Starting gossip service with fan-out {}", self.config.fan_out);
        
        // Start message cache cleanup
        self.start_cache_cleanup().await?;
        
        // Start heartbeat
        self.start_heartbeat().await?;
        
        // Start statistics reporting
        self.start_stats_reporting().await?;
        
        self.running = true;
        info!("✅ Gossip service started successfully");
        
        Ok(())
    }
    
    /// Stops gossip service
    pub async fn stop(&mut self) -> Result<(), NetworkError> {
        if !self.running {
            return Ok(());
        }
        
        info!("🛑 Stopping gossip service...");
        
        // Clear message cache
        self.message_cache.clear();
        
        // Clear subscriptions
        self.subscriptions.clear();
        
        self.running = false;
        info!("✅ Gossip service stopped");
        
        Ok(())
    }
    
    /// Publishes message to gossip network
    pub async fn publish(&self, message_type: GossipMessageType, payload: Vec<u8>, topic: String) -> Result<Uuid, NetworkError> {
        // Validate message size
        if payload.len() > self.config.max_message_size {
            return Err(NetworkError::Internal(format!("Message too large: {} bytes", payload.len())));
        }
        
        // Create gossip message
        let message = GossipMessage {
            id: Uuid::new_v4(),
            sender_id: self.node_id,
            message_type,
            payload,
            timestamp: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            ttl: self.config.max_ttl,
            hop_count: 0,
            signature: None, // TODO: Add signature
            topic,
        };
        
        // Cache message
        self.cache_message(&message).await;
        
        // Forward to peers
        self.forward_message(&message).await?;
        
        info!("📤 Published gossip message {} to topic {}", message.id, message.topic);
        Ok(message.id)
    }
    
    /// Handles incoming gossip message
    pub async fn handle_message(&self, message: GossipMessage) -> Result<(), NetworkError> {
        // Check if we've seen this message
        if self.config.enable_dedup && self.is_message_seen(&message.id).await {
            debug!("🔄 Duplicate message {} ignored", message.id);
            return Ok(());
        }
        
        // Validate message
        if self.config.enable_validation && !self.validate_message(&message).await? {
            warn!("⚠️ Invalid message {} dropped", message.id);
            return Ok(());
        }
        
        // Check TTL
        if message.hop_count >= message.ttl {
            debug!("⏰ Message {} TTL expired", message.id);
            return Ok(());
        }
        
        // Cache message
        self.cache_message(&message).await;
        
        // Update statistics
        {
            let mut stats = self.stats.lock();
            stats.messages_received += 1;
        }
        
        // Process message based on type
        match message.message_type {
            GossipMessageType::Transaction => {
                self.handle_transaction_message(&message).await?;
            }
            GossipMessageType::Block => {
                self.handle_block_message(&message).await?;
            }
            GossipMessageType::StateUpdate => {
                self.handle_state_update_message(&message).await?;
            }
            GossipMessageType::PeerDiscovery => {
                self.handle_peer_discovery_message(&message).await?;
            }
            GossipMessageType::Heartbeat => {
                self.handle_heartbeat_message(&message).await?;
            }
            GossipMessageType::Custom(ref custom_type) => {
                self.handle_custom_message(&message, custom_type).await?;
            }
        }
        
        // Forward message to other peers (fan-out)
        if message.hop_count < message.ttl {
            self.forward_message(&message).await?;
        }
        
        Ok(())
    }
    
    /// Forwards message to peers using fan-out strategy
    async fn forward_message(&self, message: &GossipMessage) -> Result<(), NetworkError> {
        let peers = self.peer_manager.read().await.get_all_peers();
        
        if peers.is_empty() {
            debug!("📤 No peers to forward message to");
            return Ok(());
        }
        
        // Select random peers for fan-out
        let selected_peers = self.select_peers_for_fanout(&peers, self.config.fan_out);
        
        // Create forwarded message
        let mut forwarded_message = message.clone();
        forwarded_message.hop_count += 1;
        
        // Forward to selected peers
        for peer in &selected_peers {
            if let Err(e) = self.send_message_to_peer(peer.id, &forwarded_message).await {
                warn!("⚠️ Failed to forward message to peer {}: {}", peer.id, e);
            }
        }
        
        // Update statistics
        {
            let mut stats = self.stats.lock();
            stats.messages_forwarded += 1;
        }
        
        debug!("📤 Forwarded message {} to {} peers", message.id, selected_peers.len());
        Ok(())
    }
    
    /// Selects random peers for fan-out
    fn select_peers_for_fanout(&self, peers: &[PeerInfo], fan_out: u8) -> Vec<PeerInfo> {
        if peers.len() <= fan_out as usize {
            return peers.to_vec();
        }
        
        let mut rng = rand::thread_rng();
        let mut selected = Vec::new();
        let mut available_indices: Vec<usize> = (0..peers.len()).collect();
        
        for _ in 0..fan_out {
            if available_indices.is_empty() {
                break;
            }
            
            let random_index = rng.gen_range(0..available_indices.len());
            let peer_index = available_indices.remove(random_index);
            selected.push(peers[peer_index].clone());
        }
        
        selected
    }
    
    /// Sends message to specific peer
    async fn send_message_to_peer(&self, peer_id: Uuid, message: &GossipMessage) -> Result<(), NetworkError> {
        // TODO: Implement actual sending through transport layer
        // For now, just log
        debug!("📤 Sending gossip message {} to peer {}", message.id, peer_id);
        Ok(())
    }
    
    /// Caches message for deduplication
    async fn cache_message(&self, message: &GossipMessage) {
        let cached_message = CachedMessage {
            message: message.clone(),
            cached_at: Instant::now(),
            seen_by: HashSet::new(),
        };
        self.message_cache.insert(message.id, cached_message);
    }
    
    /// Checks if message has been seen
    async fn is_message_seen(&self, message_id: &Uuid) -> bool {
        self.message_cache.contains_key(message_id)
    }
    
    /// Validates message
    async fn validate_message(&self, message: &GossipMessage) -> Result<bool, NetworkError> {
        // Check message size
        if message.payload.len() > self.config.max_message_size {
            return Ok(false);
        }
        
        // Check timestamp (reject messages older than 1 hour)
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        if now.saturating_sub(message.timestamp) > 3600 {
            return Ok(false);
        }
        
        // TODO: Add signature validation
        // TODO: Add content validation based on message type
        
        Ok(true)
    }
    
    /// Handles transaction message
    async fn handle_transaction_message(&self, message: &GossipMessage) -> Result<(), NetworkError> {
        info!("📨 Processing transaction message: {}", message.id);
        
        // TODO: Process transaction
        // For now, just log
        Ok(())
    }
    
    /// Handles block message
    async fn handle_block_message(&self, message: &GossipMessage) -> Result<(), NetworkError> {
        info!("📨 Processing block message: {}", message.id);
        
        // TODO: Process block
        // For now, just log
        Ok(())
    }
    
    /// Handles state update message
    async fn handle_state_update_message(&self, message: &GossipMessage) -> Result<(), NetworkError> {
        info!("📨 Processing state update message: {}", message.id);
        
        // TODO: Process state update
        // For now, just log
        Ok(())
    }
    
    /// Handles peer discovery message
    async fn handle_peer_discovery_message(&self, message: &GossipMessage) -> Result<(), NetworkError> {
        info!("📨 Processing peer discovery message: {}", message.id);
        
        // TODO: Process peer discovery
        // For now, just log
        Ok(())
    }
    
    /// Handles heartbeat message
    async fn handle_heartbeat_message(&self, message: &GossipMessage) -> Result<(), NetworkError> {
        debug!("💓 Processing heartbeat message: {}", message.id);
        
        // TODO: Process heartbeat
        // For now, just log
        Ok(())
    }
    
    /// Handles custom message
    async fn handle_custom_message(&self, message: &GossipMessage, custom_type: &str) -> Result<(), NetworkError> {
        info!("📨 Processing custom message {} of type: {}", message.id, custom_type);
        
        // TODO: Process custom message
        // For now, just log
        Ok(())
    }
    
    /// Starts message cache cleanup
    async fn start_cache_cleanup(&self) -> Result<(), NetworkError> {
        let message_cache = self.message_cache.clone();
        let config = self.config.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            
            loop {
                interval.tick().await;
                
                let now = Instant::now();
                message_cache.retain(|_, cached| now.duration_since(cached.cached_at) < config.message_cache_ttl);
                debug!("🧹 Cache cleanup: {} messages remaining", message_cache.len());
            }
        });
        
        Ok(())
    }
    
    /// Starts heartbeat
    async fn start_heartbeat(&self) -> Result<(), NetworkError> {
        let node_id = self.node_id;
        let config = self.config.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(config.heartbeat_interval);
            
            loop {
                interval.tick().await;
                
                // TODO: Send heartbeat message
                debug!("💓 Sending heartbeat from node {}", node_id);
            }
        });
        
        Ok(())
    }
    
    /// Starts statistics reporting
    async fn start_stats_reporting(&self) -> Result<(), NetworkError> {
        let stats = self.stats.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(300)); // 5 minutes
            
            loop {
                interval.tick().await;
                
                let stats_guard = stats.lock();
                info!("📊 Gossip Stats: {} received, {} forwarded, {} dropped, {:.2}% cache hit", 
                    stats_guard.messages_received,
                    stats_guard.messages_forwarded,
                    stats_guard.messages_dropped,
                    stats_guard.cache_hit_rate * 100.0
                );
            }
        });
        
        Ok(())
    }
    
    /// Subscribes to a topic
    pub async fn subscribe(&self, topic: String) -> Result<(), NetworkError> {
        self.subscriptions.insert(topic.clone());
        info!("📡 Subscribed to gossip topic: {}", topic);
        Ok(())
    }
    
    /// Unsubscribes from a topic
    pub async fn unsubscribe(&self, topic: &str) -> Result<(), NetworkError> {
        self.subscriptions.remove(topic);
        info!("📡 Unsubscribed from gossip topic: {}", topic);
        Ok(())
    }
    
    /// Gets gossip statistics
    pub async fn get_stats(&self) -> GossipStats { self.stats.lock().clone() }
    
    /// Gets active topics
    pub async fn get_active_topics(&self) -> Vec<String> { self.subscriptions.iter().map(|s| s.clone()).collect() }
}
