use std::sync::Arc;
use tokio::sync::mpsc;
use uuid::Uuid;
use log::{debug, error, info};


use crate::network::error::NetworkError;
use crate::network::types::{Message, NetworkEvent};
use bytes::Bytes;
use crate::network::transport::TransportService;
use crate::network::MessageType;

/// Сервис для обработки и маршрутизации сообщений
#[derive(Debug)]
pub struct MessageService {
    /// ID текущего узла
    node_id: Uuid,
    /// Канал для отправки событий сети
    event_sender: mpsc::Sender<NetworkEvent>,
    /// Транспортный сервис для отправки сообщений
    transport: Arc<TransportService>,
}

impl MessageService {
    /// Создаёт новый сервис сообщений
    pub fn new(
        node_id: Uuid,
        event_sender: mpsc::Sender<NetworkEvent>,
        transport: Arc<TransportService>,
    ) -> Self {
        Self {
            node_id,
            event_sender,
            transport,
        }
    }
    
    /// Отправляет сообщение конкретному узлу
    pub async fn send_to_peer(&self, peer_id: Uuid, message_type: MessageType, payload: Vec<u8>) -> Result<(), NetworkError> {
        // Создаём сообщение
        let message = Message::new(
            self.node_id,
            Some(peer_id),
            message_type,
            Bytes::from(payload),
        );
        
        // Отправляем сообщение
        self.transport.send_message(peer_id, message).await
    }
    
    /// Отправляет сообщение всем известным узлам
    pub async fn broadcast(&self, message_type: MessageType, payload: Vec<u8>) -> Result<(), NetworkError> {
        // Создаём сообщение для broadcast
        let message = Message::new(
            self.node_id,
            None, // Broadcast
            message_type.clone(),
            Bytes::from(payload),
        );
        
        info!("Рассылка сообщения всем узлам: тип={:?}, id={}", message_type, message.id);
        
        // Получаем список всех активных узлов
        let peers = self.transport.get_active_peers().await?;
        
        // Отправляем сообщение каждому узлу
        let mut errors = Vec::new();
        for peer in peers {
            match self.transport.send_message(peer.id, message.clone()).await {
                Ok(_) => {
                    debug!("Сообщение отправлено узлу {}: {}", peer.name, peer.id);
                }
                Err(e) => {
                    error!("Ошибка отправки сообщения узлу {}: {}", peer.id, e);
                    errors.push(e);
                }
            }
        }
        
        // Если были ошибки, возвращаем первую
        if let Some(first_error) = errors.into_iter().next() {
            Err(first_error)
        } else {
            Ok(())
        }
    }
    
    /// Обрабатывает входящее сообщение
    pub async fn handle_message(&self, from: Uuid, message: Message) -> Result<(), NetworkError> {
        debug!("Обработка сообщения от {}: тип={:?}, id={}", from, message.message_type, message.id);
        
        // Все остальные типы сообщений тоже передаём в обработчик
        if let Err(e) = self.event_sender.send(NetworkEvent::MessageReceived {
            from,
            message,
        }).await {
            error!("Ошибка отправки события MessageReceived: {}", e);
        }
        
        Ok(())
    }
} 

