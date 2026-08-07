use std::sync::{Arc, OnceLock};
use std::time::Duration;

use h3_quinn::Connection as H3QuinnConnection;

use crate::config::WorkerConfig;
use crate::handlers::handle_h3_connection;
use crate::sockets::create_reuseport_udp_socket;

/// Components of a QUIC server config that are expensive to construct:
/// the TLS→QUIC crypto conversion clones certificate chains and
/// private-key material. These are cached via [`OnceLock`] so the
/// conversion happens at most once across all calls to
/// [`spawn_quic_workers`].
struct CachedQuicConfig {
    crypto: Arc<dyn quinn::crypto::ServerConfig>,
    transport: Arc<quinn::TransportConfig>,
}

/// Once-initialized QUIC server-config components.
/// On the first call to [`spawn_quic_workers`] the TLS config is
/// converted into Quinn's native format and stored here; every
/// subsequent call (and every worker within a call) reuses the cached
/// values via cheap [`Arc::clone`].
static QUIC_CONFIG: OnceLock<CachedQuicConfig> = OnceLock::new();

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

    // ── Build / retrieve cached QUIC config components ──────────────
    // The TLS→QUIC conversion (`try_into`) and rustls::ServerConfig
    // clone are the expensive parts — caching them via OnceLock means
    // they run at most once for the lifetime of the process. Subsequent
    // calls to this function (if any) get the pre-built config for
    // free, and `tls_config` is simply dropped.
    let cached = if let Some(c) = QUIC_CONFIG.get() {
        c
    } else {
        let mut quic_tls = (*tls_config).clone();
        quic_tls.alpn_protocols = vec![b"h3".to_vec()];
        let crypto: quinn::crypto::rustls::QuicServerConfig = quic_tls
            .try_into()
            .map_err(|e| format!("Failed to create QUIC crypto config: {e}"))?;

        let mut transport = quinn::TransportConfig::default();
        transport.max_idle_timeout(Some(quinn::IdleTimeout::from(
            quinn::VarInt::from_u32(30_000),
        )));
        transport.keep_alive_interval(Some(Duration::from_secs(10)));

        let config = CachedQuicConfig {
            crypto: Arc::new(crypto),
            transport: Arc::new(transport),
        };
        // set() fails if another thread raced us — fall through to
        // get() which returns the winner's identical config.
        let _ = QUIC_CONFIG.set(config);
        QUIC_CONFIG.get().unwrap()
    };

    let quic_tls = Arc::clone(&cached.crypto);
    let transport = Arc::clone(&cached.transport);
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
                                    let remote_addr = conn.remote_address();
                                    let h3_conn = H3QuinnConnection::new(conn);
                                    if let Err(e) =
                                        handle_h3_connection(h3_conn, remote_addr, log_mode).await
                                    {
                                        if !crate::error::is_client_cancel(&*e) {
                                            elog!("h3 connection error: {e}");
                                        }
                                    }
                                }
                                Err(e) => {
                                    elog!("QUIC incoming error: {e}");
                                }
                            }
                        });
                    }
                    None => {
                        elog!("QUIC endpoint closed on worker {i}");
                        break;
                    }
                }
            }
        });

        handles.push(handle);
    }

    Ok(handles)
}
