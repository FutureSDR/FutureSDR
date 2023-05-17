use axum::extract::State;
use axum::response::Html;
use axum::routing::get;
use axum::Router;
use std::sync::{Arc, Mutex};
use std::time;

use futuresdr::anyhow::Result;
use futuresdr::blocks::MessageSourceBuilder;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Pmt;
use futuresdr::runtime::Runtime;
use futuresdr::runtime::RuntimeHandle;

#[derive(Clone)]
struct WebState {
    rt: Arc<Mutex<Option<RuntimeHandle>>>,
}

fn main() -> Result<()> {
    let mut fg = Flowgraph::new();

    fg.add_block(
        MessageSourceBuilder::new(
            Pmt::String("foo".to_string()),
            time::Duration::from_millis(100),
        )
        .build(),
    );

    let state = WebState {
        rt: Arc::new(Mutex::new(None)),
    };
    let router = Router::new()
        .route("/start_fg/", get(start_fg))
        .route("/my_route/", get(my_route))
        .with_state(state.clone());

    let rt = Runtime::with_custom_routes(router);
    let handle = rt.handle();
    *state.rt.lock().unwrap() = Some(handle);

    println!("Visit http://127.0.0.1:1337/my_route/");
    rt.run(fg)?;

    Ok(())
}

async fn my_route() -> Html<&'static str> {
    Html(
        r#"
    <html>
        <head>
            <meta charset='utf-8' />
            <title>FutureSDR</title>
        </head>
        <body>
            <h1>My Custom Route</h1>
        </body>
    </html>
    "#,
    )
}

async fn start_fg(State(ws): State<WebState>) {
    let mut fg = Flowgraph::new();
    fg.add_block(
        MessageSourceBuilder::new(
            Pmt::String("foo".to_string()),
            time::Duration::from_millis(100),
        )
        .n_messages(50)
        .build(),
    );
    let rt_handle = ws.rt.lock().unwrap().as_ref().unwrap().clone();
    let mut fg_handle = rt_handle.start(fg).await;
    dbg!(fg_handle.description().await.unwrap());
}
