# Inkwell Desktop Specification

**Status: Finished**

This specification is complete for the initial desktop application. Backend implementation and the production browser extension are outside its scope. Their behavior is described only at the protocol boundary where it determines what the desktop receives and returns.

## 1. Purpose

Inkwell Desktop is a Microsoft Windows 11 desktop application that signs PDF documents with a digital certificate available on the user's computer.

The application is launched and controlled by a browser extension through Google Chrome's native messaging mechanism. It displays a preview clone of a PDF, allows the user to choose a local certificate, signs the supplied original PDF ByteRange content, and returns a detached CMS result for insertion into the original PDF by the backend.

The desktop application's primary responsibilities are:

- Display the supplied PDF preview.
- List supported certificates available to the current Windows user.
- Sign the supplied PDF ByteRange content with the certificate selected by the user.
- Return the signed CMS bytes to the calling extension.

## 2. Goals

- Provide a clear and secure user-controlled PDF signing flow.
- Discover supported RSA and ECDSA signing certificates available through Windows.
- Display the received PDF before the user approves signing.
- Hash and sign the original PDF ByteRange content with the certificate selected by the user.
- Return detached CMS SignedData to the calling browser extension.
- Start automatically when invoked through browser native messaging, even when the desktop application is not already running.
- Keep document and certificate processing local to the user's computer.

## 3. Non-Goals

- Supporting operating systems other than Microsoft Windows 11.
- Guaranteeing certificate discovery or signing on macOS, Linux, or older Windows versions.
- Importing PDFs directly from the file system through the desktop user interface.
- Sending PDFs or private key material to a remote service.
- Managing, issuing, renewing, or revoking certificates.
- Editing PDF content.
- Implementing or specifying the backend beyond the high-level signing integration contract.
- Implementing or specifying the production browser extension.
- Inserting the returned CMS into the original PDF or validating the final assembled PDF.

## 4. Supported Platform and Technology

- Operating system: Microsoft Windows 11.
- Desktop framework: Tauri.
- Backend and native integration: Rust.
- Frontend: Vue 3 with TypeScript in a Tauri webview-based user interface.
- Browser integration: Google Chrome native messaging through an installed native messaging host manifest.
- Initial architecture: Windows 11 x64.

Certificate detection relies on Windows certificate facilities. Behavior on other operating systems is unsupported and is not guaranteed.

## 5. Actors and Components

### 5.1 User

The user reviews the PDF, chooses a certificate, and explicitly approves or cancels the signing operation.

### 5.2 Browser Extension Boundary

The production browser extension is outside the scope of this specification. From the desktop application's perspective, an extension:

- Initiates the native messaging connection.
- Sends the signing request and PDF data to the desktop application.
- Receives the signed part or a structured error response.
- Reassembles or otherwise processes the final PDF outside the desktop application, if required by the integration protocol.
- Supplies the website origin and document name for display to the user. These values are trusted claims from the approved extension, not independently authenticated by the desktop application.

### 5.3 Test Browser Extension

A minimal test extension must be provided for desktop application development and acceptance testing. It is not a production browser extension and does not define production extension architecture or user experience.

The test extension:

- Is loaded manually into Google Chrome as an unpacked extension through Developer mode.
- Simulates the backend and production extension behavior required by the native messaging contract.
- Allows a tester to provide or select a prepared preview PDF and concatenated ByteRange content.
- Opens the native messaging connection and sends a valid signing request.
- Displays or saves the returned detached CMS bytes for verification.
- Displays structured cancellation and error responses.
- Includes deterministic test fixtures and request identifiers where needed for repeatable tests.

### 5.4 Native Messaging Host

The native messaging host:

- Is registered with Google Chrome.
- Is started by Chrome when the extension opens a native messaging connection.
- Exchanges length-prefixed JSON messages over standard input and standard output according to the browser native messaging protocol.
- Starts or activates the Tauri desktop user interface for an incoming signing request.
- Must not write logs or diagnostic text to standard output because standard output is reserved for native messaging messages.

