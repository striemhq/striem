//! Detection-admin gRPC protocol code.
//!
//! The build makes this code from `proto/detection.proto` (package
//! `striem.detection.v1`). This crate has only the protocol. It has the
//! `Detections` service client stubs and server stubs. It also has the message
//! types. The detection microservice (server) and the API service (client)
//! share these types.

#![allow(clippy::all)]

include!(concat!(env!("OUT_DIR"), "/striem.detection.v1.rs"));
