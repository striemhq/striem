//! Shared observability for the StrIEM services.
//!
//! This crate gives two things to each service:
//!
//! - **Traces and spans.** It installs a `tracing` subscriber. When you set the
//!   `OTEL_EXPORTER_OTLP_ENDPOINT` environment variable, it also sends the spans
//!   to an OpenTelemetry collector over OTLP.
//! - **Metrics.** It holds a Prometheus registry with latency histograms and
//!   system-resource gauges. A background task samples the resources. The
//!   service shows the registry at a `/metrics` endpoint (see [`metrics_router`]).
//!
//! Call [`init`] one time at service start. Use [`metrics`] to record values.

use std::sync::OnceLock;
use std::time::Duration;

use axum::{Router, response::IntoResponse, routing::get};
use prometheus::{
    Encoder, GaugeVec, HistogramOpts, HistogramVec, IntCounterVec, Opts, Registry, TextEncoder,
};

static METRICS: OnceLock<Metrics> = OnceLock::new();

/// The telemetry state for a service: the Prometheus registry and the metric
/// instruments. Every metric carries a `service` label so the dashboard can
/// tell the services apart when it collects them together.
pub struct Metrics {
    /// The name of this service (for example `api` or `detection`).
    pub service: String,
    /// The Prometheus registry that holds every metric.
    pub registry: Registry,
    /// Request latency in seconds, by request kind, route, and status.
    pub latency: HistogramVec,
    /// Counters for pipeline volume, by kind (for example `events`, `findings`).
    pub events: IntCounterVec,
    /// System-resource gauges (CPU percent, memory, threads).
    pub resources: GaugeVec,
}

impl Metrics {
    fn new(service: &str) -> Self {
        let registry = Registry::new();

        let latency = HistogramVec::new(
            HistogramOpts::new(
                "striem_request_duration_seconds",
                "Request latency in seconds.",
            )
            .buckets(vec![
                0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0,
            ]),
            &["service", "kind", "route", "status"],
        )
        .expect("latency metric");

        let events = IntCounterVec::new(
            Opts::new("striem_events_total", "Count of pipeline events, by kind."),
            &["service", "kind"],
        )
        .expect("events metric");

        let resources = GaugeVec::new(
            Opts::new(
                "striem_resource",
                "System-resource samples (see the `resource` label).",
            ),
            &["service", "resource"],
        )
        .expect("resources metric");

        registry.register(Box::new(latency.clone())).ok();
        registry.register(Box::new(events.clone())).ok();
        registry.register(Box::new(resources.clone())).ok();

        Self {
            service: service.to_string(),
            registry,
            latency,
            events,
            resources,
        }
    }

    /// Records the latency of one request.
    pub fn record_latency(&self, kind: &str, route: &str, status: u16, seconds: f64) {
        self.latency
            .with_label_values(&[&self.service, kind, route, &status.to_string()])
            .observe(seconds);
    }

    /// Adds `count` to the counter for one event kind.
    pub fn incr_events(&self, kind: &str, count: u64) {
        self.events
            .with_label_values(&[&self.service, kind])
            .inc_by(count);
    }

    fn set_resource(&self, resource: &str, value: f64) {
        self.resources
            .with_label_values(&[&self.service, resource])
            .set(value);
    }
}

/// Returns the global metrics for this service. [`init`] must run first; if it
/// did not, this function makes an unnamed set so callers never panic.
pub fn metrics() -> &'static Metrics {
    METRICS.get_or_init(|| Metrics::new("unknown"))
}

/// Starts telemetry for a service. Call this one time at start.
///
/// It installs the `tracing` subscriber and the metrics. It also starts the
/// resource sampler task. Give the tokio runtime before you call this function,
/// because the sampler and the OTLP exporter need it.
pub fn init(service: &'static str) {
    METRICS.get_or_init(|| Metrics::new(service));
    init_tracing(service);
    spawn_resource_sampler();
}

