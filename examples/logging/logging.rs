use futuresdr::anyhow::Result;
use futuresdr::blocks::MessageSourceBuilder;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Pmt;
use futuresdr::runtime::Runtime;
use futuresdr::tracing::info;
use futuresdr::tracing::level_filters::LevelFilter;
use std::time;
use tracing_subscriber::filter::EnvFilter;
use tracing_subscriber::fmt;
use tracing_subscriber::prelude::*;

fn main() -> Result<()> {
    let format = fmt::layer()
        .with_level(true)
        .with_target(true)
        .with_thread_ids(true)
        .with_thread_names(true)
        .compact();

    let filter = EnvFilter::from_env("FOO_LOG").add_directive(LevelFilter::DEBUG.into());

    tracing_subscriber::registry()
        .with(filter)
        .with(format)
        .init();

    let mut fg = Flowgraph::new();

    let msg_source = MessageSourceBuilder::new(Pmt::Null, time::Duration::from_millis(100))
        .n_messages(20)
        .build();
    fg.add_block(msg_source);

    let now = time::Instant::now();
    info!("starting flowgraph");
    Runtime::new().run(fg)?;
    let elapsed = now.elapsed();
    info!("flowgraph took {elapsed:?}");

    Ok(())
}
