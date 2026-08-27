# Test Fixtures

Deterministic, non-sensitive fixtures belong in the following directories:

- `pdf/`: preview PDFs, including malformed and encrypted cases
- `byte-range/`: concatenated ByteRange content and expected SHA-256 values
- `certificates/`: test-only public certificates and documented key setup
- `requests/`: complete valid and invalid protocol envelopes

Fixtures must never contain real customer documents, production certificates,
or reusable private credentials.
