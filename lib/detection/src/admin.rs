//! Detection-admin gRPC service for the detection service.
//!
//! This service implements the shared [`StrIem`] protocol (from
//! `striem_detection`). Thus the StrIEM API and UI rule-management proxy works
//! against this engine too. This is the same contract that the `sigmars`
//! service gives.
//!
//! rsigma manages the rules **on disk**. Thus this service is a thin layer over
//! the engine's rules directory ([`Processor::rules_path`]):
//!
//! - `List` and `Get` read and parse the rule files.
//! - `Create` saves an uploaded rule as `<rule id>.yaml` in that directory.
//! - `SetEnabled` writes the change into the rule's existing YAML file, as a
//!   top-level `enabled: false` line (see [`crate::rulefile`]). The engine
//!   skips a disabled rule when it loads the rules.
//!
//! Each write goes to `<file>.bak` first, then the service copies it over
//! `<file>`. After each change, the service reloads the engine with
//! [`Processor::reload_rules`]. If the reload fails (for example, the new rule
//! does not compile), the service undoes the change on disk, and the engine
//! keeps the rules that it had.
//!
//! An older version of this service disabled a rule by renaming its file with
//! a `.disabled` suffix. This service still shows such a file as disabled. When
//! the rule is enabled again, its YAML moves back to the name without the
//! suffix.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use rsigma_parser::{SigmaRule, parse_sigma_yaml};
use serde_json::json;
use tokio::task::spawn_blocking;
use tonic::{Request, Response, Status};

use crate::proto::{
    CreateRequest, CreateResponse, GetRequest, ListRequest, ListResponse, RuleResponse,
    SetEnabledRequest, sigma_collection_server::SigmaCollection
};

use crate::engine::Processor;
use crate::rulefile::{
    remove_rule_file, rule_enabled, set_enabled_in_yaml, validate_id, write_rule_file,
};

/// The suffix that an older version of this service used to disable a rule.
const DISABLED_SUFFIX: &str = ".disabled";

/// The extension of an uploaded rule file: `<rule id>.yaml`.
const RULE_EXTENSION: &str = "yaml";

/// The gRPC service for rule administration over the engine's rules directory.
pub struct CollectionAdmin {
    processor: Arc<Processor>,
    /// Lets only one change write files and reload the engine at a time. Thus
    /// two changes cannot write the same file, and an undo restores the right
    /// content.
    write_lock: Arc<Mutex<()>>,
}

impl CollectionAdmin {
    pub fn new(processor: Arc<Processor>) -> Self {
        Self {
            processor,
            write_lock: Arc::new(Mutex::new(())),
        }
    }
}

/// A rule file on disk, with its parsed rule and its enabled state.
struct RuleEntry {
    path: PathBuf,
    enabled: bool,
    rule: SigmaRule,
}

/// Reads every Sigma rule under `dir`, and under its subdirectories: each `.yml`
/// and `.yaml` file, and each such file with the older `.disabled` suffix. A
/// rule is disabled if its YAML has `enabled: false`, or if its file has the
/// `.disabled` suffix.
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
                        enabled: !disabled && rule_enabled(&rule),
                        rule,
                    });
                }
            }
        }
    }
    out
}

impl CollectionAdmin {
    fn rules_dir(&self) -> PathBuf {
        self.processor.rules_path()
    }
}

#[tonic::async_trait]
impl SigmaCollection for CollectionAdmin {
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
        let write_lock = self.write_lock.clone();

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
            validate_id(&id).map_err(Status::invalid_argument)?;

            let _guard = write_lock.lock().unwrap_or_else(PoisonError::into_inner);
            let dir = processor.rules_path();
            std::fs::create_dir_all(&dir).map_err(internal)?;
            let path = dir.join(format!("{id}.{RULE_EXTENSION}"));
            if path.exists()
                || scan_rules(&dir)
                    .iter()
                    .any(|e| e.rule.id.as_deref() == Some(id.as_str()))
            {
                return Err(Status::already_exists(format!(
                    "rule with id {id} already exists"
                )));
            }

            write_rule_file(&path, &yaml).map_err(internal)?;
            if let Err(e) = processor.reload_rules() {
                // Do not leave a rule on disk that the engine cannot load: it
                // would make every later reload fail too.
                remove_rule_file(&path).map_err(internal)?;
                return Err(Status::invalid_argument(format!("rule does not load: {e}")));
            }
            Ok(id)
        })
        .await
        .map_err(internal)??;

        Ok(Response::new(CreateResponse { id }))
    }

    async fn set_enabled(
        &self,
        request: Request<SetEnabledRequest>,
    ) -> Result<Response<RuleResponse>, Status> {
        let SetEnabledRequest { id, enabled } = request.into_inner();
        let processor = self.processor.clone();
        let write_lock = self.write_lock.clone();

        let rule = spawn_blocking(move || -> Result<String, Status> {
            let _guard = write_lock.lock().unwrap_or_else(PoisonError::into_inner);
            let dir = processor.rules_path();
            let entry = scan_rules(&dir)
                .into_iter()
                .find(|e| e.rule.id.as_deref() == Some(id.as_str()))
                .ok_or_else(|| Status::not_found("rule not found"))?;

            if entry.enabled != enabled {
                let original = std::fs::read_to_string(&entry.path).map_err(internal)?;
                let updated =
                    set_enabled_in_yaml(&original, &id, enabled).map_err(Status::internal)?;

                // A file with the older `.disabled` suffix moves back to its
                // name without the suffix. Any other file changes in place.
                let legacy = entry.path.to_string_lossy().ends_with(DISABLED_SUFFIX);
                let target = if legacy {
                    PathBuf::from(entry.path.to_string_lossy().trim_end_matches(DISABLED_SUFFIX))
                } else {
                    entry.path.clone()
                };

                write_rule_file(&target, &updated).map_err(internal)?;
                if let Err(e) = processor.reload_rules() {
                    // Undo the change, so the file matches the engine again.
                    if legacy {
                        remove_rule_file(&target).map_err(internal)?;
                    } else {
                        write_rule_file(&target, &original).map_err(internal)?;
                    }
                    return Err(Status::failed_precondition(format!(
                        "cannot {} rule {id}: {e}",
                        if enabled { "enable" } else { "disable" }
                    )));
                }
                if legacy {
                    std::fs::remove_file(&entry.path).map_err(internal)?;
                }
            }

            serde_json::to_string(&entry.rule).map_err(internal)
        })
        .await
        .map_err(internal)??;

        Ok(Response::new(RuleResponse { rule }))
    }
}

