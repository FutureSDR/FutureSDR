//use log::{info, Level};
use once_cell::sync::Lazy;
pub use opentelemetry;
//pub use opentelemetry_appender_log;
pub use opentelemetry_otlp;
pub use opentelemetry_sdk;

use opentelemetry::{
    global::{self, BoxedTracer},
    metrics::{Meter, MetricsError},
    trace::{TraceError, TracerProvider as _},
    KeyValue,
};

// use opentelemetry_appender_log::OpenTelemetryLogBridge;
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::metrics::{
    reader::{DefaultAggregationSelector, DefaultTemporalitySelector},
    PeriodicReader, SdkMeterProvider,
};
use opentelemetry_sdk::trace as sdktrace;
use opentelemetry_sdk::{logs as sdklogs, Resource};

use tracing::info;
use tracing_subscriber::prelude::*;
use tracing_subscriber::EnvFilter;

use std::{collections::HashSet, time::Duration};

pub struct TelemetryResource {
    meter: Meter,
    tracer: BoxedTracer,
}

impl TelemetryResource {
    pub fn new(name: String, version: String) -> Self {
        let tracer_common_scope_attributes =
            vec![KeyValue::new("tracer-scope-key", "tracer-scope-value")];
        let tracer = global::tracer_provider()
            .tracer_builder(name.clone())
            .with_version(version.clone())
            .with_schema_url("https://opentelemetry.io/schemas/1.17.0") // Might be dynamically detected from opentelemetry crate
            .with_attributes(tracer_common_scope_attributes.clone())
            .build();

        let meter_common_scope_attributes =
            vec![KeyValue::new("meter-scope-key", "meter-scope-value")];
        let meter = global::meter_with_version(
            name.clone(),
            Some(version.clone()),
            Some("https://opentelemetry.io/schemas/1.17.0"), // Might be dynamically detected from opentelemetry crate
            Some(meter_common_scope_attributes.clone()),
        );

        Self { meter, tracer }
    }

    pub fn get_meter(&self) -> &Meter {
        &self.meter
    }

    pub fn get_tracer(&self) -> &BoxedTracer {
        &self.tracer
    }
}
/// Telemetry Configuration
#[derive(Debug)]
pub struct TelemetryConfig {
    active_metrics: HashSet<String>,
    active_traces: HashSet<String>,
}

impl TelemetryConfig {
    pub fn new(active_metrics: HashSet<String>, active_traces: HashSet<String>) -> Self {
        Self {
            active_metrics,
            active_traces,
        }
    }

    pub fn active_metrics(&self) -> &HashSet<String> {
        &self.active_metrics
    }

    pub fn active_traces(&self) -> &HashSet<String> {
        &self.active_traces
    }

    pub fn toggle_metric(&mut self, label: &str, active: bool) -> &Self {
        if active {
            let _ = &self.active_metrics.insert(label.to_string());
        } else {
            let _ = &self.active_metrics.remove(label);
        }

        self
    }

    pub fn toggle_trace(&mut self, label: &str, active: bool) -> &Self {
        if active {
            let _ = &self.active_traces.insert(label.to_string());
        } else {
            let _ = &self.active_traces.remove(label);
        }

        self
    }
}

static RESOURCE: Lazy<Resource> = Lazy::new(|| {
    Resource::new(vec![KeyValue::new(
        opentelemetry_semantic_conventions::resource::SERVICE_NAME,
        "futuresdr-opentelemetry-service",
    )])
});

pub fn init_logger_provider(
    logs_endpoint: String,
) -> Result<sdklogs::LoggerProvider, opentelemetry::logs::LogError> {
    opentelemetry_otlp::new_pipeline()
        .logging()
        .with_resource(RESOURCE.clone())
        .with_exporter(
            opentelemetry_otlp::new_exporter()
                //.http() // HTTP
                .tonic() // gRPC
                .with_endpoint(logs_endpoint), //"http://localhost:4318/v1/logs"
        )
        .install_batch(opentelemetry_sdk::runtime::Tokio)
}

