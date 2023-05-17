//! Remote Control through REST API
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{StatusCode, Uri};
use axum::response::Redirect;
use axum::routing::{any, get, get_service};
use axum::Json;
use axum::Router;
use slab::Slab;
use std::path;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread::JoinHandle;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

use crate::runtime::config;
use crate::runtime::scheduler::Scheduler;
use crate::runtime::BlockDescription;
use crate::runtime::Flowgraph;
use crate::runtime::FlowgraphDescription;
use crate::runtime::FlowgraphHandle;
use crate::runtime::Pmt;
use crate::runtime::PortId;

macro_rules! relative {
    ($path:expr) => {
        if cfg!(windows) {
            concat!(env!("CARGO_MANIFEST_DIR"), "\\", $path)
        } else {
            concat!(env!("CARGO_MANIFEST_DIR"), "/", $path)
        }
    };
}

async fn flowgraphs(State(rt): State<RuntimeHandle>) -> Json<Vec<usize>> {
    Json::from(rt.get_flowgraphs())
}

async fn flowgraph_description(
    Path(fg): Path<usize>,
    State(rt): State<RuntimeHandle>,
) -> Result<Json<FlowgraphDescription>, StatusCode> {
    let fg = rt.get_flowgraph(fg);
    if let Some(mut fg) = fg {
        if let Ok(d) = fg.description().await {
            return Ok(Json::from(d));
        }
    }
    Err(StatusCode::BAD_REQUEST)
}

async fn block_description(
    Path((fg, blk)): Path<(usize, usize)>,
    State(rt): State<RuntimeHandle>,
) -> Result<Json<BlockDescription>, StatusCode> {
    let fg = rt.get_flowgraph(fg);
    if let Some(mut fg) = fg {
        if let Ok(d) = fg.block_description(blk).await {
            return Ok(Json::from(d));
        }
    }

    Err(StatusCode::BAD_REQUEST)
}

async fn handler_id(
    Path((fg, blk, handler)): Path<(usize, usize, String)>,
    State(rt): State<RuntimeHandle>,
) -> Result<Json<Pmt>, StatusCode> {
    let fg = rt.get_flowgraph(fg);
    let handler = match handler.parse::<usize>() {
        Ok(i) => PortId::Index(i),
        Err(_) => PortId::Name(handler),
    };
    if let Some(mut fg) = fg {
        if let Ok(ret) = fg.callback(blk, handler, Pmt::Null).await {
            return Ok(Json::from(ret));
        }
    }

    Err(StatusCode::BAD_REQUEST)
}

async fn handler_id_post(
    Path((fg, blk, handler)): Path<(usize, usize, String)>,
    State(rt): State<RuntimeHandle>,
    Json(pmt): Json<Pmt>,
) -> Result<Json<Pmt>, StatusCode> {
    let fg = rt.get_flowgraph(fg);
    let handler = match handler.parse::<usize>() {
        Ok(i) => PortId::Index(i),
        Err(_) => PortId::Name(handler),
    };
    if let Some(mut fg) = fg {
        if let Ok(ret) = fg.callback(blk, handler, pmt).await {
            return Ok(Json::from(ret));
        }
    }

    Err(StatusCode::BAD_REQUEST)
}

#[async_trait]
trait Spawn {
    async fn start(&self, fg: Flowgraph) -> FlowgraphHandle;
}

#[async_trait]
impl<S: Scheduler + Sync + 'static> Spawn for S {
    async fn start(&self, fg: Flowgraph) -> FlowgraphHandle {
        use crate::runtime::runtime::run_flowgraph;
        use crate::runtime::FlowgraphMessage;
        use futures::channel::mpsc::channel;
        use futures::channel::oneshot;

        let queue_size = config::config().queue_size;
        let (fg_inbox, fg_inbox_rx) = channel::<FlowgraphMessage>(queue_size);

        let (tx, rx) = oneshot::channel::<()>();
        self.spawn(run_flowgraph(
            fg,
            self.clone(),
            fg_inbox.clone(),
            fg_inbox_rx,
            tx,
        ))
        .detach();
        rx.await
            .expect("run_flowgraph did not signal startup completed");
        FlowgraphHandle::new(fg_inbox)
    }
}

