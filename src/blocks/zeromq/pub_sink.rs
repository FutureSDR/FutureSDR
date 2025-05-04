use crate::prelude::*;

/// Push samples into [ZeroMQ](https://zeromq.org/) socket.
#[derive(Block)]
pub struct PubSink<T, I = circular::Reader<T>>
where
    T: Send + 'static,
    I: CpuBufferReader<Item = T>,
{
    #[input]
    input: I,
    address: String,
    publisher: Option<zmq::Socket>,
    min_item: usize,
}

impl<T, I> PubSink<T, I>
where
    T: Send + 'static,
    I: CpuBufferReader<Item = T>,
{
    /// Create PubSink
    pub fn new(address: impl Into<String>, min_item: usize) -> Self {
        Self {
            input: I::default(),
            address: address.into(),
            publisher: None,
            min_item,
        }
    }
}

#[doc(hidden)]
impl<T, I> Kernel for PubSink<T, I>
where
    T: Send + 'static,
    I: CpuBufferReader<Item = T>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = self.input.slice();

        let n = i.len();
        if n > 0 && n > self.min_item {
            let i = self.input.slice();
            let ptr = i.as_ptr() as *const u8;
            let byte_len = i.len() * std::mem::size_of::<T>();
            let data = unsafe { std::slice::from_raw_parts(ptr, byte_len) };
            self.publisher.as_mut().unwrap().send(data, 0).unwrap();
            self.input.consume(n);
        }

        if self.input.finished() {
            io.finished = true;
        }

        Ok(())
    }

    async fn init(&mut self, _mio: &mut MessageOutputs, _meta: &mut BlockMeta) -> Result<()> {
        let context = zmq::Context::new();
        let publisher = context.socket(zmq::PUB)?;
        info!("SubSource Binding to {:?}", self.address);
        publisher.bind(&self.address)?;
        self.publisher = Some(publisher);

        Ok(())
    }
}

/// Build a ZeroMQ [PubSink].
pub struct PubSinkBuilder<T, I = circular::Reader<T>>
where
    T: Send + 'static,
    I: CpuBufferReader<Item = T>,
{
    address: String,
    _type: std::marker::PhantomData<I>,
    /// Minimum number of items per send
    min_item: usize,
}

impl<T, I> PubSinkBuilder<T, I>
where
    T: Send + 'static,
    I: CpuBufferReader<Item = T>,
{
    /// Create PubSink builder
    pub fn new() -> Self {
        PubSinkBuilder {
            address: "tcp://*:5555".into(),
            _type: std::marker::PhantomData,
            min_item: 1,
        }
    }

    /// Remote socket address
    #[must_use]
    pub fn address(mut self, address: &str) -> Self {
        self.address = address.to_string();
        self
    }

    /// Set minimum number of items in send buffer
    pub fn min_item_per_send(mut self, min_item: usize) -> Self {
        self.min_item = min_item;
        self
    }

    /// Build PubSink
    pub fn build(self) -> PubSink<T, I> {
        PubSink::<T, I>::new(self.address, self.min_item)
    }
}

impl<T, I> Default for PubSinkBuilder<T, I>
where
    T: Send + 'static,
    I: CpuBufferReader<Item = T>,
{
    fn default() -> Self {
        Self::new()
    }
}
