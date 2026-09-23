//! The detection worker: input events → rsigma engine → OCSF findings.
//!
//! This worker is the same as StrIEM's `DetectionHandler`. But it evaluates
//! through rsigma's [`RuntimeEngine`](rsigma_runtime::RuntimeEngine). A
//! [`LogProcessor`](rsigma_runtime::LogProcessor) holds the engine for an atomic
//! hot-reload. The engine's
//! [`LogSourceExtractor`](rsigma_eval::LogSourceExtractor) drives the
//! `logsource_compatible` conflict-pruning path.
//!
//! The worker gives each event to the engine as a [`LogsourceEvent`]. The log
//! body (`Event.data`) gives all the values that the rules match on. StrIEM
//! carries the logsource in `Event.metadata["logsource"]`, *not* in the body.
//! The worker shows the logsource only to the extractor. Each matched rule
//! becomes an OCSF Detection Finding. The finding links back to the event that
//! caused it. The worker sends it to the Vector output client.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use log::{debug, error, info};
use rsigma_runtime::LogProcessor;
use serde_json::Value;
use striem_common::{SysMessage, event::Event};
use uuid::Uuid;

use crate::event::{LogsourceEvent, logsource_from_metadata};
use crate::ocsf::result_to_ocsf;
use tokio::sync::{broadcast, mpsc};

/// The background task that changes batches of input events into OCSF findings.
pub struct DetectionHandler {
    /// The raw events from the Vector server (one consumer).
    src: mpsc::Receiver<Vec<Event>>,
    /// The findings. The worker sends them to the downstream sinks (the Vector
    /// output client).
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

    /// The main loop. It supports a clean shutdown. The worker logs a failure
    /// for one batch, but it does not stop.
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

    /// Evaluates a batch of input events and makes the findings.
    fn apply_batch(&self, events: Vec<Event>) {
        striem_telemetry::metrics().incr_events("ingested", events.len() as u64);
        for chunk in events.chunks(self.batch_size) {
            // The values that the rules match against. StrIEM can normalize an
            // event to OCSF (metadata.ocsf set) and keep a `raw_data` string.
            // For such an event, match the original vendor log. Thus a
            // vendor-shaped Sigma rule still fires after normalization. These
            // values are owned. Thus the `LogsourceEvent`s below can point to
            // them for the full batch.
            let payloads: Vec<Value> = chunk.iter().map(payload_value).collect();

            // Join each payload with the logsource from its metadata. The
            // LogsourceEvent shows this logsource only to the extractor.
            let sigma_events: Vec<LogsourceEvent> = chunk
                .iter()
                .zip(&payloads)
                .map(|(event, data)| {
                    LogsourceEvent::new(data, logsource_from_metadata(&event.metadata))
                })
                .collect();
            let refs: Vec<&LogsourceEvent> = sigma_events.iter().collect();

            // Evaluate through the runtime engine (detection and correlation).
            // Hold the engine lock only for the batch. `engine_snapshot` keeps
            // the hot-reload atomic. A reload at the same time swaps the Arc.
            // This batch finishes with the engine that it already loaded.
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
                        // Keep the source event's Vector metadata. Thus the
                        // output client adds the source_type and the timestamps
                        // again downstream.
                        metadata: event.metadata.clone(),
                    };
                    // Keep the finding's own uid on the wire metadata too. Thus a
                    // Vector transform can use it without a read of the OCSF
                    // body.
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
            striem_telemetry::metrics().incr_events("findings", findings.len() as u64);
            // A send error means there is no output subscriber. This is not
            // fatal. You can still see the findings in the logs.
            if let Err(e) = self.dest.send(Arc::new(findings)) {
                error!("no active finding subscriber: {e}");
            }
        }
    }
}

/// The JSON value that the engine evaluates an event against.
fn payload_value(event: &Event) -> Value {
    if event.metadata.contains_key("ocsf")
        && let Some(Value::String(raw)) = event.data.get("raw_data")
        && let Ok(parsed) = serde_json::from_str::<Value>(raw)
    {
        return parsed;
    }
    event.data.clone()
}

/// The correlation id that links a finding to the event that caused it. It is
/// the OCSF `metadata.uid` on the source event if there is one. If not, it is
/// the event's own id.
fn correlation_uid(event: &Event) -> String {
    event
        .data
        .get("metadata")
        .and_then(|m| m.get("uid"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| event.id.to_string())
}

/// The event time in epoch milliseconds. It is Vector's `ingest_timestamp` if
/// there is one. If not, it is the current time.
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