The native messaging host is a small companion Rust executable. It owns native messaging standard input and standard output, starts or activates the Tauri application, and waits for a terminal result. The companion host and Tauri application communicate through authenticated local IPC restricted to the current Windows user. PDF data, signing data, and authentication secrets must not be passed in command-line arguments.

### 5.5 Desktop Application

The desktop application:

- Displays one active signing request at a time.
- Renders the received PDF for review.
- Queries Windows for supported certificates.
- Performs the signing operation after explicit user approval.
- Returns the result to the native messaging host and browser extension.
- Shows an informational waiting screen when launched directly without a Chrome request. The screen explains that signing must be initiated from the browser extension and provides a Close action.

## 6. Primary Workflow

1. The browser extension opens a connection to the registered native messaging host.
2. Windows starts the native messaging host if it is not already running.
3. The extension sends a signing request containing a preview clone of the PDF and the concatenated bytes covered by the original PDF's ByteRange, in PDF order.
4. The native host validates the message structure, declared sizes, request identifier, and supported protocol version.
5. The desktop application opens or is brought to the foreground.
6. The application displays the PDF and basic request information to the user.
7. The user chooses to continue to certificate selection or cancels the request.
8. The application lists supported certificates found on the local Windows system.
9. The user selects a certificate and explicitly confirms signing.
10. Windows may request additional authorization, such as a smart-card PIN, depending on the certificate and private key provider.
11. The application computes SHA-256 over the supplied ByteRange content, constructs CMS signed attributes, and signs the DER-encoded signed attributes with the selected private key.
12. The application returns detached DER-encoded CMS SignedData to the browser extension in a success response.
13. The application briefly displays successful completion and exits after writing the response.
14. The application clears temporary request data from memory and local storage as soon as practical.

## 7. Functional Requirements

### 7.1 Request Intake

- Documents must enter the desktop application only through native messaging.
- Every request must include a unique request identifier.
- Every request must declare a protocol version.
- Binary values transported in JSON must be Base64 encoded.
- The preview PDF and concatenated ByteRange content must each have a maximum decoded size of 50 MiB.
- The application must reject malformed, incomplete, unsupported, or oversized requests before displaying them.
- The application must allow only one active signing request. A concurrent request must receive a busy error and must not be queued.
- An unanswered request must time out after 15 minutes, return a terminal timeout response when the connection remains available, clear request data, and exit.
- The request must include the website origin and a human-readable document name for display.
- Retrying after cancellation, timeout, or failure requires a new native messaging request. Request data must not be retained for an in-process retry.

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
- Any RSA or ECDSA certificate with an accessible private key must be selectable. Certificate validity, trust, and fitness for the intended legal purpose are the user's responsibility.
- Expired, not-yet-valid, or untrusted certificates must remain selectable and must have their status shown to the user when Windows exposes it.
- The certificate list must display, when available:
  - Subject or holder name.
  - Issuer name.
  - Validity period.
  - Certificate thumbprint or another stable identifier.
  - An indication that the key is hardware-backed or requires additional authorization, when Windows exposes that information.
- The application must handle certificates backed by Windows CNG/CryptoAPI providers, smart cards, or similar Windows-integrated key providers without exporting private keys.
- If no supported certificate is found, the application must explain this and allow the user to cancel or retry discovery.

### 7.4 Signing

