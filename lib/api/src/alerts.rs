//! The alerts endpoints. They read through [`StrIEMData`](crate::data::StrIEMData).

use std::collections::HashMap;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::get,
};
use chrono::{DateTime, Utc};

use crate::ApiState;
use crate::data::Alert;

/// The count of alerts that the list endpoint gives.
const ALERTS_LIMIT: usize = 10;

pub fn create_router() -> axum::Router<ApiState> {
    axum::Router::new()
        .route("/", get(get_alerts))
        .route("/{id}", get(get_alert_by_id))
}

/// Lists the newest alerts. The `start` and `end` query parameters (RFC 3339)
/// set the time range. The default is the last 24 hours.
async fn get_alerts(
    State(state): State<ApiState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<axum::Json<Vec<Alert>>, (StatusCode, String)> {
    let now = Utc::now();
    let start = params
        .get("start")
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or(now - chrono::Duration::hours(24));
    let end = params
        .get("end")
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or(now);

    let alerts = state
        .data
        .list_alerts(start, end, ALERTS_LIMIT)
        .await
        .map_err(|e| {
            log::error!("error fetching alerts: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        })?;

    Ok(axum::Json(alerts))
}

/// Gets the full finding of one alert. The `f` query parameter is the `_file`
/// hint from the alerts list.
async fn get_alert_by_id(
    State(state): State<ApiState>,
    Path(id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<axum::Json<serde_json::Value>, (StatusCode, String)> {
    let file = params.get("f").map(String::as_str);
    fetch_alert(&id, file, &state)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .map(axum::Json)
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("alert {id} not found")))
}

/// Gets one alert, with its empty fields removed. The actions endpoint uses it
/// too.
pub(crate) async fn fetch_alert(
    id: &str,
    file: Option<&str>,
    state: &ApiState,
) -> anyhow::Result<Option<serde_json::Value>> {
    let file = file.map(str::trim).filter(|f| !f.is_empty());
    let mut alert = state.data.get_alert(id, file).await?;
    if let Some(alert) = alert.as_mut() {
        strip_nulls(alert);
    }
    Ok(alert)
}

/// Removes the null values, and the empty objects and arrays, from a finding.
/// Thus the detail view shows only the fields that have a value.
fn strip_nulls(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            let keys_to_remove: Vec<String> = map
                .into_iter()
                .filter_map(|(k, v)| {
                    strip_nulls(v);
                    if v.is_null() {
                        Some(k.clone())
                    } else if let serde_json::Value::Object(o) = v {
                        if o.is_empty() { Some(k.clone()) } else { None }
                    } else if let serde_json::Value::Array(a) = v {
                        if a.is_empty() { Some(k.clone()) } else { None }
                    } else {
                        None
                    }
                })
                .collect();
            for k in keys_to_remove {
                map.remove(&k);
            }
        }
        serde_json::Value::Array(arr) => {
            arr.iter_mut().for_each(strip_nulls);
        }
        _ => {}
    }
}
