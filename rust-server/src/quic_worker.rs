use std::sync::Arc;
use std::time::Duration;

use h3_quinn::Connection as H3QuinnConnection;

use crate::handlers::handle_h3_connection;
use crate::logging::LogMode;
use crate::sockets::create_reuseport_udp_socket;

/// Spawn `num_workers` QUIC (HTTP/3) listener tasks, each on its own
/// SO_REUSEPORT UDP socket. Returns handles that can be awaited for
/// graceful shutdown.
pub(crate) fn spawn_quic_workers(
    num_workers: usize,
    port: u16,
    tls_config: Arc<rustls::ServerConfig>,
    log_mode: LogMode,
    shutdown_rx: tokio::sync::watch::Receiver<bool>,
) -> Vec<tokio::task::JoinHandle<()>> {
    let mut handles = Vec::with_capacity(num_workers);

    for i in 0..num_workers {
        let udp_socket = match create_reuseport_udp_socket(port) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Failed to create UDP socket for QUIC worker {i}: {e}");
                continue;
            }
        };
        let log_mode = log_mode.clone();
        let mut shutdown_rx = shutdown_rx.clone();

        let quic_tls_config: quinn::crypto::rustls::QuicServerConfig = {
            let mut quic_tls = (*tls_config).clone();
            quic_tls.alpn_protocols = vec![b"h3".to_vec()];
            match quic_tls.try_into() {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("Failed to create QUIC crypto config on worker {i}: {e}");
                    continue;
                }
            }
        };
        let mut quic_server_config =
            quinn::ServerConfig::with_crypto(Arc::new(quic_tls_config));
        let mut transport = quinn::TransportConfig::default();
        transport.max_idle_timeout(Some(quinn::IdleTimeout::from(
            quinn::VarInt::from_u32(30_000),
        )));
        transport.keep_alive_interval(Some(Duration::from_secs(10)));
        quic_server_config.transport_config(Arc::new(transport));

        let endpoint = match quinn::Endpoint::new(
            quinn::EndpointConfig::default(),
            Some(quic_server_config),
            udp_socket,
            Arc::new(quinn::TokioRuntime),
        ) {
            Ok(ep) => ep,
            Err(e) => {
                eprintln!("Failed to create QUIC endpoint on worker {i}: {e}");
                continue;
            }
        };

        let handle = tokio::spawn(async move {
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

    handles
}
