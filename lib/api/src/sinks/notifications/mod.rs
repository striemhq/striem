//! Notification sinks. They send detection findings (the `alerts` stream) to
//! Slack, Email, or a general Webhook. They have the same resource shape as the
//! destinations.

pub mod email;
pub mod slack;
pub mod webhook;

use axum::{
    Router,
    extract::{Path, State},
    routing::get,
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::ApiState;
use crate::sinks::{Sink, SinkCategory};

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationType {
    Slack,
    Email,
    Webhook,
}

async fn list(State(state): State<ApiState>) -> axum::Json<Vec<Value>> {
    crate::sinks::list_by_category(&state, SinkCategory::Notification).await
}

async fn get_one(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<axum::Json<Value>, (axum::http::StatusCode, String)> {
    let sinks = state.sinks.read().await;
    let sink = sinks
        .iter()
        .find(|s| s.id() == id && s.category() == SinkCategory::Notification)
        .ok_or_else(|| {
            (
                axum::http::StatusCode::NOT_FOUND,
                format!("Notification with id {} not found", id),
            )
        })?;
    Ok(axum::Json(json!({
        "id": sink.id(),
        "sinktype": sink.typename(),
        "name": sink.name(),
        "config": sink.settings(),
    })))
}

async fn delete_one(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<axum::Json<()>, (axum::http::StatusCode, String)> {
    crate::sinks::delete(&state, SinkCategory::Notification, &id).await
}

async fn add(
    State(state): State<ApiState>,
    Path(notificationtype): Path<NotificationType>,
    axum::extract::Json(config): axum::extract::Json<Value>,
) -> Result<axum::Json<Value>, (axum::http::StatusCode, String)> {
    let id = uuid::Uuid::now_v7().to_string();
    let bad = |e: serde_json::Error| (axum::http::StatusCode::BAD_REQUEST, e.to_string());

    let sink: Box<dyn Sink> = match notificationtype {
        NotificationType::Slack => Box::new(slack::Slack {
            id,
            settings: serde_json::from_value(config).map_err(bad)?,
        }),
        NotificationType::Email => Box::new(email::Email {
            id,
            settings: serde_json::from_value(config).map_err(bad)?,
        }),
        NotificationType::Webhook => Box::new(webhook::Webhook {
            id,
            settings: serde_json::from_value(config).map_err(bad)?,
        }),
    };

    let sinktype = sink.typename();
    let id = crate::sinks::add(&state, sink).await?;
    Ok(axum::Json(json!({ "id": id, "sinktype": sinktype })))
}

pub fn create_router() -> Router<ApiState> {
    Router::new().route("/", get(list)).route(
        "/{id}",
        get(get_one).delete(delete_one).post(add),
    )
}
