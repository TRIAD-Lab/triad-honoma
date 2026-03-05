//! Discovery module using libp2p for efficient P2P networking
//! Implements QUIC/TCP transport, Kademlia DHT, GossipSub, Identify, Ping,
//! AutoNAT and DCUtR hole punching. Bootstrap-only relay server, optional WebRTC fallback.

pub mod quic_transport;
pub mod gossip;
pub mod dht;
pub mod compression;
pub mod state_sync;
pub mod sharding;

use std::{env, net::Ipv4Addr, str::FromStr, sync::Arc, io};
use std::fs;
use std::path::PathBuf;
use std::future::Future;
use tokio::sync::{mpsc, RwLock};
use uuid::Uuid;
use log::{info, error, debug, warn};
use libp2p::{
    core::upgrade,
    core::transport::choice::OrTransport,
    futures::StreamExt,
    gossipsub,
    identify,
    identity,
    kad::{store::MemoryStore, Kademlia, KademliaEvent},
    multiaddr::Protocol,
    noise,
    ping,
    autonat,
    dcutr,
    relay,
    request_response::{Behaviour as RequestResponseBehaviour, Codec as RRCodec, Event as RequestResponseEvent, ProtocolName, ProtocolSupport, Message as RequestResponseMessage, ResponseChannel},
    swarm::{NetworkBehaviour, Swarm, SwarmBuilder, SwarmEvent},
    tcp,
    yamux,
    Multiaddr, PeerId, Transport,
};
use libp2p_quic as quic;
use futures::future::Either;

#[cfg(feature = "webrtc")]
use libp2p_webrtc as webrtc;

use crate::network::types::{PeerInfo, NetworkEvent, Message};
use crate::network::peer::PeerManager;
use libp2p::swarm::behaviour::toggle::Toggle;
use crate::network::error::NetworkError;
use futures::future::BoxFuture;
use async_trait::async_trait;

/// Main discovery service using libp2p
pub struct LibP2PDiscoveryService {
    /// Local identity
    local_key: identity::Keypair,
    /// Local PeerId
    local_peer_id: PeerId,
    /// Node name
    node_name: String,
    /// TCP listen port
    tcp_port: u16,
    /// QUIC listen port
    quic_port: u16,
    /// Channel for sending network events
    event_sender: mpsc::Sender<NetworkEvent>,
    /// Peer manager
    peer_manager: Arc<RwLock<PeerManager>>,
    /// Sender for shutdown signal
    shutdown_tx: Arc<RwLock<Option<tokio::sync::oneshot::Sender<()>>>>,
    /// Whether this node acts as the only bootstrap
    is_bootstrap: bool,
    /// Bootstrap multiaddr to dial (if not bootstrap)
    bootstrap_addr: Option<Multiaddr>,
    /// Bootstrap peer id (extracted from multiaddr), if present
    bootstrap_peer: Option<PeerId>,
}

/// Combined network behaviour
#[derive(NetworkBehaviour)]
#[behaviour]
struct DiscoveryBehaviour {
    gossipsub: gossipsub::Behaviour,
    kademlia: Kademlia<MemoryStore>,
    identify: identify::Behaviour,
    ping: ping::Behaviour,
    autonat: autonat::Behaviour,
    dcutr: dcutr::Behaviour,
    #[allow(dead_code)]
    relay_client: relay::client::Behaviour,
    #[allow(dead_code)]
    relay_server: Toggle<relay::Behaviour>,
    request_response: RequestResponseBehaviour<ConsensusCodec>,
}

// Note: Using the auto-generated `DiscoveryBehaviourEvent` from the derive macro

// --- Request-Response protocol for consensus batches ---

#[derive(Clone)]
struct ConsensusProtocol;

impl ProtocolName for ConsensusProtocol {
    fn protocol_name(&self) -> &[u8] {
        b"/triad/consensus/1"
    }
}

#[derive(Debug, Clone)]
struct ConsensusRequest(pub Vec<u8>);

#[derive(Debug, Clone)]
struct ConsensusResponse(pub Vec<u8>);

#[derive(Clone, Default)]
struct ConsensusCodec;

#[async_trait]
impl RRCodec for ConsensusCodec {
    type Protocol = ConsensusProtocol;
    type Request = ConsensusRequest;
    type Response = ConsensusResponse;

