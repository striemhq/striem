use crate::{ApiState, actions, alerts, detections, sources, vector};

use crate::query;

use axum::{Router, http::StatusCode, routing::get};

pub fn create_router() -> Router<ApiState> {
    Router::new()
        .route("/health", get(health))
        .nest("/vector", vector::create_router())
        .nest("/api/1/alerts", alerts::create_router())
        .nest("/api/1/sources", sources::create_router())
        .nest("/api/1/detections", detections::create_router())
        .nest("/api/1/actions", actions::create_router())
        .nest("/api/1/query", query::create_router())
        .nest("/api/1/destination", crate::destination::create_router())
}

async fn health() -> StatusCode {
    StatusCode::OK
}
