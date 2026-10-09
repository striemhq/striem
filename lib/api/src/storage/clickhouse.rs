//! ClickHouse storage. It reads the OCSF table that the ClickHouse destination
//! writes (`striem_ocsf` by default).
//!
//! The service uses ClickHouse's HTTP interface. Each request:
//! - runs with `readonly=2`, so no request can change data or tables, and a
//!   live-search query cannot either;
//! - gives each value as a query parameter (`{name:Type}` in the SQL), not as
//!   text in the SQL;
//! - gets its rows as `JSONEachRow`.
//!
//! Only the table name goes into the SQL text. It is checked as an identifier
//! when the storage is made.

use std::collections::HashMap;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::data::{Alert, StrIEMData};
use crate::sinks::destinations::clickhouse::TABLE as DEFAULT_TABLE;

use super::Storage;

/// The OCSF class of a Detection Finding: an alert.
const DETECTION_FINDING: i32 = 2004;

/// The longest time that one query can run, in seconds.
const MAX_EXECUTION_SECS: u64 = 30;

/// The most rows that one live search can give.
const MAX_SEARCH_ROWS: usize = 10_000;

#[derive(Serialize, Deserialize, Clone)]
pub struct ClickhouseSettings {
    /// The HTTP endpoint, for example `http://clickhouse:8123`.
    pub endpoint: String,
    /// The database. The default is `default`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
    /// The OCSF table. The default is the table of the ClickHouse destination.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
}

pub struct Clickhouse {
    id: String,
    settings: ClickhouseSettings,
    client: reqwest::Client,
}

impl Clickhouse {
    /// Makes the storage. The endpoint must be an `http` or `https` URL, and
    /// the table must be a plain identifier.
    pub fn new(id: String, settings: ClickhouseSettings) -> Result<Self> {
        let url = reqwest::Url::parse(&settings.endpoint)
            .with_context(|| format!("invalid ClickHouse endpoint '{}'", settings.endpoint))?;
        if !matches!(url.scheme(), "http" | "https") {
            bail!("the ClickHouse endpoint must be an http or https URL");
        }
        if let Some(table) = &settings.table
            && !is_identifier(table)
        {
            bail!("invalid ClickHouse table name '{table}'");
        }
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(MAX_EXECUTION_SECS + 10))
            .build()?;
        Ok(Self {
            id,
            settings,
            client,
        })
    }

    fn database(&self) -> &str {
        self.settings
            .database
            .as_deref()
            .filter(|d| !d.is_empty())
            .unwrap_or("default")
    }

    fn table(&self) -> &str {
        self.settings
            .table
            .as_deref()
            .filter(|t| !t.is_empty())
            .unwrap_or(DEFAULT_TABLE)
    }

    /// Runs `sql` and gives each `JSONEachRow` row. `params` gives the values
    /// for the `{name:Type}` placeholders. `settings` adds ClickHouse settings.
    async fn query(
        &self,
        sql: &str,
        params: &[(&str, String)],
        settings: &[(&str, String)],
    ) -> Result<Vec<Value>> {
        let mut query: Vec<(String, String)> = vec![
            ("database".into(), self.database().into()),
            ("readonly".into(), "2".into()),
            ("default_format".into(), "JSONEachRow".into()),
            ("max_execution_time".into(), MAX_EXECUTION_SECS.to_string()),
            // Give 64-bit numbers (for example, `time`) as JSON numbers.
            ("output_format_json_quote_64bit_integers".into(), "0".into()),
        ];
        query.extend(settings.iter().map(|(k, v)| (k.to_string(), v.clone())));
        query.extend(params.iter().map(|(k, v)| (format!("param_{k}"), v.clone())));

        let mut request = self
            .client
            .post(&self.settings.endpoint)
            .query(&query)
            .body(sql.to_string());
        if let Some(user) = self.settings.user.as_deref().filter(|u| !u.is_empty()) {
            request = request.basic_auth(user, self.settings.password.as_deref());
        }

        let response = request
            .send()
            .await
            .with_context(|| format!("cannot reach ClickHouse at {}", self.settings.endpoint))?;
        let status = response.status();
        let body = response.text().await?;
        if !status.is_success() {
            bail!("ClickHouse error ({status}): {}", body.trim());
        }

        body.lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| {
                serde_json::from_str(line)
                    .context("ClickHouse gave a row that is not JSON (use FORMAT JSONEachRow)")
            })
            .collect()
    }
}

/// Tells if `name` is a plain SQL identifier.
fn is_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && name.len() <= 128
}

/// The OCSF severity name for a `severity_id`.
fn severity_name(id: Option<i32>) -> &'static str {
    match id {
        Some(1) => "Informational",
        Some(2) => "Low",
        Some(3) => "Medium",
        Some(4) => "High",
        Some(5) => "Critical",
        Some(6) => "Fatal",
        Some(99) => "Other",
        _ => "Unknown",
    }
}

/// One row of the alerts query.
#[derive(Deserialize)]
struct AlertRow {
    id: String,
    time: i64,
    severity: Option<String>,
    severity_id: Option<i32>,
    title: Option<String>,
}

