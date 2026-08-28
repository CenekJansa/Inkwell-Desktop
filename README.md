# Inkwell Desktop

Windows 11 desktop application for reviewing a PDF and signing supplied PDF
ByteRange content with a certificate managed by Windows.

The product contract is defined in [`spec.md`](spec.md). The implementation is
organized as a Rust workspace with a Vue 3 and Tauri 2 desktop frontend.

## Status

The repository currently contains the project foundation and implementation
backlog. Protocol handling, PDF rendering, certificate discovery, signing, and
installation are tracked in [`docs/implementation-backlog.md`](docs/implementation-backlog.md).

## Prerequisites

- Node.js 22.13 or newer
- npm 10 or newer
- Rust stable (installed automatically from `rust-toolchain.toml` by rustup)
- Windows 11 x64 with the Microsoft C++ Build Tools for desktop development

Windows is required for native messaging, certificate-provider, installer, and
end-to-end validation. Frontend development and platform-independent Rust tests
can run on other supported development systems once Rust is installed.

## Commands

```sh
npm install
npm run dev
npm run check
npm test
cargo test --workspace
```

See [`docs/development.md`](docs/development.md) for detailed setup and
[`docs/architecture.md`](docs/architecture.md) for component boundaries.
