//! Boilerplate shared by all daemon `main`s: runtime, socket, signals.

use crate::ipc::{EventBus, Handler, Server};
use std::sync::Arc;

/// Build the single-threaded runtime every daemon uses.
pub fn runtime() -> std::io::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
}

/// Bind `<runtime>/<name>.sock`, serve `handler`, return the server task.
pub async fn serve_control(
    name: &str,
    bus: EventBus,
    handler: Arc<dyn Handler>,
) -> crate::Result<tokio::task::JoinHandle<()>> {
    let path = crate::paths::socket_path(name);
    let l = Server::bind(&path).await?;
    tracing::info!("control socket {}", path.display());
    let server = Arc::new(Server::new(name, bus));
    Ok(tokio::spawn(server.serve(l, handler)))
}

/// Resolve on SIGTERM or SIGINT.
pub async fn shutdown_signal() {
    use tokio::signal::unix::{signal, SignalKind};
    let mut term = signal(SignalKind::terminate()).expect("sigterm");
    let mut int = signal(SignalKind::interrupt()).expect("sigint");
    tokio::select! { _ = term.recv() => {}, _ = int.recv() => {} }
}

/// Remove the control socket at shutdown.
pub fn cleanup(name: &str) {
    let _ = std::fs::remove_file(crate::paths::socket_path(name));
}

/// Unix seconds now.
pub fn now_s() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Background tasks returned by a daemon's setup; aborted on shutdown.
pub type Tasks = Vec<tokio::task::JoinHandle<()>>;

/// Standard daemon `main`: logging, current-thread runtime, run `setup`
/// (which builds state and returns the request handler plus background
/// tasks), serve the control socket, wait for SIGTERM/SIGINT, clean up.
pub fn run_daemon<F, Fut>(name: &'static str, setup: F)
where
    F: FnOnce(EventBus) -> Fut,
    Fut: std::future::Future<Output = crate::Result<(Arc<dyn Handler>, Tasks)>>,
{
    crate::logging::init(name);
    let rt = runtime().expect("runtime");
    let res: crate::Result<()> = rt.block_on(async {
        let bus = EventBus::new();
        let (handler, tasks) = setup(bus.clone()).await?;
        let server = serve_control(name, bus, handler).await?;
        shutdown_signal().await;
        for t in tasks {
            t.abort();
        }
        server.abort();
        cleanup(name);
        Ok(())
    });
    if let Err(e) = res {
        tracing::error!("{e}");
        std::process::exit(1);
    }
}

/// Wrap a sync dispatch function over shared state into a [`Handler`].
pub fn sync_handler<S, D>(state: Arc<S>, dispatch: D) -> Arc<dyn Handler>
where
    S: Send + Sync + 'static,
    D: Fn(&S, &str, serde_json::Value) -> crate::ipc::RpcResult + Send + Sync + 'static,
{
    let dispatch = Arc::new(dispatch);
    Arc::new(move |m: String, p: serde_json::Value| {
        let s = state.clone();
        let d = dispatch.clone();
        crate::ipc::boxed(async move { d(&s, &m, p) })
    })
}
