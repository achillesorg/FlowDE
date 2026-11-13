//! JSON-lines control protocol over Unix sockets.
//!
//! Request:  `{"id":1,"method":"settings.get","params":{...}}`
//! Response: `{"id":1,"result":...}` or `{"id":1,"error":{"code":"..","message":".."}}`
//! Event:    `{"event":"power.changed","data":...}` (after a `subscribe` call)
//!
//! Built-in methods handled by the server itself: `ping`, `health`, `subscribe`.
//! The socket lives in the 0700 runtime dir and is chmod 0600; peer
//! credentials are therefore enforced by filesystem permissions.

use crate::health::Metrics;
use crate::{FlowError, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::broadcast;

/// Hard cap per protocol line.
pub const MAX_LINE: usize = 1 << 20;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Request {
    #[serde(default)]
    pub id: u64,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RpcError {
    pub code: String,
    pub message: String,
}

impl RpcError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new("invalid", message)
    }
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new("not_found", message)
    }
    pub fn unavailable(message: impl Into<String>) -> Self {
        Self::new("unavailable", message)
    }
    pub fn unknown_method(m: &str) -> Self {
        Self::new("unknown_method", format!("unknown method {m}"))
    }
}

impl From<FlowError> for RpcError {
    fn from(e: FlowError) -> Self {
        match e {
            FlowError::Invalid(m) => Self::invalid(m),
            FlowError::NotFound(m) => Self::not_found(m),
            FlowError::Unavailable(m) => Self::unavailable(m),
            other => Self::new("internal", other.to_string()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Response {
    pub id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Event {
    pub event: String,
    #[serde(default)]
    pub data: Value,
}

pub type RpcResult = std::result::Result<Value, RpcError>;
pub type BoxFut<T> = Pin<Box<dyn Future<Output = T> + Send>>;

/// Daemon request handler. Implemented for closures
/// `Fn(String, Value) -> BoxFut<RpcResult>`.
pub trait Handler: Send + Sync + 'static {
    fn call(&self, method: String, params: Value) -> BoxFut<RpcResult>;
}

impl<F> Handler for F
where
    F: Fn(String, Value) -> BoxFut<RpcResult> + Send + Sync + 'static,
{
    fn call(&self, method: String, params: Value) -> BoxFut<RpcResult> {
        self(method, params)
    }
}

/// Fan-out bus for events pushed to subscribed connections.
#[derive(Clone)]
pub struct EventBus {
    tx: broadcast::Sender<Event>,
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl EventBus {
    pub fn new() -> Self {
        Self {
            tx: broadcast::channel(64).0,
        }
    }
    /// Publish; silently dropped when nobody listens (zero cost idle).
    pub fn emit(&self, event: &str, data: impl Serialize) {
        if self.tx.receiver_count() == 0 {
            return;
        }
        if let Ok(data) = serde_json::to_value(data) {
            let _ = self.tx.send(Event {
                event: event.to_string(),
                data,
            });
        }
    }
    /// Number of currently subscribed connections (lets daemons sleep sensors when 0).
    pub fn subscribers(&self) -> usize {
        self.tx.receiver_count()
    }
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.tx.subscribe()
    }
}

/// Build a handler future from an async block.
pub fn boxed<F>(f: F) -> BoxFut<RpcResult>
where
    F: Future<Output = RpcResult> + Send + 'static,
{
    Box::pin(f)
}

/// Deserialise params, mapping errors to `invalid`.
pub fn params<T: serde::de::DeserializeOwned>(v: Value) -> std::result::Result<T, RpcError> {
    let v = if v.is_null() {
        Value::Object(Default::default())
    } else {
        v
    };
    serde_json::from_value(v).map_err(|e| RpcError::invalid(e.to_string()))
}

/// Serialise a result value.
pub fn ok<T: Serialize>(v: T) -> RpcResult {
    serde_json::to_value(v).map_err(|e| RpcError::new("internal", e.to_string()))
}

/// Control socket server.
pub struct Server {
    pub name: String,
    pub metrics: Arc<Metrics>,
    pub bus: EventBus,
}

impl Server {
    pub fn new(name: &str, bus: EventBus) -> Self {
        Self {
            name: name.to_string(),
            metrics: Arc::new(Metrics::new()),
            bus,
        }
    }

    /// Bind the socket (replacing a stale one) with mode 0600.
    pub async fn bind(path: &Path) -> Result<UnixListener> {
        if let Some(dir) = path.parent() {
            crate::paths::ensure_private_dir(dir)?;
        }
        if path.exists() {
            if UnixStream::connect(path).await.is_ok() {
                return Err(FlowError::Unavailable(format!(
                    "{} already in use by a running daemon",
                    path.display()
                )));
            }
            std::fs::remove_file(path)?;
        }
        let l = UnixListener::bind(path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        }
        Ok(l)
    }

    /// Accept loop; runs until the task is dropped.
    pub async fn serve(self: Arc<Self>, listener: UnixListener, handler: Arc<dyn Handler>) {
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let me = self.clone();
                    let h = handler.clone();
                    tokio::spawn(async move {
                        if let Err(e) = me.connection(stream, h).await {
                            tracing::debug!("ipc connection ended: {e}");
                        }
                    });
                }
                Err(e) => {
                    tracing::warn!("accept failed: {e}");
                    tokio::time::sleep(Duration::from_millis(200)).await;
                }
            }
        }
    }

    async fn connection(&self, stream: UnixStream, handler: Arc<dyn Handler>) -> Result<()> {
        let (r, mut w) = stream.into_split();
        let mut reader = BufReader::new(r);
        let mut sub: Option<broadcast::Receiver<Event>> = None;
        let mut buf = String::new();
        loop {
            // `buf` keeps partial data across select! cancellations.
            let line_fut = read_line_bounded(&mut reader, &mut buf);
            let n = match sub.as_mut() {
                Some(rx) => {
                    tokio::select! {
                        n = line_fut => n?,
                        ev = rx.recv() => {
                            match ev {
                                Ok(ev) => {
                                    self.metrics.inc(&self.metrics.events);
                                    write_json(&mut w, &ev).await?;
                                }
                                Err(broadcast::error::RecvError::Lagged(_)) => {}
                                Err(_) => sub = None,
                            }
                            continue;
                        }
                    }
                }
                None => line_fut.await?,
            };
            if n == 0 && buf.is_empty() {
                return Ok(());
            }
            if !buf.ends_with('\n') && n > 0 {
                continue; // partial line, keep reading
            }
            let line = std::mem::take(&mut buf);
            if line.trim().is_empty() {
                continue;
            }
            self.metrics.inc(&self.metrics.requests);
            let resp = match serde_json::from_str::<Request>(&line) {
                Err(e) => {
                    self.metrics.inc(&self.metrics.errors);
                    Response {
                        id: 0,
                        result: None,
                        error: Some(RpcError::invalid(e.to_string())),
                    }
                }
                Ok(req) => {
                    let id = req.id;
                    let out = match req.method.as_str() {
                        "ping" => Ok(Value::String("pong".into())),
                        "health" => ok(self.metrics.health(&self.name)),
                        "subscribe" => {
                            sub = Some(self.bus.subscribe());
                            Ok(Value::Bool(true))
                        }
                        _ => handler.call(req.method, req.params).await,
                    };
                    match out {
                        Ok(v) => Response {
                            id,
                            result: Some(v),
                            error: None,
                        },
                        Err(e) => {
                            self.metrics.inc(&self.metrics.errors);
                            Response {
                                id,
                                result: None,
                                error: Some(e),
                            }
                        }
                    }
                }
            };
            write_json(&mut w, &resp).await?;
        }
    }
}

async fn read_line_bounded<R: tokio::io::AsyncBufRead + Unpin>(
    r: &mut R,
    buf: &mut String,
) -> Result<usize> {
    let n = r.take(MAX_LINE as u64).read_line(buf).await?;
    if n >= MAX_LINE && !buf.ends_with('\n') {
        return Err(FlowError::Invalid("line too long".into()));
    }
    Ok(n)
}

async fn write_json<W: tokio::io::AsyncWrite + Unpin, T: Serialize>(
    w: &mut W,
    v: &T,
) -> Result<()> {
    let mut s = serde_json::to_string(v)?;
    s.push('\n');
    w.write_all(s.as_bytes()).await?;
    Ok(())
}

/// Default per-call timeout of the client.
pub const CALL_TIMEOUT: Duration = Duration::from_secs(5);

/// Control socket client.
pub struct Client {
    reader: BufReader<tokio::net::unix::OwnedReadHalf>,
    writer: tokio::net::unix::OwnedWriteHalf,
    next_id: u64,
}

impl Client {
    pub async fn connect(path: &Path) -> Result<Self> {
        let stream = tokio::time::timeout(CALL_TIMEOUT, UnixStream::connect(path))
            .await
            .map_err(|_| FlowError::Timeout)?
            .map_err(|e| FlowError::Unavailable(format!("{}: {e}", path.display())))?;
        let (r, w) = stream.into_split();
        Ok(Self {
            reader: BufReader::new(r),
            writer: w,
            next_id: 1,
        })
    }

