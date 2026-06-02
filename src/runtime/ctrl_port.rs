//! Native HTTP control port for flowgraph inspection and message calls.
use axum::Json;
use axum::Router;
use axum::body::Body;
use axum::extract::Path;
use axum::extract::State;
use axum::http::StatusCode;
use axum::http::Uri;
use axum::http::header::CONTENT_TYPE;
use axum::response::IntoResponse;
use axum::response::Response;
use axum::routing::get;
use futures::FutureExt;
use futures::future::FusedFuture;
use futures::select;
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use std::future::Future;
use std::io;
use std::net::SocketAddr;
use std::path::Component;
use std::path::Path as FsPath;
use std::path::PathBuf;
use std::pin::Pin;
use std::slice;
use std::sync::Arc;
use std::task::Context;
use std::task::Poll;
use std::time::Duration;
use std::time::Instant;
use tower_http::cors::CorsLayer;
use tower_service::Service as TowerService;

use crate::runtime::BlockDescription;
use crate::runtime::BlockId;
use crate::runtime::FlowgraphDescription;
use crate::runtime::FlowgraphId;
use crate::runtime::Pmt;
use crate::runtime::PortId;
use crate::runtime::RuntimeHandle;
use crate::runtime::channel::oneshot;
use crate::runtime::config;
use crate::runtime::scheduler::Scheduler;
use crate::runtime::scheduler::Task;

macro_rules! relative {
    ($path:expr_2021) => {
        if cfg!(windows) {
            concat!(env!("CARGO_MANIFEST_DIR"), "\\", $path)
        } else {
            concat!(env!("CARGO_MANIFEST_DIR"), "/", $path)
        }
    };
}

async fn flowgraphs<S: Scheduler + Sync>(
    State(rt): State<RuntimeHandle<S>>,
) -> Json<Vec<FlowgraphId>> {
    Json::from(rt.get_flowgraphs().await)
}

async fn flowgraph_description<S: Scheduler + Sync>(
    Path(fg): Path<usize>,
    State(rt): State<RuntimeHandle<S>>,
) -> Result<Json<FlowgraphDescription>, StatusCode> {
    let Some(fg) = rt.get_flowgraph(FlowgraphId(fg)).await else {
        return Err(StatusCode::NOT_FOUND);
    };

    fg.describe()
        .await
        .map(Json::from)
        .map_err(status_from_error)
}

async fn block_description<S: Scheduler + Sync>(
    Path((fg, blk)): Path<(usize, BlockId)>,
    State(rt): State<RuntimeHandle<S>>,
) -> Result<Json<BlockDescription>, StatusCode> {
    let Some(fg) = rt.get_flowgraph(FlowgraphId(fg)).await else {
        return Err(StatusCode::NOT_FOUND);
    };

    fg.describe_block(blk)
        .await
        .map(Json::from)
        .map_err(status_from_error)
}

async fn handler_id<S: Scheduler + Sync>(
    Path((fg, blk, handler)): Path<(usize, BlockId, PortId)>,
    State(rt): State<RuntimeHandle<S>>,
) -> Result<Json<Pmt>, StatusCode> {
    let Some(fg) = rt.get_flowgraph(FlowgraphId(fg)).await else {
        return Err(StatusCode::NOT_FOUND);
    };

    fg.call(blk, handler, Pmt::Null)
        .await
        .map(Json::from)
        .map_err(status_from_error)
}

async fn handler_id_post<S: Scheduler + Sync>(
    Path((fg, blk, handler)): Path<(usize, BlockId, PortId)>,
    State(rt): State<RuntimeHandle<S>>,
    Json(pmt): Json<Pmt>,
) -> Result<Json<Pmt>, StatusCode> {
    let Some(fg) = rt.get_flowgraph(FlowgraphId(fg)).await else {
        return Err(StatusCode::NOT_FOUND);
    };

    fg.call(blk, handler, pmt)
        .await
        .map(Json::from)
        .map_err(status_from_error)
}

