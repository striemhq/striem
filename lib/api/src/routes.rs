use crate::{ApiState, actions, alerts, detections, observability, sinks, sources, vector};

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
        .nest("/api/1/destinations", sinks::destinations::create_router())
        .nest("/api/1/notifications", sinks::notifications::create_router())
        // `/metrics` (Prometheus) and `/api/1/dashboard` (aggregated metrics).
        .merge(observability::create_router())
}

async fn health() -> StatusCode {
    StatusCode::OK
}
