use futures::FutureExt;
use std::future::Future;
use std::pin::Pin;

use crate::anyhow::Result;
use crate::runtime::Block;
use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::Kernel;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::Pmt;
use crate::runtime::StreamIoBuilder;

/// Forward messages.
pub struct MessageCopy {}

impl MessageCopy {
    pub fn new() -> Block {
        Block::new(
            BlockMetaBuilder::new("MessageCopy").build(),
            StreamIoBuilder::new().build(),
            MessageIoBuilder::new()
                .add_output("out")
                .add_input("in", MessageCopy::handler)
                .build(),
            MessageCopy {},
        )
    }

    fn handler<'a>(
        &'a mut self,
        mio: &'a mut MessageIo<Self>,
        _meta: &'a mut BlockMeta,
        p: Pmt,
    ) -> Pin<Box<dyn Future<Output = Result<Pmt>> + Send + 'a>> {
        async move {
            mio.post(0, p).await;
            Ok(Pmt::Null)
        }
        .boxed()
    }
}

#[doc(hidden)]
#[async_trait]
impl Kernel for MessageCopy {}
