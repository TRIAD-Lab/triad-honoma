//! QUIC transport implementation for high-performance networking
//! Features: 0-RTT handshake, multiplexing, connection migration

use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, RwLock};
use uuid::Uuid;
use log::{error, info};
use quinn::{Endpoint, ServerConfig, ClientConfig, Connection};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::collections::HashMap;
use std::time::{Duration, Instant};


use crate::network::error::NetworkError;
use crate::network::types::{Message, NetworkEvent};
use crate::network::peer::PeerManager;

/// QUIC transport configuration
#[derive(Debug, Clone)]
pub struct QUICConfig {
    /// Maximum concurrent streams per connection
    pub max_concurrent_streams: u32,
    /// Connection idle timeout
    pub idle_timeout: Duration,
    /// Maximum UDP payload size
    pub max_udp_payload_size: usize,
    /// Enable 0-RTT
    pub enable_0rtt: bool,
    /// Congestion control algorithm
    pub congestion_control: CongestionControl,
}

/// Congestion control algorithms
#[derive(Debug, Clone)]
pub enum CongestionControl {
    /// BBR (Bottleneck Bandwidth and RTT)
    BBR,
    /// Cubic
    Cubic,
    /// Reno
    Reno,
}

impl Default for QUICConfig {
    fn default() -> Self {
        Self {
            max_concurrent_streams: 100,
            idle_timeout: Duration::from_secs(300), // 5 minutes
            max_udp_payload_size: 65527, // Max UDP payload
            enable_0rtt: true,
            congestion_control: CongestionControl::BBR,
        }
    }
}

/// QUIC transport service
#[derive(Debug)]
pub struct QUICTransportService {
    /// Current node ID
    node_id: Uuid,
    /// QUIC port
    port: u16,
    /// Configuration
    config: QUICConfig,
    /// QUIC endpoint
    endpoint: Arc<Mutex<Option<Endpoint>>>,
    /// Active connections
    connections: Arc<RwLock<HashMap<Uuid, QUICConnection>>>,
    /// Event sender
    event_sender: mpsc::Sender<NetworkEvent>,
    /// Peer manager
    peer_manager: Arc<RwLock<PeerManager>>,
    /// Performance statistics
    stats: Arc<Mutex<QUICStats>>,
    /// Running flag
    running: bool,
}

/// QUIC connection wrapper
#[derive(Debug)]
struct QUICConnection {
    /// Connection handle
    connection: Connection,
    /// Last activity time
    last_activity: Instant,
    /// Stream counter
    stream_counter: u64,
    /// Connection quality metrics
    quality: ConnectionQuality,
}

/// Connection quality metrics
#[derive(Debug, Clone)]
struct ConnectionQuality {
    /// Round-trip time in milliseconds
    rtt_ms: u32,
    /// Available bandwidth in Mbps
    bandwidth_mbps: f32,
    /// Packet loss percentage
    packet_loss_percent: f32,
    /// Connection score (0-100)
    score: u8,
}

/// QUIC performance statistics
#[derive(Debug, Default, Clone)]
struct QUICStats {
    /// Total connections established
    total_connections: u64,
    /// Active connections
    active_connections: u64,
    /// Total bytes sent
    bytes_sent: u64,
    /// Total bytes received
    bytes_received: u64,
    /// Average RTT in milliseconds
    avg_rtt_ms: f32,
    /// Connection success rate
    connection_success_rate: f32,
}

impl QUICTransportService {
    /// Creates new QUIC transport service
    pub fn new(
        node_id: Uuid,
        port: u16,
        config: QUICConfig,
        event_sender: mpsc::Sender<NetworkEvent>,
        peer_manager: Arc<RwLock<PeerManager>>,
    ) -> Self {
        Self {
            node_id,
            port,
            config,
            endpoint: Arc::new(Mutex::new(None)),
            connections: Arc::new(RwLock::new(HashMap::new())),
            event_sender,
            peer_manager,
            stats: Arc::new(Mutex::new(QUICStats::default())),
            running: false,
        }
    }
    