- Signing must occur only after explicit user confirmation.
- The application must use the private key through its Windows key provider and must never export or persist private key material.
- The initial digest algorithm must be SHA-256. The signature operation must use the RSA or ECDSA algorithm supported by the selected private key.
- RSA signatures must use RSASSA-PKCS1-v1_5 with SHA-256. ECDSA signatures must use SHA-256 and a curve supported by the Windows key provider.
- The application must produce a detached CMS/PKCS#7 SignedData result and return it instead of a complete signed PDF.
- The CMS result must include the signer certificate and available intermediate certificates, but not the root certificate.
- The CMS `encapContentInfo` must identify `id-data` and omit the detached content.
- The signed attributes must include `content-type`, `message-digest`, `signing-certificate-v2`, and `signing-time`.
- `message-digest` must contain SHA-256 over the decoded concatenated ByteRange content.
- `signing-certificate-v2` must identify the selected signer certificate using SHA-256.
- `signing-time` must use the current Windows system time. It is informational and is not a trusted timestamp.
- Signed attributes and CMS SignedData must use deterministic DER encoding.
- CMS generation must follow RFC 5652. The `signing-certificate-v2` attribute must follow RFC 5035.
- Intermediate certificates may be sourced only from certificates already available in local Windows Current User or Local Machine CA stores. Missing intermediates must not be downloaded.
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
- A directly launched idle application may remain open until the user closes it. If a native host connects, the existing window must transition from the waiting screen to the signing request.

## 8. Native Messaging Protocol

### 8.1 Transport

Native messaging uses browser-managed standard input and standard output. Each JSON message is prefixed with a 32-bit message length in the byte order required by the browser native messaging specification.

JSON messages must use UTF-8. Base64 fields must use standard padded RFC 4648 Base64. SHA-256 fields must use 64 lowercase hexadecimal characters.

The initial protocol uses one JSON request message rather than chunking or a temporary-file handoff. Google Chrome allows a much larger message from the extension to the native host than from the host to the extension, but the application must enforce its own 50 MiB decoded limit for each binary request field. The encoded response must remain within Chrome's native-host-to-extension message limit, currently 1 MiB.

### 8.2 Request Envelope

```json
{
  "version": 1,
  "type": "sign_request",
  "requestId": "123e4567-e89b-42d3-a456-426614174000",
  "websiteOrigin": "https://example.com",
  "documentName": "document.pdf",
  "previewPdf": {
    "encoding": "base64",
    "data": "...",
    "sha256": "..."
  },
  "byteRangeContent": {
    "encoding": "base64",
    "data": "...",
    "sha256": "..."
  }
}
```

`byteRangeContent.data` contains all bytes covered by the original PDF ByteRange concatenated in PDF order. The desktop hashes exactly the decoded bytes and does not parse offsets or reconstruct the original PDF. `byteRangeContent.sha256` protects transport integrity and must equal the SHA-256 value calculated by the desktop. The trusted extension/backend is responsible for ensuring the preview clone accurately represents the original document and that the supplied content is the correct ByteRange content.

Request field requirements:

- `version` must equal `1`.
- `type` must equal `sign_request`.
- `requestId` must be a UUID generated by the extension and must not be reused.
- `websiteOrigin` must be a valid serialized web origin and must not exceed 2,048 UTF-8 bytes.
- `documentName` must be display text only, must not be interpreted as a file path, and must not exceed 255 UTF-8 bytes.
- `previewPdf.sha256` and `byteRangeContent.sha256` must match the decoded field data.
- The decoded PDF must begin with a valid PDF header and pass the selected renderer's structural validation.
- Unknown fields may be ignored for forward compatibility. Unknown protocol versions must be rejected.

### 8.3 Success Response

```json
{
  "version": 1,
  "type": "sign_success",
  "requestId": "123e4567-e89b-42d3-a456-426614174000",
  "cms": {
    "format": "cms-signed-data",
    "encoding": "base64-der",
    "data": "...",
    "sha256": "..."
  }
}
```

### 8.4 Cancellation Response

```json
{
  "version": 1,
  "type": "sign_cancelled",
  "requestId": "123e4567-e89b-42d3-a456-426614174000",
  "reason": "user_cancelled"
}
```

Cancellation `reason` is `user_cancelled` when the user selects Cancel and `window_closed` when the user closes an active request window.

### 8.5 Error Response

```json
{
  "version": 1,
  "type": "sign_error",
  "requestId": "123e4567-e89b-42d3-a456-426614174000",
  "error": {
    "code": "NO_CERTIFICATES",
    "message": "No supported signing certificate was found."
  }
}
```

Error codes must be stable and machine-readable. Human-readable messages may evolve and must be suitable for display by the extension.

