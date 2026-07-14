use std::collections::BTreeMap;

use axum::{Router, extract::{Path, State}, routing::get};
use serde::{Deserialize, Serialize};
use crate::{ApiState, graph::Transform, sinks::{AuthConfig, BatchConfig, Codec, Encoding, Framing}};

use super::{Sink, SinkType};
pub(crate) struct Slack {
    pub id: String,
    pub token: String,
    pub channel: String
}

/// Persisted settings for a [`Slack`] sink.
#[derive(Serialize, Deserialize)]
pub(crate) struct SlackSettings {
    pub token: String,
    pub channel: String,
}

/// The Vector transform id feeding the Slack sink for a given channel.
fn pre_transform_id(channel: &str) -> String {
    format!("sink-pre-slack_{}", channel)
}

/// VRL shaping an alert into a Slack `chat.postMessage` payload.
fn pre_slack_vrl(channel: &str) -> String {
    format!(
        r#"
        msg = {{}}
        msg.channel = "{}"
        msg.text = .message || .finding_info.title || "Alert"
        . = msg
        "#,
        channel
    )
}

impl Sink for Slack {
    fn id(&self) -> String {
        self.channel.clone()
    }

    fn config(&self) -> SinkType {
        SinkType::Http {
            uri: "https://slack.com/api/chat.postMessage".to_string(),
            encoding: Encoding { codec: Codec::Json, only_fields: Some(vec!["channel".to_string(), "text".to_string(), "blocks".to_string()]) },
            inputs: vec![pre_transform_id(&self.channel)],
            auth: Some(AuthConfig {
                strategy: "bearer".to_string(),
                token: self.token.clone(),
            }),
            batch: Some(BatchConfig { max_events: 1 }),
            framing: Some(Framing { method: "bytes".to_string() }),
        }
    }

    fn typename(&self) -> String {
        "slack".to_string()
    }

    fn settings(&self) -> serde_json::Value {
        serde_json::json!({
            "token": self.token,
            "channel": self.channel,
        })
    }

    fn transforms(&self) -> BTreeMap<String, Transform> {
        BTreeMap::from([(
            pre_transform_id(&self.channel),
            Transform::remap(pre_slack_vrl(&self.channel)).with_inputs(["alerts"]),
        )])
    }
}


async fn list_sinks(State(state): State<ApiState>) -> axum::Json<Vec<serde_json::Value>> {
    let sinks = state.sinks.read().await;
    let json_sinks = sinks.iter().map(|sink| {
        serde_json::to_value(sink.config()).unwrap_or_else(|_| serde_json::json!({ "error": "Failed to serialize sink config" }))
    }).collect();
    axum::Json(json_sinks)
}

async fn get_sink_by_id(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<axum::Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    let sinks = state.sinks.read().await;
    let sink = sinks.iter().find(|s| s.id() == id);
    match sink {
        Some(s) => Ok(axum::Json(serde_json::to_value(s.config()).unwrap_or_else(|_| serde_json::json!({ "error": "Failed to serialize sink config" })))),
        None => Err((axum::http::StatusCode::NOT_FOUND, format!("Sink with id '{}' not found", id))),
    }
}

async fn add_slack_sink(
    state: &ApiState,
    token: String,
    channel: String,
) -> Result<(), (axum::http::StatusCode, String)> {
    let new_sink = Slack { id: channel.clone(), token, channel };

    state
        .store
        .add_sink(&new_sink)
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    state.sinks.write().await.push(Box::new(new_sink));
    Ok(())
}

pub fn create_router() -> Router<ApiState> {
    axum::Router::new()
        .route("/", get(list_sinks).post(|State(state): State<ApiState>, axum::extract::Json(payload): axum::extract::Json<serde_json::Value>| async move {
            let token = payload.get("token").and_then(|v| v.as_str()).unwrap_or_default().to_string();
            let channel = payload.get("channel").and_then(|v| v.as_str()).unwrap_or_default().to_string();
            add_slack_sink(&state, token, channel).await?;
            Ok::<_, (axum::http::StatusCode, String)>(axum::Json(serde_json::json!({"status": "success"})))
        }))
        .route("/{id}", get(get_sink_by_id))
}
