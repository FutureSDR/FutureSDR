//! Remote Control through REST API
use axum::Json;
use axum::Router;
use axum::extract::Path;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;
use axum::routing::get_service;
use futures::channel::oneshot;
use std::path;
use std::thread::JoinHandle;
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

use crate::runtime::BlockDescription;
use crate::runtime::BlockId;
use crate::runtime::FlowgraphDescription;
use crate::runtime::FlowgraphId;
use crate::runtime::Pmt;
use crate::runtime::PortId;
use crate::runtime::RuntimeHandle;
use crate::runtime::config;

macro_rules! relative {
    ($path:expr_2021) => {
        if cfg!(windows) {
            concat!(env!("CARGO_MANIFEST_DIR"), "\\", $path)
        } else {
            concat!(env!("CARGO_MANIFEST_DIR"), "/", $path)
        }
    };
}

async fn flowgraphs(State(rt): State<RuntimeHandle>) -> Json<Vec<FlowgraphId>> {
    Json::from(rt.get_flowgraphs())
}

async fn flowgraph_description(
    Path(fg): Path<usize>,
    State(rt): State<RuntimeHandle>,
) -> Result<Json<FlowgraphDescription>, StatusCode> {
    let fg = rt.get_flowgraph(FlowgraphId(fg));
    if let Some(mut fg) = fg {
        if let Ok(d) = fg.description().await {
            return Ok(Json::from(d));
        }
    }
    Err(StatusCode::BAD_REQUEST)
}

async fn block_description(
    Path((fg, blk)): Path<(usize, BlockId)>,
    State(rt): State<RuntimeHandle>,
) -> Result<Json<BlockDescription>, StatusCode> {
    let fg = rt.get_flowgraph(FlowgraphId(fg));
    if let Some(mut fg) = fg {
        if let Ok(d) = fg.block_description(blk).await {
            return Ok(Json::from(d));
        }
    }

    Err(StatusCode::BAD_REQUEST)
}

async fn handler_id(
    Path((fg, blk, handler)): Path<(usize, BlockId, PortId)>,
    State(rt): State<RuntimeHandle>,
) -> Result<Json<Pmt>, StatusCode> {
    let fg = rt.get_flowgraph(FlowgraphId(fg));
    if let Some(mut fg) = fg {
        if let Ok(ret) = fg.callback(blk, handler, Pmt::Null).await {
            return Ok(Json::from(ret));
        }
    }

    Err(StatusCode::BAD_REQUEST)
}

async fn handler_id_post(
    Path((fg, blk, handler)): Path<(usize, BlockId, PortId)>,
    State(rt): State<RuntimeHandle>,
    Json(pmt): Json<Pmt>,
) -> Result<Json<Pmt>, StatusCode> {
    let fg = rt.get_flowgraph(FlowgraphId(fg));
    if let Some(mut fg) = fg {
        if let Ok(ret) = fg.callback(blk, handler, pmt).await {
            return Ok(Json::from(ret));
        }
    }

    Err(StatusCode::BAD_REQUEST)
}

pub struct ControlPort {
    thread: Option<(oneshot::Sender<()>, JoinHandle<()>)>,
    handle: RuntimeHandle,
}

impl ControlPort {
    pub fn new(handle: RuntimeHandle, routes: Router) -> Self {
        let mut cp = ControlPort {
            handle,
            thread: None,
        };
        cp.start(Some(routes));
        cp
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
            Some(ServeDir::new(p))
        } else if path::Path::new(relative!("crates/prophecy/dist")).is_dir() {
            Some(ServeDir::new(relative!("crates/prophecy/dist")))
        } else {
            None
        };

        if let Some(service) = frontend {
            app = app.fallback_service(get_service(service));
        }

        let (tx_shutdown, rx_shutdown) = oneshot::channel::<()>();

        let handle = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();

            runtime.spawn(async move {
                let addr = config::config().ctrlport_bind.unwrap();
                match TcpListener::bind(&addr).await {
                    Ok(listener) => {
                        debug!("Listening on {}", addr);
                        axum::serve(listener, app.into_make_service())
                            .await
                            .unwrap();
                    }
                    _ => {
                        warn!("CtrlPort address {} already in use", addr);
                    }
                }
            });

            runtime.block_on(async move {
                let _ = rx_shutdown.await;
            });
        });

        self.thread = Some((tx_shutdown, handle));
    }
}

impl Drop for ControlPort {
    fn drop(&mut self) {
        if let Some((tx, handle)) = self.thread.take() {
            let _ = tx.send(());
            let _ = handle.join();
        }
    }
}