For a structurally valid envelope, an error response must repeat its `requestId`. If the host cannot decode a valid envelope or request identifier, it may return `INVALID_MESSAGE` with `requestId` set to `null`. If writing a valid native message is impossible, it must close the connection without writing non-protocol output.

The success response contains no complete PDF and no raw private-key signature field. The backend is responsible for checking that the DER CMS fits the reserved PDF signature placeholder and for inserting it into the original PDF.

### 8.6 Terminal Behavior

- Every accepted request receives exactly one terminal response when the Chrome connection remains available.
- `sign_success`, `sign_cancelled`, and `sign_error` are terminal.
- A retry always uses a new request identifier and a new native messaging connection.
- If Chrome disconnects, the host must cancel any unsigned operation, tell the Tauri application to clear the request, and exit. A completed result must not be persisted for later delivery.
- The 15-minute timeout applies while waiting for review, certificate selection, or confirmation. Once the Windows private-key operation has begun, it must be allowed to complete or be cancelled by the key provider rather than being forcefully interrupted by the application timeout.

## 9. Security and Privacy Requirements

- All PDF processing and signing must occur locally.
- The application must not contact timestamp authorities, revocation services, telemetry endpoints, or other external services in the initial release.
- The application must not transmit document content, signatures, or certificate details anywhere except through the existing native messaging connection to the approved Chrome extension.
- Private keys must remain under the control of the Windows certificate/key provider.
- Temporary document data must not be written to disk unless required by a selected PDF-rendering or signing implementation.
- If temporary files are unavoidable, they must use restrictive permissions and be deleted after completion, cancellation, failure, or application restart recovery.
- Logs must not contain PDF content, ByteRange content, CMS output, PINs, private key data, or certificate identities.
- Persistent diagnostics must use metadata-only rotating logs under the current user's local application-data directory. Logs may contain application version, event code, timing, and request ID, but not website origin, document name, sensitive request data, certificate identity, or key-provider secrets.
- Logs must be bounded by rotation and removed during uninstall. Exact rotation sizes are an implementation configuration and must have a safe finite default.
- Incoming sizes and decoded lengths must be bounded to prevent memory exhaustion.
- Hashes included for transport integrity must be verified before preview or signing.
- The application must clearly display the requesting website origin as a claim supplied by the approved extension.
- The native messaging manifest must allow only explicitly supported browser extension identifiers.
- Production application executables and the installer must be code-signed for Windows.

## 10. Error Handling

The application must distinguish at least the following conditions:

- Unsupported protocol version.
- Invalid or malformed message.
- Request exceeds the configured size limit.
- PDF cannot be decoded, validated, or rendered.
- ByteRange content fails decoding, size, or transport-integrity validation.
- No supported certificate is available.
- Selected certificate becomes unavailable.
- Certificate access is denied.
- User cancels certificate-provider authorization.
- Signing operation fails.
- Native messaging connection is lost.
- An internal application error occurs.

Errors shown to the user must explain whether they can retry certificate discovery within the current request or must return to the browser and start a new request. Technical diagnostics must be written only to a safe log destination, never to native messaging standard output.

The initial machine-readable error codes are:

- `UNSUPPORTED_VERSION`
- `INVALID_MESSAGE`
- `REQUEST_TOO_LARGE`
- `BUSY`
- `REQUEST_TIMEOUT`
- `EXTENSION_DISCONNECTED`
- `PDF_INVALID`
- `PDF_ENCRYPTED`
- `PDF_RENDER_FAILED`
- `BYTERANGE_CONTENT_INVALID`
- `NO_CERTIFICATES`
- `CERTIFICATE_UNAVAILABLE`
- `KEY_ALGORITHM_UNSUPPORTED`
- `KEY_ACCESS_DENIED`
- `PROVIDER_CANCELLED`
- `SIGNING_FAILED`
- `RESPONSE_TOO_LARGE`
- `IPC_FAILED`
- `INTERNAL_ERROR`

## 11. User Interface Requirements

