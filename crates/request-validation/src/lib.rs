//! Bounded request decoding and validation.
//!
//! Validation includes envelope fields, lengths, Base64 decoding, transport
//! hashes, and initial PDF checks. It must complete before request display.

use base64::{
    Engine as _, alphabet,
    engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig},
};
use inkwell_protocol::{BinaryPayload, ErrorCode, SignRequest, VERSION};
use sha2::{Digest, Sha256};
use thiserror::Error;
use url::Url;
use uuid::Uuid;

pub const MAX_BINARY_BYTES: usize = 50 * 1024 * 1024;
pub const MAX_ORIGIN_BYTES: usize = 2_048;
pub const MAX_DOCUMENT_NAME_BYTES: usize = 255;

const BASE64_ENGINE: GeneralPurpose = GeneralPurpose::new(
    &alphabet::STANDARD,
    GeneralPurposeConfig::new()
        .with_decode_padding_mode(DecodePaddingMode::RequireCanonical)
        .with_decode_allow_trailing_bits(false),
);

pub struct ValidatedRequest {
    request_id: String,
    website_origin: String,
    document_name: String,
    preview_pdf: Vec<u8>,
    byte_range_content: Vec<u8>,
}

impl ValidatedRequest {
    #[must_use]
    pub fn request_id(&self) -> &str {
        &self.request_id
    }

    #[must_use]
    pub fn website_origin(&self) -> &str {
        &self.website_origin
    }

    #[must_use]
    pub fn document_name(&self) -> &str {
        &self.document_name
    }

    #[must_use]
    pub fn preview_pdf(&self) -> &[u8] {
        &self.preview_pdf
    }

    #[must_use]
    pub fn byte_range_content(&self) -> &[u8] {
        &self.byte_range_content
    }

    #[must_use]
    pub fn into_parts(self) -> ValidatedRequestParts {
        ValidatedRequestParts {
            request_id: self.request_id,
            website_origin: self.website_origin,
            document_name: self.document_name,
            preview_pdf: self.preview_pdf,
            byte_range_content: self.byte_range_content,
        }
    }
}

