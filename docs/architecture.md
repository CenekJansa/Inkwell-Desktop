# Architecture

Inkwell is split into independently testable processes and libraries. Sensitive
document bytes stay in memory and are passed only over local, access-controlled
channels.

## Processes

| Component | Responsibility | Trust boundary |
| --- | --- | --- |
| Chrome extension | Supplies requests and receives one terminal response | Approved extension ID only |
| Native host | Owns Chrome stdin/stdout framing and connection lifecycle | No non-protocol stdout |
| Tauri desktop | Owns user interaction and request orchestration | One active request |
| PDF renderer | Converts PDF pages into static images through PDFium | No network or interactive PDF behavior |
| Windows key provider | Owns private keys and authorization prompts | Keys are never exported |

## Rust crates

| Crate | Owns | Must not own |
| --- | --- | --- |
| `inkwell-protocol` | Wire models and stable codes | I/O or business rules |
| `inkwell-request-validation` | Bounded decoding and integrity checks | UI or signing |
| `inkwell-app-core` | State transitions and terminal outcomes | Tauri or Windows APIs |
| `inkwell-local-ipc` | Authenticated current-user IPC | Native messaging framing |
| `inkwell-windows-certificates` | Store discovery and provider operations | CMS encoding or UI |
| `inkwell-cms-signing` | Deterministic DER CMS construction | Private-key ownership |
| `inkwell-deployment-config` | Validated build-time host and extension identity | Registration or credentials |
| `inkwell-safe-logging` | Metadata-only diagnostics | Sensitive payload values |

Dependencies point inward toward protocol and app-core. As feature milestones
are implemented, the Tauri application and binaries will compose these
libraries; shared crates do not depend on Tauri.

## Request lifecycle

The platform-independent state machine supports `reviewing`, `discovering`,
`confirming`, `signing`, and `responding`, with absence of an active token
representing idle. Only the state machine may claim or release the active slot.
Terminal methods consume the matching token, clear request-owned buffers, and
return the outcome to the caller without retaining it for retry.

## Initial technical choices

- Vue 3, TypeScript, Vite, and Tauri 2 for the desktop UI.
- Rust workspace for native components and shared logic.
- Windows named pipes with current-user access control for host-to-app IPC.
- PDFium in an isolated sidecar for static page rasterization.
- Tauri NSIS packaging for the initial per-user Windows installer.

Choices involving Windows APIs, ASN.1/CMS libraries, and the exact PDFium
distribution must pass a focused proof of concept before feature implementation.

## Host-to-desktop lifecycle

The native host validates and decodes requests before transferring compact raw
buffers over authenticated, sequenced frames. On Windows the transport is a
named pipe with a protected DACL granting access only to SYSTEM and the current
owner; an ephemeral session key is exchanged inside that pipe and never through
process arguments. The desktop accepts connections concurrently but its shared
state machine rejects a second active request with `BUSY`.

The host starts the desktop without request data or secrets in its arguments.
Tauri's single-instance plugin activates the existing main window, and an IPC
event transitions a directly launched waiting screen to the incoming request.
Chrome EOF races with the terminal desktop response; EOF sends an authenticated
disconnect command, clears unsigned work, and does not retain the generated
disconnect outcome.
