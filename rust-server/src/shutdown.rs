use std::time::Duration;
use tokio::sync::watch;
use tokio::task::JoinHandle;

/// Wait for Ctrl+C, signal all workers to stop, then drain them with a
/// timeout. Prints status messages to stderr throughout.
pub(crate) async fn wait_for_shutdown(
    shutdown_tx: watch::Sender<bool>,
    handles: Vec<JoinHandle<()>>,
    shutdown_timeout: Duration,
) {
    tokio::signal::ctrl_c().await.ok();
    eprintln!(
        "\nReceived shutdown signal — draining in-flight requests (timeout: {}s)...",
        shutdown_timeout.as_secs()
    );

    // Signal all workers to stop accepting new connections.
    let _ = shutdown_tx.send(true);
    // Drop the sender so workers in `changed()` see the channel as closed.
    drop(shutdown_tx);

    let drain_future = async {
        for handle in handles {
            let _ = handle.await;
        }
    };

    match tokio::time::timeout(shutdown_timeout, drain_future).await {
        Ok(()) => eprintln!("Shutdown complete — all workers exited cleanly."),
        Err(_elapsed) => {
            eprintln!(
                "Shutdown timed out after {}s — forcing exit (some connections may have been dropped).",
                shutdown_timeout.as_secs()
            );
        }
    }
}
