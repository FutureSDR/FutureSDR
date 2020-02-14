use anyhow::Result;
use env_logger::Builder;
use log::info;
use log::LevelFilter;
use std::time;

use futuresdr::blocks::MessageSourceBuilder;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Pmt;
use futuresdr::runtime::Runtime;

fn main() -> Result<()> {
    let mut builder = Builder::from_default_env();
    builder
        .filter(Some("rocket"), LevelFilter::Off)
        .filter(None, LevelFilter::Info)
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
    info!("flowgraph took {:?}", elapsed);

    Ok(())
}
