use axum::{Json, extract::State, routing::post};
use serde_json::{Map, Value, json};
use std::path::PathBuf;

use crate::ApiState;

async fn set_destination(
    State(state): State<ApiState>,
    Json(payload): Json<Map<String, Value>>,
) -> Result<axum::Json<Value>, (axum::http::StatusCode, String)> {
    let dest_path = payload
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            (
                axum::http::StatusCode::BAD_REQUEST,
                "missing 'path' in request body".to_string(),
            )
        })?;
    if !PathBuf::from(dest_path).exists() {
        return Err((
            axum::http::StatusCode::BAD_REQUEST,
            "'path' must be an absolute path".to_string(),
        ));
    }

    log::info!("updating storage destination to '{}'", dest_path);

    let storage = state
        .config
        .load()
        .storage
        .as_ref()
        .and_then(|s| serde_json::to_value(s).ok())
        .ok_or_else(|| {
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "no storage configuration found".to_string(),
            )
        })?
        .as_object_mut()
        .map(|storage| {
            storage
                .entry("path")
                .and_modify(|e| *e = serde_json::value::Value::String(dest_path.to_string()));
            storage.clone()
        })
        .ok_or_else(|| {
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "failed to parse current storage configuration".to_string(),
            )
        })?;

    state
        .sys
        .send(crate::SysMessage::Update(Box::new(
            json!({"storage": storage})
                .as_object()
                .ok_or_else(|| {
                    (
                        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                        "failed to create storage update message".to_string(),
                    )
                })?
                .clone(),
        )))
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(axum::Json(storage.into()))
}

pub fn create_router() -> axum::Router<ApiState> {
    axum::Router::new().route("/", post(set_destination))
}
