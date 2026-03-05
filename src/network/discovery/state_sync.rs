//! Parallel state synchronization module
//! Features: shard-based sync, Merkle-Patricia proofs, parallel downloads

use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, RwLock, Semaphore};
use uuid::Uuid;
use log::{error, info, warn};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use serde::{Serialize, Deserialize};
use tokio::task::JoinHandle;

use crate::network::error::NetworkError;
use crate::network::types::{PeerInfo, NetworkEvent};
use crate::network::peer::PeerManager;

/// Shard identifier
pub type ShardId = u32;
/// State version/height
pub type StateVersion = u64;

/// State synchronization configuration
#[derive(Debug, Clone)]
pub struct StateSyncConfig {
    /// Maximum concurrent shard syncs
    pub max_concurrent_shards: usize,
    /// Maximum concurrent downloads per shard
    pub max_concurrent_downloads: usize,
    /// Chunk size for downloads
    pub chunk_size: usize,
    /// Sync timeout per shard
    pub sync_timeout: Duration,
    /// Enable parallel verification
    pub parallel_verification: bool,
    /// Enable incremental sync
    pub incremental_sync: bool,
}

impl Default for StateSyncConfig {
    fn default() -> Self {
        Self {
            max_concurrent_shards: 4,
            max_concurrent_downloads: 8,
            chunk_size: 1024 * 1024, // 1MB chunks
            sync_timeout: Duration::from_secs(300), // 5 minutes
            parallel_verification: true,
            incremental_sync: true,
        }
    }
}

/// State chunk information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateChunk {
    /// Chunk ID
    pub chunk_id: Uuid,
    /// Shard ID
    pub shard_id: ShardId,
    /// Chunk index
    pub index: usize,
    /// Total chunks in shard
    pub total_chunks: usize,
    /// Chunk data
    pub data: Vec<u8>,
    /// Merkle proof for this chunk
    pub merkle_proof: MerkleProof,
    /// Chunk version
    pub version: StateVersion,
}

/// Merkle proof for state verification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerkleProof {
    /// Root hash
    pub root_hash: Vec<u8>,
    /// Sibling hashes
    pub siblings: Vec<Vec<u8>>,
    /// Path to leaf
    pub path: Vec<bool>,
    /// Leaf hash
    pub leaf_hash: Vec<u8>,
}

/// Shard synchronization status
#[derive(Debug, Clone)]
pub enum ShardSyncStatus {
    /// Not started
    NotStarted,
    /// In progress
    InProgress {
        downloaded_chunks: usize,
        total_chunks: usize,
        start_time: Instant,
    },
    /// Completed successfully
    Completed {
        total_chunks: usize,
        duration: Duration,
        verification_time: Duration,
    },
    /// Failed with error
    Failed {
        error: String,
        downloaded_chunks: usize,
        total_chunks: usize,
    },
}

/// State synchronization service
#[derive(Debug)]
pub struct StateSyncService {
    /// Configuration
    config: StateSyncConfig,
    /// Peer manager
    peer_manager: Arc<RwLock<PeerManager>>,
    /// Event sender
    event_sender: mpsc::Sender<NetworkEvent>,
    /// Shard sync status
    shard_status: Arc<RwLock<HashMap<ShardId, ShardSyncStatus>>>,
    /// Active sync tasks
    active_syncs: Arc<RwLock<HashMap<ShardId, JoinHandle<Result<(), NetworkError>>>>>,
    /// Sync semaphore for limiting concurrent operations
    sync_semaphore: Arc<Semaphore>,
    /// Download semaphore per shard
    download_semaphores: Arc<RwLock<HashMap<ShardId, Arc<Semaphore>>>>,
    /// Sync statistics
    stats: Arc<Mutex<StateSyncStats>>,
    /// Running flag
    running: bool,
}

