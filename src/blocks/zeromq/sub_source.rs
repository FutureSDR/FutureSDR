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

/// Read samples from [ZeroMQ](https://zeromq.org/) socket.
pub struct SubSource {
    item_size: usize,
    address: String,
    receiver: Option<zmq::Socket>,
}

impl SubSource {
    pub fn new(item_size: usize, address: impl Into<String>) -> Block {
        Block::new(
            BlockMetaBuilder::new("SubSource").blocking().build(),
            StreamIoBuilder::new().add_output("out", item_size).build(),
            MessageIoBuilder::new().build(),
            SubSource {
                item_size,
                address: address.into(),
                receiver: None,
            },
        )
    }
}

#[doc(hidden)]
#[async_trait]
impl Kernel for SubSource {
    async fn work(
        &mut self,
        _io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let o = sio.output(0).slice::<u8>();
        let mut n_bytes = self.receiver.as_mut().unwrap().recv_into(o, 0)?;
        n_bytes = std::cmp::min(n_bytes, o.len());
        debug_assert_eq!(o.len() % self.item_size, 0);
        let n = n_bytes / self.item_size;
        debug!("SubSource received {}", n);
        sio.output(0).produce(n);

        Ok(())
    }

    async fn init(
        &mut self,
        _sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        debug!("SubSource Init");

        let context = zmq::Context::new();
        let receiver = context.socket(zmq::SUB).unwrap();
        info!("SubSource Connecting to {:?}", self.address);
        receiver.connect(&self.address)?;
        receiver.set_subscribe(b"")?;
        self.receiver = Some(receiver);
        Ok(())
    }
}

/// Build a ZeroMQ [SubSource].
pub struct SubSourceBuilder {
    item_size: usize,
    address: String,
}

impl SubSourceBuilder {
    pub fn new(item_size: usize) -> SubSourceBuilder {
        SubSourceBuilder {
            item_size,
            address: "tcp://*:5555".into(),
        }
    }

    #[must_use]
    pub fn address(mut self, address: &str) -> SubSourceBuilder {
        self.address = address.to_string();
        self
    }

    pub fn build(self) -> Block {
        SubSource::new(self.item_size, self.address)
    }
}
