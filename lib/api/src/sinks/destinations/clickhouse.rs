//! Clickhouse destination — streams the OCSF event stream into a Clickhouse table.

use serde::{Deserialize, Serialize};

use crate::graph::{Pipeline, RenderCtx, Transform, component};
use crate::sinks::{BasicAuth, Sink, SinkCategory, SinkType, OCSF_INPUT};

#[derive(Serialize, Deserialize, Clone)]
pub struct ClickhouseSettings {
    pub endpoint: String,
    /// Target database; defaults to `default` when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
}

pub struct Clickhouse {
    pub id: String,
    pub settings: ClickhouseSettings,
}

/// The table [`CREATE_TABLES_SQL`] creates and the sink writes to.
const TABLE: &str = "striem_ocsf";

/// DDL creating the [`TABLE`] destination table, run against ClickHouse when the
/// sink is added.
const CREATE_TABLES_SQL: &str = include_str!("clickhouse.sql");

impl Clickhouse {
    /// Target database, defaulting to `default`.
    fn database(&self) -> &str {
        self.settings.database.as_deref().unwrap_or("default")
    }

    /// Build the ClickHouse sink body reading from `inputs`.
    fn sink_type(&self, inputs: Vec<String>) -> SinkType {
        let auth = match (&self.settings.user, &self.settings.password) {
            (Some(user), Some(password)) if !user.is_empty() => Some(BasicAuth {
                user: user.clone(),
                password: password.clone(),
                strategy: Some("basic".to_string()),
            }),
            _ => None,
        };
        SinkType::Clickhouse {
            endpoint: self.settings.endpoint.clone(),
            database: self.database().to_string(),
            table: TABLE.to_string(),
            inputs,
            auth,
        }
    }

    /// Connect to the ClickHouse HTTP endpoint and run [`CREATE_TABLES_SQL`] so
    /// the destination table exists before Vector starts writing to it.
    pub async fn create_tables(&self) -> anyhow::Result<()> {
        let mut request = reqwest::Client::new()
            .post(&self.settings.endpoint)
            .query(&[("database", self.database())])
            .body(CREATE_TABLES_SQL);

        if let Some(user) = self.settings.user.as_deref().filter(|u| !u.is_empty()) {
            request = request.basic_auth(user, self.settings.password.as_deref());
        }

        let response = request.send().await.map_err(|e| {
            anyhow::anyhow!(
                "failed to reach ClickHouse at {}: {}",
                self.settings.endpoint,
                e
            )
        })?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("ClickHouse table creation failed ({}): {}", status, body.trim());
        }
        Ok(())
    }
}

impl Sink for Clickhouse {
    fn id(&self) -> String {
        self.id.clone()
    }

    fn category(&self) -> SinkCategory {
        SinkCategory::Destination
    }

    fn typename(&self) -> String {
        "clickhouse".to_string()
    }

    fn name(&self) -> String {
        format!("Clickhouse ({}.{})", self.database(), TABLE)
    }

    fn config(&self) -> SinkType {
        self.sink_type(vec![OCSF_INPUT.to_string()])
    }

    fn pipeline(&self, _ctx: &RenderCtx) -> anyhow::Result<Pipeline> {
        let base = format!("sink-{}_{}", self.typename(), self.id());
        let remap_id = format!("{}-remap", base);

        let mut pipeline = Pipeline::default();

        // Populate the table's `_event_id` column from the OCSF event uid.
        pipeline.transforms.insert(
            remap_id.clone(),
            Transform::remap("._event_id = .metadata.uid").with_inputs([OCSF_INPUT]),
        );

        pipeline
            .sinks
            .insert(base, component(self.sink_type(vec![remap_id]))?);

        Ok(pipeline)
    }

    fn settings(&self) -> serde_json::Value {
        serde_json::to_value(&self.settings).unwrap_or_default()
    }
}
