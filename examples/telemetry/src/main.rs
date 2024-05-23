// cargo run --example telemetry --features telemetry

use std::collections::HashSet;
use std::time;

use futuresdr::anyhow::Result;
use futuresdr::blocks::MessageCopy;
use futuresdr::blocks::MessageSink;
use futuresdr::blocks::MessageSourceBuilder;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Pmt;
use futuresdr::runtime::Runtime;

use futuresdr::telemetry::TelemetryConfig;

#[tokio::main]
async fn main() -> Result<()> {
    let (meter_provider, tracer_provider, logger_provider) = futuresdr::telemetry::init_globals(
        "http://localhost:4317".to_string(),
        "http://localhost:4317".to_string(),
        "http://localhost:4317".to_string(),
    );

    //    "http://localhost:4318/v1/metrics".to_string(),
    //    "http://localhost:4318/v1/traces".to_string(),
    //    "http://localhost:4318/v1/logs".to_string(),

    // Configure Flowgraph
    let mut fg = Flowgraph::new();

    let msg_source = MessageSourceBuilder::new(
        Pmt::String("foo".to_string()),
        time::Duration::from_millis(100),
    )
    .n_messages(20)
    .build();
    let msg_copy = MessageCopy::new();
    let msg_sink = MessageSink::new();

    let msg_copy_block_id = fg.add_block(msg_copy);
    let msg_source_block_id = fg.add_block(msg_source);
    let msg_sink_block_id = fg.add_block(msg_sink);

    fg.connect_message(msg_source_block_id, "out", msg_copy_block_id, "in")?;
    fg.connect_message(msg_copy_block_id, "out", msg_sink_block_id, "in")?;

    let now = time::Instant::now();
    let rt = Runtime::new();
    let (th, mut fgh) = rt.start_sync(fg);

    // Enable Telemetry after one second of waiting
    // tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    let telemetry_config = TelemetryConfig::new(
        HashSet::from(["message_count".to_string()]),
        HashSet::from(["test_trace".to_string()]),
    );
    // Send telemetry config to MessageSource Block. Config is activated immediately.
    let _ = fgh
        .configure_telemetry(msg_source_block_id, telemetry_config)
        .await;

    let _ = th.await;

    let elapsed = now.elapsed();
    println!("flowgraph took {elapsed:?}");

    //global::shutdown_tracer_provider();
    tracer_provider.force_flush();
    logger_provider.shutdown()?;
    // Metrics are exported by default every 30 seconds when using stdout exporter,
    // however shutting down the MeterProvider here instantly flushes
    // the metrics, instead of waiting for the 30 sec interval.
    meter_provider.shutdown()?;

    Ok(())
}
