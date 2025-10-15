use anyhow::Result;
use futuresdr::blocks::MessageCopy;
use futuresdr::blocks::MessageSink;
use futuresdr::blocks::MessageSourceBuilder;
use futuresdr::prelude::*;

fn main() -> Result<()> {
    let mut fg = Flowgraph::new();

    let msg_source = MessageSourceBuilder::new(
        Pmt::String("foo".to_string()),
        std::time::Duration::from_millis(100),
    )
    .n_messages(20)
    .build();
    let msg_copy = MessageCopy;
    let msg_sink = MessageSink::new();

    connect!(fg, msg_source | msg_copy | msg_sink);

    Runtime::new().run(fg)?;
    Ok(())
}