/// State synchronization statistics
#[derive(Debug, Default, Clone)]
struct StateSyncStats {
    /// Total shards synced
    total_shards_synced: u64,
    /// Total chunks downloaded
    total_chunks_downloaded: u64,
    /// Total bytes downloaded
    total_bytes_downloaded: u64,
    /// Average sync time per shard
    avg_sync_time_ms: f32,
    /// Total verification time
    total_verification_time_ms: u64,
    /// Failed syncs count
    failed_syncs: u64,
}

impl StateSyncService {
    /// Creates new state sync service
    pub fn new(
        config: StateSyncConfig,
        peer_manager: Arc<RwLock<PeerManager>>,
        event_sender: mpsc::Sender<NetworkEvent>,
    ) -> Self {
        Self {
            config: config.clone(),
            peer_manager,
            event_sender,
            shard_status: Arc::new(RwLock::new(HashMap::new())),
            active_syncs: Arc::new(RwLock::new(HashMap::new())),
            sync_semaphore: Arc::new(Semaphore::new(config.max_concurrent_shards)),
            download_semaphores: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(Mutex::new(StateSyncStats::default())),
            running: false,
        }
    }
    
    /// Starts state sync service
    pub async fn start(&mut self) -> Result<(), NetworkError> {
        if self.running {
            return Err(NetworkError::ServiceAlreadyStarted);
        }
        
        info!("🚀 Starting state sync service with {} concurrent shards", 
            self.config.max_concurrent_shards);
        
        self.running = true;
        Ok(())
    }
    
    /// Stops state sync service
    pub async fn stop(&mut self) -> Result<(), NetworkError> {
        if !self.running {
            return Ok(());
        }
        
        info!("🛑 Stopping state sync service...");
        
        // Cancel all active syncs
        let mut active_syncs = self.active_syncs.write().await;
        for (shard_id, handle) in active_syncs.iter_mut() {
            info!("🛑 Cancelling sync for shard {}", shard_id);
            handle.abort();
        }
        active_syncs.clear();
        
        self.running = false;
        Ok(())
    }
    
    /// Synchronizes a specific shard
    pub async fn sync_shard(&self, shard_id: ShardId, target_version: StateVersion) -> Result<(), NetworkError> {
        if !self.running {
            return Err(NetworkError::ServiceAlreadyStarted);
        }
        
        // Check if shard is already syncing
        {
            let status = self.shard_status.read().await;
            if let Some(ShardSyncStatus::InProgress { .. }) = status.get(&shard_id) {
                return Err(NetworkError::Internal(format!("Shard {} is already syncing", shard_id)));
            }
        }
        
        // Acquire sync semaphore
        let _permit = self.sync_semaphore.acquire().await
            .map_err(|e| NetworkError::Internal(format!("Failed to acquire sync permit: {}", e)))?;
        
        // Create download semaphore for this shard
        {
            let mut semaphores = self.download_semaphores.write().await;
            semaphores.insert(shard_id, Arc::new(Semaphore::new(self.config.max_concurrent_downloads)));
        }
        
        // Update status
        {
            let mut status = self.shard_status.write().await;
            status.insert(shard_id, ShardSyncStatus::InProgress {
                downloaded_chunks: 0,
                total_chunks: 0,
                start_time: Instant::now(),
            });
        }
        
        // Start sync task
        let service = Arc::new(self.clone());
        let handle = tokio::spawn(async move {
            service.sync_shard_internal(shard_id, target_version).await
        });
        
        // Store handle
        {
            let mut active_syncs = self.active_syncs.write().await;
            active_syncs.insert(shard_id, handle);
        }
        
        info!("🚀 Started sync for shard {} to version {}", shard_id, target_version);
        Ok(())
    }
    
