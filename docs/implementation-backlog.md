# Implementation Backlog

This backlog splits [`../spec.md`](../spec.md) into issue-sized work. Items are
ordered as vertical milestones so each milestone ends with an observable,
testable capability. Production extension and backend implementation remain out
of scope.

## Status values

- `Foundation`: repository structure exists, but feature behavior is not built.
- `Complete`: completion criteria are implemented and verified where the current platform permits.
- `Ready`: dependencies and expected outcome are defined.
- `Blocked`: requires an earlier item or an explicit proof-of-concept decision.

## M0: Workspace foundation

| ID | Status | Work item | Completion criteria |
| --- | --- | --- | --- |
| M0.1 | Complete | Create Rust workspace | Desktop, host, renderer, and shared crates are workspace members with pinned formatting and lint tooling. |
| M0.2 | Foundation | Create Vue and Tauri desktop | Vue, TypeScript, Vite, and Tauri build configuration exists; direct launch renders the waiting screen. |
| M0.3 | Complete | Establish repository checks | npm formatting, linting, tests, frontend build, Rust formatting, clippy, tests, and Windows CI are configured. |
| M0.4 | Complete | Define deployment configuration | Development host name and extension ID are injected at build time; production values and credentials remain outside source control. |

Milestone exit: a clean checkout can run all platform-independent checks, and a
Windows developer can open the direct-launch application.

## M1: Protocol-to-UI walking skeleton

| ID | Depends on | Work item | Completion criteria |
| --- | --- | --- | --- |
| M1.1 | M0 | Define protocol models | Version 1 request and all terminal envelopes serialize exactly as specified; all stable error codes are represented and tested. |
| M1.2 | M1.1 | Implement native messaging framing | Host reads and writes browser-compatible length-prefixed JSON, handles truncation, and never writes diagnostics to stdout. |
| M1.3 | M1.1 | Implement bounded request validation | Version, type, UUID, origin, name, Base64, decoded sizes, hashes, and PDF header are validated before display. |
| M1.4 | M1.2, M1.3 | Deliver cancellation walking skeleton | An in-process host-to-UI harness displays origin and document name and returns one valid cancellation response; secure process IPC follows in M2.1. |

Milestone exit: Chrome-compatible input can cross the host boundary and produce
a structured terminal response without PDF rendering or signing.

## M2: Secure IPC and lifecycle

| ID | Depends on | Work item | Completion criteria |
| --- | --- | --- | --- |
| M2.1 | M0.4 | Implement current-user IPC | Host and app exchange authenticated framed data through a named pipe restricted to the current Windows user; no secret is passed on the command line. |
| M2.2 | M2.1 | Implement single-instance activation | Direct launch shows waiting guidance; host launch starts or activates one existing UI instance. |
| M2.3 | M1.1 | Implement request state machine | Legal states, one active request, `BUSY`, and exactly-once terminal transitions are unit tested outside Tauri. |
| M2.4 | M2.3 | Implement cancellation and timeout | User cancel, window close, 15-minute timeout, and pre-signing disconnect clear data and return the correct outcome when possible. |
| M2.5 | M2.1, M2.3 | Implement host disconnect propagation | Chrome disconnect causes the host to notify the app, cancel unsigned work, clear memory, and exit without persisting results. |

Milestone exit: lifecycle behavior is complete with mock document and signing
adapters, including busy, timeout, disconnect, and direct-launch behavior.

## M3: Static PDF preview

| ID | Depends on | Work item | Completion criteria |
| --- | --- | --- | --- |
| M3.1 | M0 | Prove PDFium sidecar packaging | A pinned PDFium distribution loads from a packaged sidecar on Windows x64 and renders an in-memory sample without network or temporary files. |
| M3.2 | M3.1 | Define private renderer protocol | Bounded framed messages support document open, page metadata, page render, close, and structured renderer errors. |
| M3.3 | M3.2 | Validate PDFs | Malformed and encrypted PDFs produce `PDF_INVALID` and `PDF_ENCRYPTED`; render failures produce `PDF_RENDER_FAILED`. |
| M3.4 | M3.2 | Render static pages | PDF pages become raster images; scripts, links, forms, attachments, actions, and external resources are never executed or opened. |
| M3.5 | M3.3, M3.4 | Build review interface | Complete-document scrolling, navigation, zoom, loading, and render-failure states work; progression remains disabled until validation succeeds. |

