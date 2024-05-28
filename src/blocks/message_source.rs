use async_io::Timer;
use std::time::Duration;
use web_time::Instant;

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

#[cfg(feature = "telemetry")]
use crate::telemetry::opentelemetry::{trace::TraceContextExt, trace::Tracer, Key, KeyValue};

/// Output the same message periodically.
pub struct MessageSource {
    message: Pmt,
    interval: Duration,
    t_last: Instant,
    n_messages: Option<usize>,
    #[cfg(feature = "telemetry")]
    telemetry_resource: crate::telemetry::TelemetryResource,
}

impl MessageSource {
    /// Create MessageSource block
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
                #[cfg(feature = "telemetry")]
                telemetry_resource: {
                    crate::telemetry::TelemetryResource::new(
                        "MessageSourceTelemetry".to_string(),
                        env!("CARGO_PKG_VERSION").to_lowercase(),
                    )
                },
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
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        // Feature Gating might be difficult for traces, which might open up a big context block
        #[cfg(feature = "telemetry")]
        let (tracer, counter, gauge) = {
            let tracer = self.telemetry_resource.get_tracer();
            let meter = self.telemetry_resource.get_meter();
            let counter = meter
                .u64_counter("u64_counter")
                .with_description("Count Values")
                .with_unit("count")
                .init();
            let gauge = meter
                .f64_gauge("f64_gauge")
                .with_description("Concrete Values")
                .with_unit("f64")
                .init();
            (tracer, counter, gauge)
        };

        if _meta
            .telemetry_config()
            .active_traces()
            .contains("test_trace")
        {
            tracer.in_span("Main operation", |cx| {
                let span = cx.span();
                span.add_event(
                    "Nice operation!".to_string(),
                    vec![Key::new("bogons").i64(100)],
                );
                span.set_attribute(KeyValue::new("another.key", "yes"));

                info!(target: "telemetry-test", "log message inside a span");

                tracer.in_span("Sub operation...", |cx| {
                    let span = cx.span();
                    span.set_attribute(KeyValue::new("another.key", "yes"));
                    span.add_event("Sub span event", vec![]);
                });
            });
        }

        let now = Instant::now();

        info!(target: "telemetry-test", "This should be collected by the opentelemetry-collector");

        if now >= self.t_last + self.interval {
            mio.post(0, self.message.clone()).await;
            self.t_last = now;
            if let Some(ref mut n) = self.n_messages {
                #[cfg(feature = "telemetry")]
                if _meta
                    .telemetry_config()
                    .active_metrics()
                    .contains("message_count")
                {
                    counter.add(1, &[KeyValue::new("type", "message_count")]);
                }

                #[cfg(feature = "telemetry")]
                if _meta
                    .telemetry_config()
                    .active_metrics()
                    .contains("concrete_value")
                {
                    println!("Recoridng Gauge Value {}", (*n as f64));
                    gauge.record(*n as f64, &[KeyValue::new("type", "concrete_value")]);
                }

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
        _meta: &mut BlockMeta,
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
    pub fn build(self) -> Block {
        MessageSource::new(self.message, self.duration, self.n_messages)
    }
}