    /// Starts QUIC transport service
    pub async fn start(&mut self) -> Result<(), NetworkError> {
        if self.running {
            return Err(NetworkError::ServiceAlreadyStarted);
        }
        
        info!("🚀 Starting QUIC transport service on port {}", self.port);
        
        // Create QUIC endpoint
        let endpoint = self.create_quic_endpoint().await?;
        
        // Store endpoint
        {
            let mut endpoint_guard = self.endpoint.lock().await;
            *endpoint_guard = Some(endpoint.clone());
        }
        
        // Start connection acceptor
        self.start_connection_acceptor(endpoint).await?;
        
        // Start connection health checker
        self.start_connection_health_checker().await?;
        
        // Start performance monitor
        self.start_performance_monitor().await?;
        
        self.running = true;
        info!("✅ QUIC transport service started successfully");
        
        Ok(())
    }
    
    /// Stops QUIC transport service
    pub async fn stop(&mut self) -> Result<(), NetworkError> {
        if !self.running {
            return Ok(());
        }
        
        info!("🛑 Stopping QUIC transport service...");
        
        // Close all connections
        let mut connections = self.connections.write().await;
        for (peer_id, conn) in connections.iter_mut() {
            info!("🔌 Closing connection to peer {}", peer_id);
            conn.connection.close(0u32.into(), b"Service stopping");
        }
        connections.clear();
        
        // Close endpoint
        if let Some(endpoint) = self.endpoint.lock().await.as_ref() {
            endpoint.close(0u32.into(), b"Service stopping");
        }
        
        self.running = false;
        info!("✅ QUIC transport service stopped");
        
        Ok(())
    }
    