    async fn read_request<T>(&mut self, _: &Self::Protocol, io: &mut T) -> std::io::Result<Self::Request>
    where
        T: libp2p::futures::io::AsyncRead + Unpin + Send + 'async_trait,
    {
        use libp2p::futures::io::AsyncReadExt;
        let mut len_buf = [0u8; 4];
        io.read_exact(&mut len_buf).await?;
        let len = u32::from_le_bytes(len_buf) as usize;
        let mut buf = vec![0u8; len];
        io.read_exact(&mut buf).await?;
        Ok(ConsensusRequest(buf))
    }

    async fn read_response<T>(&mut self, _: &Self::Protocol, io: &mut T) -> std::io::Result<Self::Response>
    where
        T: libp2p::futures::io::AsyncRead + Unpin + Send + 'async_trait,
    {
        use libp2p::futures::io::AsyncReadExt;
        let mut len_buf = [0u8; 4];
        io.read_exact(&mut len_buf).await?;
        let len = u32::from_le_bytes(len_buf) as usize;
        let mut buf = vec![0u8; len];
        io.read_exact(&mut buf).await?;
        Ok(ConsensusResponse(buf))
    }

    async fn write_request<T>(&mut self, _: &Self::Protocol, io: &mut T, ConsensusRequest(data): ConsensusRequest) -> std::io::Result<()>
    where
        T: libp2p::futures::io::AsyncWrite + Unpin + Send + 'async_trait,
    {
        use libp2p::futures::io::AsyncWriteExt;
        let len = data.len() as u32;
        io.write_all(&len.to_le_bytes()).await?;
        io.write_all(&data).await?;
        io.flush().await
    }

    async fn write_response<T>(&mut self, _: &Self::Protocol, io: &mut T, ConsensusResponse(data): ConsensusResponse) -> std::io::Result<()>
    where
        T: libp2p::futures::io::AsyncWrite + Unpin + Send + 'async_trait,
    {
        use libp2p::futures::io::AsyncWriteExt;
        let len = data.len() as u32;
        io.write_all(&len.to_le_bytes()).await?;
        io.write_all(&data).await?;
        io.flush().await
    }
}

impl LibP2PDiscoveryService {
    /// Creates new libp2p discovery service
    pub fn new(
        node_name: String,
        quic_port: u16,
        event_sender: mpsc::Sender<NetworkEvent>,
        peer_manager: Arc<RwLock<PeerManager>>,
    ) -> Self {
        let local_key = Self::load_or_generate_identity();
        let local_peer_id = PeerId::from(local_key.public());

        // Only your machine should be the bootstrap
        // Configure via env vars:
        // TRIAD_BOOTSTRAP=1 on the bootstrap node
        // TRIAD_BOOTSTRAP_MULTIADDR=/ip4/<public_ip>/tcp/<port> on other nodes
        let is_bootstrap = env::var("TRIAD_BOOTSTRAP").ok().as_deref() == Some("1");
        let bootstrap_addr = env::var("TRIAD_BOOTSTRAP_MULTIADDR")
            .ok()
            .and_then(|s| Multiaddr::from_str(&s).ok());
        let bootstrap_peer = bootstrap_addr.as_ref().and_then(|ma| {
            ma.iter().filter_map(|p| match p { Protocol::P2p(mh) => PeerId::from_multihash(mh).ok(), _ => None }).last()
        });

        let tcp_port = env::var("TRIAD_LISTEN_TCP_PORT")
            .ok()
            .and_then(|s| s.parse::<u16>().ok())
            .unwrap_or(4001);

        info!("🚀 Created libp2p discovery: peer_id={}, node={}, bootstrap_mode={}, bootstrap_addr={:?}, bootstrap_peer={:?}",
              local_peer_id, node_name, is_bootstrap, bootstrap_addr, bootstrap_peer);

        Self {
            local_key,
            local_peer_id,
            node_name,
            tcp_port,
            quic_port,
            event_sender,
            peer_manager,
            shutdown_tx: Arc::new(RwLock::new(None)),
            is_bootstrap,
            bootstrap_addr,
            bootstrap_peer,
        }
    }

    fn identity_path() -> PathBuf {
        let mut p = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        p.push(".triad");
        p.push("identity.secp256k1");
        p
    }

