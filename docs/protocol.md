# Protocol Implementation Notes

The normative protocol is section 8 of [`../spec.md`](../spec.md). This document
records implementation boundaries and must not redefine that contract.

The native host owns Chrome's 32-bit length prefix and standard streams. The
`inkwell-protocol` crate owns JSON models, while `inkwell-request-validation`
owns validation, bounded Base64 decoding, and transport hash verification.

Unknown fields are ignored for forward compatibility. Unknown versions are
rejected. Every accepted request receives exactly one terminal response while
the connection remains available.

No implementation may write diagnostics, panic text, progress output, or child
process output to the native host's standard output.
