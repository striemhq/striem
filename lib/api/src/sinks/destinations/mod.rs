//! Destination sinks: archive the normalized `ocsf-*` event stream to Local
//! Files (parquet), AWS S3, or Clickhouse. Mirrors the `sources` resource shape
//! (list / get / delete / add-by-type).

pub mod aws_s3;
pub mod clickhouse;
pub mod local_file;

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
pub enum DestinationType {
    LocalFile,
    AwsS3,
    Clickhouse,
}

async fn list(State(state): State<ApiState>) -> axum::Json<Vec<Value>> {
    crate::sinks::list_by_category(&state, SinkCategory::Destination).await
}

async fn get_one(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<axum::Json<Value>, (axum::http::StatusCode, String)> {
    let sinks = state.sinks.read().await;
    let sink = sinks
        .iter()
        .find(|s| s.id() == id && s.category() == SinkCategory::Destination)
        .ok_or_else(|| {
            (
                axum::http::StatusCode::NOT_FOUND,
                format!("Destination with id {} not found", id),
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
    crate::sinks::delete(&state, SinkCategory::Destination, &id).await
}

async fn add(
    State(state): State<ApiState>,
    Path(destinationtype): Path<DestinationType>,
    axum::extract::Json(config): axum::extract::Json<Value>,
) -> Result<axum::Json<Value>, (axum::http::StatusCode, String)> {
    let id = uuid::Uuid::now_v7().to_string();
    let bad = |e: serde_json::Error| (axum::http::StatusCode::BAD_REQUEST, e.to_string());

    let sink: Box<dyn Sink> = match destinationtype {
        DestinationType::LocalFile => Box::new(local_file::LocalFile {
            id,
            settings: serde_json::from_value(config).map_err(bad)?,
        }),
        DestinationType::AwsS3 => Box::new(aws_s3::AwsS3 {
            id,
            settings: serde_json::from_value(config).map_err(bad)?,
        }),
        DestinationType::Clickhouse => {
            let sink = clickhouse::Clickhouse {
                id,
                settings: serde_json::from_value(config).map_err(bad)?,
            };
            // Create the destination table over HTTP before persisting, so an
            // unreachable/misconfigured endpoint surfaces to the user now
            // instead of silently failing inside Vector.
            sink.create_tables()
                .await
                .map_err(|e| (axum::http::StatusCode::BAD_GATEWAY, e.to_string()))?;
            Box::new(sink)
        }
    };

    let sinktype = sink.typename();
    // The Local Files destination owns the parquet path the query engine reads,
    // so mirror it into storage config (like the old /destination endpoint).
    if let Some(path) = local_file::storage_path(sink.as_ref()) {
        let _ = state.sys.send(striem_common::SysMessage::Update(Box::new(
            json!({ "storage": { "path": path } })
                .as_object()
                .cloned()
                .unwrap_or_default(),
        )));
    }

    let id = crate::sinks::add(&state, sink).await?;
    Ok(axum::Json(json!({ "id": id, "sinktype": sinktype })))
}

pub fn create_router() -> Router<ApiState> {
    Router::new().route("/", get(list)).route(
        "/{id}",
        get(get_one).delete(delete_one).post(add),
    )
}
