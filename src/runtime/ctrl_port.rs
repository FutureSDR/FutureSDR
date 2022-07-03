use axum::extract::{Extension, Path};
use axum::http::StatusCode;
use axum::routing::{get, get_service, post};
use axum::Json;
use axum::Router;
use futures::channel::mpsc;
use futures::channel::oneshot;
use futures::prelude::*;
use slab::Slab;
use std::fmt;
use std::path;
use tower_http::add_extension::AddExtensionLayer;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

use crate::runtime::config;
use crate::runtime::BlockMessage;
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

async fn index(Extension(boxes): Extension<Slab<Option<mpsc::Sender<BlockMessage>>>>) -> String {
    format!("number of Blocks {:?}", boxes.len())
}

async fn handler_id(
    Path((blk, handler)): Path<(usize, usize)>,
    Extension(boxes): Extension<Slab<Option<mpsc::Sender<BlockMessage>>>>,
) -> String {
    let mut b = match boxes.get(blk) {
        Some(Some(s)) => s.clone(),
        _ => return "block not found".to_string(),
    };

    let (tx, rx) = oneshot::channel::<Pmt>();

    b.send(BlockMessage::Callback {
        port_id: handler,
        data: Pmt::Null,
        tx,
    })
    .await
    .unwrap();

    let ret = rx.await.unwrap();

    format!("{:?}", ret)
}

async fn handler_id_post(
    Path((blk, handler)): Path<(usize, usize)>,
    Json(pmt): Json<Pmt>,
    Extension(boxes): Extension<Slab<Option<mpsc::Sender<BlockMessage>>>>,
) -> String {
    let mut b = match boxes.get(blk) {
        Some(Some(s)) => s.clone(),
        _ => return "block not found".to_string(),
    };

    let (tx, rx) = oneshot::channel::<Pmt>();

    b.send(BlockMessage::Callback {
        port_id: handler,
        data: pmt,
        tx,
    })
    .await
    .unwrap();

    let ret = rx.await.unwrap();

    format!("{:?}", ret)
}

pub async fn start_control_port(inboxes: Slab<Option<mpsc::Sender<BlockMessage>>>, svc: Option<Router>) {
    if !config::config().ctrlport_enable {
        return;
    }

    let mut app = Router::new()
        .route("/api/", get(index))
        .route("/api/block/:blk/call/:handler/", get(handler_id))
        .route("/api/block/:blk/call/:handler/", post(handler_id_post))
        .layer(AddExtensionLayer::new(inboxes))
        .layer(CorsLayer::permissive());
    if let Some(svc) = svc {
        app = app.nest("/main", svc);
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
