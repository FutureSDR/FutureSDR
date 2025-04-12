use futuresdr::blocks::Copy;
use futuresdr::blocks::MessageCopy;
use futuresdr::blocks::MessageSink;
use futuresdr::blocks::MessageSourceBuilder;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::VectorSource;
use futuresdr::prelude::*;

fn main() -> anyhow::Result<()> {
    let mut fg = Flowgraph::new();

    let src = VectorSource::<_>::new(vec![0u32, 1, 2, 3]);
    let cpy0 = Copy::<u32>::new();
    let cpy1 = Copy::<u32>::new();
    let cpy2 = Copy::<u32>::new();
    let cpy3 = Copy::<u32>::new();
    let snk = NullSink::<u32>::new();

    // > indicates stream connections
    // default port names (output/input) can be omitted
    // blocks can be chained
    connect!(fg,
             src.output > input.cpy0;
             cpy0 > cpy1;
             cpy1 > input.cpy2.output > cpy3 > snk
    );

    let msg_source = MessageSourceBuilder::new(
        Pmt::String("foo".to_string()),
        std::time::Duration::from_millis(100),
    )
    .n_messages(20)
    .build();
    let msg_copy0 = MessageCopy::new();
    let msg_copy1 = MessageCopy::new();
    let msg_sink = MessageSink::new();
    let handler = Handler::new();

    // | indicates message connections
    connect!(fg,
             msg_source | msg_copy0;
             msg_copy0 | msg_copy1 | msg_sink;
             msg_copy1 | r#in.handler;
    );

    // add a block with no inputs or outputs
    let dummy = Dummy::new();
    connect!(fg, dummy);

    Runtime::new().run(fg)?;

    Ok(())
}

#[derive(Block)]
pub struct Dummy;

impl Dummy {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self
    }
}

impl Kernel for Dummy {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        io.finished = true;
        Ok(())
    }
}

#[derive(Block)]
#[message_inputs(r#in)]
#[null_kernel]
pub struct Handler;

impl Handler {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self
    }

    async fn r#in(
        &mut self,
        _io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
        _p: Pmt,
    ) -> Result<Pmt> {
        Ok(Pmt::Null)
    }
}
