//! The live-search endpoint. It reads through
//! [`StrIEMData`](crate::data::StrIEMData).

use axum::{extract::State, http::StatusCode};
use serde::Deserialize;

use crate::ApiState;
use crate::data::Unavailable;

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

/// Runs a live-search query. The answer is a JSON array of rows. When there is
/// no search backend, the answer is `501 Not Implemented`.
async fn post_query(
    State(state): State<ApiState>,
    axum::extract::Json(payload): axum::extract::Json<QueryRequest>,
) -> Result<axum::Json<serde_json::Value>, (StatusCode, String)> {
    state
        .data
        .search(&payload.sql, payload.limit)
        .await
        .map(axum::Json)
        .map_err(|e| {
            if e.is::<Unavailable>() {
                return (StatusCode::NOT_IMPLEMENTED, e.to_string());
            }
            log::error!("live search failed: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        })
}
