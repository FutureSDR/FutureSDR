// cargo run --example telemetry --features telemetry

use futuresdr::anyhow::Result;
use futuresdr::blocks::Apply;
use futuresdr::blocks::ConsoleSink;
use futuresdr::blocks::Head;

use futuresdr::blocks::SignalSourceBuilder;
use futuresdr::blocks::Throttle;
use futuresdr::macros::connect;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;
use futuresdr::telemetry::opentelemetry::KeyValue;
use num_complex::Complex32;
use num_complex::ComplexFloat;

/* use std::collections::HashSet;
use std::time;
use futuresdr::telemetry::TelemetryConfig;
use futuresdr::blocks::MessageCopy;
use futuresdr::blocks::MessageSink;
use futuresdr::blocks::MessageSourceBuilder;
use futuresdr::runtime::Pmt; */

#[tokio::main]
async fn main() -> Result<()> {
    let (meter_provider, tracer_provider, logger_provider) = futuresdr::telemetry::init_globals(
        "http://localhost:4317".to_string(),
        "http://localhost:4317".to_string(),
        "http://localhost:4317".to_string(),
    );

    let mpp = meter_provider.clone();
    // For HTTP instead of gRPC use the following endpoints
    //    "http://localhost:4318/v1/metrics".to_string(),
    //    "http://localhost:4318/v1/traces".to_string(),
    //    "http://localhost:4318/v1/logs".to_string(),

    let rt = Runtime::new();

    let sample_rate = 50.0;
    let freq = 1.0;
    let items = 200;

    // Telemetry Setup Start
    let meter = futuresdr::telemetry::opentelemetry::global::meter_with_version(
        "StreamSourceTelemetry".to_string(),
        Some(env!("CARGO_PKG_VERSION").to_lowercase()),
        Some("https://opentelemetry.io/schemas/1.17.0"), // Might be dynamically detected from opentelemetry crate
        None,
    );
    let gauge = meter
        .f64_gauge("agc_gauge")
        .with_description("Gauge to measure AGC parameters")
        .with_unit("dB")
        .init();
    // Telemetry Setup End

    gauge.record(0.5, &[KeyValue::new("type", "test")]);
    gauge.record(1.0, &[KeyValue::new("type", "test")]);
    gauge.record(2.0, &[KeyValue::new("type", "test2")]);

    let mut fg1 = Flowgraph::new();

    let src = SignalSourceBuilder::<Complex32>::sin(freq, sample_rate).build();
    let throttle = Throttle::<Complex32>::new(sample_rate as f64);
    let head = Head::<Complex32>::new(items);
    let apply = Apply::<_, Complex32, f32>::new(move |x| {
        let absolute = x.abs();
        gauge.record(x.re() as f64, &[KeyValue::new("type", "re")]);
        gauge.record(x.im() as f64, &[KeyValue::new("type", "im")]);
        gauge.record(absolute as f64, &[KeyValue::new("type", "absolute")]);
        println!("re: {}, im: {}, abs: {}", x.re(), x.im(), absolute);
        // We need a force_flush() here on the meter_provider to record the exact values and dont aggregate them over time.
        // Might have to wait for implementation here: https://github.com/open-telemetry/opentelemetry-specification/issues/617
        let _ = mpp.force_flush(); // Make sure metrics are flushed immediately and not aggregated.
        absolute.into()
    });
    let snk = ConsoleSink::<f32>::new(", ");

    connect!(fg1, src > throttle > head > apply > snk);

    rt.run(fg1)?;

    /* let mut fg2 = Flowgraph::new();
    let msg_source = MessageSourceBuilder::new(
        Pmt::String("foo".to_string()),
        time::Duration::from_millis(1000),
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

    let rt = Runtime::new();
    let (th, mut fgh) = rt.start_sync(fg2);

    let telemetry_config = TelemetryConfig::new(
        HashSet::from(["message_count".to_string(), "concrete_value".to_string()]), // "message_count".to_string(),
        HashSet::from(["test_trace".to_string()]), //"test_trace".to_string()
    );

    // Send telemetry config to MessageSource Block. Config is activated immediately.
    let _ = fgh
        .configure_telemetry(msg_source_block_id, telemetry_config)
        .await;

    let _ = th.await;

    //let elapsed = now.elapsed();
    //println!("flowgraph took {elapsed:?}"); */

    //global::shutdown_tracer_provider();
    tracer_provider.force_flush();
    logger_provider.shutdown()?;
    // Metrics are exported by default every 30 seconds when using stdout exporter,
    // however shutting down the MeterProvider here instantly flushes
    // the metrics, instead of waiting for the 30 sec interval.
    meter_provider.shutdown()?;

    Ok(())
}