Milestone exit: a valid request can be reviewed as static page images and an
invalid, encrypted, or unrenderable preview cannot proceed.

## M4: Windows certificate discovery

| ID | Depends on | Work item | Completion criteria |
| --- | --- | --- | --- |
| M4.1 | M0 | Prove Windows provider access | A focused Windows test enumerates `CurrentUser\\MY` and acquires RSA and ECDSA provider handles without exporting key material. |
| M4.2 | M4.1 | Implement certificate enumeration | Selectable certificates include any accessible RSA or ECDSA private key across supported CNG, CryptoAPI, smart-card, and hardware providers. |
| M4.3 | M4.2 | Map certificate display data | Subject, issuer, validity, thumbprint, algorithm, status, and exposed hardware or authorization indicators are represented without entering logs. |
| M4.4 | M4.3 | Build discovery and selection UI | Loading, empty, failure, retry, selection, and invalid or untrusted status states are usable and responsive. |
| M4.5 | M4.2 | Revalidate selected certificate | Removed, inaccessible, or changed certificates fail with the correct stable error before signing. |

Milestone exit: users can distinguish and select all eligible local certificates,
including expired, not-yet-valid, or untrusted certificates.

## M5: Detached CMS signing

| ID | Depends on | Work item | Completion criteria |
| --- | --- | --- | --- |
| M5.1 | M4.1 | Prove CMS library interoperability | A selected ASN.1 strategy builds custom DER signed attributes and accepts externally provider-produced RSA and ECDSA signatures. |
| M5.2 | M5.1 | Construct signed attributes | DER attributes include content type, SHA-256 message digest, signing-certificate-v2, and Windows system signing time. |
| M5.3 | M4.5, M5.2 | Implement RSA provider signing | SHA-256 signed attributes are signed with provider-backed PKCS#1 v1.5 and verify independently. |
| M5.4 | M4.5, M5.2 | Implement ECDSA provider signing | SHA-256 signed attributes are signed with the provider-supported curve and encoded in CMS-compatible DER form. |
| M5.5 | M4.2 | Build local certificate chain set | Signer and locally available intermediates are included; roots are excluded and missing certificates are never downloaded. |
| M5.6 | M5.2-M5.5 | Assemble detached SignedData | Deterministic DER CMS identifies detached `id-data`, verifies against exact ByteRange fixture bytes, and contains no raw signature response field. |
| M5.7 | M5.6 | Enforce response size | The encoded native response is checked against Chrome's host-to-extension limit and returns `RESPONSE_TOO_LARGE` atomically. |

Milestone exit: known RSA and ECDSA ByteRange vectors produce independently
verifiable detached CMS with all required attributes and certificates.

## M6: Complete signing experience

| ID | Depends on | Work item | Completion criteria |
| --- | --- | --- | --- |
| M6.1 | M3.5, M4.4 | Add explicit confirmation | Confirmation identifies the selected certificate and cannot be triggered by focus or an accidental single keystroke. |
| M6.2 | M5.6, M6.1 | Orchestrate provider signing | UI stays responsive, duplicate submissions are impossible, and provider PIN or consent prompts remain provider-controlled. |
| M6.3 | M2.4, M6.2 | Apply signing timeout semantics | Request timeout stops once provider signing starts; the app does not forcibly interrupt the provider operation. |
| M6.4 | M1.2, M5.7 | Deliver success response | Success is shown only after host acceptance; app then briefly confirms and exits without retaining the result. |
| M6.5 | M6.2 | Normalize signing failures | Provider cancel, access denial, unavailable key, unsupported algorithm, signing failure, and internal error never produce partial success. |

