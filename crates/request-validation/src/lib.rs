//! Bounded request decoding and validation.
//!
//! Validation includes envelope fields, lengths, Base64 decoding, transport
//! hashes, and initial PDF checks. It must complete before request display.