- The application must show the product name and clearly state that a document is awaiting review.
- The application must show the extension-supplied website origin and document name before signing.
- The PDF preview must be the primary focus of the signing screen.
- Signing controls must remain disabled until the PDF is successfully loaded and validated.
- Certificate selection must provide enough information for the user to distinguish certificates.
- The final confirmation must identify the selected certificate.
- Destructive or irreversible actions must not be triggered by window focus or a single accidental keystroke.
- The interface must remain responsive while certificates are discovered and signing is in progress.
- The application must prevent duplicate signing submissions while one operation is in progress.
- User-visible status must cover loading, certificate discovery, awaiting confirmation, signing, success, cancellation, and failure.
- On success, the application must briefly show confirmation after the native host accepts the response, then close automatically.
- The PDF viewer must provide only static page rendering, navigation, scrolling, and zoom. It must not execute scripts or expose forms, links, attachments, or embedded external content.

## 12. Installation and Registration

The Windows installer must:

- Install the desktop application and any companion native host executable.
- Install a native messaging host manifest containing the executable path and allowed extension identifiers.
- Register the manifest for Google Chrome in the current user's Windows Registry hive.
- Ensure executable and manifest paths remain valid after updates.
- Remove application files and native messaging registration during uninstall.
- Remove local diagnostic logs during uninstall.

The initial installer targets Windows 11 x64 and performs a per-user installation without machine-wide native host registration. The development installer must allow the unpacked test extension's stable extension identifier. A future production installer must receive its production extension identifier at build time and embed only identifiers intended for that build.

## 13. Initial Acceptance Criteria

- On Windows 11, the browser extension can invoke the application when it is not running.
- The unpacked test extension can be loaded through Chrome Developer mode and exercise the complete desktop request/response flow without a production backend.
- The application receives a valid PDF request and displays the PDF to the user.
- The application discovers and lists supported RSA and ECDSA certificates from Windows.
- The user can select a certificate and explicitly approve signing.
- The selected private key is used without being exported from its Windows provider.
- The application returns a detached CMS signed part containing the signer and available intermediate certificates, associated with the original request identifier.
- The application returns structured cancellation and error responses.
- Invalid input, a missing certificate, a signing failure, or an extension disconnect does not leave sensitive temporary data behind.
- The application does not emit non-protocol output on native messaging standard output.
- The application rejects concurrent requests and exits after success, cancellation, timeout, or error.
- The application rejects encrypted PDFs and request fields larger than 50 MiB after decoding.
- The application performs no timestamping, revocation lookup, telemetry, or other network access.
- For a known ByteRange test vector, the returned DER CMS verifies against the supplied content and selected certificate.
- RSA CMS signatures use SHA-256 with PKCS#1 v1.5; ECDSA CMS signatures use SHA-256.
- CMS signed attributes include content type, message digest, signing-certificate-v2, and local signing time.
- Direct launch shows browser-extension instructions, while an incoming request activates the existing UI.
- Cancellation, timeout, and failure require a fresh Chrome request to retry.

## 14. External Integration Contract

The desktop contract is complete without a backend implementation. At a high level, the external backend/extension flow must:

- Prepare the original PDF with an appropriately sized signature placeholder.
- Calculate the original PDF ByteRange and concatenate the covered bytes in PDF order.
- Supply those bytes and an accurate preview clone through the native messaging request.
- Receive the detached DER CMS bytes returned by the desktop application.
- Insert the CMS bytes into the original PDF's reserved signature placeholder.
- Validate and distribute the final signed PDF as required by the external system.

The production deployment values below are intentionally external configuration, not unfinished desktop requirements:

- Production Chrome extension identifier.
- Native messaging host name agreed with the extension.

Desktop development must use test vectors containing a preview PDF, concatenated ByteRange content, expected digest, and a signature placeholder large enough for representative CMS certificate chains. These fixtures must be usable from the unpacked test extension without a real backend.

The backend's PDF preparation, signature-placeholder sizing, CMS insertion, final PDF validation, and legal classification as QES, AdES, or another category are outside the desktop application's scope.

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
