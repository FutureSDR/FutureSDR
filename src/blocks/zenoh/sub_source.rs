use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::Kernel;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::Result;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::TypedBlock;
use crate::runtime::WorkIo;

/// Read samples from [Zenoh](https://zenoh.io/) socket.
pub struct SubSource<T: Send + 'static> {
    config: zenoh::Config,
    key_expression: String,
    session: Option<zenoh::Session>,
    subscriber: Option<zenoh::pubsub::Subscriber<zenoh::handlers::FifoChannelHandler<zenoh::sample::Sample>>>,
    _type: std::marker::PhantomData<T>,
}

impl<T: Send + 'static> SubSource<T> {
    /// Create SubSource block
    pub fn new(config: zenoh::Config, key_expression: impl Into<String>) -> TypedBlock<Self> {
        TypedBlock::new(
            BlockMetaBuilder::new("SubSource").blocking().build(),
            StreamIoBuilder::new().add_output::<T>("out").build(),
            MessageIoBuilder::new().build(),
            SubSource {
                config: config,
                key_expression: key_expression.into(),
                session: None,
                subscriber: None,
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
        _work_io: &mut WorkIo,
        stream_io: &mut StreamIo,
        _message_io: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let output = stream_io.output(0).slice_unchecked::<u8>();

        if let Ok(sample) = self.subscriber.as_mut().unwrap().recv_async().await {
            let payload = sample.payload().to_bytes();

            let output_len = std::cmp::min(output.len(), payload.as_ref().len());

            output[..output_len].copy_from_slice(&payload.as_ref()[..output_len]);

            debug!("SubSource received {}", output_len);

            stream_io.output(0).produce(output_len);
        }

        Ok(())
    }

    async fn init(
        &mut self,
        _stream_io: &mut StreamIo,
        _message_io: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        debug!("SubSource init");

        let session = zenoh::open(self.config.clone()).await.unwrap();
        let subscriber = session.declare_subscriber(self.key_expression.clone()).await.unwrap();

        info!("SubSource declared for {:?}", self.key_expression);

        self.session = Some(session);
        self.subscriber = Some(subscriber);

        Ok(())
    }
}

/// Build a Zenoh [SubSource].
pub struct SubSourceBuilder<T: Send + 'static> {
    config: zenoh::Config,
    key_expression: String,
    _type: std::marker::PhantomData<T>,
}

impl<T: Send + 'static> SubSourceBuilder<T> {
    /// Create SubSource builder
    pub fn new() -> SubSourceBuilder<T> {
        SubSourceBuilder {
            config: zenoh::Config::default(),
            key_expression: "future-sdr/*".into(),
            _type: std::marker::PhantomData,
        }
    }

    /// Zenoh configuration
    #[must_use]
    pub fn config(mut self, config: zenoh::Config) -> SubSourceBuilder<T> {
        self.config = config;
        self
    }

    /// Zenoh key expression
    #[must_use]
    pub fn key_expression(mut self, key_expression: &str) -> SubSourceBuilder<T> {
        self.key_expression = key_expression.to_string();
        self
    }

    /// Build SubSource
    pub fn build(self) -> TypedBlock<SubSource<T>> {
        SubSource::<T>::new(self.config, self.key_expression)
    }
}

impl<T: Send + 'static> Default for SubSourceBuilder<T> {
    fn default() -> Self {
        Self::new()
    }
}
