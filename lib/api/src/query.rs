use anyhow::Result;
use arrow_json::writer::ArrayWriter;
use axum::extract::State;
use log::error;
use serde::Deserialize;

use crate::ApiState;

static INTERNAL_SERVER_ERROR: fn() -> (axum::http::StatusCode, String) = || {
    (
        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        "internal server error".to_string(),
    )
};

#[derive(Deserialize)]
pub struct QueryRequest {
    pub sql: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    10
}

pub fn create_router() -> axum::Router<ApiState> {
    axum::Router::new().route("/", axum::routing::post(post_query))
}

async fn post_query(
    State(state): State<ApiState>,
    axum::extract::Json(payload): axum::extract::Json<QueryRequest>,
) -> Result<axum::Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    let conn = if let Some(pool) = &state.db {
        pool.get().map_err(|e| {
            error!("Database Connection Error: {}", e);
            INTERNAL_SERVER_ERROR()
        })?
    } else {
        return Err(INTERNAL_SERVER_ERROR());
    };

    conn.execute(
        "SET file_search_path = ?",
        duckdb::params![state.data.as_deref().unwrap_or("")],
    )
    .map_err(|e| {
        error!("Database Error: {}", e);
        INTERNAL_SERVER_ERROR()
    })?;

    let sql = &payload.sql;
    let limit = payload.limit;

    let sql = if !sql.trim().to_lowercase().contains("limit") {
        format!("{} LIMIT {}", sql.trim_end_matches(';'), limit)
    } else {
        sql.to_string()
    };

    let mut stmt = conn.prepare(&sql).map_err(|_| {
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "SQL Error".to_string(),
        )
    })?;

    let res = stmt
        .query_arrow([])
        .map_err(|_| {
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "SQL Error".to_string(),
            )
        })?
        .collect::<Vec<_>>();

    let buf = Vec::new();
    let mut writer = ArrayWriter::new(buf);
    let batch_refs: Vec<&_> = res.iter().collect();

    writer.write_batches(&batch_refs).map_err(|e| {
        error!("Arrow Error writing batches: {}", e);
        INTERNAL_SERVER_ERROR()
    })?;

    writer.finish().map_err(|e| {
        error!("Arrow Error: {}", e);
        INTERNAL_SERVER_ERROR()
    })?;

    let out: serde_json::Value =
        serde_json::from_reader(writer.into_inner().as_slice()).map_err(|e| {
            error!("JSON Serialization Error: {}", e);
            INTERNAL_SERVER_ERROR()
        })?;

    Ok(axum::Json(out))
}
