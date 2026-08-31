use serde::{Deserialize, Serialize};

pub const VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SignRequest {
    pub version: u32,
    #[serde(rename = "type")]
    pub message_type: String,
    pub request_id: String,
    pub website_origin: String,
    pub document_name: String,
    pub preview_pdf: BinaryPayload,
    pub byte_range_content: BinaryPayload,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BinaryPayload {
    pub encoding: String,
    pub data: String,
    pub sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CmsPayload {
    pub format: String,
    pub encoding: String,
    pub data: String,
    pub sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SignSuccess {
    pub version: u32,
    #[serde(rename = "type")]
    message_type: SignSuccessType,
    pub request_id: String,
    pub cms: CmsPayload,
}

impl SignSuccess {
    #[must_use]
    pub const fn new(request_id: String, cms: CmsPayload) -> Self {
        Self {
            version: VERSION,
            message_type: SignSuccessType::SignSuccess,
            request_id,
            cms,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum SignSuccessType {
    SignSuccess,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SignCancelled {
    pub version: u32,
    #[serde(rename = "type")]
    message_type: SignCancelledType,
    pub request_id: String,
    pub reason: CancellationReason,
}

impl SignCancelled {
    #[must_use]
    pub const fn new(request_id: String, reason: CancellationReason) -> Self {
        Self {
            version: VERSION,
            message_type: SignCancelledType::SignCancelled,
            request_id,
            reason,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum SignCancelledType {
    SignCancelled,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CancellationReason {
    UserCancelled,
    WindowClosed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SignError {
    pub version: u32,
    #[serde(rename = "type")]
    message_type: SignErrorType,
    pub request_id: Option<String>,
    pub error: ErrorDetail,
}

impl SignError {
    #[must_use]
    pub const fn new(request_id: Option<String>, code: ErrorCode, message: String) -> Self {
        Self {
            version: VERSION,
            message_type: SignErrorType::SignError,
            request_id,
            error: ErrorDetail { code, message },
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum SignErrorType {
    SignError,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ErrorDetail {
    pub code: ErrorCode,
    pub message: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    UnsupportedVersion,
    InvalidMessage,
    RequestTooLarge,
    Busy,
    RequestTimeout,
    ExtensionDisconnected,
    PdfInvalid,
    PdfEncrypted,
    PdfRenderFailed,
    ByterangeContentInvalid,
    NoCertificates,
    CertificateUnavailable,
    KeyAlgorithmUnsupported,
    KeyAccessDenied,
    ProviderCancelled,
    SigningFailed,
    ResponseTooLarge,
    IpcFailed,
    InternalError,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub enum TerminalResponse {
    Success(SignSuccess),
    Cancelled(SignCancelled),
    Error(SignError),
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    const REQUEST_ID: &str = "123e4567-e89b-42d3-a456-426614174000";

    #[test]
    fn request_uses_the_version_one_wire_shape_and_ignores_unknown_fields() {
        let value = json!({
            "version": 1,
            "type": "sign_request",
            "requestId": REQUEST_ID,
            "websiteOrigin": "https://example.com",
            "documentName": "document.pdf",
            "previewPdf": { "encoding": "base64", "data": "JVBERi0xLjc=", "sha256": "a".repeat(64) },
            "byteRangeContent": { "encoding": "base64", "data": "YWJj", "sha256": "b".repeat(64) },
            "futureField": true
        });

        let request: SignRequest = serde_json::from_value(value).expect("request should decode");
        let encoded = serde_json::to_value(request).expect("request should encode");

        assert_eq!(encoded["type"], "sign_request");
        assert_eq!(encoded["requestId"], REQUEST_ID);
        assert!(encoded.get("futureField").is_none());
    }

    #[test]
    fn terminal_envelopes_serialize_exact_discriminators() {
        let success = SignSuccess::new(
            REQUEST_ID.to_owned(),
            CmsPayload {
                format: "cms-signed-data".to_owned(),
                encoding: "base64-der".to_owned(),
                data: "AQID".to_owned(),
                sha256: "a".repeat(64),
            },
        );
        let cancelled =
            SignCancelled::new(REQUEST_ID.to_owned(), CancellationReason::UserCancelled);
        let error = SignError::new(
            None,
            ErrorCode::InvalidMessage,
            "The request is invalid.".to_owned(),
        );

        assert_eq!(
            json_value(success),
            json!({
                "version": 1,
                "type": "sign_success",
                "requestId": REQUEST_ID,
                "cms": {
                    "format": "cms-signed-data",
                    "encoding": "base64-der",
                    "data": "AQID",
                    "sha256": "a".repeat(64)
                }
            })
        );
        assert_eq!(
            json_value(cancelled),
            json!({
                "version": 1,
                "type": "sign_cancelled",
                "requestId": REQUEST_ID,
                "reason": "user_cancelled"
            })
        );
        assert_eq!(
            json_value(error),
            json!({
                "version": 1,
                "type": "sign_error",
                "requestId": Value::Null,
                "error": {
                    "code": "INVALID_MESSAGE",
                    "message": "The request is invalid."
                }
            })
        );
    }

    #[test]
    fn both_cancellation_reasons_have_stable_values() {
        assert_eq!(
            json_value(CancellationReason::UserCancelled),
            "user_cancelled"
        );
        assert_eq!(
            json_value(CancellationReason::WindowClosed),
            "window_closed"
        );
    }

    #[test]
    fn every_stable_error_code_has_the_required_wire_value() {
        let cases = [
            (ErrorCode::UnsupportedVersion, "UNSUPPORTED_VERSION"),
            (ErrorCode::InvalidMessage, "INVALID_MESSAGE"),
            (ErrorCode::RequestTooLarge, "REQUEST_TOO_LARGE"),
            (ErrorCode::Busy, "BUSY"),
            (ErrorCode::RequestTimeout, "REQUEST_TIMEOUT"),
            (ErrorCode::ExtensionDisconnected, "EXTENSION_DISCONNECTED"),
            (ErrorCode::PdfInvalid, "PDF_INVALID"),
            (ErrorCode::PdfEncrypted, "PDF_ENCRYPTED"),
            (ErrorCode::PdfRenderFailed, "PDF_RENDER_FAILED"),
            (
                ErrorCode::ByterangeContentInvalid,
                "BYTERANGE_CONTENT_INVALID",
            ),
            (ErrorCode::NoCertificates, "NO_CERTIFICATES"),
            (ErrorCode::CertificateUnavailable, "CERTIFICATE_UNAVAILABLE"),
            (
                ErrorCode::KeyAlgorithmUnsupported,
                "KEY_ALGORITHM_UNSUPPORTED",
            ),
            (ErrorCode::KeyAccessDenied, "KEY_ACCESS_DENIED"),
            (ErrorCode::ProviderCancelled, "PROVIDER_CANCELLED"),
            (ErrorCode::SigningFailed, "SIGNING_FAILED"),
            (ErrorCode::ResponseTooLarge, "RESPONSE_TOO_LARGE"),
            (ErrorCode::IpcFailed, "IPC_FAILED"),
            (ErrorCode::InternalError, "INTERNAL_ERROR"),
        ];

        for (code, expected) in cases {
            assert_eq!(json_value(code), expected);
        }
    }

    fn json_value<T: Serialize>(value: T) -> Value {
        serde_json::to_value(value).expect("test value should serialize")
    }
}
