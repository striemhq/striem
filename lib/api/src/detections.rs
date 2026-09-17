//! Endpoints to manage Sigma detection rules.
//!
//! These endpoints give CRUD operations for Sigma rules:
//! - GET /api/1/detections - lists all the rules (a summary view)
//! - GET /api/1/detections/:id - gets the full details of one rule
//! - PATCH /api/1/detections/:id - enables or disables a rule
//! - POST /api/1/detections - adds a new YAML rule
//!
//! These endpoints are a thin proxy. The rules are in the detection
//! microservice. That microservice owns the SigmaCollection and the storage.
//! Each handler sends the request to the detection-admin gRPC service. Then it
//! changes the gRPC status code to an HTTP response.

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;
use tonic::Code;

use striem_detection::{CreateRequest, GetRequest, ListRequest, SetEnabledRequest};

use crate::ApiState;

/// Changes a gRPC status from the detection service to an HTTP error.
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

/// Lists all the detection rules with summary information.
///
/// # Response Format
/// This handler gives an array of rule summaries. Each summary has an id, a
/// title, a description, the enabled state, a level, and a logsource. The
/// detection service makes the summaries. It skips a rule with bad format.
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

    // Each summary is a JSON object string. Skip a summary that does not parse.
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

/// Adds a new Sigma rule from YAML content.
///
/// # Request Format
/// The request body must be raw YAML. Do not put it in a JSON object. The
/// Content-Type must be text/yaml or application/x-yaml.
///
/// # Validation and Side Effects
/// The detection service parses the YAML and checks it. It rejects an id that
/// is already in use. It adds the rule to the live collection. It also saves the
/// rule to disk.
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
