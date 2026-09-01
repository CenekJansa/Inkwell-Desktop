//! Chrome native-messaging framing and request pipeline.
//!
//! Standard output belongs exclusively to the framed writer supplied here.

mod framing;

use std::{
    borrow::Cow,
    io::{Read, Write},
};

pub use framing::{
    FrameError, MAX_REQUEST_FRAME_BYTES, MAX_RESPONSE_FRAME_BYTES, read_frame, write_frame,
};
use inkwell_protocol::{ErrorCode, SignError, SignRequest, TerminalResponse};
use inkwell_request_validation::{
    ValidatedRequest, ValidationError, is_valid_request_id, validate_request,
};
use serde::Deserialize;
use thiserror::Error;

#[derive(Deserialize)]
struct EnvelopeMetadata<'a> {
    version: Option<u32>,
    #[serde(borrow, rename = "requestId")]
    request_id: Option<Cow<'a, str>>,
}

#[derive(Clone, Copy, Debug, Error)]
#[error("the desktop UI is unavailable")]
pub struct UiUnavailable;

pub trait UiAdapter {
    /// Transfers a validated request and waits for one terminal outcome.
    ///
    /// # Errors
    ///
    /// Returns `UiUnavailable` when the temporary or production UI channel is
    /// not available.
    fn handle_request(
        &mut self,
        request: ValidatedRequest,
    ) -> Result<TerminalResponse, UiUnavailable>;
}

