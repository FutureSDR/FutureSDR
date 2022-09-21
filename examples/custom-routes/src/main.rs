use axum::response::Html;
use axum::routing::get;
use axum::Router;
use std::time;

use futuresdr::anyhow::Result;
use futuresdr::blocks::MessageSourceBuilder;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Pmt;
use futuresdr::runtime::Runtime;

fn main() -> Result<()> {
    let mut fg = Flowgraph::new();

    fg.add_block(
        MessageSourceBuilder::new(
            Pmt::String("foo".to_string()),
            time::Duration::from_millis(100),
        )
        .build(),
    );

    let router = Router::new().route("/my_route/", get(my_route));
    fg.set_custom_routes(router);

    println!("Visit http://127.0.0.1:1337/my_route/");
    Runtime::new().run(fg)?;

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
