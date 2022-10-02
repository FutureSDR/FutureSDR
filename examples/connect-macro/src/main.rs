use futuresdr::anyhow::Result;
use futuresdr::async_trait::async_trait;
use futuresdr::blocks::Copy;
use futuresdr::blocks::MessageCopy;
use futuresdr::blocks::MessageSink;
use futuresdr::blocks::MessageSourceBuilder;
use futuresdr::blocks::VectorSinkBuilder;
use futuresdr::blocks::VectorSource;
use futuresdr::macros::connect;
use futuresdr::runtime::Block;
use futuresdr::runtime::BlockMeta;
use futuresdr::runtime::BlockMetaBuilder;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Kernel;
use futuresdr::runtime::MessageIo;
use futuresdr::runtime::MessageIoBuilder;
use futuresdr::runtime::Pmt;
use futuresdr::runtime::Runtime;
use futuresdr::runtime::StreamIo;
use futuresdr::runtime::StreamIoBuilder;
use futuresdr::runtime::WorkIo;

fn main() -> Result<()> {
    let mut fg = Flowgraph::new();

    let src = VectorSource::new(vec![0u32, 1, 2, 3]);
    let cpy0 = Copy::<u32>::new();
    let cpy1 = Copy::<u32>::new();
    let cpy2 = Copy::<u32>::new();
    let cpy3 = Copy::<u32>::new();
    let snk = VectorSinkBuilder::<u32>::new().build();

    // > indicates stream connections
    // default port names (out/in) can be omitted
    // blocks can be chained
    connect!(fg,
             src.out > cpy0.in;
             cpy0 > cpy1;
             cpy1 > cpy2 > cpy3 > snk
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

    // | indicates message connections
    connect!(fg,
             msg_source | msg_copy0;
             msg_copy0 | msg_copy1 | msg_sink
    );

    // add a block with no inputs or outputs
    let dummy = Dummy::new();
    connect!(fg, dummy);

    Runtime::new().run(fg)?;

    Ok(())
}

pub struct Dummy;

impl Dummy {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Block {
        Block::new(
            BlockMetaBuilder::new("Dummy").build(),
            StreamIoBuilder::new().build(),
            MessageIoBuilder::new().build(),
            Self,
        )
    }
}

#[async_trait]
impl Kernel for Dummy {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        io.finished = true;

        Ok(())
    }
}
