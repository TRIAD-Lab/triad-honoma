use tokio_tungstenite::connect_async;
use tokio_tungstenite::{WebSocketStream, MaybeTlsStream};
use tokio_tungstenite::tungstenite::Message as WsMessage;
use futures::stream::{SplitSink, SplitStream};
use tokio::net::TcpStream;
use futures::{SinkExt, StreamExt};

use log::{info, debug, trace};
use std::time::Instant;


use super::websocket_server::{WebSocketMessage, WebSocketResponse};

#[derive(Debug)]
pub struct WebSocketClient {
    pub node_id: String,
    pub server_url: String,
}

impl WebSocketClient {
    pub fn new(node_id: String, server_url: String) -> Self {
        Self {
            node_id,
            server_url,
        }
    }

    pub async fn send_consensus_request(
        &self,
        round: usize,
        transactions: Vec<Vec<u8>>,
        shard_id: usize,
    ) -> Result<WebSocketResponse, String> {
        let url = if self.server_url.starts_with("ws://") || self.server_url.starts_with("wss://") {
            self.server_url.clone()
        } else {
            format!("ws://{}/consensus", self.server_url)
        };
        
        info!("📡 Connecting to WebSocket server: {}", url);
        
        let (ws_stream, _) = connect_async(&url).await
            .map_err(|e| format!("Failed to connect to WebSocket: {}", e))?;
        
        let message = WebSocketMessage {
            message_type: "consensus".to_string(),
            round,
            shard_id,
            transactions,
            node_id: self.node_id.clone(),
        };
        
        let message_json = serde_json::to_string(&message)
            .map_err(|e| format!("Failed to serialize message: {}", e))?;
        
        let (mut write, mut read) = ws_stream.split();
        
        // Отправляем сообщение
        write.send(tokio_tungstenite::tungstenite::Message::Text(message_json)).await
            .map_err(|e| format!("Failed to send WebSocket message: {}", e))?;
        
        // Получаем ответ
        if let Some(msg) = read.next().await {
            match msg {
                Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => {
                    let response: WebSocketResponse = serde_json::from_str(&text)
                        .map_err(|e| format!("Failed to deserialize response: {}", e))?;
                    
                    info!("📥 Received WebSocket response: {} transactions processed", response.processed_count);
                    
                    if let Some(metrics) = &response.gpu_metrics {
                        info!("🎮 GPU Metrics: {} | Memory: {}MB | Time: {:.2}ms", 
                              metrics.gpu_name, metrics.memory_used / 1024 / 1024, metrics.processing_time_ms);
                    }
                    
                    Ok(response)
                }
                Ok(tokio_tungstenite::tungstenite::Message::Close(_)) => {
                    Err("WebSocket connection closed".to_string())
                }
                Err(e) => {
                    Err(format!("WebSocket error: {}", e))
                }
                _ => {
                    Err("Unexpected message type".to_string())
                }
            }
        } else {
            Err("No response received".to_string())
        }
    }