pub struct ValidatedRequestParts {
    pub request_id: String,
    pub website_origin: String,
    pub document_name: String,
    pub preview_pdf: Vec<u8>,
    pub byte_range_content: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ValidationError {
    #[error("the protocol version is unsupported")]
    UnsupportedVersion,
    #[error("the request envelope is invalid")]
    InvalidMessage,
    #[error("the request exceeds a configured size limit")]
    RequestTooLarge,
    #[error("the preview PDF is invalid")]
    PdfInvalid,
    #[error("the ByteRange content is invalid")]
    ByteRangeContentInvalid,
}

impl ValidationError {
    #[must_use]
    pub const fn error_code(self) -> ErrorCode {
        match self {
            Self::UnsupportedVersion => ErrorCode::UnsupportedVersion,
            Self::InvalidMessage => ErrorCode::InvalidMessage,
            Self::RequestTooLarge => ErrorCode::RequestTooLarge,
            Self::PdfInvalid => ErrorCode::PdfInvalid,
            Self::ByteRangeContentInvalid => ErrorCode::ByterangeContentInvalid,
        }
    }
}

/// Validates and decodes a version-one request before it reaches any UI.
///
/// # Errors
///
/// Returns a stable validation category for every invalid request field.
pub fn validate_request(request: SignRequest) -> Result<ValidatedRequest, ValidationError> {
    let SignRequest {
        version,
        message_type,
        request_id,
        website_origin,
        document_name,
        preview_pdf,
        byte_range_content,
    } = request;

    if version != VERSION {
        return Err(ValidationError::UnsupportedVersion);
    }
    if message_type != "sign_request" || !is_valid_request_id(&request_id) {
        return Err(ValidationError::InvalidMessage);
    }
    validate_origin(&website_origin)?;
    if document_name.len() > MAX_DOCUMENT_NAME_BYTES
        || document_name.trim().is_empty()
        || document_name.chars().any(char::is_control)
    {
        return Err(ValidationError::InvalidMessage);
    }

    let preview_pdf = validate_binary(preview_pdf, PayloadKind::PreviewPdf)?;
    if !has_valid_pdf_header(&preview_pdf) {
        return Err(ValidationError::PdfInvalid);
    }
    let byte_range_content = validate_binary(byte_range_content, PayloadKind::ByteRangeContent)?;

    Ok(ValidatedRequest {
        request_id,
        website_origin,
        document_name,
        preview_pdf,
        byte_range_content,
    })
}

#[must_use]
pub fn is_valid_request_id(request_id: &str) -> bool {
    Uuid::parse_str(request_id)
        .is_ok_and(|uuid| !uuid.is_nil() && uuid.hyphenated().to_string().as_str() == request_id)
}

fn validate_origin(origin: &str) -> Result<(), ValidationError> {
    if origin.len() > MAX_ORIGIN_BYTES {
        return Err(ValidationError::InvalidMessage);
    }

    let parsed = Url::parse(origin).map_err(|_| ValidationError::InvalidMessage)?;
    let scheme_is_web = matches!(parsed.scheme(), "http" | "https");
    let has_credentials = !parsed.username().is_empty() || parsed.password().is_some();
    let serialized_origin = parsed.origin().ascii_serialization();
    if !scheme_is_web
        || has_credentials
        || parsed.path() != "/"
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || serialized_origin != origin
    {
        return Err(ValidationError::InvalidMessage);
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum PayloadKind {
    PreviewPdf,
    ByteRangeContent,
}

fn validate_binary(payload: BinaryPayload, kind: PayloadKind) -> Result<Vec<u8>, ValidationError> {
    let BinaryPayload {
        encoding,
        data,
        sha256,
    } = payload;
    if encoding != "base64" || !is_lowercase_sha256(&sha256) {
        return Err(kind.invalid_error());
    }

    let decoded = match decode_bounded(&data, MAX_BINARY_BYTES) {
        Ok(decoded) => decoded,
        Err(ValidationError::RequestTooLarge) => return Err(ValidationError::RequestTooLarge),
        Err(_) => return Err(kind.invalid_error()),
    };
    drop(data);
    let actual_hash = Sha256::digest(&decoded);
    if !constant_time_eq(actual_hash.as_slice(), sha256.as_bytes()) {
        return Err(kind.invalid_error());
    }
    Ok(decoded)
}

fn decode_bounded(data: &str, max_decoded_bytes: usize) -> Result<Vec<u8>, ValidationError> {
    let max_encoded_bytes = max_decoded_bytes.div_ceil(3) * 4;
    if data.len() > max_encoded_bytes {
        return Err(ValidationError::RequestTooLarge);
    }
    let decoded = BASE64_ENGINE
        .decode(data)
        .map_err(|_| ValidationError::InvalidMessage)?;
    if decoded.len() > max_decoded_bytes {
        return Err(ValidationError::RequestTooLarge);
    }
    Ok(decoded)
}

impl PayloadKind {
    const fn invalid_error(self) -> ValidationError {
        match self {
            Self::PreviewPdf => ValidationError::PdfInvalid,
            Self::ByteRangeContent => ValidationError::ByteRangeContentInvalid,
        }
    }
}

fn is_lowercase_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn constant_time_eq(actual: &[u8], expected_hex: &[u8]) -> bool {
    let mut difference = 0_u8;
    for (index, actual_byte) in actual.iter().copied().enumerate() {
        let high = hex_nibble(expected_hex[index * 2]);
        let low = hex_nibble(expected_hex[index * 2 + 1]);
        difference |= actual_byte ^ (high << 4 | low);
    }
    difference == 0
}

const fn hex_nibble(value: u8) -> u8 {
    match value {
        b'0'..=b'9' => value - b'0',
        b'a'..=b'f' => value - b'a' + 10,
        _ => 0,
    }
}

fn has_valid_pdf_header(pdf: &[u8]) -> bool {
    matches!(
        pdf,
        [b'%', b'P', b'D', b'F', b'-', b'1', b'.', b'0'..=b'7', ..]
            | [b'%', b'P', b'D', b'F', b'-', b'2', b'.', b'0', ..]
    )
}

#[cfg(test)]
mod tests {
    use base64::engine::general_purpose::STANDARD;

    use super::*;

    const REQUEST_ID: &str = "123e4567-e89b-42d3-a456-426614174000";

    #[test]
    fn validates_and_decodes_a_complete_request() {
        let request = valid_request();

        let validated = validate_request(request).expect("request should be valid");

        assert_eq!(validated.request_id(), REQUEST_ID);
        assert_eq!(validated.website_origin(), "https://example.com");
        assert_eq!(validated.document_name(), "document.pdf");
        assert_eq!(validated.preview_pdf(), b"%PDF-1.7\n");
        assert_eq!(validated.byte_range_content(), b"byte range");
    }

    #[test]
    fn distinguishes_unsupported_versions() {
        let mut request = valid_request();
        request.version = 2;

        assert_eq!(
            validate_request(request).err(),
            Some(ValidationError::UnsupportedVersion)
        );
    }

    #[test]
    fn validates_type_and_uuid() {
        let mut wrong_type = valid_request();
        wrong_type.message_type = "other".to_owned();
        let mut wrong_id = valid_request();
        wrong_id.request_id = "not-a-uuid".to_owned();
        let mut noncanonical_id = valid_request();
        noncanonical_id.request_id = "123e4567e89b42d3a456426614174000".to_owned();
        let mut nil_id = valid_request();
        nil_id.request_id = "00000000-0000-0000-0000-000000000000".to_owned();

        assert_eq!(
            validate_request(wrong_type).err(),
            Some(ValidationError::InvalidMessage)
        );
        assert_eq!(
            validate_request(wrong_id).err(),
            Some(ValidationError::InvalidMessage)
        );
        assert_eq!(
            validate_request(noncanonical_id).err(),
            Some(ValidationError::InvalidMessage)
        );
        assert_eq!(
            validate_request(nil_id).err(),
            Some(ValidationError::InvalidMessage)
        );
    }

    #[test]
    fn accepts_only_serialized_http_or_https_origins() {
        for invalid in [
            "file:///tmp/file",
            "https://user@example.com",
            "https://example.com/path",
            "https://example.com/?query",
            "https://example.com/#fragment",
            "https://example.com/",
        ] {
            let mut request = valid_request();
            request.website_origin = invalid.to_owned();
            assert_eq!(
                validate_request(request).err(),
                Some(ValidationError::InvalidMessage),
                "origin should fail: {invalid}"
            );
        }
    }

    #[test]
    fn applies_text_limits_in_utf8_bytes() {
        let mut valid_origin = valid_request();
        valid_origin.website_origin = format!("https://{}.com", "a".repeat(2_036));
        assert_eq!(valid_origin.website_origin.len(), MAX_ORIGIN_BYTES);

        let mut long_origin = valid_request();
        long_origin.website_origin = format!("https://{}.com", "a".repeat(2_037));
        let mut valid_name = valid_request();
        valid_name.document_name = "a".repeat(MAX_DOCUMENT_NAME_BYTES);
        let mut long_name = valid_request();
        long_name.document_name = "é".repeat(128);

        assert!(validate_request(valid_origin).is_ok());
        assert!(validate_request(valid_name).is_ok());
        assert_eq!(
            validate_request(long_origin).err(),
            Some(ValidationError::InvalidMessage)
        );
        assert_eq!(
            validate_request(long_name).err(),
            Some(ValidationError::InvalidMessage)
        );
    }

    #[test]
    fn rejects_noncanonical_or_oversized_base64_before_use() {
        assert_eq!(
            decode_bounded("YQ", 1).err(),
            Some(ValidationError::InvalidMessage)
        );
        assert_eq!(
            decode_bounded("YWI=", 1).err(),
            Some(ValidationError::RequestTooLarge)
        );
        assert_eq!(
            decode_bounded("__8=", 2).err(),
            Some(ValidationError::InvalidMessage)
        );
        assert_eq!(decode_bounded("YQ==", 1).expect("valid Base64"), b"a");
    }

    #[test]
    fn maps_preview_and_byte_range_integrity_failures_separately() {
        let mut preview = valid_request();
        preview.preview_pdf.sha256 = "0".repeat(64);
        let mut byte_range = valid_request();
        byte_range.byte_range_content.sha256 = "A".repeat(64);

        assert_eq!(
            validate_request(preview).err(),
            Some(ValidationError::PdfInvalid)
        );
        assert_eq!(
            validate_request(byte_range).err(),
            Some(ValidationError::ByteRangeContentInvalid)
        );
    }

    #[test]
    fn maps_payload_decoding_failures_separately() {
        let mut preview = valid_request();
        preview.preview_pdf.data = "not base64".to_owned();
        let mut byte_range = valid_request();
        byte_range.byte_range_content.data = "not base64".to_owned();

        assert_eq!(
            validate_request(preview).err(),
            Some(ValidationError::PdfInvalid)
        );
        assert_eq!(
            validate_request(byte_range).err(),
            Some(ValidationError::ByteRangeContentInvalid)
        );
    }

    #[test]
    fn rejects_document_names_that_are_not_display_text() {
        for invalid_name in ["", "   ", "line\nbreak.pdf", "nul\0name.pdf"] {
            let mut request = valid_request();
            request.document_name = invalid_name.to_owned();
            assert_eq!(
                validate_request(request).err(),
                Some(ValidationError::InvalidMessage)
            );
        }
    }

    #[test]
    fn requires_a_pdf_header_at_byte_zero() {
        for invalid_pdf in [b"PDF-1.7".as_slice(), b" %PDF-1.7", b"%PDF-2.1"] {
            let mut request = valid_request();
            request.preview_pdf = payload(invalid_pdf);
            assert_eq!(
                validate_request(request).err(),
                Some(ValidationError::PdfInvalid)
            );
        }
    }

    fn valid_request() -> SignRequest {
        SignRequest {
            version: VERSION,
            message_type: "sign_request".to_owned(),
            request_id: REQUEST_ID.to_owned(),
            website_origin: "https://example.com".to_owned(),
            document_name: "document.pdf".to_owned(),
            preview_pdf: payload(b"%PDF-1.7\n"),
            byte_range_content: payload(b"byte range"),
        }
    }

    fn payload(data: &[u8]) -> BinaryPayload {
        BinaryPayload {
            encoding: "base64".to_owned(),
            data: STANDARD.encode(data),
            sha256: format!("{:x}", Sha256::digest(data)),
        }
    }
}
