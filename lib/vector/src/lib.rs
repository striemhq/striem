//! Vector gRPC protocol implementation for StrIEM.
//!
//! This crate owns Vector's event protocol ([`event`]/[`vector`]) for ingesting
//! and forwarding logs. It provides the reusable protocol pieces ([`Server`],
//! [`Client`], the generated service stubs); running a service is the caller's
//! responsibility. The detection-admin protocol lives in the `striem_detection`
//! crate, which is unrelated to this Vector proxy logic.

mod convert;

mod client;
mod server;

#[allow(unused)]
pub mod event {
    include!(concat!(env!("OUT_DIR"), "/proto/event.rs"));
}

#[allow(unused)]
pub mod vector {
    include!(concat!(env!("OUT_DIR"), "/proto/vector.rs"));
}

pub use client::Client;
pub use server::{Server, VectorService};
