use log::info;
use once_cell::sync::Lazy;
pub use opentelemetry;
pub use opentelemetry_appender_log;
pub use opentelemetry_otlp;
pub use opentelemetry_sdk;

use opentelemetry::{
    global::{self, BoxedTracer},
    metrics::{Meter, MetricsError},
    trace::{TraceError, TracerProvider as _},
    KeyValue,
};

use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::trace as sdktrace;
use opentelemetry_sdk::{
    logs::{self as sdklogs, Config},
    Resource,
};
use std::collections::HashSet;

// TODO: Read otel colelctor URLs from Config.toml
/* pub trait Telemetry {
    // fn telemetry_config()
    fn collectable_metrics(&self) -> HashSet<String>;
    fn collectable_traces(&self) -> HashSet<String>;
} */

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
    //logger: Logger,
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

pub fn init_logs() -> Result<sdklogs::LoggerProvider, opentelemetry::logs::LogError> {
    opentelemetry_otlp::new_pipeline()
        .logging()
        .with_log_config(Config::default().with_resource(RESOURCE.clone()))
        .with_exporter(
            opentelemetry_otlp::new_exporter()
                .http()
                .with_endpoint("http://localhost:4318/v1/logs"),
        )
        .install_batch(opentelemetry_sdk::runtime::Tokio)
}

pub fn init_tracer() -> Result<sdktrace::Tracer, TraceError> {
    opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_trace_config(sdktrace::config().with_resource(RESOURCE.clone()))
        .with_exporter(
            opentelemetry_otlp::new_exporter()
                .http()
                .with_endpoint("http://localhost:4318/v1/traces"),
        )
        .install_batch(opentelemetry_sdk::runtime::Tokio)
}

pub fn init_metrics() -> Result<opentelemetry_sdk::metrics::SdkMeterProvider, MetricsError> {
    opentelemetry_otlp::new_pipeline()
        .metrics(opentelemetry_sdk::runtime::Tokio)
        .with_exporter(
            opentelemetry_otlp::new_exporter()
                .http()
                .with_endpoint("http://localhost:4318/v1/metrics"),
        )
        .with_resource(RESOURCE.clone())
        .build()
}

pub fn init_globals() {
    info!("Initializing Telemetry");
    let result = init_tracer();
    assert!(
        result.is_ok(),
        "Init tracer failed with error: {:?}",
        result.err()
    );

    if let Some(provider) = result.unwrap().provider() {
        info!("Setting global tracer provider!");
        global::set_tracer_provider(provider);
    }

    let result = init_metrics();
    assert!(
        result.is_ok(),
        "Init metrics failed with error: {:?}",
        result.err()
    );

    info!("Setting global meter provider!");
    global::set_meter_provider(result.unwrap());

    // Opentelemetry will not provide a global API to manage the logger
    // provider. Application users must manage the lifecycle of the logger
    // provider on their own. Dropping logger providers will disable log
    // emitting.

    //let logger_provider = init_logs().unwrap();
}
