use serde::{Deserialize, Serialize};
use bytes::Bytes;
use std::net::SocketAddr;
use uuid::Uuid;
use std::time::{Duration, Instant};
use crate::network::MessageType;

/// Network message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Unique message identifier
    pub id: Uuid,
    /// Sender identifier
    pub sender_id: Uuid,
    /// Recipient identifier (if None, then broadcast)
    pub recipient_id: Option<Uuid>,
    /// Message type
    pub message_type: MessageType,
    /// Message timestamp
    pub timestamp: u64,
    /// Message payload (zero-copy)
    pub payload: Bytes,
}

impl Message {
    /// Creates a new message
    pub fn new(
        sender_id: Uuid,
        recipient_id: Option<Uuid>,
        message_type: MessageType,
        payload: Bytes,
    ) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_else(|_| Duration::from_secs(0))
            .as_secs();

        Self {
            id: Uuid::new_v4(),
            sender_id,
            recipient_id,
            message_type,
            timestamp: now,
            payload,
        }
    }
}

/// Node information
#[derive(Debug, Clone)]
pub struct PeerInfo {
    /// Unique node identifier
    pub id: Uuid,
    /// Node name
    pub name: String,
    /// Node address and port
    pub address: SocketAddr,
    /// Last interaction time
    pub last_seen: Instant,
}

/// Serializable version of PeerInfo for network transfer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializablePeerInfo {
    /// Unique node identifier
    pub id: Uuid,
    /// Node name
    pub name: String,
    /// Node address and port as a string
    pub address: String,
}

impl From<&PeerInfo> for SerializablePeerInfo {
    fn from(peer: &PeerInfo) -> Self {
        Self {
            id: peer.id,
            name: peer.name.clone(),
            address: peer.address.to_string(),
        }
    }
}

impl PeerInfo {
    /// Creates new node information
    pub fn new(id: Uuid, name: String, address: SocketAddr) -> Self {
        Self {
            id,
            name,
            address,
            last_seen: Instant::now(),
        }
    }
    
    /// Updates last interaction time
    pub fn update_last_seen(&mut self) {
        self.last_seen = Instant::now();
    }
    
    /// Checks if the node is active
    pub fn is_active(&self, timeout: Duration) -> bool {
        self.last_seen.elapsed() < timeout
    }
} 

#[derive(Debug, Clone)]
pub enum NetworkEvent {
    MessageReceived { from: uuid::Uuid, message: Message },
    PeerConnected(PeerInfo),
    PeerDisconnected(uuid::Uuid),
    // Other events can be added as needed
} 