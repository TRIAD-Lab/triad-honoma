//! Kademlia DHT implementation for automatic peer discovery
//! Features: distributed hash table, automatic routing, peer lookup

use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, RwLock};
use uuid::Uuid;
use log::{debug, info};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use serde::{Serialize, Deserialize};
use std::net::SocketAddr;

use crate::network::error::NetworkError;
use crate::network::types::NetworkEvent;
use crate::network::peer::PeerManager;

/// DHT bucket size (Kademlia standard)
const DHT_BUCKET_SIZE: usize = 20;
/// DHT key space size (160 bits for SHA-1)
const DHT_KEY_SPACE: usize = 160;
/// DHT refresh interval
const DHT_REFRESH_INTERVAL: Duration = Duration::from_secs(60);
/// DHT peer timeout
const DHT_PEER_TIMEOUT: Duration = Duration::from_secs(300); // 5 minutes

/// DHT entry representing a peer
#[derive(Debug, Clone)]
pub struct DHTEntry {
    /// Peer ID
    pub peer_id: Uuid,
    /// Peer address
    pub address: SocketAddr,
    /// Last seen timestamp
    pub last_seen: Instant,
    /// Distance from local node
    pub distance: u64,
    /// Peer quality score (0-100)
    pub quality_score: u8,
    /// Peer capabilities
    pub capabilities: PeerCapabilities,
}

/// Peer capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerCapabilities {
    /// Can serve as relay
    pub can_relay: bool,
    /// Can serve as bootnode
    pub can_bootnode: bool,
    /// Supports QUIC
    pub supports_quic: bool,
    /// Supports sharding
    pub supports_sharding: bool,
    /// Maximum TPS
    pub max_tps: u32,
}

/// DHT bucket for Kademlia
#[derive(Debug, Clone)]
struct DHTBucket {
    /// Entries in this bucket
    entries: Vec<DHTEntry>,
    /// Last refresh time
    last_refresh: Instant,
}

/// DHT service using Kademlia algorithm
#[derive(Debug)]
pub struct DHTService {
    /// Local node ID
    local_node_id: Uuid,
    /// DHT buckets
    buckets: Arc<RwLock<HashMap<usize, DHTBucket>>>,
    /// Peer manager
    peer_manager: Arc<RwLock<PeerManager>>,
    /// Event sender
    event_sender: mpsc::Sender<NetworkEvent>,
    /// DHT statistics
    stats: Arc<Mutex<DHTStats>>,
    /// Running flag
    running: bool,
}

/// DHT statistics
#[derive(Debug, Default, Clone)]
struct DHTStats {
    /// Total peers in DHT
    total_peers: u64,
    /// Active buckets
    active_buckets: usize,
    /// Lookup success rate
    lookup_success_rate: f32,
    /// Average lookup time
    avg_lookup_time_ms: f32,
    /// Total lookups performed
    total_lookups: u64,
    /// Successful lookups
    successful_lookups: u64,
}

impl DHTService {
    /// Creates new DHT service
    pub fn new(
        local_node_id: Uuid,
        peer_manager: Arc<RwLock<PeerManager>>,
        event_sender: mpsc::Sender<NetworkEvent>,
    ) -> Self {
        Self {
            local_node_id,
            buckets: Arc::new(RwLock::new(HashMap::new())),
            peer_manager,
            event_sender,
            stats: Arc::new(Mutex::new(DHTStats::default())),
            running: false,
        }
    }
    
    /// Starts DHT service
    pub async fn start(&mut self) -> Result<(), NetworkError> {
        if self.running {
            return Err(NetworkError::ServiceAlreadyStarted);
        }
        
        info!("🚀 Starting DHT service with {} buckets", DHT_KEY_SPACE);
        
        // Initialize buckets
        self.initialize_buckets().await;
        
        // Start periodic DHT maintenance
        self.start_dht_maintenance().await?;
        
        // Start peer discovery
        self.start_peer_discovery().await?;
        
        self.running = true;
        info!("✅ DHT service started successfully");
        
        Ok(())
    }
    
    /// Stops DHT service
    pub async fn stop(&mut self) -> Result<(), NetworkError> {
        if !self.running {
            return Ok(());
        }
        
        info!("🛑 Stopping DHT service...");
        
        // Clear buckets
        self.buckets.write().await.clear();
        
        self.running = false;
        info!("✅ DHT service stopped");
        
        Ok(())
    }
    
    /// Initializes DHT buckets
    async fn initialize_buckets(&self) {
        let mut buckets = self.buckets.write().await;
        
        for i in 0..DHT_KEY_SPACE {
            buckets.insert(i, DHTBucket {
                entries: Vec::new(),
                last_refresh: Instant::now(),
            });
        }
        
        info!("📦 Initialized {} DHT buckets", DHT_KEY_SPACE);
    }
    