pub fn init_tracer_provider(
    tracer_endpoint: String,
) -> Result<sdktrace::TracerProvider, TraceError> {
    opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_trace_config(sdktrace::config().with_resource(RESOURCE.clone()))
        .with_exporter(
            opentelemetry_otlp::new_exporter()
                //.http() // HTTP
                .tonic() // gRPC
                .with_endpoint(tracer_endpoint), //"http://localhost:4318/v1/traces"
        )
        .install_batch(opentelemetry_sdk::runtime::Tokio)
}

pub fn init_meter_provider(
    metrics_endpoint: String,
) -> Result<opentelemetry_sdk::metrics::SdkMeterProvider, MetricsError> {
    let exporter = opentelemetry_otlp::new_exporter()
        //.http() // HTTP
        .tonic() // gRPC
        .with_endpoint(metrics_endpoint)
        .build_metrics_exporter(
            Box::new(DefaultAggregationSelector::new()),
            Box::new(DefaultTemporalitySelector::new()),
        )
        .unwrap();
    let reader = PeriodicReader::builder(exporter, opentelemetry_sdk::runtime::Tokio)
        .with_interval(Duration::from_secs(1))
        .build();

    Ok(SdkMeterProvider::builder()
        .with_reader(reader)
        .with_resource(RESOURCE.clone())
        .build())

    /* opentelemetry_otlp::new_pipeline()
    .metrics(opentelemetry_sdk::runtime::Tokio)
    .with_resource(RESOURCE.clone())
    .with_exporter(
        opentelemetry_otlp::new_exporter()
            //.http() // HTTP
            .tonic() // gRPC
            .with_endpoint(metrics_endpoint), //"http://localhost:4318/v1/metrics"
    )
    .build() */
}

pub fn init_globals(
    metrics_endpoint: String,
    tracer_endpoint: String,
    logger_endpoint: String,
) -> (
    opentelemetry_sdk::metrics::SdkMeterProvider,
    opentelemetry_sdk::trace::TracerProvider,
    opentelemetry_sdk::logs::LoggerProvider,
) {
    // info!("Initializing Telemetry");

    // Setup Meter
    let meter_provider =
        init_meter_provider(metrics_endpoint).expect("Failed to initialize meter provider.");
    info!("Setting global meter provider!");
    global::set_meter_provider(meter_provider.clone());

    // Setup Tracer
    let tracer_provider =
        init_tracer_provider(tracer_endpoint).expect("Failed to initialize tracer provider.");
    info!("Setting global tracer provider!");
    global::set_tracer_provider(tracer_provider.clone());

    // Setup Logger
    // Opentelemetry will not provide a global API to manage the logger
    // provider. Application users must manage the lifecycle of the logger
    // provider on their own. Dropping logger providers will disable log
    // emitting.
    let logger_provider =
        init_logger_provider(logger_endpoint).expect("Failed to initialize logger provider.");
    // Create a new OpenTelemetryTracingBridge using the above LoggerProvider.
    let layer = OpenTelemetryTracingBridge::new(&logger_provider);
    info!("Setting global logger provider!");

    // Add a tracing filter to filter events from crates used by opentelemetry-otlp.
    // The filter levels are set as follows:
    // - Allow `info` level and above by default.
    // - Restrict `hyper`, `tonic`, and `reqwest` to `error` level logs only.
    // This ensures events generated from these crates within the OTLP Exporter are not looped back,
    // thus preventing infinite event generation.
    // Note: This will also drop events from these crates used outside the OTLP Exporter.
    // For more details, see: https://github.com/open-telemetry/opentelemetry-rust/issues/761
    let filter = EnvFilter::new("debug")
        .add_directive("h2=error".parse().unwrap())
        .add_directive("tower=error".parse().unwrap())
        .add_directive("hyper=error".parse().unwrap())
        .add_directive("tonic=error".parse().unwrap())
        .add_directive("reqwest=error".parse().unwrap());

    tracing_subscriber::registry()
        .with(filter)
        .with(layer)
        .init();

    //let otel_log_appender = OpenTelemetryLogBridge::new(&logger_provider);
    //log::set_boxed_logger(Box::new(otel_log_appender)).unwrap();
    //log::set_max_level(Level::Info.to_level_filter());

    (meter_provider, tracer_provider, logger_provider)
}
