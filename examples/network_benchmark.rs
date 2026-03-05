use log::{error, info};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Barrier;
use triad::network::quic_server::{QuicServer, create_quic_client};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    info!("Starting Network Benchmark (QUIC + BBR)...");

    // 1. Start Server
    let server_addr = "127.0.0.1:5000".parse()?;
    let server = QuicServer::new(server_addr).await?;

    let barrier = Arc::new(Barrier::new(2));
    let barrier_clone = barrier.clone();

    // Server loop
    tokio::spawn(async move {
        info!("Server listening on {}", server_addr);
        barrier_clone.wait().await; // Wait for client to be ready

        while let Some(connecting) = server.accept().await {
            let connection = connecting.await.unwrap();
            tokio::spawn(async move {
                while let Ok((mut send, mut recv)) = connection.accept_bi().await {
                    let mut buf = vec![0u8; 1024 * 64]; // 64KB buffer
                    while let Ok(Some(n)) = recv.read(&mut buf).await {
                        // Echo back
                        let _ = send.write_all(&buf[..n]).await;
                    }
                    let _ = send.finish();
                }
            });
        }
    });

    // 2. Start Client
    barrier.wait().await; // Ensure server is up (conceptually)
    tokio::time::sleep(Duration::from_millis(100)).await; // Give server a moment

    let client_config = create_quic_client()?;
    let mut endpoint = quinn::Endpoint::client("0.0.0.0:0".parse().unwrap())?;
    endpoint.set_default_client_config(client_config);

    let connection = endpoint.connect(server_addr, "localhost")?.await?;
    info!("Client connected to {}", server_addr);

    // 3. Benchmark
    let msg_size = 1024 * 4; // 4KB message
    let msg_count = 10_000;
    let payload = vec![1u8; msg_size];

    info!(
        "Benchmarking {} messages of {} bytes...",
        msg_count, msg_size
    );

    let start = Instant::now();

    let mut handles = vec![];

    // Parallel streams
    for _ in 0..10 {
        // 10 concurrent streams
        let connection = connection.clone();
        let payload = payload.clone();

        handles.push(tokio::spawn(async move {
            let (mut send, mut recv) = connection.open_bi().await.unwrap();

            for _ in 0..(msg_count / 10) {
                send.write_all(&payload).await.unwrap();
                let mut buf = vec![0u8; payload.len()];
                recv.read_exact(&mut buf).await.unwrap();
            }

            send.finish().unwrap();
        }));
    }

    for handle in handles {
        handle.await?;
    }

    let duration = start.elapsed();
    let total_bytes = msg_size * msg_count;
    let tps = msg_count as f64 / duration.as_secs_f64();
    let mbps = (total_bytes as f64 / 1_000_000.0) / duration.as_secs_f64();

    info!("Benchmark Complete:");
    info!("Time: {:?}", duration);
    info!("TPS: {:.2}", tps);
    info!("Throughput: {:.2} MB/s", mbps);

    Ok(())
}