    /// Internal shard synchronization implementation
    async fn sync_shard_internal(&self, shard_id: ShardId, target_version: StateVersion) -> Result<(), NetworkError> {
        let start_time = Instant::now();
        
        // Get available peers for this shard
        let peers = self.get_peers_for_shard(shard_id).await?;
        if peers.is_empty() {
            return Err(NetworkError::Internal(format!("No peers available for shard {}", shard_id)));
        }
        
        // Get shard metadata
        let metadata = self.get_shard_metadata(shard_id, target_version, &peers).await?;
        
        // Update status with total chunks
        {
            let mut status = self.shard_status.write().await;
            if let Some(ShardSyncStatus::InProgress { downloaded_chunks, total_chunks, start_time: _ }) = status.get_mut(&shard_id) {
                *total_chunks = metadata.total_chunks;
            }
        }
        
        // Download chunks in parallel
        let chunks = self.download_shard_chunks(shard_id, &metadata, &peers).await?;
        
        // Verify chunks
        let verification_start = Instant::now();
        self.verify_shard_chunks(&chunks).await?;
        let verification_time = verification_start.elapsed();
        
        // Apply state changes
        self.apply_shard_state(shard_id, &chunks).await?;
        
        let total_time = start_time.elapsed();
        
        // Update status to completed
        {
            let mut status = self.shard_status.write().await;
            status.insert(shard_id, ShardSyncStatus::Completed {
                total_chunks: metadata.total_chunks,
                duration: total_time,
                verification_time,
            });
        }
        
        // Update statistics
        {
            let mut stats = self.stats.lock().await;
            stats.total_shards_synced += 1;
            stats.total_chunks_downloaded += metadata.total_chunks as u64;
            stats.total_bytes_downloaded += chunks.iter().map(|c| c.data.len() as u64).sum::<u64>();
            stats.total_verification_time_ms += verification_time.as_millis() as u64;
            
            // Update average sync time
            let total_sync_time = stats.avg_sync_time_ms * (stats.total_shards_synced - 1) as f32 + total_time.as_millis() as f32;
            stats.avg_sync_time_ms = total_sync_time / stats.total_shards_synced as f32;
        }
        
        // Remove from active syncs
        {
            let mut active_syncs = self.active_syncs.write().await;
            active_syncs.remove(&shard_id);
        }
        
        info!("✅ Shard {} synced successfully in {:.2}s (verification: {:.2}s)", 
            shard_id, total_time.as_secs_f32(), verification_time.as_secs_f32());
        
        Ok(())
    }
    
    /// Gets peers that can serve a specific shard
    async fn get_peers_for_shard(&self, shard_id: ShardId) -> Result<Vec<PeerInfo>, NetworkError> {
        let peer_manager = self.peer_manager.read().await;
        let all_peers = peer_manager.get_all_peers();
        
        // TODO: Implement peer selection based on shard capabilities
        // For now, return all peers
        Ok(all_peers)
    }
    
    /// Gets shard metadata from peers
    async fn get_shard_metadata(&self, shard_id: ShardId, version: StateVersion, peers: &[PeerInfo]) -> Result<ShardMetadata, NetworkError> {
        // Try to get metadata from multiple peers for redundancy
        let mut metadata_candidates = Vec::new();
        
        for peer in peers.iter().take(3) { // Try first 3 peers
            match self.request_shard_metadata(peer, shard_id, version).await {
                Ok(metadata) => metadata_candidates.push(metadata),
                Err(e) => {
                    warn!("⚠️ Failed to get metadata from peer {}: {}", peer.id, e);
                }
            }
        }
        
        if metadata_candidates.is_empty() {
            return Err(NetworkError::Internal("Failed to get shard metadata from any peer".to_string()));
        }
        
        // Verify metadata consistency
        let first_metadata = &metadata_candidates[0];
        for metadata in &metadata_candidates[1..] {
            if metadata.total_chunks != first_metadata.total_chunks ||
               metadata.root_hash != first_metadata.root_hash {
                return Err(NetworkError::Internal("Inconsistent shard metadata from peers".to_string()));
            }
        }
        
        Ok(first_metadata.clone())
    }
    
