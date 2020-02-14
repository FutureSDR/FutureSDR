use futures::channel::mpsc;
use futures::channel::oneshot;
use futures::prelude::*;
use rocket::fs::{relative, FileServer};
use rocket::serde::json::Json;
use rocket::{config::Shutdown, get, post, routes};
use slab::Slab;
use std::path::Path;

use crate::runtime::config;
use crate::runtime::AsyncMessage;
use crate::runtime::Pmt;

fn routes() -> Vec<rocket::Route> {
    routes![index, handler_id, handler_id_post]
}

#[get("/")]
fn index(boxes: &rocket::State<Slab<Option<mpsc::Sender<AsyncMessage>>>>) -> String {
    format!("number of Blocks {:?}", boxes.len())
}

#[get("/block/<blk>/call/<handler>")]
async fn handler_id(
    blk: usize,
    handler: usize,
    boxes: &rocket::State<Slab<Option<mpsc::Sender<AsyncMessage>>>>,
) -> String {
    let mut b = match boxes.get(blk) {
        Some(Some(s)) => s.clone(),
        _ => return "block not found".to_string(),
    };

    let (tx, rx) = oneshot::channel::<Pmt>();

    b.send(AsyncMessage::Callback {
        port_id: handler,
        data: Pmt::Null,
        tx,
    })
    .await
    .unwrap();

    let ret = rx.await.unwrap();

    format!("{:?}", ret)
}

#[post("/block/<blk>/call/<handler>", data = "<pmt>")]
async fn handler_id_post(
    blk: usize,
    handler: usize,
    pmt: Json<Pmt>,
    boxes: &rocket::State<Slab<Option<mpsc::Sender<AsyncMessage>>>>,
) -> String {
    let mut b = match boxes.get(blk) {
        Some(Some(s)) => s.clone(),
        _ => return "block not found".to_string(),
    };

    let (tx, rx) = oneshot::channel::<Pmt>();

    b.send(AsyncMessage::Callback {
        port_id: handler,
        data: pmt.into_inner(),
        tx,
    })
    .await
    .unwrap();

    let ret = rx.await.unwrap();

    format!("{:?}", ret)
}

pub fn start_control_port(inboxes: Slab<Option<mpsc::Sender<AsyncMessage>>>) {
    if !config::config().ctrlport_enable {
        return;
    }

    let addr = config::config().ctrlport_bind.unwrap();

    let mut config = rocket::config::Config::debug_default();
    config.address = addr.ip();
    config.port = addr.port();
    config.shutdown = Shutdown {
        ctrlc: false,
        force: true,
        ..Default::default()
    };

    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        runtime.block_on(async move {
            let mut r = rocket::custom(config)
                .manage(inboxes)
                .mount("/api/", routes());

            if let Some(ref p) = config::config().frontend_path {
                r = r.mount("/", FileServer::from(p));
            } else if Path::new(relative!("frontend/dist")).is_dir() {
                r = r.mount("/", FileServer::from(relative!("frontend/dist")))
            }

            if let Err(e) = r.launch().await {
                info!("rocket server failed to start");
                info!("{:?}", e);
            }
        });
    });
}
