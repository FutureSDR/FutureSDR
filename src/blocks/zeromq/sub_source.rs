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
pub struct SubSource<T: Send + 'static> {
    address: String,
    receiver: Option<zmq::Socket>,
    _type: std::marker::PhantomData<T>,
}

impl<T: Send + 'static> SubSource<T> {
    pub fn new(address: impl Into<String>) -> Block {
        Block::new(
            BlockMetaBuilder::new("SubSource").blocking().build(),
            StreamIoBuilder::new().add_output::<T>("out").build(),
            MessageIoBuilder::new().build(),
            SubSource {
                address: address.into(),
                receiver: None,
                _type: std::marker::PhantomData::<T>,
            },
        )
    }
}

#[doc(hidden)]
#[async_trait]
impl<T: Send + 'static> Kernel for SubSource<T> {
    async fn work(
        &mut self,
        _io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let o = sio.output(0).slice_unchecked::<u8>();
        let n_bytes = self.receiver.as_mut().unwrap().recv_into(o, 0)?;
        debug_assert_eq!(o.len() % std::mem::size_of::<T>(), 0);
        let n = n_bytes / std::mem::size_of::<T>();
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
pub struct SubSourceBuilder<T: Send + 'static> {
    address: String,
    _type: std::marker::PhantomData<T>,
}

impl<T: Send + 'static> SubSourceBuilder<T> {
    pub fn new() -> SubSourceBuilder<T> {
        SubSourceBuilder {
            address: "tcp://*:5555".into(),
            _type: std::marker::PhantomData,
        }
    }

    #[must_use]
    pub fn address(mut self, address: &str) -> SubSourceBuilder<T> {
        self.address = address.to_string();
        self
    }

    pub fn build(self) -> Block {
        SubSource::<T>::new(self.address)
    }
}

impl<T: Send + 'static> Default for SubSourceBuilder<T> {
    fn default() -> Self {
        Self::new()
    }
}
