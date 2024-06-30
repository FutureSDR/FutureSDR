pub use opentelemetry;
pub use opentelemetry_otlp;
pub use opentelemetry_sdk;
use std::sync::LazyLock;

use opentelemetry::{
    global, // {self, BoxedTracer}
    metrics::MetricsError,
    trace::TraceError, // , TracerProvider as _}
    KeyValue,
};

use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::trace as sdktrace;
use opentelemetry_sdk::{logs as sdklogs, Resource};
use std::collections::HashSet;

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

static RESOURCE: LazyLock<Resource> = LazyLock::new(|| {
    Resource::new(vec![KeyValue::new(
        opentelemetry_semantic_conventions::resource::SERVICE_NAME,
        "futuresdr-opentelemetry-service",
    )])
});

pub enum ExporterType {
    GRPC,
    HTTP,
}

pub fn init_logger_provider<T: Into<String>>(
    exporter_type: ExporterType,
    endpoint: T,
) -> Result<sdklogs::LoggerProvider, opentelemetry::logs::LogError> {
    let exporter: opentelemetry_otlp::LogExporterBuilder = match exporter_type {
        ExporterType::GRPC => opentelemetry_otlp::new_exporter()
            .tonic()
            .with_endpoint(endpoint.into())
            .into(),
        ExporterType::HTTP => opentelemetry_otlp::new_exporter()
            .http()
            .with_endpoint(endpoint.into())
            .into(),
    };

    opentelemetry_otlp::new_pipeline()
        .logging()
        .with_resource(RESOURCE.clone())
        .with_exporter(exporter)
        .install_batch(opentelemetry_sdk::runtime::Tokio)
}

pub fn init_tracer_provider<T: Into<String>>(
    exporter_type: ExporterType,
    endpoint: T,
) -> Result<sdktrace::TracerProvider, TraceError> {
    let exporter: opentelemetry_otlp::SpanExporterBuilder = match exporter_type {
        ExporterType::GRPC => opentelemetry_otlp::new_exporter()
            .tonic()
            .with_endpoint(endpoint.into())
            .into(),
        ExporterType::HTTP => opentelemetry_otlp::new_exporter()
            .http()
            .with_endpoint(endpoint.into())
            .into(),
    };
    opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_trace_config(sdktrace::Config::default().with_resource(RESOURCE.clone()))
        .with_exporter(exporter)
        .install_batch(opentelemetry_sdk::runtime::Tokio)
}

pub fn init_meter_provider<T: Into<String>>(
    exporter_type: ExporterType,
    endpoint: T,
) -> Result<opentelemetry_sdk::metrics::SdkMeterProvider, MetricsError> {
    let exporter: opentelemetry_otlp::MetricsExporterBuilder = match exporter_type {
        ExporterType::GRPC => opentelemetry_otlp::new_exporter()
            .tonic()
            .with_endpoint(endpoint.into())
            .into(),
        ExporterType::HTTP => opentelemetry_otlp::new_exporter()
            .http()
            .with_endpoint(endpoint.into())
            .into(),
    };

    opentelemetry_otlp::new_pipeline()
        .metrics(opentelemetry_sdk::runtime::Tokio)
        .with_resource(RESOURCE.clone())
        .with_exporter(exporter)
        .build()
}

pub fn init_globals(
    meter_provider: opentelemetry_sdk::metrics::SdkMeterProvider,
    tracer_provider: opentelemetry_sdk::trace::TracerProvider,
    logger_provider: opentelemetry_sdk::logs::LoggerProvider,
) {
    global::set_meter_provider(meter_provider);
    global::set_tracer_provider(tracer_provider);

    // TODO: Uncommenting this leads to a panic
    use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
    use tracing_subscriber::prelude::*;
    use tracing_subscriber::EnvFilter;

    // Setup Logger
    // Opentelemetry will not provide a global API to manage the logger
    // provider. Application users must manage the lifecycle of the logger
    // provider on their own. Dropping logger providers will disable log
    // emitting.
    // Create a new OpenTelemetryTracingBridge using the above LoggerProvider.
    let layer = OpenTelemetryTracingBridge::new(&logger_provider);

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

    // TODO: This is leading to a panic
    tracing_subscriber::registry()
        .with(filter)
        .with(layer)
        .init();
}
