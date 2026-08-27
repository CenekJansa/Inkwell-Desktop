# Development

## Toolchain

Install Node.js 22.12 or newer, npm 10 or newer, and rustup. The checked-in Rust
toolchain file pins Rust and installs the Windows x64 MSVC target.

On Windows, also install the Microsoft Visual Studio Build Tools with the
"Desktop development with C++" workload and WebView2.

## Setup

```sh
npm install
cargo fetch
```

Do not add production extension identifiers, code-signing credentials, private
test certificates, or document fixtures containing real data to the repository.

## Development commands

```sh
npm run dev
npm run check
npm run desktop:build
npm test
cargo fmt --all --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
```

`npm run dev` starts the Tauri application. `npm run build` builds only the web
assets because Tauri invokes it as a pre-build step; `npm run desktop:build`
builds the desktop executable and installer. Running only the web frontend is
available through `npm run dev --workspace=@inkwell/desktop`.

## Windows-only work

Native messaging registration, named-pipe security, PDFium packaging, Windows
certificate discovery, provider-backed signing, and installer acceptance must
be developed and verified on Windows 11 x64. Platform-independent protocol,
state-machine, CMS structure, and frontend tests should remain runnable in CI.

## Definition of done

Every backlog item must include automated tests where practical, update the
acceptance matrix, avoid sensitive logging, and document manual Windows steps.
An accepted signing request must never gain more than one terminal response.
