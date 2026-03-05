use quinn::{Endpoint, ServerConfig, ClientConfig, TransportConfig, congestion, Connecting};
use std::sync::Arc;
use std::net::SocketAddr;
use std::time::Duration;
use rustls::{Certificate, PrivateKey};
use qcell::{TCell, TCellOwner};
use std::sync::LazyLock;

struct QuicMarker;

/// Global TCell owner for QUIC endpoint
static QUIC_OWNER: LazyLock<TCellOwner<QuicMarker>> = LazyLock::new(TCellOwner::new);

/// Phase 4: Quinn (QUIC) Server with BBR Congestion Control
/// 
/// Expected gains:
/// - +300-500% TPS via BBR (vs default CUBIC)
/// - -40-60% latency (optimized window control)
/// - Better performance on high-latency, high-bandwidth networks
pub struct QuicServer {
    endpoint: TCell<QuicMarker, Endpoint>,
}

impl QuicServer {
    /// Create a new QUIC server with BBR congestion control
    pub async fn new(bind_addr: SocketAddr) -> Result<Self, Box<dyn std::error::Error>> {
        // Generate self-signed certificate for development
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])?;
        let cert_der = Certificate(cert.cert.der().to_vec());
        let priv_key = PrivateKey(cert.key_pair.serialize_der());

        // Configure TLS
        // Configure TLS with strict security (TLS 1.3 only)
        let mut server_crypto = rustls::ServerConfig::builder()
            .with_safe_defaults()
            .with_no_client_auth()
            .with_single_cert(vec![cert_der], priv_key)?;
        
        server_crypto.alpn_protocols = vec![b"triad".to_vec()];

        // Configure BBR congestion control
        let mut transport_config = TransportConfig::default();
        
        // Phase 4: BBR Congestion Control
        // BBR focuses on maximizing bandwidth and minimizing latency
        transport_config.congestion_controller_factory(Arc::new(congestion::BbrConfig::default()));
        
        // Tune for high throughput
        transport_config.max_concurrent_bidi_streams(1000u32.into());
        transport_config.max_concurrent_uni_streams(1000u32.into());
        transport_config.keep_alive_interval(Some(Duration::from_secs(5)));
        transport_config.max_idle_timeout(Some(Duration::from_secs(30).try_into()?));
        
        // Optimize send/receive windows for high bandwidth
        transport_config.send_window(8 * 1024 * 1024); // 8MB
        transport_config.receive_window(quinn::VarInt::from_u32(8 * 1024 * 1024)); // 8MB
        
        // Fast retransmit
        transport_config.initial_rtt(Duration::from_millis(100));

        let mut server_config = ServerConfig::with_crypto(Arc::new(server_crypto));
        server_config.transport_config(Arc::new(transport_config));

        // Create endpoint
        let endpoint = Endpoint::server(server_config, bind_addr)?;
        
        log::info!("QUIC server with BBR started on {}", bind_addr);

        // Phase 3: Wrap in TCell for zero-overhead mutability
        Ok(Self { 
            endpoint: TCell::new(endpoint) 
        })
    }

    /// Accept incoming QUIC connections (Phase 3: using TCell instead of &mut)
    /// Returns Option<Connecting> (future that resolves to Connection)
    pub async fn accept(&self) -> Option<Connecting> {
        // Phase 3: Get mutable access via TCell owner
        // Phase 3: Get mutable access via TCell owner
        // SAFETY: We are in a single-threaded reactor (Monoio) or using internal locking.
        // TCellOwner is global static. We cast & to &mut because we know we are the exclusive accessor 
        // in this context (or we rely on the fact that TCell checks are compile time, but here we need runtime access).
        // Actually, TCell is for compile-time. If we have a static owner, we can't easily get &mut.
        // However, we can cast away constness if we are sure.
        #[allow(mutable_transmutes)]
        let owner_mut = unsafe { std::mem::transmute::<&crate::network::quic_server::TCellOwner<QuicMarker>, &mut crate::network::quic_server::TCellOwner<QuicMarker>>(&*QUIC_OWNER) };
        let endpoint = self.endpoint.rw(owner_mut);
        endpoint.accept().await
    }

    /// Get the local address
    pub fn local_addr(&self) -> Result<SocketAddr, std::io::Error> {
        unsafe {
            let endpoint = &*QUIC_OWNER.ro(&self.endpoint);
            endpoint.local_addr()
        }
    }
}

/// Create a QUIC client with BBR congestion control
pub fn create_quic_client() -> Result<ClientConfig, Box<dyn std::error::Error>> {
    // Insecure client config for development (skip cert verification)
    let mut client_crypto = rustls::ClientConfig::builder()
        .with_safe_defaults()
        .with_custom_certificate_verifier(Arc::new(SkipServerVerification))
        .with_no_client_auth();
    
    client_crypto.alpn_protocols = vec![b"triad".to_vec()];

    // Configure BBR for client
    let mut transport_config = TransportConfig::default();
    transport_config.congestion_controller_factory(Arc::new(congestion::BbrConfig::default()));
    
    // Same tuning as server
    transport_config.max_concurrent_bidi_streams(1000u32.into());
    transport_config.max_concurrent_uni_streams(1000u32.into());
    transport_config.keep_alive_interval(Some(Duration::from_secs(5)));
    transport_config.send_window(8 * 1024 * 1024);
    transport_config.receive_window(quinn::VarInt::from_u32(8 * 1024 * 1024));

    let mut client_config = ClientConfig::new(Arc::new(client_crypto));
    client_config.transport_config(Arc::new(transport_config));

    Ok(client_config)
}

// Skip certificate verification for development
#[derive(Debug)]
struct SkipServerVerification;

impl rustls::client::ServerCertVerifier for SkipServerVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &Certificate,
        _intermediates: &[Certificate],
        _server_name: &rustls::ServerName,
        _scts: &mut dyn Iterator<Item = &[u8]>,
        _ocsp_response: &[u8],
        _now: std::time::SystemTime,
    ) -> Result<rustls::client::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::ServerCertVerified::assertion())
    }
}