    /// One request/response round trip. Events arriving meanwhile are skipped.
    pub async fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        let req = Request {
            id,
            method: method.to_string(),
            params,
        };
        write_json(&mut self.writer, &req).await?;
        let fut = async {
            loop {
                let mut line = String::new();
                if read_line_bounded(&mut self.reader, &mut line).await? == 0 {
                    return Err(FlowError::Unavailable("connection closed".into()));
                }
                if let Ok(resp) = serde_json::from_str::<Response>(&line) {
                    if resp.id == id || resp.id == 0 {
                        return match (resp.result, resp.error) {
                            (_, Some(e)) => Err(FlowError::Remote {
                                code: e.code,
                                message: e.message,
                            }),
                            (Some(v), None) => Ok(v),
                            (None, None) => Ok(Value::Null),
                        };
                    }
                }
            }
        };
        tokio::time::timeout(CALL_TIMEOUT, fut)
            .await
            .map_err(|_| FlowError::Timeout)?
    }

    /// Next pushed event (after calling `subscribe`). No timeout.
    pub async fn next_event(&mut self) -> Result<Event> {
        loop {
            let mut line = String::new();
            if read_line_bounded(&mut self.reader, &mut line).await? == 0 {
                return Err(FlowError::Unavailable("connection closed".into()));
            }
            if let Ok(ev) = serde_json::from_str::<Event>(&line) {
                return Ok(ev);
            }
        }
    }
}

