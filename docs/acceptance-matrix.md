# Acceptance Matrix

This matrix traces specification areas to backlog items. Test IDs and evidence
links should be added as items are implemented.

| Specification requirement | Backlog coverage | Planned evidence |
| --- | --- | --- |
| Build-time deployment identity | M0.4 | Configuration validation and development public-key derivation tests |
| 7.1 Request intake | M1.1-M1.4, M2.3-M2.5 | Protocol and validation tests; M2 state-machine and desktop-state tests cover one-active-request, `BUSY`, timeout, cleanup, and terminal consumption |
| 7.2 PDF display | M3.1-M3.5 | PDF fixtures, renderer tests, and UI tests |
| 7.3 Certificate discovery | M4.1-M4.5 | Windows store integration tests and UI tests |
| 7.4 Signing | M5.1-M5.7, M6.1-M6.5 | Independent CMS verification vectors |
| 7.5 Cancellation | M2.4, M2.5, M6.5 | M2 state-machine and IPC-state tests cover user cancel, window close, timeout, and pre-signing disconnect; provider cases follow in M6.5 |
| 7.6 Application lifecycle | M2.1-M2.5, M6.4 | M2 authenticated transport, single-instance activation, host launch, busy, timeout, and disconnect tests; Windows runtime execution remains required |
| 8.1 Native transport | M1.2, M1.3 | M1: framing truncation, size-boundary, literal-prefix, and pipeline tests |
| 8.2 Request envelope | M1.1, M1.3, M7.3 | M1: exact wire-model and bounded field/payload validation tests; fixture coverage follows in M7.3 |
| 8.3 Success response | M1.1, M5.6, M5.7 | M1: exact success-envelope schema test; CMS evidence follows in M5 |
| 8.4 Cancellation response | M1.1, M2.4 | Exact schema, Vue cancellation, host pipeline, state-machine user/window cancellation, and desktop IPC notification tests |
| 8.5 Error response | M1.1, M6.5 | M1: all stable-code serialization and malformed/unsupported request response tests |
| 8.6 Terminal behavior | M2.3-M2.5, M6.3-M6.5 | M2 tokenized exactly-once, stale-token, busy, timeout, disconnect, response-delivery, and drop-cleanup tests; signing failure injection follows in M6/M8 |
| 9 Security and privacy | M2.1, M3.4, M8.1-M8.5 | M2 HMAC tamper/replay/direction tests, redacted key debug test, compact bounded framing, and Windows DACL compile evidence; runtime ACL inspection follows on Windows |
| 10 Error handling | M1.1, M3.3, M4.5, M6.5, M8.4 | Error mapping and end-to-end fixture tests |
| 11 User interface | M0.2, M3.5, M4.4, M6.1-M6.5 | Component tests and Windows usability checks |
| 12 Installation | M9.1-M9.4 | Clean install, upgrade, uninstall, and signature checks |
| 13 Initial acceptance | M7.5, M8.5, M9.5 | Recorded Windows 11 x64 acceptance run |
| 14 Test vectors and external boundary | M7.3, M7.4 | Deterministic fixtures and extension workflow |

## Acceptance gates

1. No milestone closes with unresolved high-severity security or exactly-once
   terminal-response defects.
2. Windows-specific claims require Windows 11 x64 evidence.
3. CMS output must verify with an implementation independent from the encoder.
4. The release installer must be tested from a clean current-user environment.
5. Deferred external components must not be pulled into desktop scope.
