//! Detection-admin gRPC service for rsigma-detection.
//!
//! Implements the shared [`Detections`] protocol (from `striem_detection`) so
//! the StrIEM API/UI's rule-management proxy works against this engine — the
//! same contract the `sigmars`-backed service exposes.
//!
//! rsigma manages rules **on disk**, so this service is a thin layer over the
//! engine's rules directory ([`LogProcessor::rules_path`]): it reads/parses the
//! rule files for `List`/`Get`, writes new files for `Create`, and toggles a
//! `.disabled` suffix for `SetEnabled` — reloading the engine after any change
//! via [`LogProcessor::reload_rules`]. Disabled rules keep a non-`.yml`
//! extension so rsigma's loader skips them while they remain visible here.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use rsigma_parser::{SigmaRule, parse_sigma_yaml};
use rsigma_runtime::LogProcessor;
use serde_json::json;
use tokio::task::spawn_blocking;
use tonic::{Request, Response, Status};

use striem_detection::{
    CreateRequest, CreateResponse, GetRequest, ListRequest, ListResponse, RuleResponse,
    SetEnabledRequest, detections_server::Detections,
};

const DISABLED_SUFFIX: &str = ".disabled";

/// gRPC service backing rule administration over the engine's rules directory.
pub struct DetectionAdmin {
    processor: Arc<LogProcessor>,
}

impl DetectionAdmin {
    pub fn new(processor: Arc<LogProcessor>) -> Self {
        Self { processor }
    }
}

/// A rule file discovered on disk, with its parsed rule and enabled state.
struct RuleEntry {
    path: PathBuf,
    enabled: bool,
    rule: SigmaRule,
}

/// Recursively read every Sigma rule under `dir`. Files ending in `.yml`/`.yaml`
/// are enabled; a trailing `.disabled` (e.g. `foo.yml.disabled`) marks a rule
/// disabled — parsed here for visibility but skipped by rsigma's loader.
fn scan_rules(dir: &Path) -> Vec<RuleEntry> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let disabled = name.ends_with(DISABLED_SUFFIX);
            let base = name.trim_end_matches(DISABLED_SUFFIX);
            if !(base.ends_with(".yml") || base.ends_with(".yaml")) {
                continue;
            }
            let Ok(content) = std::fs::read_to_string(&path) else {
                continue;
            };
            if let Ok(collection) = parse_sigma_yaml(&content) {
                for rule in collection.rules {
                    out.push(RuleEntry {
                        path: path.clone(),
                        enabled: !disabled,
                        rule,
                    });
                }
            }
        }
    }
    out
}

impl DetectionAdmin {
    fn rules_dir(&self) -> PathBuf {
        self.processor.rules_path()
    }
}

#[tonic::async_trait]
impl Detections for DetectionAdmin {
    async fn list(&self, _: Request<ListRequest>) -> Result<Response<ListResponse>, Status> {
        let dir = self.rules_dir();
        let summaries = spawn_blocking(move || {
            scan_rules(&dir)
                .iter()
                .filter_map(|entry| {
                    let summary = json!({
                        "id": entry.rule.id,
                        "title": entry.rule.title,
                        "description": entry.rule.description,
                        "enabled": entry.enabled,
                        "level": entry.rule.level,
                        "logsource": entry.rule.logsource,
                    });
                    serde_json::to_string(&summary).ok()
                })
                .collect::<Vec<_>>()
        })
        .await
        .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(ListResponse { summaries }))
    }

    async fn get(&self, request: Request<GetRequest>) -> Result<Response<RuleResponse>, Status> {
        let id = request.into_inner().id;
        let dir = self.rules_dir();
        let rule = spawn_blocking(move || {
            scan_rules(&dir)
                .into_iter()
                .find(|e| e.rule.id.as_deref() == Some(id.as_str()))
        })
        .await
        .map_err(|e| Status::internal(e.to_string()))?
        .ok_or_else(|| Status::not_found("rule not found"))?;

        let rule = serde_json::to_string(&rule.rule).map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(RuleResponse { rule }))
    }

    async fn create(
        &self,
        request: Request<CreateRequest>,
    ) -> Result<Response<CreateResponse>, Status> {
        let yaml = request.into_inner().yaml;
        let processor = self.processor.clone();

        let id = spawn_blocking(move || -> Result<String, Status> {
            let collection = parse_sigma_yaml(&yaml)
                .map_err(|e| Status::invalid_argument(format!("invalid rule: {e}")))?;
            let rule = collection
                .rules
                .into_iter()
                .next()
                .ok_or_else(|| Status::invalid_argument("no detection rule in document"))?;
            let id = rule
                .id
                .clone()
                .ok_or_else(|| Status::invalid_argument("rule has no id"))?;

            let dir = processor.rules_path();
            std::fs::create_dir_all(&dir).map_err(|e| Status::internal(e.to_string()))?;
            if scan_rules(&dir)
                .iter()
                .any(|e| e.rule.id.as_deref() == Some(id.as_str()))
            {
                return Err(Status::already_exists(format!(
                    "rule with id {id} already exists"
                )));
            }

            std::fs::write(dir.join(format!("{id}.yml")), yaml)
                .map_err(|e| Status::internal(e.to_string()))?;
            processor.reload_rules().map_err(Status::internal)?;
            Ok(id)
        })
        .await
        .map_err(|e| Status::internal(e.to_string()))??;

        Ok(Response::new(CreateResponse { id }))
    }

    async fn set_enabled(
        &self,
        request: Request<SetEnabledRequest>,
    ) -> Result<Response<RuleResponse>, Status> {
        let SetEnabledRequest { id, enabled } = request.into_inner();
        let processor = self.processor.clone();

        let rule = spawn_blocking(move || -> Result<String, Status> {
            let dir = processor.rules_path();
            let entry = scan_rules(&dir)
                .into_iter()
                .find(|e| e.rule.id.as_deref() == Some(id.as_str()))
                .ok_or_else(|| Status::not_found("rule not found"))?;

            if entry.enabled != enabled {
                // Toggle the `.disabled` suffix so rsigma's loader (which only
                // reads `.yml`/`.yaml`) skips or picks up the rule accordingly.
                let target = if enabled {
                    PathBuf::from(entry.path.to_string_lossy().trim_end_matches(DISABLED_SUFFIX))
                } else {
                    let mut name = entry.path.clone().into_os_string();
                    name.push(DISABLED_SUFFIX);
                    PathBuf::from(name)
                };
                std::fs::rename(&entry.path, &target).map_err(|e| Status::internal(e.to_string()))?;
                processor.reload_rules().map_err(Status::internal)?;
            }

            serde_json::to_string(&entry.rule).map_err(|e| Status::internal(e.to_string()))
        })
        .await
        .map_err(|e| Status::internal(e.to_string()))??;

        Ok(Response::new(RuleResponse { rule }))
    }
}
