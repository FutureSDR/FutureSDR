use crate::prelude::*;

/// Read samples from [ZeroMQ](https://zeromq.org/) socket.
#[derive(Block)]
pub struct SubSource<T, O = DefaultCpuWriter<T>>
where
    T: Send + 'static,
    O: CpuBufferWriter<Item = T>,
{
    #[output]
    output: O,
    address: String,
    receiver: Option<zmq::Socket>,
}

impl<T, O> SubSource<T, O>
where
    T: Send + 'static,
    O: CpuBufferWriter<Item = T>,
{
    /// Create SubSource block
    pub fn new(address: impl Into<String>) -> Self {
        Self {
            output: O::default(),
            address: address.into(),
            receiver: None,
        }
    }
}

#[doc(hidden)]
impl<T, O> Kernel for SubSource<T, O>
where
    T: Send + 'static,
    O: CpuBufferWriter<Item = T>,
{
    async fn work(
        &mut self,
        _io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let o = self.output.slice();
        let ptr = o.as_ptr() as *mut u8;
        let byte_len = std::mem::size_of_val(o);
        let buffer = unsafe { std::slice::from_raw_parts_mut(ptr, byte_len) };

        let n_bytes = self.receiver.as_mut().unwrap().recv_into(buffer, 0)?;
        debug_assert_eq!(o.len() % std::mem::size_of::<T>(), 0);
        let n = n_bytes / std::mem::size_of::<T>();
        debug!("SubSource received {}", n);
        self.output.produce(n);

        Ok(())
    }

    async fn init(&mut self, _mio: &mut MessageOutputs, _meta: &mut BlockMeta) -> Result<()> {
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
pub struct SubSourceBuilder<T, O = DefaultCpuWriter<T>>
where
    T: Send + 'static,
    O: CpuBufferWriter<Item = T>,
{
    address: String,
    _type: std::marker::PhantomData<O>,
}

impl<T, O> SubSourceBuilder<T, O>
where
    T: Send + 'static,
    O: CpuBufferWriter<Item = T>,
{
    /// Create SubSource builder
    pub fn new() -> Self {
        SubSourceBuilder {
            address: "tcp://*:5555".into(),
            _type: std::marker::PhantomData,
        }
    }

    /// Set address
    #[must_use]
    pub fn address(mut self, address: &str) -> Self {
        self.address = address.to_string();
        self
    }

    /// Build ZMQ source
    pub fn build(self) -> SubSource<T, O> {
        SubSource::<T, O>::new(self.address)
    }
}

impl<T, O> Default for SubSourceBuilder<T, O>
where
    T: Send + 'static,
    O: CpuBufferWriter<Item = T>,
{
    fn default() -> Self {
        Self::new()
    }
}