    /// Adds peer to DHT
    pub async fn add_peer(&self, peer: DHTEntry) -> Result<(), NetworkError> {
        let distance = self.calculate_distance(self.local_node_id, peer.peer_id);
        let bucket_index = self.get_bucket_index(distance);
        
        let mut buckets = self.buckets.write().await;
        
        if let Some(bucket) = buckets.get_mut(&bucket_index) {
            // Check if peer already exists
            if let Some(existing_index) = bucket.entries.iter().position(|e| e.peer_id == peer.peer_id) {
                // Update existing entry
                bucket.entries[existing_index] = peer.clone();
                debug!("🔄 Updated peer {} in bucket {}", peer.peer_id, bucket_index);
            } else {
                // Add new entry
                if bucket.entries.len() < DHT_BUCKET_SIZE {
                    bucket.entries.push(peer.clone());
                    debug!("➕ Added peer {} to bucket {}", peer.peer_id, bucket_index);
                } else {
                    // Bucket is full, replace oldest entry
                    bucket.entries.sort_by_key(|e| e.last_seen);
                    bucket.entries[0] = peer.clone();
                    debug!("🔄 Replaced oldest peer in bucket {} with {}", bucket_index, peer.peer_id);
                }
            }
            
            // Update bucket refresh time
            bucket.last_refresh = Instant::now();
        }
        
        // Update statistics
        {
            let mut stats = self.stats.lock().await;
            stats.total_peers = self.count_total_peers(&buckets);
        }
        
        Ok(())
    }
    
    /// Removes peer from DHT
    pub async fn remove_peer(&self, peer_id: Uuid) -> Result<(), NetworkError> {
        let mut buckets = self.buckets.write().await;
        
        for bucket in buckets.values_mut() {
            bucket.entries.retain(|e| e.peer_id != peer_id);
        }
        
        // Update statistics
        {
            let mut stats = self.stats.lock().await;
            stats.total_peers = self.count_total_peers(&buckets);
        }
        
        debug!("🗑️ Removed peer {} from DHT", peer_id);
        Ok(())
    }
    
    /// Looks up peers by distance
    pub async fn lookup_peers(&self, target_id: Uuid, count: usize) -> Result<Vec<DHTEntry>, NetworkError> {
        let start_time = Instant::now();
        
        let distance = self.calculate_distance(self.local_node_id, target_id);
        let bucket_index = self.get_bucket_index(distance);
        
        let buckets = self.buckets.read().await;
        let mut candidates = Vec::new();
        
        // Start with the target bucket
        if let Some(bucket) = buckets.get(&bucket_index) {
            candidates.extend(bucket.entries.clone());
        }
        
        // Add peers from nearby buckets
        let mut bucket_offset = 1;
        while candidates.len() < count && bucket_offset < 8 {
            let bucket_index_plus = (bucket_index + bucket_offset) % DHT_KEY_SPACE;
            let bucket_index_minus = bucket_index.saturating_sub(bucket_offset);
            
            if let Some(bucket) = buckets.get(&bucket_index_plus) {
                candidates.extend(bucket.entries.clone());
            }
            
            if bucket_index_minus != bucket_index && bucket_index_minus < DHT_KEY_SPACE {
                if let Some(bucket) = buckets.get(&bucket_index_minus) {
                    candidates.extend(bucket.entries.clone());
                }
            }
            
            bucket_offset += 1;
        }
        
        // Sort by distance and quality
        candidates.sort_by(|a, b| {
            let dist_a = self.calculate_distance(target_id, a.peer_id);
            let dist_b = self.calculate_distance(target_id, b.peer_id);
            
            // Primary sort by distance
            let dist_cmp = dist_a.cmp(&dist_b);
            if dist_cmp != std::cmp::Ordering::Equal {
                return dist_cmp;
            }
            
            // Secondary sort by quality score
            b.quality_score.cmp(&a.quality_score)
        });
        
        // Limit results
        candidates.truncate(count);
        
        // Update statistics
        {
            let mut stats = self.stats.lock().await;
            stats.total_lookups += 1;
            if !candidates.is_empty() {
                stats.successful_lookups += 1;
            }
            
            let lookup_time = start_time.elapsed().as_millis() as f32;
            stats.avg_lookup_time_ms = (stats.avg_lookup_time_ms * (stats.total_lookups - 1) as f32 + lookup_time) / stats.total_lookups as f32;
            stats.lookup_success_rate = stats.successful_lookups as f32 / stats.total_lookups as f32;
        }
        
        info!("🔍 DHT lookup for {} found {} peers in {:.2}ms", target_id, candidates.len(), start_time.elapsed().as_millis());
        Ok(candidates)
    }
    
    /// Finds peers by capability
    pub async fn find_peers_by_capability(&self, capability: &str) -> Result<Vec<DHTEntry>, NetworkError> {
        let buckets = self.buckets.read().await;
        let mut candidates = Vec::new();
        
        for bucket in buckets.values() {
            for entry in &bucket.entries {
                match capability {
                    "relay" if entry.capabilities.can_relay => candidates.push(entry.clone()),
                    "bootnode" if entry.capabilities.can_bootnode => candidates.push(entry.clone()),
                    "quic" if entry.capabilities.supports_quic => candidates.push(entry.clone()),
                    "sharding" if entry.capabilities.supports_sharding => candidates.push(entry.clone()),
                    _ => {}
                }
            }
        }
        
        // Sort by quality score
        candidates.sort_by_key(|e| e.quality_score);
        candidates.reverse();
        
        info!("🔍 Found {} peers with capability '{}'", candidates.len(), capability);
        Ok(candidates)
    }
    
