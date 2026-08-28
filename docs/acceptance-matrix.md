# Acceptance Matrix

This matrix traces specification areas to backlog items. Test IDs and evidence
links should be added as items are implemented.

| Specification requirement | Backlog coverage | Planned evidence |
| --- | --- | --- |
| Build-time deployment identity | M0.4 | Configuration validation and development public-key derivation tests |
| 7.1 Request intake | M1.1-M1.4, M2.3-M2.5 | Protocol unit tests and native-host integration tests |
| 7.2 PDF display | M3.1-M3.5 | PDF fixtures, renderer tests, and UI tests |
| 7.3 Certificate discovery | M4.1-M4.5 | Windows store integration tests and UI tests |
| 7.4 Signing | M5.1-M5.7, M6.1-M6.5 | Independent CMS verification vectors |
| 7.5 Cancellation | M2.4, M2.5, M6.5 | State-machine and end-to-end cancellation tests |
| 7.6 Application lifecycle | M2.1-M2.5, M6.4 | Windows process and activation tests |
| 8.1 Native transport | M1.2, M1.3 | Framing fuzz and boundary tests |
| 8.2 Request envelope | M1.1, M1.3, M7.3 | Serialization and invalid fixture tests |
| 8.3 Success response | M1.1, M5.6, M5.7 | Schema and CMS response tests |
| 8.4 Cancellation response | M1.1, M2.4 | Schema and UI cancellation tests |
| 8.5 Error response | M1.1, M6.5 | Stable-code and response tests |
| 8.6 Terminal behavior | M2.3-M2.5, M6.3-M6.5 | Exactly-once failure-injection tests |
| 9 Security and privacy | M2.1, M3.4, M8.1-M8.5 | Security audit and runtime network inspection |
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
