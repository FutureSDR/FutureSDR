use axum::body::Body;
use axum::extract::State;
use axum::response::Html;
use axum::routing::get;
use axum::Router;
use std::time;

use futuresdr::anyhow::Result;
use futuresdr::blocks::MessageSourceBuilder;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Pmt;
use futuresdr::runtime::Runtime;
use futuresdr::runtime::RuntimeHandle;

fn main() -> Result<()> {
    let mut fg = Flowgraph::new();

    fg.add_block(
        MessageSourceBuilder::new(
            Pmt::String("foo".to_string()),
            time::Duration::from_millis(100),
        )
        .build(),
    );

    let router = Router::<RuntimeHandle, Body>::new()
        .route("/start_fg/", get(start_fg))
        .route("/my_route/", get(my_route));

    println!("Visit http://127.0.0.1:1337/my_route/");
    Runtime::with_custom_routes(router).run(fg)?;

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

async fn start_fg(State(rt): State<RuntimeHandle>) {
    let mut fg = Flowgraph::new();
    fg.add_block(
        MessageSourceBuilder::new(
            Pmt::String("foo".to_string()),
            time::Duration::from_millis(100),
        )
        .n_messages(50)
        .build(),
    );
    let mut handle = rt.start(fg).await;
    dbg!(handle.description().await.unwrap());
}
