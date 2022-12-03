//! Remote Control through REST API
use axum::extract::{Extension, Path};
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
use tower_http::add_extension::AddExtensionLayer;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

use crate::runtime::config;
use crate::runtime::BlockDescription;
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

async fn flowgraphs(
    Extension(flowgraphs): Extension<Arc<Mutex<Slab<FlowgraphHandle>>>>,
) -> Json<usize> {
    let l = flowgraphs.lock().unwrap().len();
    Json::from(l)
}

async fn flowgraph_description(
    Path(fg): Path<usize>,
    Extension(flowgraphs): Extension<Arc<Mutex<Slab<FlowgraphHandle>>>>,
) -> Result<Json<FlowgraphDescription>, StatusCode> {
    let fg = flowgraphs.lock().unwrap().get(fg).cloned();
    if let Some(mut fg) = fg {
        if let Ok(d) = fg.description().await {
            return Ok(Json::from(d));
        }
    }
    Err(StatusCode::BAD_REQUEST)
}

async fn block_description(
    Path((fg, blk)): Path<(usize, usize)>,
    Extension(flowgraphs): Extension<Arc<Mutex<Slab<FlowgraphHandle>>>>,
) -> Result<Json<BlockDescription>, StatusCode> {
    let fg = flowgraphs.lock().unwrap().get(fg).cloned();
    if let Some(mut fg) = fg {
        if let Ok(d) = fg.block_description(blk).await {
            return Ok(Json::from(d));
        }
    }

    Err(StatusCode::BAD_REQUEST)
}

async fn handler_id(
    Path((fg, blk, handler)): Path<(usize, usize, String)>,
    Extension(flowgraphs): Extension<Arc<Mutex<Slab<FlowgraphHandle>>>>,
) -> Result<Json<Pmt>, StatusCode> {
    let fg = flowgraphs.lock().unwrap().get(fg).cloned();
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
    Json(pmt): Json<Pmt>,
    Extension(flowgraphs): Extension<Arc<Mutex<Slab<FlowgraphHandle>>>>,
) -> Result<Json<Pmt>, StatusCode> {
    let fg = flowgraphs.lock().unwrap().get(fg).cloned();
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

pub struct ControlPort {
    flowgraphs: Arc<Mutex<Slab<FlowgraphHandle>>>,
    thread: Option<JoinHandle<()>>,
}

impl ControlPort {
    pub fn new() -> Self {
        let mut cp = ControlPort {
            flowgraphs: Arc::new(Mutex::new(Slab::new())),
            thread: None,
        };
        cp.start(None);
        cp
    }

    pub fn with_routes(routes: Router) -> Self {
        let mut cp = ControlPort {
            flowgraphs: Arc::new(Mutex::new(Slab::new())),
            thread: None,
        };
        cp.start(Some(routes));
        cp
    }

    pub fn add_flowgraph(&self, handle: FlowgraphHandle) -> usize {
        let mut v = self.flowgraphs.lock().unwrap();
        v.insert(handle)
    }

    fn start(&mut self, custom_routes: Option<Router>) {
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
                    Redirect::permanent(&format!("/api/fg/0/block/{u}"))
                }),
            )
            .layer(AddExtensionLayer::new(self.flowgraphs.clone()))
            .layer(CorsLayer::permissive());

        if let Some(c) = custom_routes {
            app = app.nest("/", c);
        }

        let frontend = if let Some(ref p) = config::config().frontend_path {
            Some(ServeDir::new(p))
        } else if path::Path::new(relative!("frontend/dist")).is_dir() {
            Some(ServeDir::new(relative!("frontend/dist")))
        } else {
            None
        };

        if let Some(service) = frontend {
            app = app.fallback(get_service(service).handle_error(
                |error: std::io::Error| async move {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Unhandled internal error: {error}"),
                    )
                },
            ));
        }

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

impl Default for ControlPort {
    fn default() -> Self {
        Self::new()
    }
}
