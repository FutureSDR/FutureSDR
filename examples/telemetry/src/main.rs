// cargo run --example telemetry --features telemetry

use std::collections::HashSet;
use std::time;

use futuresdr::anyhow::Result;
use futuresdr::blocks::MessageCopy;
use futuresdr::blocks::MessageSink;
use futuresdr::blocks::MessageSourceBuilder;
use futuresdr::log::{self, info, Level};
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Pmt;
use futuresdr::runtime::Runtime;

use futuresdr::telemetry::{
    opentelemetry::global, opentelemetry_appender_log::OpenTelemetryLogBridge, TelemetryConfig,
};

#[tokio::main]
async fn main() -> Result<()> {
    let result = futuresdr::telemetry::init_metrics();

    assert!(
        result.is_ok(),
        "Init metrics failed with error: {:?}",
        result.err()
    );

    // handle must be present for now, to allow telemetry collection. Global handle somehow does not work yet
    let meter_provider = result.unwrap();

    // Opentelemetry will not provide a global API to manage the logger
    // provider. Application users must manage the lifecycle of the logger
    // provider on their own. Dropping logger providers will disable log
    // emitting.
    let logger_provider = futuresdr::telemetry::init_logs().unwrap();

    // Create a new OpenTelemetryLogBridge using the above LoggerProvider.
    let otel_log_appender = OpenTelemetryLogBridge::new(&logger_provider);
    log::set_boxed_logger(Box::new(otel_log_appender)).unwrap();
    log::set_max_level(Level::Info.to_level_filter());

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

    info!(target: "first telemetry-test", "This should be collected by the opentelemetry-collector");

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

    global::shutdown_tracer_provider();
    logger_provider.shutdown()?;
    // Metrics are exported by default every 30 seconds when using stdout exporter,
    // however shutting down the MeterProvider here instantly flushes
    // the metrics, instead of waiting for the 30 sec interval.
    meter_provider.shutdown()?;

    Ok(())
}
