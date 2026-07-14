//! Detection-admin gRPC protocol bindings.
//!
//! Generated from `proto/detection.proto` (package `striem.detection.v1`). This
//! crate is protocol-only: it carries the `Detections` service client/server
//! stubs and message types shared between the detection microservice (server)
//! and the API service (client).

#![allow(clippy::all)]

include!(concat!(env!("OUT_DIR"), "/striem.detection.v1.rs"));
