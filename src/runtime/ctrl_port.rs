use axum::extract::{Extension, Path};
use axum::http::StatusCode;
use axum::routing::{get, get_service, post};
use axum::Json;
use axum::Router;
use std::path;
use tower_http::add_extension::AddExtensionLayer;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

use crate::runtime::config;
use crate::runtime::BlockDescription;
use crate::runtime::FlowgraphDescription;
use crate::runtime::FlowgraphHandle;
use crate::runtime::Pmt;

macro_rules! relative {
    ($path:expr) => {
        if cfg!(windows) {
            concat!(env!("CARGO_MANIFEST_DIR"), "\\", $path)
        } else {
            concat!(env!("CARGO_MANIFEST_DIR"), "/", $path)
        }
    };
}

async fn index(Extension(mut flowgraph): Extension<FlowgraphHandle>) -> String {
    format!("flowgraph {:?}", flowgraph.description().await.unwrap())
}

async fn flowgraph_description(
    Extension(mut flowgraph): Extension<FlowgraphHandle>,
) -> Result<Json<FlowgraphDescription>, StatusCode> {
    Ok(Json::from(flowgraph.description().await.unwrap()))
}

async fn block_description(
    Path(blk): Path<usize>,
    Extension(mut flowgraph): Extension<FlowgraphHandle>,
) -> Result<Json<BlockDescription>, StatusCode> {
    Ok(Json::from(flowgraph.block_description(blk).await.unwrap()))
}

async fn handler_id(
    Path((blk, handler)): Path<(usize, usize)>,
    Extension(mut flowgraph): Extension<FlowgraphHandle>,
) -> String {
    let ret = flowgraph.callback(blk, handler, Pmt::Null).await.unwrap();
    format!("{:?}", ret)
}

async fn handler_id_post(
    Path((blk, handler)): Path<(usize, usize)>,
    Json(pmt): Json<Pmt>,
    Extension(mut flowgraph): Extension<FlowgraphHandle>,
) -> String {
    let ret = flowgraph.callback(blk, handler, pmt).await.unwrap();
    format!("{:?}", ret)
}

pub async fn start_control_port(flowgraph: FlowgraphHandle, custom_routes: Option<Router>) {
    if !config::config().ctrlport_enable {
        return;
    }

    let mut app = Router::new()
        .route("/api/", get(index))
        .route("/api/fg/", get(flowgraph_description))
        .route("/api/block/:blk/", get(block_description))
        .route("/api/block/:blk/call/:handler/", get(handler_id))
        .route("/api/block/:blk/call/:handler/", post(handler_id_post))
        .layer(AddExtensionLayer::new(flowgraph))
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
        app = app.fallback(
            get_service(service).handle_error(|error: std::io::Error| async move {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Unhandled internal error: {}", error),
                )
            }),
        );
    }

    std::thread::spawn(move || {
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
}