fn status_from_error(error: crate::runtime::Error) -> StatusCode {
    match error {
        crate::runtime::Error::FlowgraphTerminated | crate::runtime::Error::BlockTerminated => {
            StatusCode::GONE
        }
        crate::runtime::Error::InvalidBlock(_)
        | crate::runtime::Error::InvalidMessagePort(_, _)
        | crate::runtime::Error::InvalidStreamPort(_, _)
        | crate::runtime::Error::InvalidParameter => StatusCode::BAD_REQUEST,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

pub struct ControlPort<S> {
    shutdown: Option<oneshot::Sender<()>>,
    task: Option<Task<()>>,
    handle: RuntimeHandle<S>,
}

impl<S: Scheduler + Sync> ControlPort<S> {
    pub fn new(handle: RuntimeHandle<S>, scheduler: S, routes: Router) -> Self {
        let mut cp = ControlPort {
            handle,
            shutdown: None,
            task: None,
        };
        cp.start(scheduler, Some(routes));
        cp
    }

    fn start(&mut self, scheduler: S, custom_routes: Option<Router>) {
        if !config::config().ctrlport_enable {
            return;
        }

        if self.task.is_some() {
            return;
        }

        let mut app = Router::new()
            .route("/api/fg/", get(flowgraphs))
            .route("/api/fg/{fg}/", get(flowgraph_description))
            .route("/api/fg/{fg}/block/{blk}/", get(block_description))
            .route(
                "/api/fg/{fg}/block/{blk}/call/{handler}/",
                get(handler_id).post(handler_id_post),
            )
            .layer(CorsLayer::permissive())
            .with_state(self.handle.clone());

        if let Some(c) = custom_routes {
            app = app.merge(c);
        }

        let frontend = if let Some(ref p) = config::config().frontend_path {
            Some(p.clone())
        } else if FsPath::new(relative!("crates/prophecy/dist")).is_dir() {
            Some(PathBuf::from(relative!("crates/prophecy/dist")))
        } else {
            None
        };

        if let Some(frontend) = frontend {
            let frontend = Arc::new(frontend);
            app = app.fallback(move |uri: Uri| serve_static_file(frontend.clone(), uri));
        }

        let addr = match config::config().ctrlport_bind.parse::<SocketAddr>() {
            Ok(addr) => addr,
            Err(_) => {
                warn!(
                    "failed to parse socket addr {}",
                    config::config().ctrlport_bind
                );
                return;
            }
        };

        let (tx_shutdown, rx_shutdown) = oneshot::channel::<()>();
        let server_scheduler = scheduler.clone();
        let task = scheduler.spawn(async move {
            run_server(addr, app, server_scheduler, rx_shutdown).await;
        });

        self.shutdown = Some(tx_shutdown);
        self.task = Some(task);
    }
}

impl<S> Drop for ControlPort<S> {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }

        if let Some(task) = self.task.take() {
            crate::runtime::block_on(task);
        }
    }
}

async fn run_server<S>(
    addr: SocketAddr,
    app: Router,
    scheduler: S,
    rx_shutdown: oneshot::Receiver<()>,
) where
    S: Scheduler + Sync,
{
    let listener = match async_net::TcpListener::bind(addr).await {
        Ok(listener) => listener,
        Err(e) if e.kind() == io::ErrorKind::AddrInUse => {
            warn!("CtrlPort address {addr} already in use");
            return;
        }
        Err(e) => {
            warn!("failed to bind CtrlPort address {addr}: {e:?}");
            return;
        }
    };

    debug!("Listening on {}", addr);

    let shutdown = rx_shutdown.fuse();
    futures::pin_mut!(shutdown);

    loop {
        let accept = listener.accept().fuse();
        futures::pin_mut!(accept);

        select! {
            _ = shutdown => break,
            accepted = accept => match accepted {
                Ok((stream, remote_addr)) => {
                    if let Err(e) = stream.set_nodelay(true) {
                        trace!("failed to set TCP_NODELAY on {remote_addr}: {e:?}");
                    }

                    let app = app.clone();
                    let task = scheduler.spawn(async move {
                        serve_connection(stream, remote_addr, app).await;
                    });
                    task.detach();
                }
                Err(e) => {
                    warn!("failed to accept CtrlPort connection: {e:?}");
                    async_io::Timer::after(Duration::from_secs(1)).await;
                }
            },
        }

        if shutdown.is_terminated() {
            break;
        }
    }
}

