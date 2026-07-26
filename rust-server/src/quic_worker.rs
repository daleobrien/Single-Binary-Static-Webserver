use std::sync::Arc;
use std::time::Duration;

use h3_quinn::Connection as H3QuinnConnection;

use crate::config::WorkerConfig;
use crate::handlers::handle_h3_connection;
use crate::sockets::create_reuseport_udp_socket;

/// Spawn `num_workers` QUIC (HTTP/3) listener tasks, each on its own
/// SO_REUSEPORT UDP socket. Returns handles that can be awaited for
/// graceful shutdown.
pub(crate) fn spawn_quic_workers(
    cfg: WorkerConfig,
) -> Result<Vec<tokio::task::JoinHandle<()>>, Box<dyn std::error::Error + Send + Sync>> {
    let WorkerConfig {
        num_workers,
        port,
        tls_config,
        log_mode,
        shutdown_rx,
    } = cfg;

    // ── Pre-build shared QUIC components once, not per-worker ─────
    // The TLS→QUIC conversion (`try_into`) and rustls::ServerConfig clone
    // are the expensive parts — doing them once instead of N times avoids
    // cloning certificate chains and private keys per worker.
    let quic_tls_config: quinn::crypto::rustls::QuicServerConfig = {
        let mut quic_tls = (*tls_config).clone();
        quic_tls.alpn_protocols = vec![b"h3".to_vec()];
        quic_tls
            .try_into()
            .map_err(|e| format!("Failed to create QUIC crypto config: {e}"))?
    };
    let quic_tls: Arc<dyn quinn::crypto::ServerConfig> = Arc::new(quic_tls_config);

    let mut transport = quinn::TransportConfig::default();
    transport.max_idle_timeout(Some(quinn::IdleTimeout::from(
        quinn::VarInt::from_u32(30_000),
    )));
    transport.keep_alive_interval(Some(Duration::from_secs(10)));
    let transport = Arc::new(transport);

    let endpoint_config = quinn::EndpointConfig::default();
    let runtime: Arc<dyn quinn::Runtime> = Arc::new(quinn::TokioRuntime);

    let mut handles = Vec::with_capacity(num_workers);

    for i in 0..num_workers {
        let log_mode = log_mode.clone();
        let mut shutdown_rx = shutdown_rx.clone();

        // Cheap Arc clones — no TLS material is copied per worker.
        let mut quic_server_config =
            quinn::ServerConfig::with_crypto(Arc::clone(&quic_tls));
        quic_server_config.transport_config(Arc::clone(&transport));
        let endpoint_config = endpoint_config.clone();
        let runtime = Arc::clone(&runtime);

        // Spawn immediately so all workers create sockets and endpoints
        // in parallel, reducing startup latency.
        let handle = tokio::spawn(async move {
            let udp_socket = match create_reuseport_udp_socket(port) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Failed to create UDP socket for QUIC worker {i}: {e}");
                    return;
                }
            };

            let endpoint = match quinn::Endpoint::new(
                endpoint_config,
                Some(quic_server_config),
                udp_socket,
                runtime,
            ) {
                Ok(ep) => ep,
                Err(e) => {
                    eprintln!("Failed to create QUIC endpoint on worker {i}: {e}");
                    return;
                }
            };

            loop {
                let incoming = tokio::select! {
                    result = endpoint.accept() => result,
                    _ = shutdown_rx.changed() => {
                        endpoint.close(0u32.into(), b"server shutting down");
                        break;
                    }
                };

                match incoming {
                    Some(incoming) => {
                        let log_mode = log_mode.clone();
                        tokio::task::spawn(async move {
                            match incoming.await {
                                Ok(conn) => {
                                    let h3_conn = H3QuinnConnection::new(conn);
                                    if let Err(e) =
                                        handle_h3_connection(h3_conn, log_mode).await
                                    {
                                        if !crate::error::is_client_cancel(&*e) {
                                            eprintln!("h3 connection error: {e}");
                                        }
                                    }
                                }
                                Err(e) => {
                                    eprintln!("QUIC incoming error: {e}");
                                }
                            }
                        });
                    }
                    None => {
                        eprintln!("QUIC endpoint closed on worker {i}");
                        break;
                    }
                }
            }
        });

        handles.push(handle);
    }

    Ok(handles)
}
