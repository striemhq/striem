use std::collections::HashMap;

use anyhow::Result;
use axum::{Router, extract::State, routing::get};
use rmcp::{
    model::CallToolRequestParam, service::ServiceExt, transport::StreamableHttpClientTransport,
};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use striem_common::prelude::*;

use crate::{ApiState, alerts::fetch_alert};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Action {
    pub id: String,
    pub title: String,
}

pub(crate) struct Mcp {
    url: String,
    inner: RwLock<HashMap<String, Action>>,
    last_update: RwLock<std::time::Instant>,
}

impl Mcp {
    pub fn new(url: String) -> Self {
        Self {
            url,
            inner: RwLock::new(HashMap::new()),
            last_update: RwLock::new(
                std::time::Instant::now()
                    - std::time::Duration::from_secs(MCP_REFRESH_INTERVAL_SECS),
            ),
        }
    }
    pub async fn get(&self, id: &str) -> Result<Option<Action>> {
        let last_update = self.last_update.read().await;
        if last_update.elapsed().as_secs() > MCP_REFRESH_INTERVAL_SECS {
            drop(last_update);
            self.refresh().await?;
        }
        let inner = self.inner.read().await;
        Ok(inner.get(id).cloned())
    }

    pub async fn list(&self) -> Result<Vec<Action>> {
        let last_update = self.last_update.read().await;
        if last_update.elapsed().as_secs() > 300 {
            drop(last_update);
            self.refresh().await?;
        }
        let inner = self.inner.read().await;
        Ok(inner.values().cloned().collect())
    }

    pub async fn execute(
        &self,
        id: &str,
        params: serde_json::Map<String, serde_json::Value>,
    ) -> Result<()> {
        let action = self
            .get(id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Action with id {} not found", id))?;

        log::info!("Executing action: {:?} with params: {:?}", action, params);
        let transport = StreamableHttpClientTransport::from_uri(self.url.clone());

        let client = ().serve(transport).await?;
        client
            .call_tool(CallToolRequestParam {
                name: action.id.clone().into(),
                arguments: Some(params),
            })
            .await?;

        Ok(())
    }

    async fn refresh(&self) -> Result<()> {
        let transport = StreamableHttpClientTransport::from_uri(self.url.clone());

        let client = ().serve(transport).await?;
        let actions = client
            .list_tools(None)
            .await?
            .tools
            .into_iter()
            .map(|tool| {
                (
                    tool.name.to_string(),
                    Action {
                        id: tool.name.to_string(),
                        title: tool.description.unwrap_or_default().to_string(),
                    },
                )
            })
            .collect::<HashMap<_, _>>();
        let mut inner = self.inner.write().await;
        inner.clear();
        inner.extend(actions);
        drop(inner);
        self.last_update
            .write()
            .await
            .clone_from(&std::time::Instant::now());
        Ok(())
    }
}

pub fn create_router() -> Router<ApiState> {
    axum::Router::new()
        .route("/", get(get_actions))
        .route("/{id}", get(get_action_by_id).post(execute_action_by_id))
}

async fn get_actions(
    State(state): State<ApiState>,
) -> Result<axum::Json<Vec<Action>>, (axum::http::StatusCode, String)> {
    if let Some(actions) = &state.actions {
        Ok(axum::Json(actions.list().await.map_err(|e| {
            (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        })?))
    } else {
        log::error!("no actions available");
        Ok(axum::Json(Vec::new()))
    }
}

pub(crate) async fn get_action_by_id(
    State(state): State<ApiState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<axum::Json<Action>, (axum::http::StatusCode, String)> {
    let mcp = state.actions.as_ref().ok_or((
        axum::http::StatusCode::NOT_FOUND,
        format!("Action with id {} not found", id),
    ))?;

    mcp.get(&id)
        .await
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .map(axum::Json)
        .ok_or((
            axum::http::StatusCode::NOT_FOUND,
            format!("Action with id {} not found", id),
        ))
}

pub(crate) async fn execute_action_by_id(
    State(state): State<ApiState>,
    axum::extract::Path(id): axum::extract::Path<String>,
    axum::extract::Json(mut params): axum::extract::Json<
        serde_json::Map<String, serde_json::Value>,
    >,
) -> Result<axum::Json<()>, (axum::http::StatusCode, String)> {
    let mcp = state.actions.as_ref().ok_or((
        axum::http::StatusCode::NOT_FOUND,
        format!("action with id {} not found", id),
    ))?;

    let alert_id = params.get("alert_id").and_then(|v| v.as_str()).ok_or((
        axum::http::StatusCode::BAD_REQUEST,
        "missing alert_id parameter".to_string(),
    ))?;

    log::info!("{:?}", params);
    let file = params.get("file").and_then(|v| v.as_str());

    let alert = fetch_alert(alert_id, file, &state)
        .await
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    params.entry("data").or_insert_with(|| alert);

    mcp.execute(&id, params)
        .await
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(axum::Json(()))
}