    pub async fn send_batch_consensus_requests(
        &self,
        round: usize,
        transaction_batches: Vec<Vec<Vec<u8>>>,
        shard_ids: Vec<usize>,
    ) -> Result<Vec<WebSocketResponse>, String> {
        let start_time = Instant::now();
        
        info!("📡 Sending {} batch requests via single WebSocket connection", transaction_batches.len());
        debug!(
            "WS batches meta: round={}, expected_responses={}, shard_ids_len={}",
            round,
            transaction_batches.len(),
            shard_ids.len()
        );
        
        // Создаем ОДНО постоянное WebSocket соединение
        let url = if self.server_url.starts_with("ws://") || self.server_url.starts_with("wss://") {
            self.server_url.clone()
        } else {
            format!("ws://{}/consensus", self.server_url)
        };
        debug!("WS connecting to {}", url);
        let (ws_stream, _) = connect_async(&url).await
            .map_err(|e| format!("Failed to connect to WebSocket: {}", e))?;
        
        let (mut write, mut read) = ws_stream.split();
        let mut responses = Vec::new();
        let expected = transaction_batches.len();

        // Фаза 1: отправляем все батчи без ожидания ответа (pipeline over one WS)
        for (idx, (batch, &shard_id)) in transaction_batches.iter().zip(shard_ids.iter()).enumerate() {
            trace!(
                "WS send batch idx={} size={} shard_id={} round={}",
                idx,
                batch.len(),
                shard_id,
                round
            );
            let message = WebSocketMessage {
                message_type: "consensus".to_string(),
                round,
                shard_id,
                transactions: batch.clone(),
                node_id: self.node_id.clone(),
            };
            let message_json = serde_json::to_string(&message)
                .map_err(|e| format!("Failed to serialize message: {}", e))?;
            write
                .send(tokio_tungstenite::tungstenite::Message::Text(message_json))
                .await
                .map_err(|e| format!("Failed to send WebSocket message: {}", e))?;
            debug!("WS sent batch idx={} size={}", idx, batch.len());
        }

        // Фаза 2: собираем все ответы
        while responses.len() < expected {
            if let Some(msg) = read.next().await {
                match msg {
                    Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => {
                        let response: WebSocketResponse = serde_json::from_str(&text)
                            .map_err(|e| format!("Failed to deserialize response: {}", e))?;
                        debug!(
                            "WS recv response idx={} processed_count={} shard_id={} round={}",
                            responses.len(),
                            response.processed_count,
                            response.shard_id,
                            response.round
                        );
                        responses.push(response);
                    }
                    Ok(tokio_tungstenite::tungstenite::Message::Close(_)) => {
                        return Err("WebSocket connection closed".to_string());
                    }
                    Err(e) => {
                        return Err(format!("WebSocket error: {}", e));
                    }
                    _ => {}
                }
            } else {
                debug!("WS stream ended early: got {}/{} responses", responses.len(), expected);
                break;
            }
        }
        
        let total_time = start_time.elapsed();
        info!("⚡ Completed {} WebSocket requests in {:?} via single connection", transaction_batches.len(), total_time);
        
        Ok(responses)
    }

    // Новое: установить ПЕРМАНЕНТНОЕ соединение и вернуть хэндл
    pub async fn connect_persistent(&self) -> Result<PersistentConnection, String> {
        let url = if self.server_url.starts_with("ws://") || self.server_url.starts_with("wss://") {
            self.server_url.clone()
        } else {
            format!("ws://{}/consensus", self.server_url)
        };
        let (ws_stream, _) = connect_async(&url)
            .await
            .map_err(|e| format!("Failed to connect to WebSocket: {}", e))?;
        let (write, read) = ws_stream.split();
        Ok(PersistentConnection { write, read, node_id: self.node_id.clone() })
    }
}

pub struct PersistentConnection {
    pub node_id: String,
    write: SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, WsMessage>,
    read: SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>,
}

impl PersistentConnection {
    pub async fn send_batches_for_round(
        &mut self,
        round: usize,
        transaction_batches: &[Vec<Vec<u8>>],
        shard_ids: &[usize],
    ) -> Result<Vec<WebSocketResponse>, String> {
        let start_time = Instant::now();

        // Фаза 1: отправляем все батчи
        for (idx, (batch, &shard_id)) in transaction_batches.iter().zip(shard_ids.iter()).enumerate() {
            let message = WebSocketMessage {
                message_type: "consensus".to_string(),
                round,
                shard_id,
                transactions: batch.clone(),
                node_id: self.node_id.clone(),
            };
            let message_json = serde_json::to_string(&message)
                .map_err(|e| format!("Failed to serialize message: {}", e))?;
            self.write
                .send(WsMessage::Text(message_json))
                .await
                .map_err(|e| format!("Failed to send WebSocket message (idx={}): {}", idx, e))?;
        }

        // Фаза 2: собираем ответы
        let expected = transaction_batches.len();
        let mut responses = Vec::with_capacity(expected);
        while responses.len() < expected {
            if let Some(msg) = self.read.next().await {
                match msg {
                    Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => {
                        let response: WebSocketResponse = serde_json::from_str(&text)
                            .map_err(|e| format!("Failed to deserialize response: {}", e))?;
                        responses.push(response);
                    }
                    Ok(tokio_tungstenite::tungstenite::Message::Close(_)) => {
                        return Err("WebSocket connection closed".to_string());
                    }
                    Err(e) => return Err(format!("WebSocket error: {}", e)),
                    _ => {}
                }
            } else {
                break;
            }
        }

        let _ = start_time.elapsed();
        Ok(responses)
    }

    pub async fn close(mut self) -> Result<(), String> {
        self.write
            .send(WsMessage::Close(None))
            .await
            .map_err(|e| format!("Failed to send Close frame: {}", e))
    }
}


