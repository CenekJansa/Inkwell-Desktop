# Security Model

The approved Chrome extension is trusted to claim the website origin, document
name, preview PDF, and ByteRange content. The desktop verifies transport hashes
but does not prove that the preview represents the signed bytes.

## Protected assets

- PDF preview and ByteRange content
- Detached CMS output
- Certificate identity and provider metadata
- IPC authentication material
- Provider PINs and private keys

## Required controls

- Bound encoded and decoded input before large allocations.
- Keep document and signing data in memory unless a reviewed dependency makes a
  temporary file unavoidable.
- Restrict local IPC to the current Windows user and authenticate each session.
- Render PDFs without scripts, links, forms, attachments, or network access.
- Leave private keys and authorization prompts under Windows provider control.
- Keep native-host stdout exclusively for framed Chrome messages.
- Store only bounded metadata diagnostics, never sensitive values.
- Perform no telemetry, timestamp, revocation, certificate download, or other
  external request in the initial release.

Security-sensitive implementation choices require focused tests and review in
the milestone where they are introduced.
