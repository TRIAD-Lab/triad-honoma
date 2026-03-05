//! Sharding module for horizontal scaling
//! Features: shard management, cross-shard transactions, load balancing

use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, RwLock};
use uuid::Uuid;
use log::{debug, info};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};
use serde::{Serialize, Deserialize};

use crate::network::error::NetworkError;
use crate::network::types::NetworkEvent;
use crate::network::peer::PeerManager;

/// Shard identifier
pub type ShardId = u32;
/// Transaction identifier
pub type TransactionId = Uuid;

/// Shard configuration
#[derive(Debug, Clone)]
pub struct ShardConfig {
    /// Total number of shards
    pub total_shards: u32,
    /// Shards per node
    pub shards_per_node: u32,
    /// Shard replication factor
    pub replication_factor: u32,
    /// Cross-shard transaction timeout
    pub cross_shard_timeout: Duration,
    /// Enable dynamic sharding
    pub dynamic_sharding: bool,
    /// Shard rebalancing threshold
    pub rebalancing_threshold: f32,
}

impl Default for ShardConfig {
    fn default() -> Self {
        Self {
            total_shards: 16,
            shards_per_node: 2,
            replication_factor: 3,
            cross_shard_timeout: Duration::from_secs(30),
            dynamic_sharding: true,
            rebalancing_threshold: 0.2, // 20% imbalance triggers rebalancing
        }
    }
}

/// Shard information
#[derive(Debug, Clone)]
pub struct ShardInfo {
    /// Shard ID
    pub shard_id: ShardId,
    /// Shard name
    pub name: String,
    /// Assigned nodes
    pub nodes: HashSet<Uuid>,
    /// Primary node
    pub primary_node: Option<Uuid>,
    /// Shard status
    pub status: ShardStatus,
    /// Transaction count
    pub transaction_count: u64,
    /// Last update time
    pub last_update: Instant,
}

/// Shard status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShardStatus {
    /// Shard is active and processing transactions
    Active,
    /// Shard is syncing state
    Syncing,
    /// Shard is rebalancing
    Rebalancing,
    /// Shard is offline
    Offline,
}

/// Cross-shard transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossShardTransaction {
    /// Transaction ID
    pub id: TransactionId,
    /// Source shard
    pub source_shard: ShardId,
    /// Target shard
    pub target_shard: ShardId,
    /// Transaction data
    pub data: Vec<u8>,
    /// Transaction type
    pub transaction_type: CrossShardTransactionType,
    /// Dependencies
    pub dependencies: Vec<TransactionId>,
    /// Status
    pub status: CrossShardTransactionStatus,
    /// Timestamp
    pub timestamp: u64,
}

/// Cross-shard transaction types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CrossShardTransactionType {
    /// Asset transfer between shards
    AssetTransfer,
    /// Contract call across shards
    ContractCall,
    /// State synchronization
    StateSync,
    /// Custom transaction
    Custom(String),
}

/// Cross-shard transaction status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CrossShardTransactionStatus {
    /// Transaction submitted
    Submitted,
    /// Processing in source shard
    ProcessingSource,
    /// Processing in target shard
    ProcessingTarget,
    /// Completed successfully
    Completed,
    /// Failed
    Failed(String),
}

/// Sharding service
#[derive(Debug)]
pub struct ShardingService {
    /// Configuration
    config: ShardConfig,
    /// Local node ID
    local_node_id: Uuid,
    /// Peer manager
    peer_manager: Arc<RwLock<PeerManager>>,
    /// Event sender
    event_sender: mpsc::Sender<NetworkEvent>,
    /// Shard assignments
    shard_assignments: Arc<RwLock<HashMap<ShardId, ShardInfo>>>,
    /// Node shard assignments
    node_shards: Arc<RwLock<HashMap<Uuid, HashSet<ShardId>>>>,
    /// Cross-shard transactions
    cross_shard_transactions: Arc<RwLock<HashMap<TransactionId, CrossShardTransaction>>>,
    /// Shard statistics
    stats: Arc<Mutex<ShardStats>>,
    /// Running flag
    running: bool,
}

/// Shard statistics
#[derive(Debug, Default, Clone)]
struct ShardStats {
    /// Total shards
    total_shards: u32,
    /// Active shards
    active_shards: u32,
    /// Total cross-shard transactions
    total_cross_shard_tx: u64,
    /// Successful cross-shard transactions
    successful_cross_shard_tx: u64,
    /// Average transaction processing time
    avg_tx_processing_time_ms: f32,
    /// Shard load distribution
    shard_load_distribution: HashMap<ShardId, f32>,
}

