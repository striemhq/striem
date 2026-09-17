//! Vector gRPC protocol for StrIEM.
//!
//! This crate holds Vector's event protocol ([`event`]/[`vector`]). The
//! protocol receives logs and forwards logs. This crate gives the protocol
//! parts that you use again: [`Server`], [`Client`], and the generated service
//! stubs. The caller must run the service. The detection-admin protocol is in
//! the `striem_detection` crate. That protocol is not part of this Vector proxy
//! logic.

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
