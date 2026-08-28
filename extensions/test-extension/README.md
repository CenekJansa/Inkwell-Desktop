# Test Extension

This directory will contain the unpacked Manifest V3 Chrome extension used for
development and acceptance testing. Its implementation is tracked by M7.1 and
M7.2 in `docs/implementation-backlog.md`.

The extension ID and native host name must be stable development configuration,
not reused as production deployment values.

The development identity is:

- Native host name: `com.inkwell.desktop.dev`
- Extension ID: `bigiacfmnlcbgamdkjepnkabampiiape`

`development-public-key.base64` contains the DER public key encoded for a future
Manifest V3 `key` field. Chrome derives the extension ID above from that key.
The extension implementation remains tracked by M7.1 and M7.2.
