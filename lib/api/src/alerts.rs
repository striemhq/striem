use anyhow::{Result, anyhow};
use axum::{
    extract::{Path, Query, State},
    routing::get,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path};

use crate::ApiState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    pub id: String,
    pub time: String,
    pub severity: String,
    pub title: String,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

pub fn create_router() -> axum::Router<ApiState> {
    axum::Router::new()
        .route("/", get(get_alerts))
        .route("/{id}", get(get_alert_by_id))
}

async fn get_alerts(
    State(state): State<ApiState>,
    axum::extract::Query(params): axum::extract::Query<HashMap<String, String>>,
) -> Result<axum::Json<Vec<Alert>>, (axum::http::StatusCode, String)> {
    let start = params
        .get("start")
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or(Utc::now() - chrono::Duration::hours(24));

    let end = params
        .get("end")
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or(Utc::now());

    let db = if let Some(pool) = &state.db {
        pool.get()
            .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    } else {
        return Ok(axum::Json(Vec::new()));
    };

    let basepath = if let Some(path) = &state.data {
        path::Path::new(path)
    } else {
        return Ok(axum::Json(Vec::new()));
    };

    let findings_path = basepath.join("findings/detection_finding");

    if !findings_path.exists() {
        return Ok(axum::Json(Vec::new()));
    }

    let mut sql = r#"SELECT metadata.uid,
                              time,
                              finding_info.title,
                              severity,
                              observables,
                              filename"#
        .to_string();

    sql = format!(
        "{} FROM read_parquet(\"{}\")",
        sql,
        findings_path.join("**/*.parquet").to_string_lossy()
    );

    sql = format!(
        "{} WHERE time >= ? AND time <= ? ORDER BY time DESC LIMIT 10;",
        sql
    );

    let mut query = db
        .prepare(&sql)
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let alerts = query
        .query_map(duckdb::params![start, end], |row| {
            let fname = &row.get::<_, String>(5)?;

            let fname = path::Path::new(&fname)
                .strip_prefix(basepath)
                .unwrap_or_else(|_| path::Path::new(&fname))
                .to_string_lossy();

            Ok(Alert {
                id: row.get(0)?,
                time: row.get(1)?,
                title: row.get(2)?,
                severity: row.get(3)?,
                extra: HashMap::from([
                    ("_file".to_string(), serde_json::Value::from(fname)),
                    (
                        "observables".to_string(),
                        serde_json::Value::from(row.get::<_, Option<String>>(4)?),
                    ),
                ]),
            })
        })
        .and_then(|r| r.collect::<Result<Vec<_>, _>>())
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(axum::Json(alerts))
}

async fn get_alert_by_id(
    State(state): State<ApiState>,
    Path(id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<axum::Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    let fname = params.get("f").map(|s| s.as_str());
    Ok(axum::Json(fetch_alert(&id, fname, &state).await.map_err(
        |e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    )?))
}

pub(crate) async fn fetch_alert(
    id: &str,
    fname: Option<&str>,
    state: &ApiState,
) -> Result<serde_json::Value> {
    let mut sql = r#"SELECT row_to_json(t) from (SELECT * "#.to_string();

    if let Some(file) = fname
        && file.trim() != ""
    {
        sql = format!(
            "{} FROM read_parquet(\"{}/{}\")",
            sql,
            state
                .data
                .as_ref()
                .ok_or_else(|| anyhow!("data path not set"))?,
            file.trim()
        );
    } else {
        sql = format!(
            "{} FROM read_parquet(\"{}/findings/detection_finding/**/*.parquet\")",
            sql,
            state
                .data
                .as_ref()
                .ok_or_else(|| anyhow!("data path not set"))?
        );
    }
    sql = format!("{} WHERE metadata.uid = ? LIMIT 1) as t;", sql);

    let db = if let Some(pool) = &state.db {
        pool.get()?
    } else {
        return Err(anyhow!("database not initialized"));
    };

    let mut q = db.prepare(&sql)?.query_row(duckdb::params![id], |row| {
        let v: serde_json::Value = row.get(0)?;
        Ok(v)
    })?;

    strip_nulls(&mut q);

    Ok(q)
}

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