#[derive(Debug, Error)]
pub enum HostError {
    #[error("native messaging framing failed")]
    Framing(#[from] FrameError),
    #[error("terminal response serialization failed")]
    Serialization(#[from] serde_json::Error),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServeOutcome {
    CleanEof,
    Responded,
}

/// Reads and handles one request, writing at most one terminal response.
///
/// # Errors
///
/// Returns an error only when a terminal response cannot be serialized or
/// written. Input failures are converted to structured terminal errors.
pub fn serve_one<R: Read, W: Write, U: UiAdapter>(
    mut reader: R,
    mut writer: W,
    ui: &mut U,
) -> Result<ServeOutcome, HostError> {
    let body = match read_frame(&mut reader, MAX_REQUEST_FRAME_BYTES) {
        Ok(Some(body)) => body,
        Ok(None) => return Ok(ServeOutcome::CleanEof),
        Err(FrameError::TooLarge) => {
            return write_terminal(
                &mut writer,
                &TerminalResponse::Error(protocol_error(
                    None,
                    ErrorCode::RequestTooLarge,
                    "The request exceeds the supported size limit.",
                )),
            );
        }
        Err(FrameError::TruncatedPrefix | FrameError::TruncatedBody) => {
            return write_terminal(
                &mut writer,
                &TerminalResponse::Error(protocol_error(
                    None,
                    ErrorCode::InvalidMessage,
                    "The native message is truncated.",
                )),
            );
        }
        Err(error @ FrameError::Io(_)) => return Err(error.into()),
    };
    drop(reader);

    let metadata: EnvelopeMetadata<'_> = match serde_json::from_slice(&body) {
        Ok(metadata) => metadata,
        Err(_) => {
            return write_terminal(
                &mut writer,
                &TerminalResponse::Error(protocol_error(
                    None,
                    ErrorCode::InvalidMessage,
                    "The request is not a valid JSON envelope.",
                )),
            );
        }
    };
    let error_request_id = metadata
        .request_id
        .as_deref()
        .filter(|request_id| is_valid_request_id(request_id))
        .map(str::to_owned);
    if metadata
        .version
        .is_some_and(|version| version != inkwell_protocol::VERSION)
    {
        return write_terminal(
            &mut writer,
            &TerminalResponse::Error(protocol_error(
                error_request_id,
                ErrorCode::UnsupportedVersion,
                "The protocol version is unsupported.",
            )),
        );
    }
    drop(metadata);

    let request: SignRequest = match serde_json::from_slice(&body) {
        Ok(request) => request,
        Err(_) => {
            return write_terminal(
                &mut writer,
                &TerminalResponse::Error(protocol_error(
                    error_request_id,
                    ErrorCode::InvalidMessage,
                    "The request envelope is incomplete or invalid.",
                )),
            );
        }
    };
    drop(body);

    let validated = match validate_request(request) {
        Ok(request) => request,
        Err(error) => {
            return write_terminal(
                &mut writer,
                &TerminalResponse::Error(validation_response(error_request_id, error)),
            );
        }
    };

    let response = handle_validated_request(ui, validated);
    write_terminal(&mut writer, &response)
}

fn handle_validated_request<U: UiAdapter>(
    ui: &mut U,
    request: ValidatedRequest,
) -> TerminalResponse {
    let request_id = request.request_id().to_owned();
    match ui.handle_request(request) {
        Ok(response) => response,
        Err(UiUnavailable) => TerminalResponse::Error(protocol_error(
            Some(request_id),
            ErrorCode::IpcFailed,
            "The desktop user interface is unavailable.",
        )),
    }
}

fn validation_response(request_id: Option<String>, error: ValidationError) -> SignError {
    let message = match error {
        ValidationError::UnsupportedVersion => "The protocol version is unsupported.",
        ValidationError::InvalidMessage => "The request envelope is invalid.",
        ValidationError::RequestTooLarge => "The request exceeds the supported size limit.",
        ValidationError::PdfInvalid => "The preview PDF is invalid.",
        ValidationError::ByteRangeContentInvalid => "The ByteRange content is invalid.",
    };
    protocol_error(request_id, error.error_code(), message)
}

fn protocol_error(request_id: Option<String>, code: ErrorCode, message: &str) -> SignError {
    SignError::new(request_id, code, message.to_owned())
}

fn write_terminal<W: Write>(
    writer: &mut W,
    response: &TerminalResponse,
) -> Result<ServeOutcome, HostError> {
    let body = serde_json::to_vec(&response)?;
    write_frame(writer, &body, MAX_RESPONSE_FRAME_BYTES)?;
    Ok(ServeOutcome::Responded)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use base64::Engine as _;
    use inkwell_protocol::{CancellationReason, SignCancelled};
    use serde_json::{Value, json};
    use sha2::{Digest, Sha256};

    use super::*;

    struct CancellingUi {
        displayed: bool,
    }

    impl UiAdapter for CancellingUi {
        fn handle_request(
            &mut self,
            request: ValidatedRequest,
        ) -> Result<TerminalResponse, UiUnavailable> {
            assert_eq!(request.website_origin(), "https://example.com");
            assert_eq!(request.document_name(), "contract.pdf");
            self.displayed = true;
            Ok(TerminalResponse::Cancelled(SignCancelled::new(
                request.request_id().to_owned(),
                CancellationReason::UserCancelled,
            )))
        }
    }

    #[test]
    fn walking_skeleton_displays_metadata_and_returns_one_cancellation() {
        let mut input = Vec::new();
        write_frame(
            &mut input,
            &serde_json::to_vec(&valid_request()).expect("fixture should serialize"),
            MAX_REQUEST_FRAME_BYTES,
        )
        .expect("fixture frame should write");
        let mut output = Vec::new();
        let mut ui = CancellingUi { displayed: false };

        let outcome =
            serve_one(Cursor::new(input), &mut output, &mut ui).expect("request should complete");

        assert_eq!(outcome, ServeOutcome::Responded);
        assert!(ui.displayed);
        let mut output = Cursor::new(output);
        let response: Value = serde_json::from_slice(
            &read_frame(&mut output, MAX_RESPONSE_FRAME_BYTES)
                .expect("response should be framed")
                .expect("response should exist"),
        )
        .expect("response should be JSON");
        assert_eq!(response["type"], "sign_cancelled");
        assert_eq!(response["reason"], "user_cancelled");
        assert!(
            read_frame(&mut output, MAX_RESPONSE_FRAME_BYTES)
                .expect("output should end cleanly")
                .is_none()
        );
    }

    #[test]
    fn malformed_json_returns_invalid_message_without_diagnostics() {
        let mut input = Vec::new();
        write_frame(&mut input, b"not json", 100).expect("fixture frame should write");
        let mut output = Vec::new();

        serve_one(
            Cursor::new(input),
            &mut output,
            &mut CancellingUi { displayed: false },
        )
        .expect("invalid request should receive a response");

        let response_body = read_frame(&mut Cursor::new(&output), MAX_RESPONSE_FRAME_BYTES)
            .expect("response frame should be valid")
            .expect("response should exist");
        assert_eq!(output.len(), response_body.len() + 4);
        let response: Value = serde_json::from_slice(&response_body).expect("response JSON");
        assert_eq!(response["error"]["code"], "INVALID_MESSAGE");
        assert_eq!(response["requestId"], Value::Null);
    }

    #[test]
    fn incomplete_envelopes_preserve_a_valid_request_id() {
        let response = run_request(&json!({
            "version": 1,
            "type": "sign_request",
            "requestId": "123e4567-e89b-42d3-a456-426614174000"
        }));

        assert_eq!(response["error"]["code"], "INVALID_MESSAGE");
        assert_eq!(
            response["requestId"],
            "123e4567-e89b-42d3-a456-426614174000"
        );
    }

    #[test]
    fn future_envelopes_are_rejected_before_version_specific_decoding() {
        let response = run_request(&json!({
            "version": 2,
            "type": "future_request",
            "requestId": "123e4567-e89b-42d3-a456-426614174000",
            "futureBody": true
        }));

        assert_eq!(response["error"]["code"], "UNSUPPORTED_VERSION");
        assert_eq!(
            response["requestId"],
            "123e4567-e89b-42d3-a456-426614174000"
        );
    }

    fn valid_request() -> Value {
        let preview = b"%PDF-1.7\n";
        let byte_range = b"byte range";
        json!({
            "version": 1,
            "type": "sign_request",
            "requestId": "123e4567-e89b-42d3-a456-426614174000",
            "websiteOrigin": "https://example.com",
            "documentName": "contract.pdf",
            "previewPdf": {
                "encoding": "base64",
                "data": base64::engine::general_purpose::STANDARD.encode(preview),
                "sha256": format!("{:x}", Sha256::digest(preview))
            },
            "byteRangeContent": {
                "encoding": "base64",
                "data": base64::engine::general_purpose::STANDARD.encode(byte_range),
                "sha256": format!("{:x}", Sha256::digest(byte_range))
            }
        })
    }

    fn run_request(request: &Value) -> Value {
        let mut input = Vec::new();
        write_frame(
            &mut input,
            &serde_json::to_vec(&request).expect("fixture should serialize"),
            MAX_REQUEST_FRAME_BYTES,
        )
        .expect("fixture frame should write");
        let mut output = Vec::new();
        serve_one(
            Cursor::new(input),
            &mut output,
            &mut CancellingUi { displayed: false },
        )
        .expect("request should receive a response");
        let body = read_frame(&mut Cursor::new(output), MAX_RESPONSE_FRAME_BYTES)
            .expect("response frame should read")
            .expect("response should exist");
        serde_json::from_slice(&body).expect("response should be JSON")
    }
}
