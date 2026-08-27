//! Windows certificate discovery and provider-backed private-key operations.
//!
//! This boundary owns access to `CurrentUser\\MY`, CNG, `CryptoAPI`, smart cards,
//! and hardware providers. Private key material must never leave the provider.
