use std::collections::BTreeMap;

use axum::{Router, extract::{Path, State}, routing::get};
use toml::{toml, Table};
use crate::{ApiState, sinks::{AuthConfig, BatchConfig, Codec, Encoding, Framing, SINKS}};

use super::{Sink, SinkType, Transform, TransformType};
pub(crate) struct Slack {
    pub id: String,
    pub token: String,
    pub channel: String
}

fn pre_slack(channel: &str) -> Transform {

    let vrl = format!(
        r#"
        msg = {{}}
        msg.channel = "{}"
        msg.text = .message || .finding_info.title || "Alert"
        . = msg
        "#,
        channel
    );

    Transform {
        inputs: vec!["alerts".to_string()],
        source: Some(vrl),
        file: None,
        condition: None,
        routes: None,
        transform_type: TransformType::Remap,
    }
}

fn slack_sink(channel: &str) -> Table {
    let inputs = format!("sink-pre-slack_{}", channel);
    toml! {
        [slack]
        type = "http"
        inputs = [inputs]
        uri = "https://slack.com/api/chat.postMessage"
        auth = { strategy = "bearer", token = "${SLACK_TOKEN}" }
        request.headers = { "content-type" = "application/json" }
        batch.max_events = 1
        encoding.codec = "json"
        encoding.only_fields = ["channel", "text", "blocks"]
        framing.method = "bytes"
    }
}

impl Sink for Slack {
    fn id(&self) -> String {
        self.channel.clone()
    }

    fn config(&self) -> SinkType {
        SinkType::Http {
            uri: "https://slack.com/api/chat.postMessage".to_string(),
            encoding: Encoding { codec: Codec::Json, only_fields: Some(vec!["channel".to_string(), "text".to_string(), "blocks".to_string()]) },
            inputs: vec![format!("sink-pre-slack_{}", self.channel)],
            auth: Some(AuthConfig {
                strategy: "bearer".to_string(),
                token: self.token.clone(),
            }),
            batch: Some(BatchConfig { max_events: 1 }),
            framing: Some(Framing { method: "bytes".to_string() }),
        }
    }

    fn pre(&self) -> Option<(BTreeMap<String, Transform>, String)> {
        let pre = pre_slack(&self.channel);

        let transforms = BTreeMap::from_iter([(format!("sink-pre-slack_{}", self.channel), pre)].into_iter());

        Some((transforms, format!("sink-pre-slack_{}", self.channel)))
    }
}


async fn list_sinks(State(_): State<ApiState>) -> axum::Json<Vec<serde_json::Value>> {
    let sinks = SINKS.read().await;
    let json_sinks = sinks.iter().map(|sink| {
        serde_json::to_value(sink.config()).unwrap_or_else(|_| serde_json::json!({ "error": "Failed to serialize sink config" }))
    }).collect();
    axum::Json(json_sinks)
}

async fn get_sink_by_id(
    State(_): State<ApiState>,
    Path(id): Path<String>,
) -> Result<axum::Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    let sinks = SINKS.read().await;
    let sink = sinks.iter().find(|s| s.id() == id);
    match sink {
        Some(s) => Ok(axum::Json(serde_json::to_value(s.config()).unwrap_or_else(|_| serde_json::json!({ "error": "Failed to serialize sink config" })))),
        None => Err((axum::http::StatusCode::NOT_FOUND, format!("Sink with id '{}' not found", id))),
    }
}
async fn add_slack_sink(token: String, channel: String) {
    let mut sinks = SINKS.write().await;
    let new_sink = Slack { id: channel.clone(), token, channel };
    sinks.push(Box::new(new_sink));
}

pub fn create_router() -> Router<ApiState> {
    axum::Router::new()
        .route("/", get(list_sinks).post(|State(_): State<ApiState>, axum::extract::Json(payload): axum::extract::Json<serde_json::Value>| async move {
            let token = payload.get("token").and_then(|v| v.as_str()).unwrap_or_default().to_string();
            let channel = payload.get("channel").and_then(|v| v.as_str()).unwrap_or_default().to_string();
            add_slack_sink(token, channel).await;
            Ok::<_, (axum::http::StatusCode, String)>(axum::Json(serde_json::json!({"status": "success"})))
        }))
        .route("/{id}", get(get_sink_by_id))
}
