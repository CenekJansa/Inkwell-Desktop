//! Platform-independent signing request state machine.
//!
//! This crate owns request exclusivity, legal state transitions, timeout
//! policy, and exactly-once terminal outcomes. UI and Windows APIs are adapters.
