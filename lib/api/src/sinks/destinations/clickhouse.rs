//! Clickhouse destination. It sends the OCSF event stream to a Clickhouse table.

use serde::{Deserialize, Serialize};

use crate::graph::{Pipeline, RenderCtx, Transform, component};
use crate::sinks::{BasicAuth, Sink, SinkCategory, SinkType, OCSF_INPUT};

#[derive(Serialize, Deserialize, Clone)]
pub struct ClickhouseSettings {
    pub endpoint: String,
    /// The target database. The default is `default` if you do not set it.
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

/// The table that [`CREATE_TABLES_SQL`] makes and the sink writes to.
pub(crate) const TABLE: &str = "striem_ocsf";

/// The DDL that makes the [`TABLE`] destination table. The service runs it
/// against ClickHouse when you add the sink.
const CREATE_TABLES_SQL: &str = include_str!("clickhouse.sql");

impl Clickhouse {
    /// The target database. The default is `default`.
    fn database(&self) -> &str {
        self.settings.database.as_deref().unwrap_or("default")
    }

    /// Makes the ClickHouse sink body that reads from `inputs`.
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

    /// Connects to the ClickHouse HTTP endpoint and runs [`CREATE_TABLES_SQL`].
    /// Thus the destination table exists before Vector writes to it.
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

        // Fill the table's `_event_id` column from the OCSF event uid.
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