/// Runtime handle added as state to web handlers
#[derive(Clone)]
pub struct RuntimeHandle {
    scheduler: Arc<dyn Spawn + Send + Sync + 'static>,
    flowgraphs: Arc<Mutex<Slab<FlowgraphHandle>>>,
}

impl RuntimeHandle {
    /// Start a [`Flowgraph`] on the runtime
    pub async fn start(&self, fg: Flowgraph) -> FlowgraphHandle {
        let handle = self.scheduler.start(fg).await;

        self.add_flowgraph(handle.clone());
        handle
    }

    /// Add a [`FlowgraphHandle`] to make it available to web handlers
    pub fn add_flowgraph(&self, handle: FlowgraphHandle) -> usize {
        let mut v = self.flowgraphs.lock().unwrap();
        v.insert(handle)
    }

    /// Get handle to a running flowgraph
    pub fn get_flowgraph(&self, id: usize) -> Option<FlowgraphHandle> {
        self.flowgraphs.lock().unwrap().get(id).cloned()
    }

    /// Get list of flowgraph IDs
    pub fn get_flowgraphs(&self) -> Vec<usize> {
        self.flowgraphs
            .lock()
            .unwrap()
            .iter()
            .map(|x| x.0)
            .collect()
    }
}

pub struct ControlPort {
    thread: Option<JoinHandle<()>>,
    handle: RuntimeHandle,
}

impl ControlPort {
    pub fn new<S: Scheduler + Send + Sync + 'static>(scheduler: S) -> Self {
        Self::with_routes(scheduler, Router::<RuntimeHandle, Body>::new())
    }

    pub fn with_routes<S: Scheduler + Send + Sync + 'static>(
        scheduler: S,
        routes: Router<RuntimeHandle, Body>,
    ) -> Self {
        let handle = RuntimeHandle {
            flowgraphs: Arc::new(Mutex::new(Slab::new())),
            scheduler: Arc::new(scheduler),
        };
        let mut cp = ControlPort {
            handle,
            thread: None,
        };
        cp.start(Some(routes));
        cp
    }

    pub fn add_flowgraph(&self, fg_handle: FlowgraphHandle) -> usize {
        self.handle.add_flowgraph(fg_handle)
    }

    fn start(&mut self, custom_routes: Option<Router<RuntimeHandle, Body>>) {
        if !config::config().ctrlport_enable {
            return;
        }

        if self.thread.is_some() {
            return;
        }

        let mut app = Router::new()
            .route("/api/fg/", get(flowgraphs))
            .route("/api/fg/:fg/", get(flowgraph_description))
            .route("/api/fg/:fg/block/:blk/", get(block_description))
            .route(
                "/api/fg/:fg/block/:blk/call/:handler/",
                get(handler_id).post(handler_id_post),
            )
            .route(
                "/api/block/*foo",
                any(|uri: Uri| async move {
                    let u = uri.to_string().split_off(11);
                    Redirect::permanent(&format!("/api/fg/0/block/{u}/"))
                }),
            )
            .layer(CorsLayer::permissive());

        if let Some(c) = custom_routes {
            app = app.nest("/", c);
        }

        let frontend = if let Some(ref p) = config::config().frontend_path {
            Some(ServeDir::new(p))
        } else if path::Path::new(relative!("crates/frontend/dist")).is_dir() {
            Some(ServeDir::new(relative!("crates/frontend/dist")))
        } else {
            None
        };

        if let Some(service) = frontend {
            app = app.fallback_service(get_service(service));
        }
        let app = app.with_state(self.handle.clone());

        let handle = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();

            runtime.block_on(async move {
                let addr = config::config().ctrlport_bind.unwrap();
                if let Ok(s) = axum::Server::try_bind(&addr) {
                    debug!("Listening on {}", addr);
                    s.serve(app.into_make_service()).await.unwrap();
                } else {
                    warn!("CtrlPort address {} already in use", addr);
                }
            });
        });

        self.thread = Some(handle);
    }
}
