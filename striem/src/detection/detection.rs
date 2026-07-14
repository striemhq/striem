//! Sigma rule detection engine.
//!
//! Evaluates streaming events against loaded Sigma rules and generates
//! OCSF detection_finding events (class_uid 2004) for matches.
//!
//! # Event Processing
//! 1. Receive batched events from Vector server
//! 2. Extract logsource metadata for rule filtering
//! 3. Use raw_data field if available (pre-normalization log)
//! 4. Evaluate against matching Sigma rules
//! 5. Generate detection finding with correlation to original event

use anyhow::Result;

use log::debug;
use log::{error, info};
use serde_json::{Value, json};
use sigmars::{SigmaCollection, SigmaRule};
use striem_common::{SysMessage, event::Event};

use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::sync::{broadcast, mpsc};
use chrono::{DateTime, Utc};

/// Background task processing events through the Sigma detection engine.
pub struct DetectionHandler {
    /// Raw ingested events from the Vector server (single-consumer).
    src: mpsc::Receiver<Vec<Event>>,
    /// Detection findings fanned out to downstream sinks (e.g. Vector client).
    dest: broadcast::Sender<Arc<Vec<Event>>>,
    rules: Arc<RwLock<SigmaCollection>>,
    shutdown: broadcast::Receiver<SysMessage>,
}

impl DetectionHandler {
    /// Create a new DetectionHandler instance.
    pub fn new(
        src: mpsc::Receiver<Vec<Event>>,
        dest: broadcast::Sender<Arc<Vec<Event>>>,
        rules: Arc<RwLock<SigmaCollection>>,
        shutdown: broadcast::Receiver<SysMessage>,
    ) -> Self {
        Self {
            src,
            dest,
            rules,
            shutdown,
        }
    }

    /// Main event processing loop with graceful shutdown support.
    ///
    /// # Error Handling
    /// Individual event processing errors are logged but don't halt the loop.
    /// This ensures one malformed event doesn't stop detection for all events.
    pub async fn run(&mut self) {
        loop {
            tokio::select! {
                msg = self.shutdown.recv() => {
                    if let Ok(SysMessage::Shutdown) = msg {
                            info!("Detection worker shutting down...");
                            return;
                    } else if msg.is_err() {
                        info!("shutdown channel closed, exiting detection worker...");
                        return;
                    }
                },
                batch = self.src.recv() => {
                    if let Some(events) = batch {
                        // Process each event independently to isolate failures
                        for event in events.iter() {
                            debug!("processing event {}...", event.id);
                            if let Err(e) = self.apply(event).await {
                                error!("error applying detection rules: {}", e);
                            }
                        }
                    } else {
                        info!("source channel closed");
                        return;
                    }
                }
            }
        }
    }

    /// Evaluate event against Sigma rules and emit detection findings.
    ///
    /// # Raw Data Handling
    /// If event is OCSF-normalized (metadata.ocsf = true) with raw_data field,
    /// rules are evaluated against the original vendor log format.
    /// This allows Sigma rules written for vendor formats to work with normalized data.
    ///
    /// # Performance Consideration
    /// Only acquires read lock on rules collection, allowing concurrent detection
    /// across multiple events. Lock is explicitly dropped after matching to avoid
    /// holding during detection finding generation.
    async fn apply(&self, event: &Event) -> Result<()> {
        // Extract logsource for rule filtering (e.g., windows/sysmon, aws/cloudtrail)
        let filter = event
            .metadata
            .get("logsource")
            .map(|v| sigmars::event::LogSource::from(v.clone()))
            .unwrap_or_default();

        let ts = event
            .metadata
            .get("vector")
            .and_then(|v| v.as_object())
            .and_then(|v| v.get("ingest_timestamp"))
            .and_then(|v| v.as_str())
            .and_then(|v| DateTime::parse_from_rfc3339(v).ok())
            .map(|dt| dt.with_timezone(&Utc).timestamp_millis())
            .unwrap_or_else(|| Utc::now().timestamp_millis());

        // For OCSF events, prefer raw_data field for rule evaluation
        // This allows vendor-specific Sigma rules to work post-normalization
        let raw_data = event
            .metadata
            .get("ocsf")
            .and_then(|_| match event.data.get("raw_data") {
                Some(Value::String(raw_data)) => serde_json::from_str::<Value>(raw_data).ok(),
                _ => None,
            });

        // Establish correlation between detection and original event
        // Uses OCSF metadata.uid if present, falls back to the incoming Vector event source_event_id
        let correlation_uid = event
            .data
            .as_object()
            .and_then(|v| v.get("metadata"))
            .and_then(|v| v.as_object())
            .and_then(|v| v.get("uid"))
            .and_then(|v| v.as_str())
            .map(|v| v.to_string())
            .unwrap_or_else(|| event.id.to_string());

        let data = match raw_data {
            Some(ref d) => d,
            None => &event.data,
        };

        let sigma_event = sigmars::event::RefEvent {
            data,
            metadata: &event.metadata,
            logsource: filter,
        };

        let rules = self.rules.read().await;

        // Get matching rules and convert to OCSF detection_finding events
        let detections = rules
            .matches(&sigma_event)
            .map_err(|e| anyhow::anyhow!("error applying rules: {}", e))?
            .iter()
            .filter_map(|d| rules.get(d))
            .filter_map(|d| {
                

                let mut ocsf = Event::default();

                // Convert Sigma detection to OCSF detection_finding (class_uid 2004)
                let mut data: Value = rule_to_ocsf(d);

                data["time"] = json!(ts);

                data["metadata"]["uid"] = json!(ocsf.id.to_string());
                data["metadata"]["correlation_uid"] = json!(correlation_uid);

                ocsf.data = data;

                ocsf.metadata
                    .extend(event.metadata.iter().map(|(k, v)| (k.clone(), v.clone())));


                Some(ocsf)
            })
            .collect::<Vec<_>>();

        drop(rules);

        if !detections.is_empty() {
            debug!("event {} matched {} detections", event.id, detections.len());
        }

        let _ = self.dest.send(Arc::new(detections));

        Ok(())
    }
}