    /// Requests shard metadata from a peer
    async fn request_shard_metadata(&self, peer: &PeerInfo, shard_id: ShardId, version: StateVersion) -> Result<ShardMetadata, NetworkError> {
        // TODO: Implement actual network request
        // For now, return mock metadata
        
        Ok(ShardMetadata {
            shard_id,
            version,
            total_chunks: 100, // Mock value
            root_hash: vec![0u8; 32], // Mock hash
            chunk_hashes: vec![vec![0u8; 32]; 100], // Mock chunk hashes
        })
    }
    
    /// Downloads shard chunks in parallel
    async fn download_shard_chunks(&self, shard_id: ShardId, metadata: &ShardMetadata, peers: &[PeerInfo]) -> Result<Vec<StateChunk>, NetworkError> {
        let mut chunks = Vec::new();
        let mut download_tasks = Vec::new();
        
        // Create download tasks for each chunk
        for chunk_index in 0..metadata.total_chunks {
            let metadata = metadata.clone();
            let peers = peers.to_vec();
            let shard_id = shard_id;
            
            let task = tokio::spawn(async move {
                Self::download_chunk(shard_id, chunk_index, &metadata, &peers).await
            });
            
            download_tasks.push(task);
        }
        
        // Wait for all downloads to complete
        for task in download_tasks {
            match task.await {
                Ok(Ok(chunk)) => {
                    chunks.push(chunk);
                    
                    // Update progress
                    {
                        let mut status = self.shard_status.write().await;
                        if let Some(ShardSyncStatus::InProgress { downloaded_chunks, .. }) = status.get_mut(&shard_id) {
                            *downloaded_chunks += 1;
                        }
                    }
                }
                Ok(Err(e)) => {
                    error!("❌ Failed to download chunk: {}", e);
                    return Err(e);
                }
                Err(e) => {
                    error!("❌ Download task failed: {}", e);
                    return Err(NetworkError::Internal(format!("Download task failed: {}", e)));
                }
            }
        }
        
        // Sort chunks by index
        chunks.sort_by_key(|c| c.index);
        
        Ok(chunks)
    }
    
    /// Downloads a single chunk
    async fn download_chunk(
        shard_id: ShardId,
        chunk_index: usize,
        metadata: &ShardMetadata,
        peers: &[PeerInfo],
    ) -> Result<StateChunk, NetworkError> {
        // Try multiple peers for redundancy
        for peer in peers {
            match Self::download_chunk_from_peer(peer, shard_id, chunk_index, metadata).await {
                Ok(chunk) => return Ok(chunk),
                Err(e) => {
                    warn!("⚠️ Failed to download chunk {} from peer {}: {}", chunk_index, peer.id, e);
                    continue;
                }
            }
        }
        
        Err(NetworkError::Internal(format!("Failed to download chunk {} from any peer", chunk_index)))
    }
    
    /// Downloads a chunk from a specific peer
    async fn download_chunk_from_peer(
        peer: &PeerInfo,
        shard_id: ShardId,
        chunk_index: usize,
        metadata: &ShardMetadata,
    ) -> Result<StateChunk, NetworkError> {
        // TODO: Implement actual network download
        // For now, return mock chunk
        
        let chunk_data = vec![0u8; 1024 * 1024]; // 1MB mock data
        
        Ok(StateChunk {
            chunk_id: Uuid::new_v4(),
            shard_id,
            index: chunk_index,
            total_chunks: metadata.total_chunks,
            data: chunk_data,
            merkle_proof: MerkleProof {
                root_hash: metadata.root_hash.clone(),
                siblings: vec![vec![0u8; 32]; 8], // Mock proof
                path: vec![false; 8], // Mock path
                leaf_hash: metadata.chunk_hashes[chunk_index].clone(),
            },
            version: metadata.version,
        })
    }
    
