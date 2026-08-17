//! The detection worker: ingested events → rsigma engine → OCSF findings.
//!
//! Mirrors StrIEM's `DetectionHandler`, but evaluates through rsigma's
//! [`RuntimeEngine`](rsigma_runtime::RuntimeEngine) (behind a
//! [`LogProcessor`](rsigma_runtime::LogProcessor) for atomic hot-reload) with
//! its [`LogSourceExtractor`](rsigma_eval::LogSourceExtractor) driving the
//! `logsource_compatible` conflict-pruning path.
//!
//! Each event is presented to the engine as a [`LogsourceEvent`]: the log body
//! (`Event.data`) supplies everything the rules match on, while the logsource —
//! which StrIEM carries in `Event.metadata["logsource"]`, *not* in the body —
//! is exposed only to the extractor. Every matched rule becomes an OCSF
//! Detection Finding, correlated back to the triggering event, and is broadcast
//! to the Vector egress client.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use log::{debug, error, info};
use rsigma_runtime::LogProcessor;
use serde_json::Value;
use striem_common::{SysMessage, event::Event};
use uuid::Uuid;

use crate::logsource_event::{LogsourceEvent, logsource_from_metadata};
use crate::ocsf::result_to_ocsf;
use tokio::sync::{broadcast, mpsc};

/// Background task turning ingested event batches into OCSF findings.
pub struct DetectionHandler {
    /// Raw ingested events from the Vector server (single-consumer).
    src: mpsc::Receiver<Vec<Event>>,
    /// Findings fanned out to downstream sinks (the Vector egress client).
    dest: broadcast::Sender<Arc<Vec<Event>>>,
    processor: Arc<LogProcessor>,
    batch_size: usize,
    shutdown: broadcast::Receiver<SysMessage>,
}

impl DetectionHandler {
    pub fn new(
        src: mpsc::Receiver<Vec<Event>>,
        dest: broadcast::Sender<Arc<Vec<Event>>>,
        processor: Arc<LogProcessor>,
        batch_size: usize,
        shutdown: broadcast::Receiver<SysMessage>,
    ) -> Self {
        Self {
            src,
            dest,
            processor,
            batch_size: batch_size.max(1),
            shutdown,
        }
    }

    /// Main loop with graceful shutdown. A failure processing one batch is
    /// logged and does not stop the worker.
    pub async fn run(&mut self) {
        loop {
            tokio::select! {
                msg = self.shutdown.recv() => {
                    match msg {
                        Ok(SysMessage::Shutdown) => {
                            info!("detection worker shutting down");
                            return;
                        }
                        Ok(_) => continue,
                        Err(_) => {
                            info!("shutdown channel closed, exiting detection worker");
                            return;
                        }
                    }
                },
                batch = self.src.recv() => {
                    match batch {
                        Some(events) => self.apply_batch(events),
                        None => {
                            info!("ingest channel closed, exiting detection worker");
                            return;
                        }
                    }
                }
            }
        }
    }

    /// Evaluate a batch of ingested events and emit any findings.
    fn apply_batch(&self, events: Vec<Event>) {
        for chunk in events.chunks(self.batch_size) {
            // The values the rules match against. For events StrIEM already
            // normalized to OCSF (metadata.ocsf set) with a `raw_data` string,
            // match the original vendor log so vendor-shaped Sigma rules still
            // fire post-normalization. Owned so the borrowed `LogsourceEvent`s
            // below can reference them for the whole batch.
            let payloads: Vec<Value> = chunk.iter().map(payload_value).collect();

            // Pair each payload with the logsource resolved from its metadata,
            // exposed to the extractor (and nothing else) via LogsourceEvent.
            let sigma_events: Vec<LogsourceEvent> = chunk
                .iter()
                .zip(&payloads)
                .map(|(event, data)| {
                    LogsourceEvent::new(data, logsource_from_metadata(&event.metadata))
                })
                .collect();
            let refs: Vec<&LogsourceEvent> = sigma_events.iter().collect();

            // Evaluate through the runtime engine (detection + correlation),
            // holding the engine lock only for the batch. `engine_snapshot`
            // keeps hot-reload atomic: a concurrent reload swaps the Arc, and
            // this batch finishes against the engine it already loaded.
            let results = {
                let guard = self.processor.engine_snapshot();
                let mut engine = guard.lock();
                engine.process_batch(&refs)
            };

            let mut findings: Vec<Event> = Vec::new();
            for (event, result) in chunk.iter().zip(results) {
                if result.is_empty() {
                    continue;
                }

                let correlation_uid = correlation_uid(event);
                let time_millis = event_time_millis(event);

                for r in &result {
                    let finding_id = Uuid::now_v7();
                    let data =
                        result_to_ocsf(r, &finding_id.to_string(), &correlation_uid, time_millis);

                    let mut finding = Event {
                        id: finding_id,
                        data,
                        // Carry the source event's Vector metadata so the egress
                        // client re-attaches source_type/timestamps downstream.
                        metadata: event.metadata.clone(),
                    };
                    // Keep the finding's own uid on the wire metadata too, so a
                    // Vector transform can key on it without descending into
                    // the OCSF body.
                    finding
                        .metadata
                        .insert("uid".to_string(), finding_id.to_string().into());
                    findings.push(finding);
                }
            }

            if findings.is_empty() {
                continue;
            }

            debug!("emitting {} OCSF finding(s)", findings.len());
            // A send error means no egress subscriber is attached; not fatal
            // (findings remain observable via logs).
            if let Err(e) = self.dest.send(Arc::new(findings)) {
                error!("no active finding subscriber: {e}");
            }
        }
    }
}

/// The JSON value an event is evaluated against.
fn payload_value(event: &Event) -> Value {
    if event.metadata.contains_key("ocsf")
        && let Some(Value::String(raw)) = event.data.get("raw_data")
        && let Ok(parsed) = serde_json::from_str::<Value>(raw)
    {
        return parsed;
    }
    event.data.clone()
}

/// Correlation id linking a finding back to its triggering event: the OCSF
/// `metadata.uid` on the source event if present, else the event's own id.
fn correlation_uid(event: &Event) -> String {
    event
        .data
        .get("metadata")
        .and_then(|m| m.get("uid"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| event.id.to_string())
}

/// The event time in epoch millis: Vector's `ingest_timestamp` when present,
/// otherwise now.
fn event_time_millis(event: &Event) -> i64 {
    event
        .metadata
        .get("vector")
        .and_then(|v| v.as_object())
        .and_then(|v| v.get("ingest_timestamp"))
        .and_then(|v| v.as_str())
        .and_then(|v| DateTime::parse_from_rfc3339(v).ok())
        .map(|dt| dt.with_timezone(&Utc).timestamp_millis())
        .unwrap_or_else(|| Utc::now().timestamp_millis())
}