/// Makes an internal-error status from any error.
fn internal(e: impl std::fmt::Display) -> Status {
    Status::internal(e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::RuntimeEngine;
    use crate::rulefile::backup_path;
    use rsigma_eval::CorrelationConfig;

    const ID: &str = "11111111-1111-1111-1111-111111111111";

    fn rule_yaml(id: &str) -> String {
        format!(
            "title: Whoami\nid: {id}\nstatus: test\nlogsource:\n    product: windows\n\
             detection:\n    sel:\n        CommandLine|contains: whoami\n    condition: sel\n"
        )
    }

    fn admin(dir: &Path) -> (CollectionAdmin, Arc<Processor>) {
        let mut engine = RuntimeEngine::new(
            dir.to_path_buf(),
            Vec::new(),
            CorrelationConfig::default(),
            false,
        );
        engine.load_rules().unwrap();
        let processor = Arc::new(Processor::new(engine));
        (CollectionAdmin::new(processor.clone()), processor)
    }

    async fn create(admin: &CollectionAdmin, yaml: String) -> Result<String, Status> {
        admin
            .create(Request::new(CreateRequest { yaml }))
            .await
            .map(|r| r.into_inner().id)
    }

    async fn set(admin: &CollectionAdmin, id: &str, enabled: bool) -> Result<(), Status> {
        admin
            .set_enabled(Request::new(SetEnabledRequest {
                id: id.to_string(),
                enabled,
            }))
            .await
            .map(|_| ())
    }

    #[tokio::test]
    async fn upload_is_saved_as_id_yaml_with_a_backup_and_loaded() {
        let dir = tempfile::tempdir().unwrap();
        let (admin, processor) = admin(dir.path());

        assert_eq!(create(&admin, rule_yaml(ID)).await.unwrap(), ID);
        let path = dir.path().join(format!("{ID}.yaml"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), rule_yaml(ID));
        assert_eq!(std::fs::read_to_string(backup_path(&path)).unwrap(), rule_yaml(ID));
        assert_eq!(processor.stats().detection_rules, 1);

        let err = create(&admin, rule_yaml(ID)).await.unwrap_err();
        assert_eq!(err.code(), tonic::Code::AlreadyExists);
    }

    #[tokio::test]
    async fn disable_and_enable_write_the_existing_yaml() {
        let dir = tempfile::tempdir().unwrap();
        let (admin, processor) = admin(dir.path());
        create(&admin, rule_yaml(ID)).await.unwrap();
        let path = dir.path().join(format!("{ID}.yaml"));

        set(&admin, ID, false).await.unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("enabled: false"));
        assert_eq!(std::fs::read_to_string(backup_path(&path)).unwrap(), text);
        assert_eq!(processor.stats().detection_rules, 0);
        let listed = scan_rules(dir.path());
        assert_eq!(listed.len(), 1);
        assert!(!listed[0].enabled);

        set(&admin, ID, true).await.unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), rule_yaml(ID));
        assert_eq!(processor.stats().detection_rules, 1);
        assert!(scan_rules(dir.path())[0].enabled);
    }

    #[tokio::test]
    async fn an_id_that_leaves_the_directory_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let rules = dir.path().join("rules");
        std::fs::create_dir(&rules).unwrap();
        let (admin, _) = admin(&rules);

        let err = create(&admin, rule_yaml("../escaped")).await.unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(!dir.path().join("escaped.yaml").exists());
    }

    #[tokio::test]
    async fn a_rule_that_does_not_load_is_not_left_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let (admin, processor) = admin(dir.path());
        // It parses, but the condition names a selection that does not exist,
        // so the engine cannot compile it.
        let bad = rule_yaml(ID).replace("condition: sel", "condition: missing");

        let err = create(&admin, bad).await.unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(err.message().starts_with("rule does not load"), "{}", err.message());
        let path = dir.path().join(format!("{ID}.yaml"));
        assert!(!path.exists() && !backup_path(&path).exists());
        assert_eq!(processor.stats().detection_rules, 0);
        // The next upload still works.
        create(&admin, rule_yaml(ID)).await.unwrap();
    }

    #[tokio::test]
    async fn a_legacy_disabled_file_moves_back_when_enabled() {
        let dir = tempfile::tempdir().unwrap();
        let legacy = dir.path().join("old.yml.disabled");
        std::fs::write(&legacy, rule_yaml(ID)).unwrap();
        let (admin, processor) = admin(dir.path());
        assert!(!scan_rules(dir.path())[0].enabled);
        assert_eq!(processor.stats().detection_rules, 0);

        set(&admin, ID, true).await.unwrap();
        assert!(!legacy.exists());
        assert_eq!(
            std::fs::read_to_string(dir.path().join("old.yml")).unwrap(),
            rule_yaml(ID)
        );
        assert_eq!(processor.stats().detection_rules, 1);
    }
}