Milestone exit: the user can review, select, explicitly approve, sign, and return
one terminal result through a mock or direct host harness.

## M7: Test extension and fixtures

| ID | Depends on | Work item | Completion criteria |
| --- | --- | --- | --- |
| M7.1 | M0.4, M1.2 | Create unpacked Manifest V3 extension | Stable development ID connects only to the development native host and submits prepared requests from Chrome Developer mode. |
| M7.2 | M7.1 | Add fixture controls and result view | Tester selects fixtures, uses deterministic request IDs, views structured errors, and saves returned CMS bytes and hash. |
| M7.3 | M1.3, M3.3 | Create protocol and PDF fixtures | Valid, malformed, unsupported, oversized, integrity-failing, encrypted, and render-failing requests are deterministic and non-sensitive. |
| M7.4 | M5.6 | Create RSA and ECDSA signing vectors | Fixtures contain preview, ByteRange bytes, expected digest, public test certificate, and placeholder sizing metadata. |
| M7.5 | M6.4, M7.2 | Automate extension-to-host smoke test | Windows test proves Chrome can launch a stopped app and receive success, cancellation, and representative errors. |

Milestone exit: the complete desktop flow is testable without a production
backend or production extension.

## M8: Security and reliability hardening

| ID | Depends on | Work item | Completion criteria |
| --- | --- | --- | --- |
| M8.1 | M1 | Implement safe rotating diagnostics | Finite metadata-only logs use local app data, never stdout, and exclude every sensitive category in the specification. |
| M8.2 | M3-M6 | Audit memory and temporary data | Allocations are bounded, request buffers are cleared promptly, renderer cleanup is reliable, and any unavoidable file has restrictive permissions and recovery deletion. |
| M8.3 | M3-M6 | Verify offline operation | Runtime inspection proves no telemetry, PDF external access, timestamping, revocation lookup, or certificate download. |
| M8.4 | M2-M6 | Exercise failure injection | Disconnects, IPC failure, renderer crash, provider cancel, key removal, timeout, panic, and response failure preserve exactly-once terminal semantics. |
| M8.5 | M1-M6 | Perform sensitive-output audit | Automated and manual checks confirm logs, errors, process arguments, crash paths, and stdout contain no protected data. |

Milestone exit: security requirements have explicit tests or documented manual
evidence, and all known failure paths clean up safely.

## M9: Installer and release acceptance

| ID | Depends on | Work item | Completion criteria |
| --- | --- | --- | --- |
| M9.1 | M3.1, M7.1 | Package per-user Windows installer | NSIS installs branded desktop, host, renderer, PDFium, manifest, and dependencies at stable current-user paths with production icon assets. |
| M9.2 | M9.1 | Register native messaging host | Installer writes the Chrome current-user registry entry and manifest permits only extension IDs selected for that build. |
| M9.3 | M9.2 | Implement upgrade and uninstall | Upgrade preserves valid paths; uninstall removes registration, files, and diagnostic logs. |
| M9.4 | M9.1 | Add release code signing | CI signs executables and installer with protected credentials and verifies signatures before publishing artifacts. |
| M9.5 | M7.5, M8, M9.4 | Execute acceptance matrix | Every initial acceptance criterion has automated or recorded Windows 11 x64 evidence for RSA, ECDSA, and available hardware-provider scenarios. |

Milestone exit: a code-signed per-user installer passes the complete initial
acceptance matrix on Windows 11 x64.

## Explicitly deferred work

- Production browser extension and backend
- PDF placeholder creation, ByteRange calculation, and CMS insertion
- Timestamp authorities and revocation lookup
- PAdES long-term validation profiles
- Legal QES or AdES classification
- Multiple signatures, automatic updates, localization, and enterprise rollout