impl From<AlertRow> for Alert {
    fn from(row: AlertRow) -> Self {
        Alert {
            id: row.id,
            time: DateTime::<Utc>::from_timestamp_millis(row.time)
                .map(|t| t.to_rfc3339())
                .unwrap_or_default(),
            severity: row
                .severity
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| severity_name(row.severity_id).to_string()),
            title: row.title.unwrap_or_default(),
            extra: HashMap::new(),
        }
    }
}

#[tonic::async_trait]
impl StrIEMData for Clickhouse {
    async fn list_alerts(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        limit: usize,
    ) -> Result<Vec<Alert>> {
        let sql = format!(
            "SELECT toString(_event_id) AS id, time, severity, severity_id, \
                    CAST(finding_info.title AS Nullable(String)) AS title \
             FROM `{}` \
             WHERE class_uid = {{class:Int32}} \
               AND time >= {{start:Int64}} AND time <= {{end:Int64}} \
             ORDER BY time DESC \
             LIMIT {{limit:UInt32}}",
            self.table()
        );
        let rows = self
            .query(
                &sql,
                &[
                    ("class", DETECTION_FINDING.to_string()),
                    ("start", start.timestamp_millis().to_string()),
                    ("end", end.timestamp_millis().to_string()),
                    ("limit", limit.to_string()),
                ],
                &[],
            )
            .await?;
        rows.into_iter()
            .map(|row| Ok(serde_json::from_value::<AlertRow>(row)?.into()))
            .collect()
    }

    async fn get_alert(&self, id: &str, _file: Option<&str>) -> Result<Option<Value>> {
        // `_event_id` is the primary key. The ClickHouse destination fills it
        // from `metadata.uid`, which is the alert id.
        let sql = format!(
            "SELECT * FROM `{}` \
             WHERE _event_id = toUUIDOrNull({{id:String}}) AND class_uid = {{class:Int32}} \
             LIMIT 1",
            self.table()
        );
        let mut rows = self
            .query(
                &sql,
                &[("id", id.to_string()), ("class", DETECTION_FINDING.to_string())],
                &[],
            )
            .await?;
        Ok(rows.pop().map(|mut row| {
            if let Some(map) = row.as_object_mut() {
                map.remove("_event_id");
            }
            row
        }))
    }

    async fn search(&self, query: &str, limit: usize) -> Result<Value> {
        let limit = limit.clamp(1, MAX_SEARCH_ROWS);
        // `max_result_rows` with `break` stops the query at about `limit` rows.
        // It stops at a block boundary, so the rows are also cut here.
        let mut rows = self
            .query(
                query,
                &[],
                &[
                    ("max_result_rows", limit.to_string()),
                    ("result_overflow_mode", "break".to_string()),
                ],
            )
            .await?;
        rows.truncate(limit);
        Ok(Value::Array(rows))
    }
}

#[tonic::async_trait]
impl Storage for Clickhouse {
    fn id(&self) -> String {
        self.id.clone()
    }

    fn typename(&self) -> String {
        "clickhouse".to_string()
    }

    fn name(&self) -> String {
        format!("Clickhouse ({}.{})", self.database(), self.table())
    }

    fn settings(&self) -> Value {
        serde_json::to_value(&self.settings).unwrap_or_default()
    }

