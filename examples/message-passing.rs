use std::time;

use futuresdr::anyhow::Result;
use futuresdr::blocks::MessageCopy;
use futuresdr::blocks::MessageSink;
use futuresdr::blocks::MessageSourceBuilder;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Pmt;
use futuresdr::runtime::Runtime;

fn main() -> Result<()> {
    let mut fg = Flowgraph::new();

    let msg_source = MessageSourceBuilder::new(
        Pmt::String("foo".to_string()),
        time::Duration::from_millis(100),
    )
    .n_messages(20)
    .build();
    let msg_copy = MessageCopy::new();
    let msg_sink = MessageSink::new();

    let msg_copy = fg.add_block(msg_copy);
    let msg_source = fg.add_block(msg_source);
    let msg_sink = fg.add_block(msg_sink);

    fg.connect_message(msg_source, "out", msg_copy, "in")?;
    fg.connect_message(msg_copy, "out", msg_sink, "in")?;

    let now = time::Instant::now();
    Runtime::new().run(fg)?;
    let elapsed = now.elapsed();
    println!("flowgraph took {elapsed:?}");

    Ok(())
}
