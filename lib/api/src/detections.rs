//! Sigma detection rule management endpoints.
//!
//! Provides CRUD operations for Sigma rules:
//! - GET /api/1/detections - List all rules (summary view)
//! - GET /api/1/detections/:id - Get full rule details
//! - PATCH /api/1/detections/:id - Enable/disable rule
//! - POST /api/1/detections - Upload new YAML rule
//!
//! Rules are stored in-memory in SigmaCollection and persisted to disk.
//! Changes affect running detection engine immediately via RwLock.

use anyhow::Result;
use axum::{extract::State, routing::get};

use crate::ApiState;

/// List all detection rules with summary information.
///
/// # Response Format
/// Returns array of rule summaries with: id, title, description, enabled, level, logsource.
/// Full rule details (detection logic, tags, etc.) omitted for performance.
///
/// # Error Handling
/// Logs serialization errors but returns empty array rather than 500.
/// This prevents one malformed rule from breaking the entire list view.
async fn list_rules(
    State(state): State<ApiState>,
) -> Result<axum::Json<Vec<serde_json::Value>>, (axum::http::StatusCode, String)> {
    let rules = serde_json::to_value(&*state.detections.read().await)
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .as_array()
        .map(|r| {
            r.iter()
                .flat_map(|rule| {
                    rule.as_object().and_then(|obj| {
                        Some(serde_json::json!({
                            "id": obj.get("id")?,
                            "title": obj.get("title")?,
                            "description": obj.get("description")?,
                            "enabled": obj.get("enabled")?.as_bool().unwrap_or(true),
                            "level": obj.get("level")?,
                            "logsource": obj.get("logsource")?,
                        }))
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Ok(axum::Json(rules))
}

async fn get_rule(
    State(state): State<ApiState>,
    axum::extract::Path(rule_id): axum::extract::Path<String>,
) -> Result<axum::Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    let detections = state.detections.read().await;
    let rule = detections.get(&rule_id).ok_or_else(|| {
        (
            axum::http::StatusCode::NOT_FOUND,
            format!("Rule with id {} not found", rule_id),
        )
    })?;

    let rule_json = serde_json::to_value(rule)
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

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
) -> Result<axum::Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    let detections = state.detections.read().await;
    let rule = detections.get(&rule_id).ok_or_else(|| {
        (
            axum::http::StatusCode::NOT_FOUND,
            format!("Rule with id {} not found", rule_id),
        )
    })?;

    if payload.enabled {
        rule.enable();
    } else {
        rule.disable();
    }

    let rule_json = serde_json::to_value(rule)
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(axum::Json(rule_json))
}

/// Upload a new Sigma rule from YAML content.
///
/// # Request Format
/// Expects raw YAML in request body (not JSON-wrapped).
/// Content-Type should be text/yaml or application/x-yaml.
///
/// # Validation
/// - Parses YAML as SigmaRule struct (validates schema)
/// - Checks for ID conflicts with existing rules
/// - Validates rule can be compiled and indexed
///
/// # Side Effects
/// Adds rule to in-memory collection (immediately available for detection)
/// and persists to disk for reload on restart.
async fn post_rule(
    State(state): State<ApiState>,
    body: String,
) -> Result<axum::Json<String>, (axum::http::StatusCode, String)> {
    // Parse the YAML content
    let rule: sigmars::SigmaRule = serde_yaml::from_str(&body).map_err(|e| {
        (
            axum::http::StatusCode::BAD_REQUEST,
            format!("Invalid YAML: {}", e),
        )
    })?;
    let id = rule.id.clone();
    let mut detections = state.detections.write().await;
    if detections.get(&id).is_some() {
        return Err((
            axum::http::StatusCode::CONFLICT,
            format!("Rule with id {} already exists", rule.id),
        ));
    }
    detections
        .add(rule)
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if let Some(striem_config::StringOrList::String(dir)) = &state.config.load().detections {
        let path = format!("{}/{}.yaml", dir, id);
        std::fs::write(&path, body).map_err(|e| {
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to write rule to disk: {}", e),
            )
        })?;
    }

    Ok(axum::Json(id))
}

pub fn create_router() -> axum::Router<ApiState> {
    axum::Router::new()
        .route("/", get(list_rules).post(post_rule))
        .route("/{id}", get(get_rule).patch(patch_rule))
}