    fn secret_keys(&self) -> &'static [&'static str] {
        &["password"]
    }

    /// Checks that ClickHouse answers, and that the OCSF table exists.
    async fn check(&self) -> Result<()> {
        let rows = self
            .query(
                "SELECT count() AS n FROM system.tables \
                 WHERE database = currentDatabase() AND name = {table:String}",
                &[("table", self.table().to_string())],
                &[],
            )
            .await?;
        let exists = rows
            .first()
            .and_then(|r| r.get("n"))
            .and_then(Value::as_u64)
            .is_some_and(|n| n > 0);
        if !exists {
            bail!(
                "table {}.{} does not exist yet (add a Clickhouse destination to make it)",
                self.database(),
                self.table()
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    use axum::extract::{Query, State};
    use serde_json::json;

    /// One request that the mock ClickHouse got.
    #[derive(Clone, Debug)]
    struct Seen {
        query: HashMap<String, String>,
        sql: String,
        auth: Option<String>,
    }

    /// Starts a mock ClickHouse that answers each request with `reply`, and
    /// records the requests.
    async fn mock(reply: &'static str) -> (String, Arc<Mutex<Vec<Seen>>>) {
        let seen: Arc<Mutex<Vec<Seen>>> = Arc::default();
        let app = axum::Router::new()
            .route(
                "/",
                axum::routing::post(
                    move |State(seen): State<Arc<Mutex<Vec<Seen>>>>,
                     Query(query): Query<HashMap<String, String>>,
                     headers: axum::http::HeaderMap,
                     sql: String| async move {
                        seen.lock().unwrap().push(Seen {
                            query,
                            sql,
                            auth: headers
                                .get("authorization")
                                .and_then(|v| v.to_str().ok())
                                .map(str::to_string),
                        });
                        reply
                    },
                ),
            )
            .with_state(seen.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (format!("http://{addr}"), seen)
    }

    fn storage(endpoint: &str) -> Clickhouse {
        Clickhouse::new(
            "s1".into(),
            serde_json::from_value(json!({
                "endpoint": endpoint, "database": "ocsf", "user": "reader", "password": "pw"
            }))
            .unwrap(),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn alerts_are_read_with_bound_parameters_in_readonly_mode() {
        let (endpoint, seen) = mock(
            "{\"id\":\"0190-a\",\"time\":1700000000000,\"severity\":\"High\",\"severity_id\":4,\"title\":\"Whoami\"}\n\
             {\"id\":\"0190-b\",\"time\":1700000000001,\"severity\":null,\"severity_id\":5,\"title\":null}\n",
        )
        .await;
        let start = DateTime::<Utc>::from_timestamp_millis(1_600_000_000_000).unwrap();
        let end = DateTime::<Utc>::from_timestamp_millis(1_800_000_000_000).unwrap();

        let alerts = storage(&endpoint).list_alerts(start, end, 10).await.unwrap();
        assert_eq!(alerts.len(), 2);
        assert_eq!(alerts[0].id, "0190-a");
        assert_eq!(alerts[0].severity, "High");
        assert_eq!(alerts[0].title, "Whoami");
        assert!(alerts[0].time.starts_with("2023-11-14T22:13:20"));
        // The name comes from `severity_id` when `severity` is empty.
        assert_eq!(alerts[1].severity, "Critical");

        let request = seen.lock().unwrap()[0].clone();
        assert_eq!(request.query["readonly"], "2");
        assert_eq!(request.query["database"], "ocsf");
        assert_eq!(request.query["param_start"], "1600000000000");
        assert_eq!(request.query["param_class"], "2004");
        assert_eq!(request.query["param_limit"], "10");
        assert!(request.sql.contains("FROM `striem_ocsf`"));
        assert!(request.sql.contains("{start:Int64}"));
        assert!(request.auth.unwrap().starts_with("Basic "));
    }

    #[tokio::test]
    async fn an_alert_is_looked_up_by_its_primary_key() {
        let (endpoint, seen) = mock("{\"_event_id\":\"x\",\"class_uid\":2004,\"time\":1}\n").await;
        let alert = storage(&endpoint).get_alert("x", None).await.unwrap().unwrap();
        assert_eq!(alert["class_uid"], 2004);
        assert!(alert.get("_event_id").is_none());

        let request = seen.lock().unwrap()[0].clone();
        assert!(request.sql.contains("_event_id = toUUIDOrNull({id:String})"));
        assert_eq!(request.query["param_id"], "x");
    }

    #[tokio::test]
    async fn a_missing_alert_is_none() {
        let (endpoint, _) = mock("").await;
        assert!(storage(&endpoint).get_alert("x", None).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn search_is_read_only_and_capped() {
        let (endpoint, seen) = mock("{\"n\":1}\n{\"n\":2}\n{\"n\":3}\n").await;
        let rows = storage(&endpoint).search("SELECT n FROM t", 2).await.unwrap();
        assert_eq!(rows, json!([{ "n": 1 }, { "n": 2 }]));

        let request = seen.lock().unwrap()[0].clone();
        assert_eq!(request.sql, "SELECT n FROM t");
        assert_eq!(request.query["readonly"], "2");
        assert_eq!(request.query["max_result_rows"], "2");
        assert_eq!(request.query["result_overflow_mode"], "break");

        // A limit of 0 would mean "no limit" in ClickHouse. It becomes 1.
        storage(&endpoint).search("SELECT 1", 0).await.unwrap();
        assert_eq!(seen.lock().unwrap()[1].query["max_result_rows"], "1");
    }

    #[tokio::test]
    async fn a_clickhouse_error_is_given_back() {
        let app = axum::Router::new().route(
            "/",
            axum::routing::post(|| async {
                (axum::http::StatusCode::BAD_REQUEST, "Code: 62. Syntax error")
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let err = storage(&format!("http://{addr}"))
            .search("SELEC", 10)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("Syntax error"), "{err}");
    }

    #[tokio::test]
    async fn check_reports_a_missing_table() {
        let (endpoint, _) = mock("{\"n\":0}\n").await;
        let err = storage(&endpoint).check().await.unwrap_err();
        assert!(err.to_string().contains("does not exist"), "{err}");

        let (endpoint, _) = mock("{\"n\":1}\n").await;
        storage(&endpoint).check().await.unwrap();
    }

    #[test]
    fn bad_settings_are_rejected() {
        let make = |v: Value| Clickhouse::new("s".into(), serde_json::from_value(v).unwrap());
        assert!(make(json!({ "endpoint": "not a url" })).is_err());
        assert!(make(json!({ "endpoint": "file:///etc/passwd" })).is_err());
        assert!(make(json!({ "endpoint": "http://ch:8123", "table": "t; DROP TABLE x" })).is_err());
        assert!(make(json!({ "endpoint": "http://ch:8123", "table": "my_table" })).is_ok());
    }
}