    fn load_or_generate_identity() -> identity::Keypair {
        let path = Self::identity_path();
        if let Some(dir) = path.parent() { let _ = fs::create_dir_all(dir); }
        // Load or generate identity key (Secp256k1)
        let id_keys = match fs::read(&path) {
            Ok(bytes) => identity::Keypair::from_protobuf_encoding(&bytes).expect("Failed to decode identity key"),
            Err(_) => {
                let key = identity::Keypair::generate_secp256k1();
                fs::write(&path, key.to_protobuf_encoding().expect("Failed to encode identity key")).expect("Failed to write identity key");
                key
            }
        };
        id_keys
    }

    /// Build transport stack for WAN: QUIC OR (Relay OR TCP) with Noise+Yamux
    fn build_transport(&self, relay_transport: relay::client::Transport) -> Result<libp2p::core::transport::Boxed<(PeerId, libp2p::core::muxing::StreamMuxerBox)>, NetworkError> {
        // TCP with Noise + Yamux
        let tcp_base = tcp::tokio::Transport::new(tcp::Config::default().nodelay(true));
        let tcp_upgraded = OrTransport::new(relay_transport, tcp_base)
            .upgrade(upgrade::Version::V1Lazy)
            .authenticate(noise::Config::new(&self.local_key).map_err(|e| NetworkError::Internal(e.to_string()))?)
            .multiplex(yamux::Config::default())
            .map(|out, _| {
                let (peer, muxer) = out;
                (peer, libp2p::core::muxing::StreamMuxerBox::new(muxer))
            })
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
            .boxed();

        // QUIC transport (security + muxing are built-in)
        let quic_transport = quic::GenTransport::<quic::tokio::Provider>::new(quic::Config::new(&self.local_key))
            .map(|out, _| {
                let (peer, muxer) = out;
                (peer, libp2p::core::muxing::StreamMuxerBox::new(muxer))
            })
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
            .boxed();

        // Final combined transport: QUIC OR (Relay|TCP+Noise+Yamux)
        let transport = OrTransport::new(quic_transport, tcp_upgraded)
            .map(|out, _| match out {
                Either::Left(v) => v,
                Either::Right(v) => v,
            })
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
            .boxed();
        Ok(transport)
    }

    /// Build behaviour (Kademlia + Gossipsub + Ping + Identify)
    fn build_behaviour(&self, relay_client: relay::client::Behaviour) -> DiscoveryBehaviour {
        // GossipSub
        let gossipsub_config = gossipsub::ConfigBuilder::default()
            .validation_mode(gossipsub::ValidationMode::Permissive)
            .build()
            .expect("valid gossipsub config");
        let mut gossipsub = gossipsub::Behaviour::new(
            gossipsub::MessageAuthenticity::Signed(self.local_key.clone()),
            gossipsub_config,
        ).expect("gossipsub");

        // Kademlia
        let store = MemoryStore::new(self.local_peer_id);
        let mut kademlia = Kademlia::new(self.local_peer_id, store);

        // Identify
        let identify = identify::Behaviour::new(identify::Config::new(
            format!("triad/{}", env::var("TRIAD_VERSION").unwrap_or_else(|_| "dev".into())),
            self.local_key.public(),
        ));

        // Ping
        let ping = ping::Behaviour::new(ping::Config::new());

        // Subscribe to topics: global and shard-specific
        let topic = gossipsub::IdentTopic::new("triad-global");
        let _ = gossipsub.subscribe(&topic);
        if let Ok(shard_id) = env::var("TRIAD_SHARD_ID").and_then(|s| s.parse::<usize>().map_err(|e| env::VarError::NotPresent)) {
            let shard_topic = gossipsub::IdentTopic::new(format!("triad-shard-{}", shard_id));
            let _ = gossipsub.subscribe(&shard_topic);
        }
        // Bootstrap mode: no-op here. Peer addresses are added dynamically on Identify/connection events.

        // NAT traversal behaviours
        let autonat = autonat::Behaviour::new(self.local_peer_id, Default::default());
        let dcutr = dcutr::Behaviour::new(self.local_peer_id);
        // Relay: client passed in; server enabled on bootstrap node
        let relay_server = if self.is_bootstrap { Toggle::from(Some(relay::Behaviour::new(self.local_peer_id, Default::default()))) } else { Toggle::from(None) };

        // Request-Response for consensus batches
        let protocols = std::iter::once((ConsensusProtocol, ProtocolSupport::Full));
        let request_response = RequestResponseBehaviour::new(ConsensusCodec::default(), protocols, Default::default());

        DiscoveryBehaviour { gossipsub, kademlia, identify, ping, autonat, dcutr, relay_client, relay_server, request_response }
    }

