use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::Duration;
use uuid::Uuid;
use crate::network::types::PeerInfo;
use crate::network::error::NetworkError;

/// Time after which a node is considered inactive
pub const PEER_TIMEOUT: Duration = Duration::from_secs(60);

/// Node manager for tracking connected peers
#[derive(Debug)]
pub struct PeerManager {
    /// Map of known nodes: key - node ID, value - node info
    peers: HashMap<Uuid, PeerInfo>,
}

impl PeerManager {
    /// Creates a new node manager
    pub fn new() -> Self {
        Self {
            peers: HashMap::new(),
        }
    }
    
    /// Adds or updates node information
    pub fn add_peer(&mut self, peer_info: PeerInfo) -> bool {
        let is_new = !self.peers.contains_key(&peer_info.id);
        self.peers.insert(peer_info.id, peer_info);
        is_new
    }
    
    /// Returns node information by ID
    pub fn get_peer(&self, peer_id: &Uuid) -> Option<&PeerInfo> {
        self.peers.get(peer_id)
    }
    
    /// Returns a mutable reference to node information by ID
    pub fn get_peer_mut(&mut self, peer_id: &Uuid) -> Option<&mut PeerInfo> {
        self.peers.get_mut(peer_id)
    }
    
    /// Removes a node from the list of known nodes
    pub fn remove_peer(&mut self, peer_id: &Uuid) -> Option<PeerInfo> {
        self.peers.remove(peer_id)
    }
    
    /// Returns a list of all known nodes
    pub fn get_all_peers(&self) -> Vec<PeerInfo> {
        self.peers.values().cloned().collect()
    }
    
    /// Returns a list of only active nodes
    pub fn get_active_peers(&self) -> Vec<PeerInfo> {
        self.peers
            .values()
            .filter(|peer| peer.is_active(PEER_TIMEOUT))
            .cloned()
            .collect()
    }
    
    /// Returns the number of known nodes
    pub fn count(&self) -> usize {
        self.peers.len()
    }
    
    /// Updates the last interaction time with a node
    pub fn update_last_seen(&mut self, peer_id: &Uuid) -> Result<(), NetworkError> {
        if let Some(peer) = self.peers.get_mut(peer_id) {
            peer.update_last_seen();
            Ok(())
        } else {
            Err(NetworkError::PeerNotFound(*peer_id))
        }
    }
    
    /// Gets the address of a node by its ID
    pub fn get_peer_address(&self, peer_id: &Uuid) -> Result<SocketAddr, NetworkError> {
        if let Some(peer) = self.peers.get(peer_id) {
            Ok(peer.address)
        } else {
            Err(NetworkError::PeerNotFound(*peer_id))
        }
    }
    
    /// Cleans up inactive nodes
    pub fn cleanup_inactive_peers(&mut self) -> Vec<Uuid> {
        let inactive: Vec<Uuid> = self.peers
            .iter()
            .filter(|(_, peer)| !peer.is_active(PEER_TIMEOUT))
            .map(|(id, _)| *id)
            .collect();
        
        for id in &inactive {
            self.peers.remove(id);
        }
        
        inactive
    }
    
    /// Finds a node by address
    pub fn find_peer_by_address(&self, address: &SocketAddr) -> Option<&PeerInfo> {
        self.peers
            .values()
            .find(|peer| peer.address == *address)
    }
} 