    /// Creates QUIC endpoint
    async fn create_quic_endpoint(&self) -> Result<Endpoint, NetworkError> {
        // Generate certificate
        let cert_der = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])
            .map_err(|e| NetworkError::Internal(format!("Failed to generate certificate: {}", e)))?;
        let cert = rustls::Certificate(cert_der.cert.der().to_vec());
        let key = rustls::PrivateKey(cert_der.key_pair.serialize_der());

        // Create rustls configuration
        let mut server_crypto = rustls::ServerConfig::builder()
            .with_safe_defaults()
            .with_no_client_auth()
            .with_single_cert(vec![cert], key)
            .map_err(|e| NetworkError::Internal(format!("Failed to create server config: {}", e)))?;
        
        // Enable 0-RTT if configured
        if self.config.enable_0rtt {
            server_crypto.max_early_data_size = u32::MAX;
        }
        
        // Create QUIC server configuration
        let mut server_config = ServerConfig::with_crypto(Arc::new(server_crypto));
        
        // Configure transport parameters
        // Note: These fields are not available in the current quinn version
        // server_config.max_concurrent_streams = self.config.max_concurrent_streams;
        // server_config.max_idle_timeout = Some(self.config.idle_timeout.as_millis() as u64);
        // server_config.max_udp_payload_size = self.config.max_udp_payload_size;
        
        // Bind to port
        let addr = format!("0.0.0.0:{}", self.port)
            .parse()
            .map_err(|e| NetworkError::Internal(format!("Failed to parse address: {}", e)))?;
        
        let endpoint = Endpoint::server(server_config, addr)
            .map_err(|e| NetworkError::Internal(format!("Failed to create QUIC endpoint: {}", e)))?;
        
        info!("🚀 QUIC endpoint created on port {}", self.port);
        Ok(endpoint)
    }
    
    /// Starts connection acceptor
    async fn start_connection_acceptor(&self, endpoint: Endpoint) -> Result<(), NetworkError> {
        let event_sender = self.event_sender.clone();
        let peer_manager = self.peer_manager.clone();
        let connections = self.connections.clone();
        let stats = self.stats.clone();
        
        tokio::spawn(async move {
            info!("📥 QUIC connection acceptor started");
            
            // Quinn 0.11: accept() returns Accept iterator
            // Need to await it to get next connection
            while let Some(incoming) = endpoint.accept().await {
                let event_sender = event_sender.clone();
                let peer_manager = peer_manager.clone();
                let connections = connections.clone();
                let stats = stats.clone();
                
                tokio::spawn(async move {
                    // await the Incoming Future to get Connection
                    match incoming.await {
                        Ok(connection) => {
                            let peer_id = Uuid::new_v4(); // TODO: Extract from connection
                            let remote_addr = connection.remote_address();
                            
                            info!("🔗 New QUIC connection from {}", remote_addr);
                            
                            // Create connection wrapper
                            let quic_conn = QUICConnection {
                                connection: connection.clone(),
                                last_activity: Instant::now(),
                                stream_counter: 0,
                                quality: ConnectionQuality {
                                    rtt_ms: 0,
                                    bandwidth_mbps: 0.0,
                                    packet_loss_percent: 0.0,
                                    score: 100,
                                },
                            };
                            
                            // Store connection
                            connections.write().await.insert(peer_id, quic_conn);
                            
                            // Update stats
                            {
                                let mut stats_guard = stats.lock().await;
                                stats_guard.total_connections += 1;
                                stats_guard.active_connections += 1;
                            }
                            
                            // Start connection handler
                            Self::handle_connection(connection, peer_id, event_sender, peer_manager).await;
                        }
                        Err(e) => {
                            error!("❌ Failed to establish QUIC connection: {}", e);
                        }
                    }
                });
            }
        });
        
        Ok(())
    }
    
    /// Handles individual QUIC connection
    async fn handle_connection(
        connection: Connection,
        peer_id: Uuid,
        event_sender: mpsc::Sender<NetworkEvent>,
        peer_manager: Arc<RwLock<PeerManager>>,
    ) {
        info!("📡 Handling QUIC connection for peer {}", peer_id);
        
        loop {
            match connection.accept_bi().await {
                Ok((mut send, mut recv)) => {
                    info!("📡 New bidirectional stream from peer {}", peer_id);
                    
                    // Clone event_sender for this iteration
                    let event_sender = event_sender.clone();
                    
                    // Handle stream
                    tokio::spawn(async move {
                        let mut buffer = Vec::new();
                        
                        // Read message
                        if let Ok(_) = recv.read_to_end(1024 * 1024).await { // 1MB limit
                            // Parse message
                            if let Ok(message) = bincode::deserialize::<Message>(&buffer) {
                                info!("📨 Received message from peer {}: {:?}", peer_id, message.message_type);
                                
                                // Send to event channel
                                if let Err(e) = event_sender.send(NetworkEvent::MessageReceived { 
                                    from: peer_id, 
                                    message 
                                }).await {
                                    error!("Failed to send message event: {}", e);
                                }
                            }
                        }
                        
                        // Send response
                        let response = bincode::serialize(&"OK").unwrap();
                        if let Err(e) = send.write_all(&response).await {
                            error!("Failed to send response: {}", e);
                        }
                        
                        if let Err(e) = send.finish().await {
                            error!("Failed to finish stream: {}", e);
                        }
                    });
                }
                Err(e) => {
                    // Check if connection is closed
                    if e.to_string().contains("closed") || e.to_string().contains("Closed") {
                        info!("🔌 QUIC connection closed for peer {}", peer_id);
                        break;
                    } else {
                        error!("❌ QUIC stream error for peer {}: {}", peer_id, e);
                    }
                }
            }
        }
        
        // Remove connection
        // TODO: Remove from connections map
        info!("🔌 QUIC connection handler finished for peer {}", peer_id);
    }
    
    /// Starts connection health checker
    async fn start_connection_health_checker(&self) -> Result<(), NetworkError> {
        let connections = self.connections.clone();
        let stats = self.stats.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(30));
            
            loop {
                interval.tick().await;
                
                let mut connections_guard = connections.write().await;
                let mut stats_guard = stats.lock().await;
                
                // Check each connection
                connections_guard.retain(|peer_id, conn| {
                    let idle_time = conn.last_activity.elapsed();
                    
                    if idle_time > Duration::from_secs(300) { // 5 minutes
                        info!("🔌 Closing idle connection to peer {}", peer_id);
                        conn.connection.close(0u32.into(), b"Idle timeout");
                        stats_guard.active_connections = stats_guard.active_connections.saturating_sub(1);
                        false
                    } else {
                        // Update connection quality
                        Self::update_connection_quality(conn);
                        true
                    }
                });
                
                // Update average RTT
                let total_rtt: u32 = connections_guard.values()
                    .map(|conn| conn.quality.rtt_ms)
                    .sum();
                let conn_count = connections_guard.len() as u32;
                
                if conn_count > 0 {
                    stats_guard.avg_rtt_ms = total_rtt as f32 / conn_count as f32;
                }
            }
        });
        
        Ok(())
    }
    
    /// Updates connection quality metrics
    fn update_connection_quality(conn: &mut QUICConnection) {
        // TODO: Implement actual quality measurement
        // For now, just update last activity
        conn.last_activity = Instant::now();
    }
    
    /// Starts performance monitor
    async fn start_performance_monitor(&self) -> Result<(), NetworkError> {
        let stats = self.stats.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            
            loop {
                interval.tick().await;
                
                let stats_guard = stats.lock().await;
                info!("📊 QUIC Stats: {} active connections, {:.2}ms avg RTT, {:.2}% success rate", 
                    stats_guard.active_connections,
                    stats_guard.avg_rtt_ms,
                    stats_guard.connection_success_rate * 100.0
                );
            }
        });
        
        Ok(())
    }
    
    /// Connects to a peer
    pub async fn connect_to_peer(&self, peer_addr: String) -> Result<Uuid, NetworkError> {
        let endpoint = self.endpoint.lock().await
            .as_ref()
            .ok_or_else(|| NetworkError::Internal("QUIC endpoint not available".to_string()))?
            .clone();
        
        // Parse address
        let addr = peer_addr.parse()
            .map_err(|e| NetworkError::Internal(format!("Failed to parse address: {}", e)))?;
        
        // Create client configuration
        let client_config = self.create_client_config().await?;
        
        // Connect
        let connecting = endpoint.connect_with(client_config, addr, "localhost")
            .map_err(|e| NetworkError::Internal(format!("Failed to initiate connection: {}", e)))?;
        
        let connection = connecting.await
            .map_err(|e| NetworkError::Internal(format!("Failed to establish connection: {}", e)))?;
        
        let peer_id = Uuid::new_v4(); // TODO: Extract from connection
        
        // Store connection
        let quic_conn = QUICConnection {
            connection: connection.clone(),
            last_activity: Instant::now(),
            stream_counter: 0,
            quality: ConnectionQuality {
                rtt_ms: 0,
                bandwidth_mbps: 0.0,
                packet_loss_percent: 0.0,
                score: 100,
            },
        };
        
        self.connections.write().await.insert(peer_id, quic_conn);
        
        info!("🔗 Connected to peer {} at {}", peer_id, peer_addr);
        Ok(peer_id)
    }
    
    /// Sends message to peer
    pub async fn send_to_peer(&self, peer_id: Uuid, message: &Message) -> Result<(), NetworkError> {
        let connections = self.connections.read().await;
        
        if let Some(conn) = connections.get(&peer_id) {
            let message_bytes = bincode::serialize(message)
                .map_err(|e| NetworkError::Internal(format!("Failed to serialize message: {}", e)))?;
            
            // Open bidirectional stream
            let (mut send, mut recv) = conn.connection.open_bi()
                .await
                .map_err(|e| NetworkError::Internal(format!("Failed to open stream: {}", e)))?;
            
            // Send message
            send.write_all(&message_bytes).await
                .map_err(|e| NetworkError::Internal(format!("Failed to send message: {}", e)))?;
            
            send.finish()
                .await
                .map_err(|e| NetworkError::Internal(format!("Failed to finish stream: {}", e)))?;
            
            // Update stats
            {
                let mut stats = self.stats.lock().await;
                stats.bytes_sent += message_bytes.len() as u64;
            }
            
            info!("📤 Message sent to peer {}: {:?}", peer_id, message.message_type);
            Ok(())
        } else {
            Err(NetworkError::Internal(format!("No connection to peer {}", peer_id)))
        }
    }
    
    /// Creates client configuration
    async fn create_client_config(&self) -> Result<ClientConfig, NetworkError> {
        let mut root_cert_store = rustls::RootCertStore::empty();
        
        // Generate certificate (same as server for testing)
        let cert_der = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])
            .map_err(|e| NetworkError::Internal(format!("Failed to generate client certificate: {}", e)))?;
        
        let cert = rustls::Certificate(cert_der.cert.der().to_vec());
        
        root_cert_store.add(&cert)
            .map_err(|e| NetworkError::Internal(format!("Failed to add certificate to store: {}", e)))?;
        
        let mut client_crypto = rustls::ClientConfig::builder()
            .with_safe_defaults()
            .with_root_certificates(root_cert_store)
            .with_no_client_auth();
        
        // Enable 0-RTT if configured
        if self.config.enable_0rtt {
            client_crypto.enable_early_data = true;
        }
        
        Ok(ClientConfig::new(Arc::new(client_crypto)))
    }
    
    /// Gets performance statistics
    pub async fn get_stats(&self) -> QUICStats {
        self.stats.lock().await.clone()
    }
    
    /// Gets active connections count
    pub async fn active_connections(&self) -> usize {
        self.connections.read().await.len()
    }
}