    /// Starts the discovery service (public interface)
    pub async fn start(self: Arc<Self>) -> Result<(), NetworkError> {
        if self.is_running().await {
            return Ok(());
        }

        info!("🚀 Starting libp2p discovery service...");

        let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel();
        *self.shutdown_tx.write().await = Some(shutdown_tx);

        let mut swarm = {
            // Relay client: transport + behaviour
            let (relay_transport, relay_client) = relay::client::new(self.local_peer_id);
            let transport = self.build_transport(relay_transport)?;
            let behaviour = self.build_behaviour(relay_client);
            SwarmBuilder::with_executor(transport, behaviour, self.local_peer_id, TokioExecutor).build()
        };

        // Listen on TCP and optionally QUIC
        swarm
            .listen_on(
                Multiaddr::empty()
                    .with(Protocol::Ip4(Ipv4Addr::UNSPECIFIED))
                    .with(Protocol::Tcp(self.tcp_port)),
            )
            .expect("listen tcp");
        let enable_quic = env::var("TRIAD_ENABLE_QUIC").ok().as_deref() == Some("1");
        if enable_quic {
            match swarm.listen_on(
                Multiaddr::empty()
                    .with(Protocol::Ip4(Ipv4Addr::UNSPECIFIED))
                    .with(Protocol::Udp(self.quic_port))
                    .with(Protocol::QuicV1),
            ) {
                Ok(_) => info!("QUIC listening enabled on udp:{} (set by TRIAD_ENABLE_QUIC=1)", self.quic_port),
                Err(e) => {
                    warn!("listen QUIC failed: {} — continuing without QUIC", e);
                }
            }
        } else {
            info!("QUIC disabled. Set TRIAD_ENABLE_QUIC=1 to enable listening on udp:{}", self.quic_port);
        }

        // Announce external addresses explicitly if provided (for WAN reachability)
        if let Ok(addrs) = env::var("TRIAD_ANNOUNCE_ADDRS") {
            for s in addrs.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()) {
                match Multiaddr::from_str(s) {
                    Ok(ma) => {
                        info!("📣 Announcing external address from env: {}", ma);
                        let _ = swarm.add_external_address(ma, libp2p::swarm::AddressScore::Infinite);
                    }
                    Err(e) => warn!("Invalid TRIAD_ANNOUNCE_ADDRS entry '{}': {}", s, e),
                }
            }
        }

        // Bootstrap: if addr configured, dial it (needed to reach relay server and reserve)
        if let (false, Some(addr)) = (self.is_bootstrap, self.bootstrap_addr.clone()) {
            match swarm.dial(addr.clone()) {
                Ok(()) => info!("📞 Dialing bootstrap: {}", addr),
                Err(e) => warn!("Failed to dial bootstrap {}: {}", addr, e),
            }
        }

        let event_sender = self.event_sender.clone();
        let peer_manager = self.peer_manager.clone();
        let node_name = self.node_name.clone();

