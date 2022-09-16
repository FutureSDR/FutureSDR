use async_io::Timer;
use std::time::Duration;
use std::time::Instant;

use crate::anyhow::Result;
use crate::runtime::Block;
use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::Kernel;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::Pmt;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::WorkIo;

/// Output the same message periodically.
pub struct MessageSource {
    message: Pmt,
    interval: Duration,
    t_last: Instant,
    n_messages: Option<usize>,
}

impl MessageSource {
    pub fn new(message: Pmt, interval: Duration, n_messages: Option<usize>) -> Block {
        Block::new(
            BlockMetaBuilder::new("MessageSource").build(),
            StreamIoBuilder::new().build(),
            MessageIoBuilder::new().add_output("out").build(),
            MessageSource {
                message,
                interval,
                t_last: Instant::now(),
                n_messages,
            },
        )
    }

    async fn sleep(dur: Duration) {
        Timer::after(dur).await;
    }
}

#[doc(hidden)]
#[async_trait]
impl Kernel for MessageSource {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _sio: &mut StreamIo,
        mio: &mut MessageIo<Self>,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        let now = Instant::now();

        if now >= self.t_last + self.interval {
            mio.post(0, self.message.clone()).await;
            self.t_last = now;
            if let Some(ref mut n) = self.n_messages {
                *n -= 1;
                if *n == 0 {
                    io.finished = true;
                }
            }
        }

        io.block_on(MessageSource::sleep(
            self.t_last + self.interval - Instant::now(),
        ));

        Ok(())
    }

    async fn init(
        &mut self,
        _sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        self.t_last = Instant::now();
        Ok(())
    }
}

/// Repeats a fixed message on an interval
///
/// # Inputs
///
/// No inputs.
///
/// # Outputs
///
/// **Message**: `out`: Message output
///
/// # Usage
/// ```
/// use std::time;
/// use futuresdr::blocks::MessageSourceBuilder;
/// use futuresdr::runtime::{Flowgraph, Pmt};
///
/// let mut fg = Flowgraph::new();
///
/// // Repeat the message "foo" every 100ms twenty times
/// let msg_source = fg.add_block(
///     MessageSourceBuilder::new(
///         Pmt::String("foo".to_string()),
///         time::Duration::from_millis(100),
///     )
///     .n_messages(20)
///     .build()
/// );
/// ```
#[cfg_attr(docsrs, doc(cfg(not(target_arch = "wasm32"))))]
pub struct MessageSourceBuilder {
    message: Pmt,
    duration: Duration,
    n_messages: Option<usize>,
}

impl MessageSourceBuilder {
    pub fn new(message: Pmt, duration: Duration) -> MessageSourceBuilder {
        MessageSourceBuilder {
            message,
            duration,
            n_messages: None,
        }
    }

    #[must_use]
    pub fn n_messages(mut self, n: usize) -> MessageSourceBuilder {
        self.n_messages = Some(n);
        self
    }

    pub fn build(self) -> Block {
        MessageSource::new(self.message, self.duration, self.n_messages)
    }
}
