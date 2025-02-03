use async_io::Timer;
use std::time::Duration;
use web_time::Instant;

use crate::runtime::BlockMeta;
use crate::runtime::Kernel;
use crate::runtime::MessageOutputs;
use crate::runtime::Pmt;
use crate::runtime::Result;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::TypedBlock;
use crate::runtime::WorkIo;

/// Output the same message periodically.
#[derive(Block)]
#[message_outputs(out)]
pub struct MessageSource {
    message: Pmt,
    interval: Duration,
    t_last: Instant,
    n_messages: Option<usize>,
}

impl MessageSource {
    /// Create MessageSource block
    pub fn new(message: Pmt, interval: Duration, n_messages: Option<usize>) -> TypedBlock<Self> {
        TypedBlock::new(
            StreamIoBuilder::new().build(),
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
impl Kernel for MessageSource {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _sio: &mut StreamIo,
        mio: &mut MessageOutputs,
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
        _mio: &mut MessageOutputs,
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
pub struct MessageSourceBuilder {
    message: Pmt,
    duration: Duration,
    n_messages: Option<usize>,
}

impl MessageSourceBuilder {
    /// Create MessageSource builder
    pub fn new(message: Pmt, duration: Duration) -> MessageSourceBuilder {
        MessageSourceBuilder {
            message,
            duration,
            n_messages: None,
        }
    }
    /// Number of message to send
    #[must_use]
    pub fn n_messages(mut self, n: usize) -> MessageSourceBuilder {
        self.n_messages = Some(n);
        self
    }
    /// Build Message Source block
    pub fn build(self) -> TypedBlock<MessageSource> {
        MessageSource::new(self.message, self.duration, self.n_messages)
    }
}