impl ShardingService {
    /// Creates new sharding service
    pub fn new(
        config: ShardConfig,
        local_node_id: Uuid,
        peer_manager: Arc<RwLock<PeerManager>>,
        event_sender: mpsc::Sender<NetworkEvent>,
    ) -> Self {
        Self {
            config,
            local_node_id,
            peer_manager,
            event_sender,
            shard_assignments: Arc::new(RwLock::new(HashMap::new())),
            node_shards: Arc::new(RwLock::new(HashMap::new())),
            cross_shard_transactions: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(Mutex::new(ShardStats::default())),
            running: false,
        }
    }
    
    /// Starts sharding service
    pub async fn start(&mut self) -> Result<(), NetworkError> {
        if self.running {
            return Err(NetworkError::ServiceAlreadyStarted);
        }
        
        info!("🚀 Starting sharding service with {} shards", self.config.total_shards);
        
        // Initialize shards
        self.initialize_shards().await;
        
        // Assign shards to nodes
        self.assign_shards_to_nodes().await?;
        
        // Start shard monitoring
        self.start_shard_monitoring().await?;
        
        // Start cross-shard transaction processing
        self.start_cross_shard_processing().await?;
        
        self.running = true;
        info!("✅ Sharding service started successfully");
        
        Ok(())
    }
    
    /// Stops sharding service
    pub async fn stop(&mut self) -> Result<(), NetworkError> {
        if !self.running {
            return Ok(());
        }
        
        info!("🛑 Stopping sharding service...");
        
        self.running = false;
        Ok(())
    }
    
    /// Initializes shards
    async fn initialize_shards(&self) {
        let mut shards = self.shard_assignments.write().await;
        
        for shard_id in 0..self.config.total_shards {
            let shard_info = ShardInfo {
                shard_id,
                name: format!("shard-{}", shard_id),
                nodes: HashSet::new(),
                primary_node: None,
                status: ShardStatus::Offline,
                transaction_count: 0,
                last_update: Instant::now(),
            };
            
            shards.insert(shard_id, shard_info);
        }
        
        info!("📦 Initialized {} shards", self.config.total_shards);
    }
    
    /// Assigns shards to nodes
    async fn assign_shards_to_nodes(&self) -> Result<(), NetworkError> {
        let peers = self.peer_manager.read().await.get_all_peers();
        let mut shards = self.shard_assignments.write().await;
        let mut node_shards = self.node_shards.write().await;
        
        // Calculate shards per node
        let shards_per_node = if peers.is_empty() {
            self.config.shards_per_node
        } else {
            (self.config.total_shards / peers.len() as u32).max(1)
        };
        
        // Assign shards to nodes
        let mut shard_index = 0;
        for peer in &peers {
            let mut peer_shards = HashSet::new();
            
            for _ in 0..shards_per_node {
                if shard_index >= self.config.total_shards {
                    break;
                }
                
                let shard_id = shard_index as ShardId;
                if let Some(shard) = shards.get_mut(&shard_id) {
                    shard.nodes.insert(peer.id);
                    peer_shards.insert(shard_id);
                    
                    // Set primary node for this shard
                    if shard.primary_node.is_none() {
                        shard.primary_node = Some(peer.id);
                        shard.status = ShardStatus::Active;
                    }
                }
                
                shard_index += 1;
            }
            
            node_shards.insert(peer.id, peer_shards);
        }
        
        // Assign remaining shards to local node if needed
        if shard_index < self.config.total_shards {
            let mut local_shards = HashSet::new();
            
            for shard_id in shard_index..self.config.total_shards {
                if let Some(shard) = shards.get_mut(&(shard_id as ShardId)) {
                    shard.nodes.insert(self.local_node_id);
                    local_shards.insert(shard_id as ShardId);
                    
                    if shard.primary_node.is_none() {
                        shard.primary_node = Some(self.local_node_id);
                        shard.status = ShardStatus::Active;
                    }
                }
            }
            
            node_shards.insert(self.local_node_id, local_shards);
        }
        
        info!("🔗 Assigned shards to {} nodes", node_shards.len());
        Ok(())
    }
    
