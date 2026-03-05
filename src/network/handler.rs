use async_trait::async_trait;
use uuid::Uuid;
use crate::network::types::{Message, PeerInfo, NetworkEvent};
use crate::network::error::NetworkError;

/// Network message handler interface
/// Handles received message
/// Handles node connection
/// Handles node disconnection
#[async_trait]
pub trait MessageHandler: Send + Sync {
    /// Обрабатывает полученное сообщение
    async fn handle_message(&mut self, from: Uuid, message: Message) -> Result<(), NetworkError>;
    
    /// Обрабатывает подключение нового узла
    async fn handle_peer_connected(&mut self, peer_info: PeerInfo) -> Result<(), NetworkError>;

    /// Обрабатывает отключение узла
    async fn handle_peer_disconnected(&mut self, peer_id: Uuid) -> Result<(), NetworkError>;
}

/// Default handler implementation for tests and debugging
pub struct DefaultMessageHandler {
    node_id: Uuid,
}

impl DefaultMessageHandler {
    /// Creates a new instance of the default handler
    pub fn new(node_id: Uuid) -> Self {
        Self { node_id }
    }
}

#[async_trait]
impl MessageHandler for DefaultMessageHandler {
    async fn handle_message(&mut self, from: Uuid, message: Message) -> Result<(), NetworkError> {
        log::info!(
            "Node {} received message from {}: type={:?}, id={}",
            self.node_id,
            from,
            message.message_type,
            message.id
        );
        Ok(())
    }
    
    async fn handle_peer_connected(&mut self, peer_info: PeerInfo) -> Result<(), NetworkError> {
        log::info!(
            "Node {} discovered new node: {} ({})",
            self.node_id,
            peer_info.name,
            peer_info.id
        );
        Ok(())
    }
    
    async fn handle_peer_disconnected(&mut self, peer_id: Uuid) -> Result<(), NetworkError> {
        log::info!(
            "Node {} lost connection with node {}",
            self.node_id,
            peer_id
        );
        Ok(())
    }
} 

