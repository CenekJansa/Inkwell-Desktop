# Repository Guide

## Current State

- This repository is still a foundation: most Rust crates and both binaries contain only boundary documentation or empty entrypoints. Treat `spec.md` as the product contract and `docs/implementation-backlog.md` as planned work; do not describe planned behavior as implemented.
- npm has one workspace, `apps/desktop`. Cargo owns the Tauri crate, the two `bins/*` processes, and all `crates/*` libraries.
- The web entrypoint is `apps/desktop/src/main.ts`; the Tauri entrypoint is `apps/desktop/src-tauri/src/lib.rs`. Shared crates must stay independent of Tauri and Windows UI concerns.

## Commands

- Use Node 22.13+ with npm 10+: the repository declares Node 22.12+, but the locked Vite version requires at least 22.13 on the Node 22 line. Install with `npm install`; fetch Rust dependencies with `cargo fetch`.
- `npm run dev` starts the full Tauri app. For browser-only Vue work, use `npm run dev --workspace=@inkwell/desktop` (Vite is fixed to `127.0.0.1:1420`).
- `npm run check` runs frontend lint/format checks, Vitest, typechecking, and the Vite build in that order. Run one frontend test with `npm run test --workspace=@inkwell/desktop -- src/App.test.ts`.
- Rust verification is `cargo fmt --all --check`, then `cargo clippy --locked --workspace --all-targets -- -D warnings`, then `cargo test --locked --workspace`. Focus a crate with `cargo test --locked -p inkwell-app-core` (substitute its Cargo package name).
- `npm run build` builds web assets only. `npm run desktop:build` builds the Tauri executable and NSIS installer.

## Platform Boundaries

- Frontend and platform-independent Rust work can run off Windows. Native messaging registration, current-user named-pipe security, PDFium packaging, Windows certificate/provider behavior, installer work, and end-to-end acceptance require Windows 11 x64.
- `apps/desktop/src-tauri/gen/schemas/`, `dist/`, `target/`, and coverage output are generated/ignored; do not hand-maintain them.
- Unit tests belong next to source. Reserve `tests/` for suites crossing crate or process boundaries. Fixtures must be deterministic and non-sensitive; follow the categories in `fixtures/README.md`.

## Security Invariants

- Native-host stdout is exclusively for length-prefixed Chrome protocol messages. Send diagnostics through the safe-logging boundary, never stdout.
- Do not put PDF/ByteRange/CMS data, origins, document names, certificate identities, provider secrets, IPC authentication material, or private keys in logs, process arguments, fixtures, or persistent storage. Private keys stay inside the Windows provider.
- Preserve one-active-request and exactly-one-terminal-response semantics: concurrent requests are rejected rather than queued, and completed results are not retained for retry or later delivery.
- Keep PDF rendering static and isolated: no scripts, links, forms, attachments, actions, network access, telemetry, certificate downloads, revocation calls, or timestamp services.
- Changes completing backlog items should update `docs/acceptance-matrix.md`; Windows-specific claims require Windows evidence, and CMS output requires verification independent of its encoder.