pub fn rule_to_ocsf(rule: &SigmaRule) -> Value {

    let mut ocsf = json!({
        "category_uid": 2,
        "category_name": "Findings",
        "class_uid": 2004,
        "class_name": "Detection Finding",
        "activity_id": 1,
        "activity_name":  "Create",
        "type_uid": 200401,
        "type_name": "Detection Finding: Create",
        "status_id": 1,
        "status": "New",
        "metadata": {
            "version": "1.8.0",
            "product": {
                "vendor_name": "StrIEM",
                "name": "StrIEM"
            }
        },
        "finding_info": {
            "title": rule.title,
            "uid": rule.id,
            "desc": rule.description.clone().unwrap_or_default(),
            "analytic": {
                "type_id": 1,
                "type": "Rule",
                "name": rule.title,
                "uid": rule.id
            }
        }
    });

    if let Some(attacks) = get_ocsf_attacks(rule) {
        ocsf["finding_info"]["attacks"] = json!(attacks);
    }

    if let Some(ref level) = rule.level {
        let severity = level.chars()
                                .take(1)
                                .flat_map(|c| c.to_uppercase())
                                .chain(level.chars().skip(1))
                                .collect::<String>();
        ocsf["severity"] = json!(severity);
        ocsf["severity_id"] = match severity.as_str() {
            "Informational" => 1,
            "Low" => 2,
            "Medium" => 3,
            "High" => 4,
            "Critical" => 5,
            _ => 99,
        }.into();
    }
    ocsf
}

fn get_ocsf_attacks(rule: &SigmaRule) -> Option<Vec<Value>> {
    if let Some(tags) = &rule.tags {
        let (techniques, other): (Vec<&String>, Vec<&String>) = tags
            .iter()
            .partition(|tag| tag.starts_with("attack.t"));

        let attacks =  techniques
            .iter()
            .filter(|tag| tag.starts_with("attack.t"))
            .map(|tag| {
                let technique = format!("T{}", tag.trim_start_matches("attack.t"));
                if technique.contains('.') {
                    json!({"sub_technique": {"uid": technique }})
                } else {
                    json!({"technique": {"uid": technique }})
                }
            })
            .chain(other
                    .iter()
                    .filter(|tag| tag.starts_with("attack."))
                    .filter_map(|tag| {
                        match tag.trim_start_matches("attack.") {
                            "initial-access" => Some(json!({"tactic": {"uid": "TA0001", "name": "Initial Access"}})),
                            "execution" => Some(json!({"tactic": {"uid": "TA0002", "name": "Execution"}})),
                            "persistence" => Some(json!({"tactic": {"uid": "TA0003", "name": "Persistence"}})),
                            "privilege-escalation" => Some(json!({"tactic": {"uid": "TA0004", "name": "Privilege Escalation"}})),
                            "defense-evasion" => Some(json!({"tactic": {"uid": "TA0005", "name": "Stealth"}})),
                            "credential-access" => Some(json!({"tactic": {"uid": "TA0006", "name": "Credential Access"}})),
                            "discovery" => Some(json!({"tactic": {"uid": "TA0007", "name": "Discovery"}})),
                            "lateral-movement" => Some(json!({"tactic": {"uid": "TA0008", "name": "Lateral Movement"}})),
                            "collection" => Some(json!({"tactic": {"uid": "TA0009", "name": "Collection"}})),
                            "exfiltration" => Some(json!({"tactic": {"uid": "TA0010", "name": "Exfiltration"}})),
                            "command-and-control" => Some(json!({"tactic": {"uid": "TA0011", "name": "Command and Control"}})),
                            "impact" => Some(json!({"tactic": {"uid": "TA0040", "name": "Impact"}})),
                            _ => None,
                        }
                    }))
            .collect::<Vec<Value>>();
        if !attacks.is_empty() {
            return Some(attacks);
        }
    }
    None
}
