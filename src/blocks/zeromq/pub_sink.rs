use crate::anyhow::Result;
use crate::runtime::Block;
use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::Kernel;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::WorkIo;

/// Push samples into [ZeroMQ](https://zeromq.org/) socket.
pub struct PubSink {
    item_size: usize,
    address: String,
    publisher: Option<zmq::Socket>,
}

impl PubSink {
    pub fn new(item_size: usize, address: impl Into<String>) -> Block {
        Block::new(
            BlockMetaBuilder::new("PubSink").blocking().build(),
            StreamIoBuilder::new().add_input("in", item_size).build(),
            MessageIoBuilder::new().build(),
            PubSink {
                item_size,
                address: address.into(),
                publisher: None,
            },
        )
    }
}

#[doc(hidden)]
#[async_trait]
impl Kernel for PubSink {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<u8>();
        debug_assert_eq!(i.len() % self.item_size, 0);

        let n = i.len() / self.item_size;
        if n > 0 {
            self.publisher.as_mut().unwrap().send(i, 0).unwrap();
            sio.input(0).consume(n);
        }

        if sio.input(0).finished() {
            io.finished = true;
        }

        Ok(())
    }

    async fn init(
        &mut self,
        _sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let context = zmq::Context::new();
        let publisher = context.socket(zmq::PUB)?;
        info!("SubSource Binding to {:?}", self.address);
        publisher.bind(&self.address)?;
        self.publisher = Some(publisher);

        Ok(())
    }
}

/// Build a ZeroMQ [PubSink].
pub struct PubSinkBuilder {
    item_size: usize,
    address: String,
}

impl PubSinkBuilder {
    pub fn new(item_size: usize) -> PubSinkBuilder {
        PubSinkBuilder {
            item_size,
            address: "tcp://*:5555".into(),
        }
    }

    #[must_use]
    pub fn address(mut self, address: &str) -> PubSinkBuilder {
        self.address = address.to_string();
        self
    }

    pub fn build(self) -> Block {
        PubSink::new(self.item_size, self.address)
    }
}
