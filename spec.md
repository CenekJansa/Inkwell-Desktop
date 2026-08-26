# Inkwell Desktop Specification

## 1. Purpose

Inkwell Desktop is a Microsoft Windows 11 desktop application that signs PDF documents with a digital certificate available on the user's computer.

The application is launched and controlled by a browser extension through the browser's native messaging mechanism. It displays a PDF received from the extension, allows the user to choose an eligible local certificate, signs the designated part of the PDF, and returns only the signed result requested by the extension.

## 2. Goals

- Provide a clear and secure user-controlled PDF signing flow.
- Discover eligible signing certificates available through Windows.
- Display the received PDF before the user approves signing.
- Sign the designated PDF content with the certificate selected by the user.
- Return the signed part to the calling browser extension.
- Start automatically when invoked through browser native messaging, even when the desktop application is not already running.
- Keep document and certificate processing local to the user's computer.

## 3. Non-Goals

- Supporting operating systems other than Microsoft Windows 11.
- Guaranteeing certificate discovery or signing on macOS, Linux, or older Windows versions.
- Importing PDFs directly from the file system through the desktop user interface.
- Sending PDFs or private key material to a remote service.
- Managing, issuing, renewing, or revoking certificates.
- Editing PDF content.

## 4. Supported Platform and Technology

- Operating system: Microsoft Windows 11.
- Desktop framework: Tauri.
- Backend and native integration: Rust.
- Frontend: Tauri webview-based user interface. The specific frontend framework is not yet defined.
- Browser integration: Google Chrome native messaging through an installed native messaging host manifest.
- Initial architecture: Windows 11 x64.

Certificate detection relies on Windows certificate facilities. Behavior on other operating systems is unsupported and is not guaranteed.

## 5. Actors and Components

### 5.1 User

The user reviews the PDF, chooses a certificate, and explicitly approves or cancels the signing operation.

### 5.2 Browser Extension

The browser extension:

- Initiates the native messaging connection.
- Sends the signing request and PDF data to the desktop application.
- Receives the signed part or a structured error response.
- Reassembles or otherwise processes the final PDF outside the desktop application, if required by the integration protocol.
- Supplies the website origin and document name for display to the user. These values are trusted claims from the approved extension, not independently authenticated by the desktop application.

### 5.3 Native Messaging Host

The native messaging host:

- Is registered with Google Chrome.
- Is started by Chrome when the extension opens a native messaging connection.
- Exchanges length-prefixed JSON messages over standard input and standard output according to the browser native messaging protocol.
- Starts or activates the Tauri desktop user interface for an incoming signing request.
- Must not write logs or diagnostic text to standard output because standard output is reserved for native messaging messages.

The native messaging host may be implemented as part of the main executable or as a small companion executable. This architecture must be decided during technical design.

### 5.4 Desktop Application

The desktop application:

- Displays one active signing request at a time.
- Renders the received PDF for review.
- Queries Windows for eligible certificates.
- Performs the signing operation after explicit user approval.
- Returns the result to the native messaging host and browser extension.

## 6. Primary Workflow

1. The browser extension opens a connection to the registered native messaging host.
2. Windows starts the native messaging host if it is not already running.
3. The extension sends a signing request containing a preview clone of the PDF and a separate cryptographic signing payload prepared for the original document.
4. The native host validates the message structure, declared sizes, request identifier, and supported protocol version.
5. The desktop application opens or is brought to the foreground.
6. The application displays the PDF and basic request information to the user.
7. The user chooses to continue to certificate selection or cancels the request.
8. The application lists eligible certificates found on the local Windows system.
9. The user selects a certificate and explicitly confirms signing.
10. Windows may request additional authorization, such as a smart-card PIN, depending on the certificate and private key provider.
11. The application signs the designated PDF part.
12. The application returns only the signed part to the browser extension in a success response.
13. The application briefly displays successful completion and exits after writing the response.
14. The application clears temporary request data from memory and local storage as soon as practical.

## 7. Functional Requirements

### 7.1 Request Intake

- Documents must enter the desktop application only through native messaging.
- Every request must include a unique request identifier.
- Every request must declare a protocol version.
- Binary values transported in JSON must be Base64 encoded.
- The preview PDF and signing payload must each have a maximum decoded size of 50 MiB.
- The application must reject malformed, incomplete, unsupported, or oversized requests before displaying them.
- The application must allow only one active signing request. A concurrent request must receive a busy error and must not be queued.
- An unanswered request must time out after 15 minutes, return a terminal timeout response when the connection remains available, clear request data, and exit.
- The request must include the website origin and a human-readable document name for display.

### 7.2 PDF Display

