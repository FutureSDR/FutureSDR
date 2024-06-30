// cargo run --example telemetry --features telemetry

use futuresdr::anyhow::Result;
use futuresdr::blocks::Apply;
use futuresdr::blocks::ConsoleSink;
use futuresdr::blocks::Head;

use futuresdr::blocks::MessageCopy;
use futuresdr::blocks::MessageSink;
use futuresdr::blocks::MessageSourceBuilder;
use futuresdr::blocks::SignalSourceBuilder;
use futuresdr::blocks::Throttle;
use futuresdr::log::debug;
use futuresdr::macros::connect;
use futuresdr::num_complex::Complex32;
use futuresdr::num_complex::ComplexFloat;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Pmt;
use futuresdr::runtime::Runtime;
use futuresdr::runtime::LOGGER_PROVIDER;
use futuresdr::runtime::METER_PROVIDER;
use futuresdr::runtime::TRACER_PROVIDER;
use futuresdr::telemetry::TelemetryConfig;
use std::collections::HashSet;
// use std::time;
use {
    futuresdr::telemetry::opentelemetry::global,
    futuresdr::telemetry::opentelemetry::metrics::Gauge,
    futuresdr::telemetry::opentelemetry::KeyValue, std::sync::LazyLock,
};

static GAUGE: LazyLock<Gauge<f64>> = LazyLock::new(|| {
    global::meter("METER")
        .f64_gauge("f64_gauge")
        .with_description("Gauge to measure concrete values")
        .with_unit("level")
        .init()
});

// static TRACER: LazyLock<Tracer<Span = _>> =
//     LazyLock::new(|| global::tracer_provider().tracer_builder("basic").build());

#[tokio::main]
async fn main() -> Result<()> {
    let rt = Runtime::new();

    let sample_rate = 50.0;
    let freq = 1.0;
    let items = 200;

    let mut fg1 = Flowgraph::new();

    let src = SignalSourceBuilder::<Complex32>::sin(freq, sample_rate).build();
    let throttle = Throttle::<Complex32>::new(sample_rate as f64);
    let head = Head::<Complex32>::new(items);
    let apply = Apply::<_, Complex32, f32>::new(move |x| {
        let absolute = x.abs();
        GAUGE.record(x.re() as f64, &[KeyValue::new("type", "re")]);
        GAUGE.record(x.im() as f64, &[KeyValue::new("type", "im")]);
        GAUGE.record(absolute as f64, &[KeyValue::new("type", "absolute")]);
        debug!("re: {}, im: {}, abs: {}", x.re(), x.im(), absolute);
        // We need a force_flush() here on the meter_provider to record the exact values and dont aggregate them over time.
        // Might have to wait for implementation here: https://github.com/open-telemetry/opentelemetry-specification/issues/617
        // Make sure metrics are flushed immediately and not aggregated.
        let _ = futuresdr::runtime::METER_PROVIDER.force_flush();
        absolute.into()
    });
    let snk = ConsoleSink::<f32>::new(", ");

    connect!(fg1, src > throttle > head > apply > snk);

    rt.run(fg1)?;

    // Second part of the example!
    let mut fg2 = Flowgraph::new();
    let msg_source = MessageSourceBuilder::new(
        Pmt::String("foo".to_string()),
        tokio::time::Duration::from_millis(100),
    )
    .n_messages(200)
    .build();
    let msg_copy = MessageCopy::new();
    let msg_sink = MessageSink::new();

    let msg_copy_block_id = fg2.add_block(msg_copy);
    let msg_source_block_id = fg2.add_block(msg_source);
    let msg_sink_block_id = fg2.add_block(msg_sink);

    fg2.connect_message(msg_source_block_id, "out", msg_copy_block_id, "in")?;
    fg2.connect_message(msg_copy_block_id, "out", msg_sink_block_id, "in")?;

    // let now = time::Instant::now();

    let (th, mut fgh) = rt.start_sync(fg2);

    let telemetry_config = TelemetryConfig::new(
        HashSet::from(["message_count".to_string(), "concrete_value".to_string()]),
        HashSet::from(["test_trace".to_string()]),
    );

    // Send telemetry config to MessageSource Block. Config is activated immediately.
    let _ = fgh
        .configure_telemetry(msg_source_block_id, telemetry_config)
        .await;

    let _ = th.await;

    //let elapsed = now.elapsed();
    //println!("flowgraph took {elapsed:?}");

    TRACER_PROVIDER.shutdown()?;
    LOGGER_PROVIDER.shutdown()?;
    // Metrics are exported by default every 30 seconds when using stdout exporter,
    // however shutting down the MeterProvider here instantly flushes
    // the metrics, instead of waiting for the 30 sec interval.
    METER_PROVIDER.shutdown()?;

    Ok(())
}