    /// Gets DHT routing table
    pub async fn get_routing_table(&self) -> HashMap<usize, Vec<DHTEntry>> {
        let buckets = self.buckets.read().await;
        buckets.iter().map(|(k, v)| (*k, v.entries.clone())).collect()
    }
    
    /// Calculates XOR distance between two node IDs
    fn calculate_distance(&self, id1: Uuid, id2: Uuid) -> u64 {
        let bytes1 = id1.as_bytes();
        let bytes2 = id2.as_bytes();
        
        let mut distance = 0u64;
        for i in 0..16 {
            distance ^= (bytes1[i] as u64) << (i * 8);
            distance ^= (bytes2[i] as u64) << (i * 8);
        }
        distance
    }
    
    /// Gets bucket index for a distance
    fn get_bucket_index(&self, distance: u64) -> usize {
        if distance == 0 {
            return 0;
        }
        
        // Find the highest set bit
        let leading_zeros = distance.leading_zeros();
        (64 - leading_zeros - 1) as usize
    }
    
    /// Counts total peers in all buckets
    fn count_total_peers(&self, buckets: &HashMap<usize, DHTBucket>) -> u64 {
        buckets.values().map(|bucket| bucket.entries.len() as u64).sum()
    }
    
    /// Starts DHT maintenance
    async fn start_dht_maintenance(&self) -> Result<(), NetworkError> {
        let buckets = self.buckets.clone();
        let stats = self.stats.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(DHT_REFRESH_INTERVAL);
            
            loop {
                interval.tick().await;
                
                let mut buckets_guard = buckets.write().await;
                let mut stats_guard = stats.lock().await;
                
                // Clean up expired peers
                let now = Instant::now();
                let mut total_peers = 0;
                let mut active_buckets = 0;
                
                for bucket in buckets_guard.values_mut() {
                    bucket.entries.retain(|e| {
                        let is_active = now.duration_since(e.last_seen) < DHT_PEER_TIMEOUT;
                        if is_active {
                            total_peers += 1;
                        }
                        is_active
                    });
                    
                    if !bucket.entries.is_empty() {
                        active_buckets += 1;
                    }
                }
                
                stats_guard.total_peers = total_peers;
                stats_guard.active_buckets = active_buckets;
                
                debug!("🧹 DHT maintenance: {} peers in {} active buckets", total_peers, active_buckets);
            }
        });
        
        Ok(())
    }
    
    /// Starts peer discovery
    async fn start_peer_discovery(&self) -> Result<(), NetworkError> {
        let dht_service = Arc::new(self.clone());
        let peer_manager = self.peer_manager.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(120)); // 2 minutes
            
            loop {
                interval.tick().await;
                
                // Look for new peers
                let peers = peer_manager.read().await.get_all_peers();
                if peers.len() < 10 {
                    info!("🔍 DHT peer discovery: only {} peers, searching for more", peers.len());
                    
                    // TODO: Implement peer discovery through DHT
                    // This would involve querying other nodes for their peer lists
                }
            }
        });
        
        Ok(())
    }
    
    /// Refreshes a specific bucket
    pub async fn refresh_bucket(&self, bucket_index: usize) -> Result<(), NetworkError> {
        let mut buckets = self.buckets.write().await;
        
        if let Some(bucket) = buckets.get_mut(&bucket_index) {
            bucket.last_refresh = Instant::now();
            
            // TODO: Implement bucket refresh by querying peers in this bucket
            // This helps maintain DHT health and discover new peers
            
            debug!("🔄 Refreshed bucket {}", bucket_index);
        }
        
        Ok(())
    }
    
    /// Gets DHT statistics
    pub async fn get_stats(&self) -> DHTStats {
        self.stats.lock().await.clone()
    }
    
    /// Gets bucket information
    pub async fn get_bucket_info(&self, bucket_index: usize) -> Option<BucketInfo> {
        let buckets = self.buckets.read().await;
        
        buckets.get(&bucket_index).map(|bucket| BucketInfo {
            index: bucket_index,
            peer_count: bucket.entries.len(),
            last_refresh: bucket.last_refresh,
            average_quality: bucket.entries.iter().map(|e| e.quality_score as f32).sum::<f32>() / bucket.entries.len() as f32,
        })
    }
}

/// Bucket information
#[derive(Debug, Clone)]
pub struct BucketInfo {
    /// Bucket index
    pub index: usize,
    /// Number of peers in bucket
    pub peer_count: usize,
    /// Last refresh time
    pub last_refresh: Instant,
    /// Average peer quality
    pub average_quality: f32,
}

impl Clone for DHTService {
    fn clone(&self) -> Self {
        Self {
            local_node_id: self.local_node_id,
            buckets: self.buckets.clone(),
            peer_manager: self.peer_manager.clone(),
            event_sender: self.event_sender.clone(),
            stats: self.stats.clone(),
            running: self.running,
        }
    }
}


