//! HTTP latency instrumentation and the metrics dashboard endpoint.
//!
//! The middleware [`track_latency`] records the time of each HTTP request. The
//! dashboard endpoint collects the metrics from every service (this API, the
//! detection service, and Vector) and returns them as one JSON list of samples.

use std::time::Instant;

use axum::{
    Json, Router,
    extract::{MatchedPath, Request},
    middleware::Next,
    response::Response,
    routing::get,
};
use serde_json::{Value, json};

use crate::ApiState;

/// axum middleware. It records the latency of each HTTP request. It labels the
/// sample with the matched route, so an id in a path does not make many labels.
pub async fn track_latency(request: Request, next: Next) -> Response {
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map(|p| p.as_str().to_string())
        .unwrap_or_else(|| request.uri().path().to_string());

    let start = Instant::now();
    let response = next.run(request).await;

    striem_telemetry::metrics().record_latency(
        "http",
        &route,
        response.status().as_u16(),
        start.elapsed().as_secs_f64(),
    );
    response
}

/// The URL of Vector's Prometheus endpoint. Vector's generated config shows its
/// internal metrics here.
fn vector_metrics_url() -> String {
    std::env::var("STRIEM_VECTOR_METRICS_URL")
        .unwrap_or_else(|_| "http://vector:9100/metrics".to_string())
}

/// The URL of the detection service's Prometheus endpoint.
fn detection_metrics_url() -> String {
    std::env::var("STRIEM_DETECTION_METRICS_URL")
        .unwrap_or_else(|_| "http://detection:9101/metrics".to_string())
}

/// One metric sample: a name, a set of labels, and a value.
fn sample(name: &str, mut labels: Value, job: &str, value: f64) -> Value {
    if let Some(map) = labels.as_object_mut() {
        map.insert("job".to_string(), json!(job));
    }
    json!({ "name": name, "labels": labels, "value": value })
}

/// Reads Prometheus text and returns a list of samples. This function keeps the
/// name, the labels, and the value of each metric line. It adds a `job` label
/// with the source name.
fn parse_prometheus(text: &str, job: &str) -> Vec<Value> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Split `name{labels}` from the value, or `name` from the value.
        let (name, labels, rest) = if let Some(open) = line.find('{') {
            let Some(close) = line.find('}') else {
                continue;
            };
            (
                &line[..open],
                parse_labels(&line[open + 1..close]),
                line[close + 1..].trim(),
            )
        } else if let Some((name, rest)) = line.split_once(char::is_whitespace) {
            (name, json!({}), rest.trim())
        } else {
            continue;
        };

        let value_token = rest.split_whitespace().next().unwrap_or("");
        let value = match value_token {
            "+Inf" => f64::INFINITY,
            "-Inf" => f64::NEG_INFINITY,
            other => match other.parse::<f64>() {
                Ok(v) => v,
                Err(_) => continue,
            },
        };
        if value.is_finite() {
            out.push(sample(name, labels, job, value));
        }
    }
    out
}

/// Reads the label block of a Prometheus line, for example
/// `service="api",route="/x"`, and returns a JSON object.
fn parse_labels(block: &str) -> Value {
    let mut map = serde_json::Map::new();
    for pair in block.split(',') {
        let pair = pair.trim();
        if let Some((key, value)) = pair.split_once('=') {
            let value = value.trim().trim_matches('"');
            map.insert(key.trim().to_string(), json!(value));
        }
    }
    Value::Object(map)
}

/// Gets the Prometheus text from `url`. Returns `None` if the source is not
/// available, so one source that is down does not stop the dashboard.
async fn scrape(url: &str) -> Option<String> {
    reqwest::Client::new()
        .get(url)
        .timeout(std::time::Duration::from_secs(3))
        .send()
        .await
        .ok()?
        .text()
        .await
        .ok()
}

/// Collects the metrics from every service and returns them as one list.
///
/// The response is `{ "samples": [ { name, labels, value }, ... ], "sources":
/// { api, detection, vector } }`. Each sample has a `job` label. The `sources`
/// map shows which sources answered.
async fn dashboard() -> Json<Value> {
    let mut samples = Vec::new();
    let mut sources = serde_json::Map::new();

    // This API's own metrics come from the local registry.
    samples.extend(parse_prometheus(&striem_telemetry::render(), "api"));
    sources.insert("api".to_string(), json!("ok"));

    for (job, url) in [
        ("detection", detection_metrics_url()),
        ("vector", vector_metrics_url()),
    ] {
        match scrape(&url).await {
            Some(text) => {
                samples.extend(parse_prometheus(&text, job));
                sources.insert(job.to_string(), json!("ok"));
            }
            None => {
                sources.insert(job.to_string(), json!("unavailable"));
            }
        }
    }

    Json(json!({ "samples": samples, "sources": Value::Object(sources) }))
}

/// The routes that serve metrics: `/metrics` (Prometheus) and the dashboard.
pub fn create_router() -> Router<ApiState> {
    striem_telemetry::metrics_router().route("/api/1/dashboard", get(dashboard))
}