/// Installs the `tracing` subscriber. It always writes to the log. When
/// `OTEL_EXPORTER_OTLP_ENDPOINT` is set, it also sends spans over OTLP.
fn init_tracing(service: &'static str) {
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;

    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let fmt = tracing_subscriber::fmt::layer();
    let base = tracing_subscriber::registry().with(filter).with(fmt);

    let tracer = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
        .ok()
        .and_then(|endpoint| otlp_tracer(service, &endpoint));

    match tracer {
        Some(tracer) => base
            .with(tracing_opentelemetry::layer().with_tracer(tracer))
            .init(),
        None => base.init(),
    }
}

/// Builds an OpenTelemetry tracer that sends spans over OTLP. Returns `None` and
/// logs a warning if the exporter cannot start.
fn otlp_tracer(
    service: &'static str,
    endpoint: &str,
) -> Option<opentelemetry_sdk::trace::Tracer> {
    use opentelemetry::trace::TracerProvider as _;
    use opentelemetry_otlp::WithExportConfig;

    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(endpoint.to_string())
        .build()
        .map_err(|e| log::warn!("OTLP exporter setup failed: {e}"))
        .ok()?;

    let provider = opentelemetry_sdk::trace::TracerProvider::builder()
        .with_batch_exporter(exporter, opentelemetry_sdk::runtime::Tokio)
        .with_resource(opentelemetry_sdk::Resource::new(vec![
            opentelemetry::KeyValue::new("service.name", service.to_string()),
        ]))
        .build();

    let tracer = provider.tracer(service);
    opentelemetry::global::set_tracer_provider(provider);
    log::info!("OTLP trace export to {endpoint}");
    Some(tracer)
}

/// Starts a task that samples CPU and memory every 5 seconds and writes the
/// values to the resource gauges.
fn spawn_resource_sampler() {
    tokio::spawn(async move {
        use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};

        let mut sys = System::new();
        let pid = sysinfo::get_current_pid().ok();

        loop {
            sys.refresh_memory();
            sys.refresh_cpu_usage();
            if let Some(pid) = pid {
                sys.refresh_processes_specifics(
                    ProcessesToUpdate::Some(&[pid]),
                    true,
                    ProcessRefreshKind::nothing().with_cpu().with_memory(),
                );
            }

            let m = metrics();
            m.set_resource("system_cpu_percent", sys.global_cpu_usage() as f64);
            m.set_resource("system_memory_used_bytes", sys.used_memory() as f64);
            m.set_resource("system_memory_total_bytes", sys.total_memory() as f64);

            if let Some(process) = pid.and_then(|pid| sys.process(pid)) {
                m.set_resource("process_cpu_percent", process.cpu_usage() as f64);
                m.set_resource("process_memory_bytes", process.memory() as f64);
            }

            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    });
}

/// Renders the metrics in the Prometheus text format.
pub fn render() -> String {
    let mut buffer = Vec::new();
    let encoder = TextEncoder::new();
    let families = metrics().registry.gather();
    encoder.encode(&families, &mut buffer).ok();
    String::from_utf8(buffer).unwrap_or_default()
}

/// An axum router with a `GET /metrics` route for Prometheus.
pub fn metrics_router<S: Clone + Send + Sync + 'static>() -> Router<S> {
    Router::new().route("/metrics", get(metrics_handler))
}

async fn metrics_handler() -> impl IntoResponse {
    (
        [("content-type", "text/plain; version=0.0.4")],
        render(),
    )
}

/// Runs a small HTTP server that shows `/metrics`. A gRPC-only service (for
/// example the detection service) uses this, because it has no HTTP server of
/// its own. This function does not return until the server stops.
pub async fn serve_metrics(addr: std::net::SocketAddr) -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    log::info!("metrics available at http://{addr}/metrics");
    axum::serve(listener, metrics_router::<()>()).await?;
    Ok(())
}
