# Scripts

Repository automation belongs here. Scripts must be non-interactive in CI and
must never embed production extension IDs, signing credentials, or test secrets.

## PDFium preparation

`prepare-pdfium.ps1` prepares the pinned Windows x64 PDFium distribution described
by `third_party/pdfium/provenance.json`. The checked-in manifest intentionally
contains `TODO` provenance values, so preparation fails before downloading or
changing staged files until every placeholder is replaced with reviewed data.

After pinning a distribution, record its exact version, HTTPS download URL,
SHA-256 digest, archive format and paths, attestation reference, and license
identifiers in the manifest.
Run the script noninteractively from the repository root:

```powershell
pwsh -NoProfile -NonInteractive -File scripts/prepare-pdfium.ps1
```

The script downloads to an operating-system temporary directory, verifies the
archive digest before extraction, checks the configured archive paths, and stages
only `pdfium.dll`, its import library, and headers under
`third_party/pdfium/artifacts/windows-x64/`. That output is ignored by Git and
must be regenerated locally or in CI; do not commit PDFium binaries.
