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

use log::{error, info, trace};
use serde_json::{Value, json};
use sigmars::SigmaCollection;
use striem_common::event::Event;

use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::sync::broadcast;

/// Background task processing events through the Sigma detection engine.
pub(crate) struct DetectionHandler {
    src: broadcast::Receiver<Arc<Vec<Event>>>,
    dest: broadcast::Sender<Arc<Vec<Event>>>,
    rules: Arc<RwLock<SigmaCollection>>,
    shutdown: broadcast::Receiver<()>,
}

impl DetectionHandler {
    pub(crate) fn new(
        src: broadcast::Receiver<Arc<Vec<Event>>>,
        dest: broadcast::Sender<Arc<Vec<Event>>>,
        rules: Arc<RwLock<SigmaCollection>>,
        shutdown: broadcast::Receiver<()>,
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
    pub(crate) async fn run(&mut self) {
        loop {
            tokio::select! {
                _ = self.shutdown.recv() => {
                    info!("Detection worker shutting down...");
                    return;
                },
                result = self.src.recv() => {
                    if let Ok(events) = result {
                        // Process each event independently to isolate failures
                        for event in events.iter() {
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

        // For OCSF events, prefer raw_data field for rule evaluation
        // This allows vendor-specific Sigma rules to work post-normalization
        let raw_data = event
            .metadata
            .get("ocsf")
            .and_then(|_| match event.data.get("raw_data") {
                Some(Value::String(raw_data)) => serde_json::from_str::<Value>(raw_data).ok(),
                _ => None,
            });

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
            .get_matches_from_ref(&sigma_event)
            .await
            .map_err(|e| anyhow::anyhow!("error applying rules: {}", e))?
            .iter()
            .filter_map(|d| rules.get(d))
            .filter_map(|d| {
                // Establish correlation between detection and original event
                // Uses OCSF metadata.uid if present, falls back to StrIEM's event ID
                let correlation_uid = event
                    .data
                    .as_object()
                    .and_then(|v| v.get("metadata"))
                    .and_then(|v| v.as_object())
                    .and_then(|v| v.get("uid"))
                    .and_then(|v| v.as_str())
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| event.id.to_string());

                let mut ocsf = Event::default();

                // Convert Sigma detection to OCSF detection_finding (class_uid 2004)
                let mut data: Value = d.into();
                data["metadata"]["uid"] = json!(event.id.to_string());
                data["metadata"]["correlation_uid"] = json!(correlation_uid);
                data["metadata"]["product"] = json!({
                    "vendor_name": "StrIEM",
                    "product_name": "StrIEM"
                });
                ocsf.data = data;
                ocsf.metadata
                    .extend(event.metadata.iter().map(|(k, v)| (k.clone(), v.clone())));
                ocsf.metadata.extend([
                    ("ocsf".to_string(), json!(true)),
                    ("striem".to_string(), json!(true)),
                ]);
                Some(ocsf)
            })
            .collect::<Vec<_>>();
        drop(rules);

        if !detections.is_empty() {
            trace!("event {} matched {} detections", event.id, detections.len());
        }
        let _ = self.dest.send(Arc::new(detections));
        Ok(())
    }
}
