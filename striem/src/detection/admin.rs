//! Detection-admin gRPC service.
//!
//! This service implements the [`Detections`] protocol (from `striem_detection`)
//! over the live in-memory [`SigmaCollection`]. This is the true source for rule
//! management. The API service is a thin gRPC proxy. It sends the HTTP requests
//! to here. A change takes effect at once for the running detection engine. The
//! service also saves the change to the configured rules directory.

use std::sync::Arc;

use arc_swap::ArcSwap;
use serde_json::json;
use sigmars::SigmaCollection;
use tokio::sync::RwLock;
use tonic::{Request, Response, Status};

use striem_config::{StrIEMConfig, StringOrList};
use striem_detection::{
    CreateRequest, CreateResponse, GetRequest, ListRequest, ListResponse, RuleResponse,
    SetEnabledRequest, detections_server::Detections,
};

/// The gRPC service for detection rule administration.
pub struct DetectionAdmin {
    detections: Arc<RwLock<SigmaCollection>>,
    config: Arc<ArcSwap<StrIEMConfig>>,
}

impl DetectionAdmin {
    pub fn new(
        detections: Arc<RwLock<SigmaCollection>>,
        config: Arc<ArcSwap<StrIEMConfig>>,
    ) -> Self {
        Self {
            detections,
            config,
        }
    }
}

#[tonic::async_trait]
impl Detections for DetectionAdmin {
    /// Lists all the rules as summary JSON objects.
    ///
    /// The function skips a rule with bad format. It does not fail the full list.
    async fn list(&self, _: Request<ListRequest>) -> Result<Response<ListResponse>, Status> {
        let value = serde_json::to_value(&*self.detections.read().await)
            .map_err(|e| Status::internal(e.to_string()))?;

        let summaries = value
            .as_array()
            .map(|rules| {
                rules
                    .iter()
                    .filter_map(|rule| {
                        let obj = rule.as_object()?;
                        let summary = json!({
                            "id": obj.get("id")?,
                            "title": obj.get("title")?,
                            "description": obj.get("description"),
                            "disabled": obj.get("disabled")?.as_bool().unwrap_or(false),
                            "level": obj.get("level"),
                            "logsource": obj.get("logsource"),
                        });
                        serde_json::to_string(&summary).ok()
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(Response::new(ListResponse { summaries }))
    }

    /// Gets the full JSON of one rule.
    async fn get(&self, request: Request<GetRequest>) -> Result<Response<RuleResponse>, Status> {
        let id = request.into_inner().id;
        let detections = self.detections.read().await;
        let rule = detections
            .get(&id)
            .ok_or_else(|| Status::not_found(format!("Rule with id {} not found", id)))?;

        let rule = serde_json::to_string(rule).map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(RuleResponse { rule }))
    }

    /// Adds a new Sigma rule from raw YAML.
    ///
    /// The function checks the YAML. It rejects an id that is already in use. It
    /// adds the rule to the live collection. Then it saves the rule to the
    /// configured rules directory.
    async fn create(
        &self,
        request: Request<CreateRequest>,
    ) -> Result<Response<CreateResponse>, Status> {
        let yaml = request.into_inner().yaml;

        let rule: sigmars::SigmaRule = serde_yaml::from_str(&yaml)
            .map_err(|e| Status::invalid_argument(format!("Invalid YAML: {}", e)))?;
        let id = rule.id.clone();

        let mut detections = self.detections.write().await;
        if detections.get(&id).is_some() {
            return Err(Status::already_exists(format!(
                "Rule with id {} already exists",
                id
            )));
        }
        detections
            .add(rule)
            .map_err(|e| Status::internal(e.to_string()))?;

        if let Some(StringOrList::String(dir)) = &self.config.load().detections {
            let path = format!("{}/{}.yaml", dir, id);
            std::fs::write(&path, yaml).map_err(|e| {
                Status::internal(format!("Failed to write rule to disk: {}", e))
            })?;
        }

        Ok(Response::new(CreateResponse { id }))
    }

    /// Enables or disables a rule that exists.
    async fn set_enabled(
        &self,
        request: Request<SetEnabledRequest>,
    ) -> Result<Response<RuleResponse>, Status> {
        let SetEnabledRequest { id, enabled } = request.into_inner();

        let detections = self.detections.read().await;
        let rule = detections
            .get(&id)
            .ok_or_else(|| Status::not_found(format!("Rule with id {} not found", id)))?;

        if enabled {
            rule.enable();
        } else {
            rule.disable();
        }

        let rule = serde_json::to_string(rule).map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(RuleResponse { rule }))
    }
}