- The user must be able to view the complete received PDF before signing.
- The viewer must support page navigation, scrolling, and zooming.
- The viewer must clearly indicate when a document failed validation or cannot be rendered.
- Failure to render the PDF must prevent signing.
- Encrypted or password-protected PDFs must be rejected in the initial release.
- The preview must be static. PDF JavaScript, external links, forms, embedded files, and other interactive behavior must not be executed or opened.
- The displayed PDF is a preview clone supplied by the trusted browser extension. The desktop application does not independently prove that the separate signing payload represents the same original document.

### 7.3 Certificate Discovery

- The application must query the current user's Windows Personal (`CurrentUser\\MY`) certificate store.
- Any certificate with an accessible private key must be selectable. Certificate validity, trust, and fitness for the intended legal purpose are the user's responsibility.
- Expired, not-yet-valid, or untrusted certificates must remain selectable and must have their status shown to the user when Windows exposes it.
- The certificate list should display, when available:
  - Subject or holder name.
  - Issuer name.
  - Validity period.
  - Certificate thumbprint or another stable identifier.
  - An indication that the key is hardware-backed or requires additional authorization, when Windows exposes that information.
- The application must handle certificates backed by Windows CNG/CryptoAPI providers, smart cards, or similar Windows-integrated key providers without exporting private keys.
- If no eligible certificate is found, the application must explain this and allow the user to cancel or retry discovery.

### 7.4 Signing

- Signing must occur only after explicit user confirmation.
- The application must use the private key through its Windows key provider and must never export or persist private key material.
- The initial digest algorithm must be SHA-256. The signature operation must use the RSA or ECDSA algorithm supported by the selected private key.
- The application must produce a detached CMS/PKCS#7 SignedData result and return it instead of a complete signed PDF.
- The CMS result must include the signer certificate and available intermediate certificates, but not the root certificate.
- The exact interpretation of the incoming signing payload and the required CMS signed attributes remain part of the unresolved browser/backend integration contract.
- The backend is responsible for inserting the returned CMS result into the original PDF.
- The application must not classify or label a signature as QES, AdES, or another legal category. The backend or verifier is responsible for classification.
- The application must support private keys on QSCD and other hardware tokens through Windows providers, including provider-controlled PIN and consent prompts. This makes QES workflows possible but does not guarantee that a resulting signature legally qualifies as QES.
- A signing failure must not produce a partial success response.

### 7.5 Cancellation

- The user may cancel before signing begins.
- Closing the application window while a request is active must be treated as cancellation unless the user is prompted to confirm.
- Cancellation must be returned to the extension as a structured response.
- If the extension disconnects, the application must cancel any unsigned request and clear its data.

### 7.6 Application Lifecycle

- Invocation by the extension must launch the native host automatically through the browser's native messaging support.
- If the desktop user interface is already running, a new invocation must activate the existing instance rather than open an unrelated duplicate user interface.
- The application must reject concurrent requests as busy rather than queue them.
- The application must return a terminal response for each accepted request: success, cancellation, or error.
- The application must exit after returning a terminal response.

## 8. Native Messaging Protocol

### 8.1 Transport

Native messaging uses browser-managed standard input and standard output. Each JSON message is prefixed with a 32-bit message length in the byte order required by the browser native messaging specification.

The initial protocol uses one JSON request message rather than chunking or a temporary-file handoff. Google Chrome allows a much larger message from the extension to the native host than from the host to the extension, but the application must enforce its own 50 MiB decoded limit for each binary request field. The encoded response must remain within Chrome's native-host-to-extension message limit, currently 1 MiB.

### 8.2 Proposed Request Envelope

```json
{
  "version": 1,
  "type": "sign_request",
  "requestId": "unique-request-id",
  "websiteOrigin": "https://example.com",
  "documentName": "document.pdf",
  "previewPdf": {
    "encoding": "base64",
    "data": "...",
    "sha256": "..."
  },
  "signingPayload": {
    "encoding": "base64",
    "data": "...",
    "sha256": "..."
  }
}
```

This envelope is provisional. `previewPdf` is a trusted display clone and `signingPayload` is prepared from the original document by the extension/backend flow. The precise format and cryptographic meaning of `signingPayload` still need to be defined.

### 8.3 Proposed Success Response

```json
{
  "version": 1,
  "type": "sign_success",
  "requestId": "unique-request-id",
  "signedPart": {
    "format": "cms-signed-data",
    "encoding": "base64-der",
    "data": "...",
    "sha256": "..."
  }
}
```

### 8.4 Proposed Cancellation Response

```json
{
  "version": 1,
  "type": "sign_cancelled",
  "requestId": "unique-request-id"
}
```

### 8.5 Proposed Error Response

```json
{
  "version": 1,
  "type": "sign_error",
  "requestId": "unique-request-id",
  "error": {
    "code": "NO_ELIGIBLE_CERTIFICATE",
    "message": "No eligible signing certificate was found."
  }
}
```

Error codes must be stable and machine-readable. Human-readable messages may evolve and should be suitable for display by the extension.

## 9. Security and Privacy Requirements

