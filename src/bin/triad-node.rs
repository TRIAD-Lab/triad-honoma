use std::env;
use std::sync::Arc;

use tokio::signal;
use tokio::sync::{RwLock, mpsc};

use env_logger::Env;
use log::{error, info};

use triad::network::types::NetworkEvent;
use triad::network::{LibP2PDiscoveryService, PeerManager};

#[tokio::main]
async fn main() {
    // Init logging
    let env = Env::default().filter_or("RUST_LOG", "info");
    let _ = env_logger::Builder::from_env(env)
        .format_timestamp_secs()
        .try_init();

    // Read basic env
    let node_name = env::var("TRIAD_NODE_NAME").unwrap_or_else(|_| "triad-node".to_string());
    let quic_port: u16 = env::var("TRIAD_LISTEN_QUIC_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(4002);

    info!(
        "Starting triad-node: name={}, quic_port={}, bootstrap_mode={} (TRIAD_BOOTSTRAP)",
        node_name,
        quic_port,
        env::var("TRIAD_BOOTSTRAP").unwrap_or_else(|_| "0".into())
    );

    // Channels and peer manager
    let (tx, mut rx) = mpsc::channel::<NetworkEvent>(128);
    let peer_manager = Arc::new(RwLock::new(PeerManager::new()));

    // Spawn simple event logger
    tokio::spawn(async move {
        while let Some(ev) = rx.recv().await {
            // For now we just log debug representation
            info!("network event: {:?}", ev);
        }
    });

    // Build discovery service
    let service = Arc::new(LibP2PDiscoveryService::new(
        node_name,
        quic_port,
        tx,
        peer_manager,
    ));

    // Run the service until Ctrl+C
    let svc = Arc::clone(&service);
    let handle = tokio::spawn(async move {
        if let Err(e) = svc.start().await {
            error!("libp2p service error: {}", e);
        }
    });

    // Wait for Ctrl+C
    info!("Press Ctrl+C to stop triad-node");
    if let Err(e) = signal::ctrl_c().await {
        error!("Failed to listen for Ctrl+C: {}", e);
    }

    // Graceful stop
    if let Err(e) = service.stop().await {
        error!("Stop error: {}", e);
    }

    // Ensure background task finishes
    if let Err(e) = handle.await {
        error!("Join error: {}", e);
    }

    info!("triad-node stopped");
}


