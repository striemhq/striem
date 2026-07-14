//! Sigma detection rule management endpoints.
//!
//! Provides CRUD operations for Sigma rules:
//! - GET /api/1/detections - List all rules (summary view)
//! - GET /api/1/detections/:id - Get full rule details
//! - PATCH /api/1/detections/:id - Enable/disable rule
//! - POST /api/1/detections - Upload new YAML rule
//!
//! These endpoints are a thin proxy: the rules themselves live in the detection
//! microservice, which owns the SigmaCollection and persistence. Each handler
//! forwards to the detection-admin gRPC service and translates gRPC status codes
//! into HTTP responses.

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;
use tonic::Code;

use striem_detection::{CreateRequest, GetRequest, ListRequest, SetEnabledRequest};

use crate::ApiState;

/// Translate a gRPC status from the detection service into an HTTP error.
fn grpc_error(status: tonic::Status) -> (StatusCode, String) {
    let code = match status.code() {
        Code::NotFound => StatusCode::NOT_FOUND,
        Code::AlreadyExists => StatusCode::CONFLICT,
        Code::InvalidArgument => StatusCode::BAD_REQUEST,
        Code::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (code, status.message().to_string())
}

/// List all detection rules with summary information.
///
/// # Response Format
/// Returns array of rule summaries with: id, title, description, enabled, level, logsource.
/// Summaries are computed by the detection service; a malformed rule is skipped there.
async fn list_rules(
    State(state): State<ApiState>,
) -> Result<axum::Json<Vec<serde_json::Value>>, (StatusCode, String)> {
    let mut client = state.detections.clone();
    let summaries = client
        .list(ListRequest {})
        .await
        .map_err(grpc_error)?
        .into_inner()
        .summaries;

    // Each summary is a JSON object string; skip any that fail to parse.
    let rules = summaries
        .iter()
        .filter_map(|s| serde_json::from_str(s).ok())
        .collect();

    Ok(axum::Json(rules))
}

async fn get_rule(
    State(state): State<ApiState>,
    axum::extract::Path(rule_id): axum::extract::Path<String>,
) -> Result<axum::Json<serde_json::Value>, (StatusCode, String)> {
    let mut client = state.detections.clone();
    let rule = client
        .get(GetRequest { id: rule_id })
        .await
        .map_err(grpc_error)?
        .into_inner()
        .rule;

    let rule_json = serde_json::from_str(&rule)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(axum::Json(rule_json))
}

#[derive(serde::Deserialize)]
struct PatchRulePayload {
    enabled: bool,
}

async fn patch_rule(
    State(state): State<ApiState>,
    axum::extract::Path(rule_id): axum::extract::Path<String>,
    axum::extract::Json(payload): axum::extract::Json<PatchRulePayload>,
) -> Result<axum::Json<serde_json::Value>, (StatusCode, String)> {
    let mut client = state.detections.clone();
    let rule = client
        .set_enabled(SetEnabledRequest {
            id: rule_id,
            enabled: payload.enabled,
        })
        .await
        .map_err(grpc_error)?
        .into_inner()
        .rule;

    let rule_json = serde_json::from_str(&rule)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(axum::Json(rule_json))
}

/// Upload a new Sigma rule from YAML content.
///
/// # Request Format
/// Expects raw YAML in request body (not JSON-wrapped).
/// Content-Type should be text/yaml or application/x-yaml.
///
/// # Validation & Side Effects
/// The detection service parses/validates the YAML, rejects id conflicts, adds
/// the rule to the live collection, and persists it to disk.
async fn post_rule(
    State(state): State<ApiState>,
    body: String,
) -> Result<axum::Json<String>, (StatusCode, String)> {
    let mut client = state.detections.clone();
    let id = client
        .create(CreateRequest { yaml: body })
        .await
        .map_err(grpc_error)?
        .into_inner()
        .id;

    Ok(axum::Json(id))
}

pub fn create_router() -> axum::Router<ApiState> {
    axum::Router::new()
        .route("/", get(list_rules).post(post_rule))
        .route("/{id}", get(get_rule).patch(patch_rule))
}
