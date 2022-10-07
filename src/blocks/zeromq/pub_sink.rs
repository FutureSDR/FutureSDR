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
pub struct PubSink<T: Send + 'static> {
    address: String,
    publisher: Option<zmq::Socket>,
    _type: std::marker::PhantomData<T>,
}

impl<T: Send + 'static> PubSink<T> {
    pub fn new(address: impl Into<String>) -> Block {
        Block::new(
            BlockMetaBuilder::new("PubSink").blocking().build(),
            StreamIoBuilder::new().add_input::<T>("in").build(),
            MessageIoBuilder::new().build(),
            PubSink {
                address: address.into(),
                publisher: None,
                _type: std::marker::PhantomData::<T>,
            },
        )
    }
}

#[doc(hidden)]
#[async_trait]
impl<T: Send + 'static> Kernel for PubSink<T> {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<T>();

        let n = i.len();
        if n > 0 {
            let i = sio.input(0).slice_unchecked::<u8>();
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
pub struct PubSinkBuilder<T: Send + 'static> {
    address: String,
    _type: std::marker::PhantomData<T>,
}

impl<T: Send + 'static> PubSinkBuilder<T> {
    pub fn new() -> PubSinkBuilder<T> {
        PubSinkBuilder {
            address: "tcp://*:5555".into(),
            _type: std::marker::PhantomData,
        }
    }

    #[must_use]
    pub fn address(mut self, address: &str) -> PubSinkBuilder<T> {
        self.address = address.to_string();
        self
    }

    pub fn build(self) -> Block {
        PubSink::<T>::new(self.address)
    }
}

impl<T: Send + 'static> Default for PubSinkBuilder<T> {
    fn default() -> Self {
        Self::new()
    }
}
