//! # detection
//!
//! A StrIEM detection-service variant powered by the rsigma engine.
//!
//! ```text
//!   Vector ──gRPC PushEvents──▶ [striem_vector server] ──▶ Processor / RuntimeEngine
//!                                                                 │  logsource_compatible
//!                                                                 │  conflict pruning
//!                                                                 ▼
//!   Vector ◀─gRPC PushEvents── [striem_vector client] ◀── OCSF Detection Findings (2004)
//! ```
//!
//! This crate uses StrIEM's Vector input and output ([`striem_vector`]) and its
//! event model ([`striem_common`]) with no change. But it replaces the `sigmars`
//! detection engine with rsigma: a [`Processor`](engine::Processor) around a
//! [`RuntimeEngine`](engine::RuntimeEngine), modeled on rsigma-runtime's. Each
//! match becomes an OCSF
//! `Detection Finding` (class_uid 2004). The crate sends the finding to a
//! downstream Vector.
//!
//! ## The pruning path
//!
//! StrIEM's `detection` service selects rules with a logsource **subset** filter
//! (a rule runs only if its logsource is a subset of the event's logsource).
//! This service installs a [`VectorLogSourceExtractor`] on the engine in
//! place of the subset filter. This selects rsigma's **conflict-based**
//! `logsource_compatible` evaluation. The engine skips a rule only when a
//! logsource dimension of the rule *conflicts* with the event's extracted
//! logsource. A rule with no conflict still runs. A rule with no logsource also
//! runs. An event with no logsource evaluates against all the rules (fail-open).
//! See [`config`] for the configuration of the extractor.

pub mod admin;
pub mod config;
pub mod detection;
pub mod engine;
pub mod event;
pub mod logsource;
pub mod ocsf;
pub mod service;
mod proto;

pub use config::{Config, LogsourceConfig};
pub use detection::DetectionHandler;
pub use engine::{EngineStats, Processor, RuntimeEngine};
pub use logsource::VectorLogSourceExtractor;
pub use service::DetectionService;
pub use proto::*;
