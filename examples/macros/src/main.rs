use futuresdr::anyhow::Result;
use futuresdr::async_trait::async_trait;
use futuresdr::blocks::Copy;
use futuresdr::blocks::MessageCopy;
use futuresdr::blocks::MessageSink;
use futuresdr::blocks::MessageSourceBuilder;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::VectorSource;
use futuresdr::macros::connect;
use futuresdr::macros::message_handler;
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
    let snk = NullSink::<u32>::new();

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

    // add a block with space in the port name
    let strange = Strange::new();
    let snk = NullSink::<u8>::new();
    connect!(fg,
             strange."foo bar" > snk);

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

pub struct Strange;

impl Strange {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Block {
        Block::new(
            BlockMetaBuilder::new("Strange").build(),
            StreamIoBuilder::new().add_output("foo bar", 1).build(),
            MessageIoBuilder::new().build(),
            Self,
        )
    }
}

#[async_trait]
impl Kernel for Strange {
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

pub struct Handler;

impl Handler {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Block {
        Block::new(
            BlockMetaBuilder::new("Handler").build(),
            StreamIoBuilder::new().build(),
            MessageIoBuilder::new()
                .add_input("handler", Self::my_handler)
                .add_input("other", Self::my_other_handler)
                .build(),
            Self,
        )
    }

    #[message_handler]
    async fn my_handler(
        &mut self,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
        _p: Pmt,
    ) -> Result<Pmt> {
        println!("asdf");
        Ok(Pmt::Null)
    }

    #[message_handler]
    async fn my_other_handler(
        &mut self,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
        _p: Pmt,
    ) -> Result<Pmt> {
        Ok(Pmt::U32(0))
    }
}

#[async_trait]
impl Kernel for Handler {
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