    /// Verifies shard chunks using Merkle proofs
    async fn verify_shard_chunks(&self, chunks: &[StateChunk]) -> Result<(), NetworkError> {
        if self.config.parallel_verification {
            // Parallel verification
            let mut verification_tasks = Vec::new();
            
            for chunk in chunks {
                let chunk = chunk.clone();
                let task = tokio::spawn(async move {
                    Self::verify_chunk(&chunk).await
                });
                verification_tasks.push(task);
            }
            
            // Wait for all verifications
            for task in verification_tasks {
                match task.await {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => return Err(e),
                    Err(e) => return Err(NetworkError::Internal(format!("Verification task failed: {}", e))),
                }
            }
        } else {
            // Sequential verification
            for chunk in chunks {
                StateSyncService::verify_chunk(chunk).await?;
            }
        }
        
        Ok(())
    }
    
    /// Verifies a single chunk
    async fn verify_chunk(chunk: &StateChunk) -> Result<(), NetworkError> {
        // TODO: Implement actual Merkle proof verification
        // For now, just check that chunk hash matches
        
        let calculated_hash = Self::calculate_chunk_hash(&chunk.data);
        if calculated_hash != chunk.merkle_proof.leaf_hash {
            return Err(NetworkError::Internal("Chunk hash verification failed".to_string()));
        }
        
        Ok(())
    }
    
    /// Calculates hash of chunk data
    fn calculate_chunk_hash(data: &[u8]) -> Vec<u8> {
        // Use blake3 for fast hashing (coding-guidelines: prefer blake3 over sha2)
        blake3::hash(data).as_bytes().to_vec()
    }
    
    /// Applies downloaded state to local storage
    async fn apply_shard_state(&self, shard_id: ShardId, chunks: &[StateChunk]) -> Result<(), NetworkError> {
        // TODO: Implement actual state application
        // This would involve:
        // 1. Validating state transitions
        // 2. Applying changes atomically
        // 3. Updating local state
        // 4. Persisting to storage
        
        info!("📝 Applied {} chunks for shard {}", chunks.len(), shard_id);
        Ok(())
    }
    
    /// Gets sync status for all shards
    pub async fn get_sync_status(&self) -> HashMap<ShardId, ShardSyncStatus> {
        self.shard_status.read().await.clone()
    }
    
    /// Gets sync status for a specific shard
    pub async fn get_shard_status(&self, shard_id: ShardId) -> Option<ShardSyncStatus> {
        self.shard_status.read().await.get(&shard_id).cloned()
    }
    
    /// Gets sync statistics
    pub async fn get_stats(&self) -> StateSyncStats {
        self.stats.lock().await.clone()
    }
    
    /// Cancels sync for a specific shard
    pub async fn cancel_shard_sync(&self, shard_id: ShardId) -> Result<(), NetworkError> {
        let mut active_syncs = self.active_syncs.write().await;
        
        if let Some(handle) = active_syncs.remove(&shard_id) {
            handle.abort();
            
            // Update status
            {
                let mut status = self.shard_status.write().await;
                status.insert(shard_id, ShardSyncStatus::Failed {
                    error: "Cancelled by user".to_string(),
                    downloaded_chunks: 0,
                    total_chunks: 0,
                });
            }
            
            info!("🛑 Cancelled sync for shard {}", shard_id);
        }
        
        Ok(())
    }
}

/// Shard metadata
#[derive(Debug, Clone)]
struct ShardMetadata {
    /// Shard ID
    shard_id: ShardId,
    /// State version
    version: StateVersion,
    /// Total number of chunks
    total_chunks: usize,
    /// Root hash of the state
    root_hash: Vec<u8>,
    /// Hashes of individual chunks
    chunk_hashes: Vec<Vec<u8>>,
}

impl Clone for StateSyncService {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            peer_manager: self.peer_manager.clone(),
            event_sender: self.event_sender.clone(),
            shard_status: self.shard_status.clone(),
            active_syncs: self.active_syncs.clone(),
            sync_semaphore: self.sync_semaphore.clone(),
            download_semaphores: self.download_semaphores.clone(),
            stats: self.stats.clone(),
            running: self.running,
        }
    }
}