- All PDF processing and signing must occur locally.
- The application must not contact timestamp authorities, revocation services, telemetry endpoints, or other external services in the initial release.
- The application must not transmit document content, signatures, or certificate details anywhere except through the existing native messaging connection to the approved Chrome extension.
- Private keys must remain under the control of the Windows certificate/key provider.
- Temporary document data must not be written to disk unless required by a selected PDF-rendering or signing implementation.
- If temporary files are unavoidable, they must use restrictive permissions and be deleted after completion, cancellation, failure, or application restart recovery.
- Logs must not contain PDF content, signing payloads, PINs, private key data, or full certificate details.
- Incoming sizes and decoded lengths must be bounded to prevent memory exhaustion.
- Hashes included for transport integrity must be verified before preview or signing.
- The application must clearly display the extension or requesting origin when that identity can be authenticated through the integration design.
- The native messaging manifest must allow only explicitly supported browser extension identifiers.
- The distributed application and installer should be code-signed for Windows.

## 10. Error Handling

The application must distinguish at least the following conditions:

- Unsupported protocol version.
- Invalid or malformed message.
- Request exceeds the configured size limit.
- PDF cannot be decoded, validated, or rendered.
- Signing payload fails decoding, size, or transport-integrity validation.
- No eligible certificate is available.
- Selected certificate becomes unavailable.
- Certificate access is denied.
- User cancels certificate-provider authorization.
- Signing operation fails.
- Native messaging connection is lost.
- An internal application error occurs.

Errors shown to the user should explain whether they can retry, choose another certificate, or must return to the browser. Technical diagnostics should be written only to a safe log destination, never to native messaging standard output.

## 11. User Interface Requirements

- The application must show the product name and clearly state that a document is awaiting review.
- The application must show the extension-supplied website origin and document name before signing.
- The PDF preview must be the primary focus of the signing screen.
- Signing controls must remain disabled until the PDF is successfully loaded and validated.
- Certificate selection must provide enough information for the user to distinguish certificates.
- The final confirmation must identify the selected certificate.
- Destructive or irreversible actions must not be triggered by window focus or a single accidental keystroke.
- The interface should remain responsive while certificates are discovered and signing is in progress.
- The application must prevent duplicate signing submissions while one operation is in progress.
- User-visible status must cover loading, certificate discovery, awaiting confirmation, signing, success, cancellation, and failure.

## 12. Installation and Registration

The Windows installer must:

- Install the desktop application and any companion native host executable.
- Install a native messaging host manifest containing the executable path and allowed extension identifiers.
- Register the manifest for Google Chrome in the current user's Windows Registry hive.
- Ensure executable and manifest paths remain valid after updates.
- Remove application files and native messaging registration during uninstall.

The initial installer targets Windows 11 x64 and performs a per-user installation without machine-wide native host registration.

## 13. Initial Acceptance Criteria

- On Windows 11, the browser extension can invoke the application when it is not running.
- The application receives a valid PDF request and displays the PDF to the user.
- The application discovers and lists eligible certificates from Windows.
- The user can select a certificate and explicitly approve signing.
- The selected private key is used without being exported from its Windows provider.
- The application returns a detached CMS signed part containing the signer and available intermediate certificates, associated with the original request identifier.
- The application returns structured cancellation and error responses.
- Invalid input, a missing certificate, a signing failure, or an extension disconnect does not leave sensitive temporary data behind.
- The application does not emit non-protocol output on native messaging standard output.
- The application rejects concurrent requests and exits after success, cancellation, timeout, or error.
- The application rejects encrypted PDFs and request fields larger than 50 MiB after decoding.
- The application performs no timestamping, revocation lookup, telemetry, or other network access.

## 14. Open Questions

The following integration details remain unresolved:

1. What exactly does `signingPayload` contain?
   - The original PDF byte-range content for the app to hash with SHA-256?
   - A precomputed SHA-256 digest of the original PDF byte ranges?
   - DER-encoded CMS signed attributes prepared by the backend?
   - Another backend-defined structure?
2. Which CMS signed attributes and encoding rules does the backend require for insertion and verification?
3. Must the CMS result conform to a specific CAdES or PAdES baseline profile, even though legal QES/AdES classification remains outside the desktop app?
4. Is the proposed DER-encoded detached CMS response the exact artifact the backend can insert, or does the backend require raw signature bytes or additional metadata?
5. What are the production and development Chrome extension identifiers for the native messaging manifest allowlist?
6. What exact error-code catalogue and retry behavior does the extension expect?

## 15. Future Considerations

These items are outside the initial scope but may affect architecture:

- Trusted timestamp authority integration.
- Certificate chain and revocation validation.
- PAdES long-term validation profiles.
- Multiple signatures on one PDF.
- Enterprise deployment and managed browser policies.
- Automatic application updates.
- Accessibility and localization requirements.
- Auditing that records events without recording sensitive document content.