async fn serve_connection(stream: async_net::TcpStream, remote_addr: SocketAddr, app: Router) {
    let service = service_fn(move |req: hyper::Request<Incoming>| {
        let mut app = app.clone();
        async move { TowerService::call(&mut app, req).await }
    });

    let mut builder = http1::Builder::new();
    builder.timer(AsyncIoTimer::new());

    if let Err(e) = builder
        .serve_connection(FuturesIo::new(stream), service)
        .await
    {
        trace!("failed to serve CtrlPort connection {remote_addr}: {e:?}");
    }
}

async fn serve_static_file(frontend: Arc<PathBuf>, uri: Uri) -> Response {
    let Some(relative_path) = sanitize_static_path(uri.path()) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let mut path = frontend.as_ref().clone();
    path.push(relative_path);

    match async_fs::metadata(&path).await {
        Ok(metadata) if metadata.is_dir() => {
            path.push("index.html");
        }
        Ok(_) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return StatusCode::NOT_FOUND.into_response();
        }
        Err(e) => {
            warn!("failed to stat frontend file {}: {e:?}", path.display());
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    }

    match async_fs::read(&path).await {
        Ok(contents) => Response::builder()
            .status(StatusCode::OK)
            .header(CONTENT_TYPE, content_type(&path))
            .body(Body::from(contents))
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            warn!("failed to read frontend file {}: {e:?}", path.display());
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

fn sanitize_static_path(path: &str) -> Option<PathBuf> {
    if path.contains('\\') {
        return None;
    }

    let mut out = PathBuf::new();
    for component in FsPath::new(path.trim_start_matches('/')).components() {
        match component {
            Component::Normal(part) => out.push(part),
            Component::CurDir => {}
            Component::RootDir | Component::ParentDir | Component::Prefix(_) => return None,
        }
    }

    if out.as_os_str().is_empty() {
        out.push("index.html");
    }

    Some(out)
}

fn content_type(path: &FsPath) -> &'static str {
    match path.extension().and_then(|x| x.to_str()) {
        Some("css") => "text/css; charset=utf-8",
        Some("gif") => "image/gif",
        Some("html") => "text/html; charset=utf-8",
        Some("ico") => "image/x-icon",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("js") => "text/javascript; charset=utf-8",
        Some("json") => "application/json",
        Some("png") => "image/png",
        Some("svg") => "image/svg+xml",
        Some("wasm") => "application/wasm",
        Some("webp") => "image/webp",
        _ => "application/octet-stream",
    }
}

/// Adapter from futures-io streams to Hyper's runtime I/O traits.
///
/// This is the small adapter pattern used by smol-hyper, kept locally so the
/// control port can drive Hyper connections on FutureSDR scheduler tasks.
#[derive(Debug, Clone, Copy)]
struct FuturesIo<T> {
    inner: T,
}

impl<T> FuturesIo<T> {
    fn new(inner: T) -> Self {
        Self { inner }
    }
}

impl<T> hyper::rt::Read for FuturesIo<T>
where
    T: futures::io::AsyncRead + Unpin,
{
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        mut buf: hyper::rt::ReadBufCursor<'_>,
    ) -> Poll<io::Result<()>> {
        let raw = unsafe {
            // SAFETY: We immediately initialize the whole unfilled slice below
            // before exposing it as `&mut [u8]` to the futures-io reader.
            buf.as_mut()
        };

        for byte in raw.iter_mut() {
            byte.write(0);
        }

        let initialized = unsafe {
            // SAFETY: Every byte in `raw` was initialized just above, and the
            // slice covers the same allocation and length.
            slice::from_raw_parts_mut(raw.as_mut_ptr().cast::<u8>(), raw.len())
        };

        let n = match Pin::new(&mut self.inner).poll_read(cx, initialized) {
            Poll::Ready(Ok(n)) => n,
            Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
            Poll::Pending => return Poll::Pending,
        };

        unsafe {
            // SAFETY: The reader reported that it initialized exactly `n`
            // bytes in `initialized`, which is backed by `buf`'s unfilled area.
            buf.advance(n);
        }

        Poll::Ready(Ok(()))
    }
}