    /// Starts shard monitoring
    async fn start_shard_monitoring(&self) -> Result<(), NetworkError> {
        let shards = self.shard_assignments.clone();
        let stats = self.stats.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            
            loop {
                interval.tick().await;
                
                let shards_guard = shards.read().await;
                let mut stats_guard = stats.lock().await;
                
                // Update statistics
                stats_guard.total_shards = shards_guard.len() as u32;
                stats_guard.active_shards = shards_guard.values()
                    .filter(|s| s.status == ShardStatus::Active)
                    .count() as u32;
                
                // Calculate load distribution
                stats_guard.shard_load_distribution.clear();
                for (shard_id, shard) in shards_guard.iter() {
                    let load = shard.transaction_count as f32;
                    stats_guard.shard_load_distribution.insert(*shard_id, load);
                }
                
                debug!("📊 Shard monitoring: {} active shards", stats_guard.active_shards);
            }
        });
        
        Ok(())
    }
    
    /// Starts cross-shard transaction processing
    async fn start_cross_shard_processing(&self) -> Result<(), NetworkError> {
        let cross_shard_tx = self.cross_shard_transactions.clone();
        let stats = self.stats.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(10));
            
            loop {
                interval.tick().await;
                
                let mut tx_guard = cross_shard_tx.write().await;
                let mut stats_guard = stats.lock().await;
                
                // Process pending transactions
                let mut to_remove = Vec::new();
                
                for (tx_id, transaction) in tx_guard.iter_mut() {
                    match transaction.status {
                        CrossShardTransactionStatus::Submitted => {
                            // Start processing in source shard
                            transaction.status = CrossShardTransactionStatus::ProcessingSource;
                            debug!("🔄 Processing cross-shard transaction {} in source shard {}", 
                                tx_id, transaction.source_shard);
                        }
                        CrossShardTransactionStatus::ProcessingSource => {
                            // Simulate source shard processing completion
                            transaction.status = CrossShardTransactionStatus::ProcessingTarget;
                            debug!("✅ Source shard {} completed for transaction {}", 
                                transaction.source_shard, tx_id);
                        }
                        CrossShardTransactionStatus::ProcessingTarget => {
                            // Simulate target shard processing completion
                            transaction.status = CrossShardTransactionStatus::Completed;
                            stats_guard.successful_cross_shard_tx += 1;
                            debug!("✅ Target shard {} completed for transaction {}", 
                                transaction.target_shard, tx_id);
                            
                            to_remove.push(*tx_id);
                        }
                        CrossShardTransactionStatus::Completed => {
                            to_remove.push(*tx_id);
                        }
                        CrossShardTransactionStatus::Failed(_) => {
                            to_remove.push(*tx_id);
                        }
                    }
                }
                
                // Remove completed/failed transactions
                for tx_id in to_remove {
                    tx_guard.remove(&tx_id);
                }
                
                stats_guard.total_cross_shard_tx = tx_guard.len() as u64;
            }
        });
        
        Ok(())
    }
    
    /// Submits a cross-shard transaction
    pub async fn submit_cross_shard_transaction(
        &self,
        source_shard: ShardId,
        target_shard: ShardId,
        data: Vec<u8>,
        transaction_type: CrossShardTransactionType,
    ) -> Result<TransactionId, NetworkError> {
        if !self.running {
            return Err(NetworkError::ServiceAlreadyStarted);
        }
        
        // Validate shards
        if source_shard >= self.config.total_shards || target_shard >= self.config.total_shards {
            return Err(NetworkError::Internal("Invalid shard ID".to_string()));
        }
        
        // Create transaction
        let transaction = CrossShardTransaction {
            id: Uuid::new_v4(),
            source_shard,
            target_shard,
            data,
            transaction_type,
            dependencies: Vec::new(),
            status: CrossShardTransactionStatus::Submitted,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        };
        
        // Store transaction
        {
            let mut tx_guard = self.cross_shard_transactions.write().await;
            tx_guard.insert(transaction.id, transaction.clone());
        }
        
        // Update statistics
        {
            let mut stats = self.stats.lock().await;
            stats.total_cross_shard_tx += 1;
        }
        
        info!("📤 Submitted cross-shard transaction {}: {} -> {}", 
            transaction.id, source_shard, target_shard);
        
        Ok(transaction.id)
    }
    
    /// Gets cross-shard transaction status
    pub async fn get_transaction_status(&self, tx_id: TransactionId) -> Option<CrossShardTransactionStatus> {
        let tx_guard = self.cross_shard_transactions.read().await;
        tx_guard.get(&tx_id).map(|tx| tx.status.clone())
    }
    
    /// Gets shard information
    pub async fn get_shard_info(&self, shard_id: ShardId) -> Option<ShardInfo> {
        let shards = self.shard_assignments.read().await;
        shards.get(&shard_id).cloned()
    }
    
    /// Gets all shard information
    pub async fn get_all_shards(&self) -> Vec<ShardInfo> {
        let shards = self.shard_assignments.read().await;
        shards.values().cloned().collect()
    }
    
    /// Gets shards assigned to a node
    pub async fn get_node_shards(&self, node_id: Uuid) -> Vec<ShardId> {
        let node_shards = self.node_shards.read().await;
        node_shards.get(&node_id)
            .map(|shards| shards.iter().cloned().collect())
            .unwrap_or_default()
    }
    
    /// Gets local node shards
    pub async fn get_local_shards(&self) -> Vec<ShardId> {
        self.get_node_shards(self.local_node_id).await
    }
    
    /// Checks if a node is responsible for a shard
    pub async fn is_node_responsible_for_shard(&self, node_id: Uuid, shard_id: ShardId) -> bool {
        let node_shards = self.node_shards.read().await;
        node_shards.get(&node_id)
            .map(|shards| shards.contains(&shard_id))
            .unwrap_or(false)
    }
    
    /// Checks if local node is responsible for a shard
    pub async fn is_local_node_responsible_for_shard(&self, shard_id: ShardId) -> bool {
        self.is_node_responsible_for_shard(self.local_node_id, shard_id).await
    }
    
    /// Gets shard statistics
    pub async fn get_stats(&self) -> ShardStats {
        self.stats.lock().await.clone()
    }
    
    /// Triggers shard rebalancing
    pub async fn trigger_rebalancing(&self) -> Result<(), NetworkError> {
        if !self.config.dynamic_sharding {
            return Err(NetworkError::Internal("Dynamic sharding is disabled".to_string()));
        }
        
        info!("🔄 Triggering shard rebalancing...");
        
        // TODO: Implement actual rebalancing logic
        // This would involve:
        // 1. Analyzing current load distribution
        // 2. Identifying overloaded/underloaded shards
        // 3. Moving shards between nodes
        // 4. Updating assignments
        
        Ok(())
    }
    
    /// Adds a node to a shard
    pub async fn add_node_to_shard(&self, node_id: Uuid, shard_id: ShardId) -> Result<(), NetworkError> {
        // Update shard assignments
        {
            let mut shards = self.shard_assignments.write().await;
            if let Some(shard) = shards.get_mut(&shard_id) {
                shard.nodes.insert(node_id);
                shard.last_update = Instant::now();
            }
        }
        
        // Update node shards
        {
            let mut node_shards = self.node_shards.write().await;
            let node_shards_set = node_shards.entry(node_id).or_insert_with(HashSet::new);
            node_shards_set.insert(shard_id);
        }
        
        info!("➕ Added node {} to shard {}", node_id, shard_id);
        Ok(())
    }
    
    /// Removes a node from a shard
    pub async fn remove_node_from_shard(&self, node_id: Uuid, shard_id: ShardId) -> Result<(), NetworkError> {
        // Update shard assignments
        {
            let mut shards = self.shard_assignments.write().await;
            if let Some(shard) = shards.get_mut(&shard_id) {
                shard.nodes.remove(&node_id);
                shard.last_update = Instant::now();
                
                // If this was the primary node, reassign
                if shard.primary_node == Some(node_id) {
                    shard.primary_node = shard.nodes.iter().next().cloned();
                    if shard.primary_node.is_none() {
                        shard.status = ShardStatus::Offline;
                    }
                }
            }
        }
        
        // Update node shards
        {
            let mut node_shards = self.node_shards.write().await;
            if let Some(node_shards_set) = node_shards.get_mut(&node_id) {
                node_shards_set.remove(&shard_id);
            }
        }
        
        info!("➖ Removed node {} from shard {}", node_id, shard_id);
        Ok(())
    }
    
    /// Gets shard load information
    pub async fn get_shard_load(&self) -> HashMap<ShardId, f32> {
        let shards = self.shard_assignments.read().await;
        shards.iter()
            .map(|(shard_id, shard)| (*shard_id, shard.transaction_count as f32))
            .collect()
    }
    
    /// Gets cross-shard transaction count
    pub async fn get_cross_shard_transaction_count(&self) -> usize {
        let tx_guard = self.cross_shard_transactions.read().await;
        tx_guard.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_sharding_service() {
        let config = ShardConfig::default();
        let local_node_id = Uuid::new_v4();
        let peer_manager = Arc::new(RwLock::new(PeerManager::new()));
        let (event_sender, _event_receiver) = mpsc::channel(100);
        
        let mut service = ShardingService::new(
            config.clone(),
            local_node_id,
            peer_manager,
            event_sender,
        );
        
        service.start().await.unwrap();
        
        // Test shard assignment
        let local_shards = service.get_local_shards().await;
        assert!(!local_shards.is_empty());
        
        // Test cross-shard transaction
        let tx_id = service.submit_cross_shard_transaction(
            0,
            1,
            b"test data".to_vec(),
            CrossShardTransactionType::AssetTransfer,
        ).await.unwrap();
        
        // Test transaction status
        let status = service.get_transaction_status(tx_id).await;
        assert!(status.is_some());
        
        service.stop().await.unwrap();
    }
}


