//! # rsigma-detection
//!
//! A StrIEM detection-service variant powered by the rsigma engine.
//!
//! ```text
//!   Vector ──gRPC PushEvents──▶ [striem_vector server] ──▶ LogProcessor / RuntimeEngine
//!                                                                 │  logsource_compatible
//!                                                                 │  conflict pruning
//!                                                                 ▼
//!   Vector ◀─gRPC PushEvents── [striem_vector client] ◀── OCSF Detection Findings (2004)
//! ```
//!
//! It reuses StrIEM's Vector ingest/egress ([`striem_vector`]) and event model
//! ([`striem_common`]) unchanged, but replaces the `sigmars` detection engine
//! with rsigma-runtime's [`LogProcessor`](rsigma_runtime::LogProcessor) wrapping
//! a [`RuntimeEngine`](rsigma_runtime::RuntimeEngine). Every match becomes an
//! OCSF `Detection Finding` (class_uid 2004) pushed back to a downstream Vector.
//!
//! ## The pruning path
//!
//! StrIEM's `detection` service selects rules with a logsource **subset**
//! filter (a rule runs only if its logsource ⊆ the event's). This service
//! instead installs a [`LogSourceExtractor`](rsigma_eval::LogSourceExtractor)
//! on the engine, selecting rsigma's **conflict-based** `logsource_compatible`
//! evaluation: a rule is skipped only when a logsource dimension it declares
//! *conflicts* with the event's extracted logsource. Rules with no conflict —
//! and all logsource-less rules — still run, and an event with no extractable
//! logsource evaluates against everything (fail-open). See [`config`] for how
//! the extractor is configured.

pub mod admin;
pub mod config;
pub mod detection;
pub mod logsource_event;
pub mod ocsf;
pub mod service;

pub use config::{Config, LogsourceConfig};
pub use detection::DetectionHandler;
pub use service::DetectionService;
