use anyhow::Result;
use futuresdr::blocks::zenoh::SubSourceBuilder;
use futuresdr::blocks::FileSink;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

fn main() -> Result<()> {
    let mut fg = Flowgraph::new();

    let zenoh_src = fg.add_block(
        SubSourceBuilder::<u8>::new()
            .build(),
    )?;
    let snk = fg.add_block(FileSink::<u8>::new("/tmp/zenoh-log.bin"))?;

    fg.connect_stream(zenoh_src, "out", snk, "in")?;

    Runtime::new().run(fg)?;

    Ok(())
}