/// Connect to a daemon by name, call once, disconnect.
pub async fn call_once(daemon: &str, method: &str, params: Value) -> Result<Value> {
    let mut c = Client::connect(&crate::paths::socket_path(daemon)).await?;
    c.call(method, params).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sock(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("flow-ipc-{tag}-{}.sock", std::process::id()))
    }

    async fn start(path: &Path) -> EventBus {
        let bus = EventBus::new();
        let server = Arc::new(Server::new("t", bus.clone()));
        let l = Server::bind(path).await.unwrap();
        let h: Arc<dyn Handler> = Arc::new(|m: String, p: Value| {
            boxed(async move {
                match m.as_str() {
                    "echo" => Ok(p),
                    "fail" => Err(RpcError::invalid("nope")),
                    _ => Err(RpcError::unknown_method(&m)),
                }
            })
        });
        tokio::spawn(server.serve(l, h));
        bus
    }

    #[tokio::test]
    async fn roundtrip_and_errors() {
        let p = sock("rt");
        start(&p).await;
        let mut c = Client::connect(&p).await.unwrap();
        assert_eq!(c.call("ping", Value::Null).await.unwrap(), json!("pong"));
        assert_eq!(
            c.call("echo", json!({"a":1})).await.unwrap(),
            json!({"a":1})
        );
        match c.call("fail", Value::Null).await {
            Err(FlowError::Remote { code, .. }) => assert_eq!(code, "invalid"),
            o => panic!("{o:?}"),
        }
        let h = c.call("health", Value::Null).await.unwrap();
        assert_eq!(h["name"], "t");
        assert!(h["errors"].as_u64().unwrap() >= 1);
        std::fs::remove_file(p).ok();
    }

    #[tokio::test]
    async fn events_after_subscribe() {
        let p = sock("ev");
        let bus = start(&p).await;
        let mut c = Client::connect(&p).await.unwrap();
        c.call("subscribe", Value::Null).await.unwrap();
        bus.emit("x.changed", json!({"v":2}));
        let ev = c.next_event().await.unwrap();
        assert_eq!(ev.event, "x.changed");
        assert_eq!(ev.data["v"], 2);
        std::fs::remove_file(p).ok();
    }

    #[tokio::test]
    async fn garbage_line_gets_error_not_crash() {
        let p = sock("g");
        start(&p).await;
        let mut s = UnixStream::connect(&p).await.unwrap();
        s.write_all(b"not json\n").await.unwrap();
        let mut r = BufReader::new(s);
        let mut line = String::new();
        r.read_line(&mut line).await.unwrap();
        assert!(line.contains("invalid"));
        std::fs::remove_file(p).ok();
    }

    #[tokio::test]
    async fn second_bind_refused_stale_replaced() {
        let p = sock("b");
        start(&p).await;
        assert!(Server::bind(&p).await.is_err());
        std::fs::remove_file(&p).ok();
        // stale file (no listener)
        std::fs::write(&p, b"").unwrap();
        assert!(Server::bind(&p).await.is_ok());
        std::fs::remove_file(p).ok();
    }

    #[test]
    fn params_null_is_empty_object() {
        #[derive(Deserialize, Default)]
        struct P {
            #[serde(default)]
            a: u8,
        }
        let p: P = params(Value::Null).unwrap();
        assert_eq!(p.a, 0);
    }
}