        loop {
            tokio::select! {
                _ = &mut shutdown_rx => {
                    info!("🛑 Shutdown signal received, stopping libp2p service loop");
                    break;
                }
                event = swarm.select_next_some() => {
                    match event {
                        SwarmEvent::NewListenAddr { address, .. } => {
                            info!("📡 Listening on {:?}", address);
                        }
                        // When we establish connection to the bootstrap (relay server), request reservation
                        SwarmEvent::ConnectionEstablished { peer_id, endpoint, .. } => {
                            if let Some(bsp) = self.bootstrap_peer {
                                if peer_id == bsp && !self.is_bootstrap {
                                    info!("🔐 Connected to relay server {:?}, requesting reservation via p2p-circuit listen...", peer_id);
                                    let relay_ma = Multiaddr::empty()
                                        .with(Protocol::P2p(peer_id.into()))
                                        .with(Protocol::P2pCircuit);
                                    match swarm.listen_on(relay_ma.clone()) {
                                        Ok(_) => info!("🛰️ Requested relay reservation: {}", relay_ma),
                                        Err(e) => warn!("Failed to request relay reservation on {}: {}", relay_ma, e),
                                    }
                                }
                            }
                            debug!("✅ Connection established: peer={:?}, endpoint={:?}", peer_id, endpoint);
                        }
                        
                        // AutoNAT diagnostics
                        SwarmEvent::Behaviour(DiscoveryBehaviourEvent::Autonat(ev)) => {
                            debug!("🔎 AutoNAT event: {:?}", ev);
                        }
                        // DCUtR diagnostics
                        SwarmEvent::Behaviour(DiscoveryBehaviourEvent::Dcutr(ev)) => {
                            debug!("🕳️ DCUtR event: {:?}", ev);
                        }
                        // Relay client diagnostics
                        SwarmEvent::Behaviour(DiscoveryBehaviourEvent::RelayClient(ev)) => {
                            debug!("🛰️ Relay client event: {:?}", ev);
                        }
                        SwarmEvent::Behaviour(DiscoveryBehaviourEvent::Kademlia(KademliaEvent::RoutingUpdated { peer, is_new_peer, .. })) => {
                            debug!("🔗 Kad routing updated: peer={:?}, is_new={}", peer, is_new_peer);
                        }
                        // Identify: track peer addresses and observed external address
                        SwarmEvent::Behaviour(DiscoveryBehaviourEvent::Identify(identify::Event::Received { peer_id, info })) => {
                            let obs = info.observed_addr.clone();
                            info!("🌐 Observed our external address via {:?}: {}", peer_id, obs);
                            let _ = swarm.add_external_address(obs, libp2p::swarm::AddressScore::Infinite);
                            info!("🤝 Identified peer: {} with {:?}", peer_id, info.listen_addrs);
                            for addr in info.listen_addrs {
                                swarm.behaviour_mut().kademlia.add_address(&peer_id, addr);
                            }
                        }
                        SwarmEvent::ConnectionClosed { peer_id, .. } => {
                             let _ = event_sender.send(NetworkEvent::PeerDisconnected(Uuid::new_v4())).await; // Note: Uuid needs to be tracked
                             debug!("🔌 Disconnected from {}", peer_id);
                        }
                        SwarmEvent::Behaviour(DiscoveryBehaviourEvent::RequestResponse(ev)) => {
                            match ev {
                                RequestResponseEvent::Message { peer, message } => {
                                    match message {
                                        RequestResponseMessage::Request { request, channel, .. } => {
                                            debug!("📥 RR request from {:?}: {} bytes", peer, request.0.len());
                                            // Echo response for now
                                            let _ = swarm.behaviour_mut().request_response.send_response(channel, ConsensusResponse(request.0)).map_err(|e| warn!("send_response error: {:?}", e));
                                        }
                                        RequestResponseMessage::Response { response, .. } => {
                                            debug!("📨 RR response from {:?}: {} bytes", peer, response.0.len());
                                        }
                                    }
                                }
                                RequestResponseEvent::OutboundFailure { peer, error, .. } => {
                                    warn!("RR outbound failure with {:?}: {:?}", peer, error);
                                }
                                RequestResponseEvent::InboundFailure { peer, error, .. } => {
                                    warn!("RR inbound failure with {:?}: {:?}", peer, error);
                                }
                                RequestResponseEvent::ResponseSent { peer, .. } => {
                                    debug!("RR response sent to {:?}", peer);
                                }
                            }
                        }
                        other => {
                            debug!("🔭 Swarm event: {:?}", other);
                        }
                    }
                }
            }
        }
        Ok(())
    }
    
    /// Stops the discovery service
    pub async fn stop(&self) -> Result<(), NetworkError> {
        if let Some(tx) = self.shutdown_tx.write().await.take() {
            info!("🛑 Sending shutdown signal to libp2p service...");
            tx.send(()).map_err(|_| NetworkError::ShutdownFailed)?;
            info!("✅ libp2p discovery service stopped");
        }
        Ok(())
    }

    /// Gets current peer ID
    pub fn peer_id(&self) -> PeerId { 
        self.local_peer_id 
    }
    
    /// Gets node name
    pub fn node_name(&self) -> &str {
        &self.node_name
    }

    /// Checks if the service is running
    pub async fn is_running(&self) -> bool {
        self.shutdown_tx.read().await.is_some()
    }
}

#[derive(Clone)]
struct TokioExecutor;

impl libp2p::swarm::Executor for TokioExecutor {
    fn exec(&self, future: std::pin::Pin<Box<dyn Future<Output = ()> + Send>>) {
        tokio::spawn(future);
    }
}