impl<T> hyper::rt::Write for FuturesIo<T>
where
    T: futures::io::AsyncWrite + Unpin,
{
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_close(cx)
    }

    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write_vectored(cx, bufs)
    }
}

#[derive(Debug, Clone, Default)]
struct AsyncIoTimer;

impl AsyncIoTimer {
    fn new() -> Self {
        Self
    }
}

impl hyper::rt::Timer for AsyncIoTimer {
    fn sleep(&self, duration: Duration) -> Pin<Box<dyn hyper::rt::Sleep>> {
        Box::pin(AsyncIoSleep(async_io::Timer::after(duration)))
    }

    fn sleep_until(&self, at: Instant) -> Pin<Box<dyn hyper::rt::Sleep>> {
        Box::pin(AsyncIoSleep(async_io::Timer::at(at)))
    }

    fn reset(&self, sleep: &mut Pin<Box<dyn hyper::rt::Sleep>>, new_deadline: Instant) {
        *sleep = Box::pin(AsyncIoSleep(async_io::Timer::at(new_deadline)));
    }
}

struct AsyncIoSleep(async_io::Timer);

impl Future for AsyncIoSleep {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match Pin::new(&mut self.0).poll(cx) {
            Poll::Ready(_) => Poll::Ready(()),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl hyper::rt::Sleep for AsyncIoSleep {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_port_serves_axum_router_on_scheduler() {
        use crate::runtime::scheduler::SmolScheduler;
        use futures::io::AsyncReadExt;
        use futures::io::AsyncWriteExt;

        let socket = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let addr = socket.local_addr().unwrap();
        drop(socket);

        let scheduler = SmolScheduler::new(1, false);
        let app = Router::new().route("/health", get(|| async { "ok" }));
        let (tx_shutdown, rx_shutdown) = oneshot::channel();
        let task = scheduler.spawn(run_server(addr, app, scheduler.clone(), rx_shutdown));

        crate::runtime::block_on(async move {
            let deadline = Instant::now() + Duration::from_secs(2);
            let mut stream = loop {
                match async_net::TcpStream::connect(addr).await {
                    Ok(stream) => break stream,
                    Err(e) if Instant::now() < deadline => {
                        trace!("waiting for test control port listener: {e:?}");
                        async_io::Timer::after(Duration::from_millis(10)).await;
                    }
                    Err(e) => panic!("failed to connect to test control port: {e:?}"),
                }
            };

            stream
                .write_all(b"GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
                .await
                .unwrap();

            let mut response = Vec::new();
            stream.read_to_end(&mut response).await.unwrap();
            let response = String::from_utf8(response).unwrap();

            tx_shutdown.send(()).unwrap();
            task.await;

            assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
            assert!(response.ends_with("ok"), "{response}");
        });
    }

    #[test]
    fn static_path_sanitization() {
        assert_eq!(
            sanitize_static_path("/").unwrap(),
            PathBuf::from("index.html")
        );
        assert_eq!(
            sanitize_static_path("/style/app.css").unwrap(),
            PathBuf::from("style/app.css")
        );
        assert!(sanitize_static_path("/../secret").is_none());
        assert!(sanitize_static_path("/nested/../../secret").is_none());
        assert!(sanitize_static_path("/nested\\secret").is_none());
    }

    #[test]
    fn static_content_types() {
        assert_eq!(
            content_type(FsPath::new("index.html")),
            "text/html; charset=utf-8"
        );
        assert_eq!(content_type(FsPath::new("app.wasm")), "application/wasm");
        assert_eq!(
            content_type(FsPath::new("unknown.bin")),
            "application/octet-stream"
        );
    }
}
