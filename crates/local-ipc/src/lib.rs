//! Authenticated local IPC restricted to the current Windows user.
//!
//! The Windows implementation will use access-controlled named pipes and will
//! keep request data and authentication material out of command-line arguments.
