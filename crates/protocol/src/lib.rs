//! Versioned wire models shared by the native host and desktop application.
//!
//! This crate owns JSON shapes and stable machine-readable codes. It does not
//! perform transport I/O, request validation, or signing.

mod v1;

pub use v1::{
    BinaryPayload, CancellationReason, CmsPayload, ErrorCode, ErrorDetail, SignCancelled,
    SignError, SignRequest, SignSuccess, TerminalResponse, VERSION,
};
