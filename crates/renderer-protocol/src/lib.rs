//! Versioned, bounded messages for the isolated PDF renderer.
//!
//! Each frame is a four-byte little-endian payload length followed by one
//! postcard-encoded request or response. Callers must use the direction-specific
//! read and write functions so untrusted lengths are checked before allocation.

use std::io::{self, Read, Write};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use zeroize::{Zeroize as _, Zeroizing};

pub const PROTOCOL_VERSION: u16 = 1;
pub const MAX_DOCUMENT_BYTES: usize = 50 * 1024 * 1024;
pub const MAX_PAGE_COUNT: u32 = 100_000;
pub const MAX_RENDER_DIMENSION: u32 = 8_192;
pub const MAX_RENDER_PIXELS: usize = 16 * 1024 * 1024;
pub const MAX_RENDER_OUTPUT_BYTES: usize = MAX_RENDER_PIXELS * 4;
pub const MAX_ERROR_MESSAGE_BYTES: usize = 1_024;
pub const MAX_REQUEST_FRAME_BYTES: usize = MAX_DOCUMENT_BYTES + 64 * 1024;
pub const MAX_RESPONSE_FRAME_BYTES: usize = MAX_RENDER_OUTPUT_BYTES + 64 * 1024;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub version: u16,
    pub request_id: u64,
    pub command: RequestCommand,
}

impl Request {
    #[must_use]
    pub const fn new(request_id: u64, command: RequestCommand) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            request_id,
            command,
        }
    }

    /// Checks protocol version and all command-specific limits.
    ///
    /// # Errors
    ///
    /// Returns an error when the request is unsupported, malformed, or exceeds
    /// a configured resource limit.
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_version(self.version)?;
        match &self.command {
            RequestCommand::OpenDocument(request) => {
                if request.document.is_empty() {
                    return Err(ProtocolError::InvalidMessage("document is empty"));
                }
                check_limit(
                    Limit::DocumentBytes,
                    request.document.len(),
                    MAX_DOCUMENT_BYTES,
                )
            }
            RequestCommand::PageMetadata(request) => validate_page_index(request.page_index),
            RequestCommand::RenderPage(request) => {
                validate_page_index(request.page_index)?;
                validate_dimensions(request.width, request.height)
            }
            RequestCommand::CloseDocument | RequestCommand::Shutdown => Ok(()),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum RequestCommand {
    OpenDocument(OpenDocument),
    PageMetadata(PageMetadataRequest),
    RenderPage(RenderPageRequest),
    CloseDocument,
    Shutdown,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OpenDocument {
    pub document: Vec<u8>,
}

impl Drop for OpenDocument {
    fn drop(&mut self) {
        self.document.zeroize();
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PageMetadataRequest {
    pub page_index: u32,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RenderPageRequest {
    pub page_index: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Response {
    pub version: u16,
    pub request_id: u64,
    pub result: ResponseResult,
}

impl Response {
    #[must_use]
    pub const fn new(request_id: u64, result: ResponseResult) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            request_id,
            result,
        }
    }

    /// Checks protocol version and all result-specific limits and invariants.
    ///
    /// # Errors
    ///
    /// Returns an error when the response is unsupported, malformed, or exceeds
    /// a configured resource limit.
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_version(self.version)?;
        match &self.result {
            ResponseResult::DocumentOpened(opened) => {
                if opened.page_count == 0 {
                    return Err(ProtocolError::InvalidMessage(
                        "opened document has no pages",
                    ));
                }
                check_limit(
                    Limit::PageCount,
                    opened.page_count as usize,
                    MAX_PAGE_COUNT as usize,
                )
            }
            ResponseResult::PageMetadata(metadata) => {
                validate_page_index(metadata.page_index)?;
                if metadata.width_millipoints == 0 || metadata.height_millipoints == 0 {
                    return Err(ProtocolError::InvalidMessage(
                        "page dimensions must be nonzero",
                    ));
                }
                Ok(())
            }
            ResponseResult::PageRendered(rendered) => validate_rendered_page(rendered),
            ResponseResult::DocumentClosed | ResponseResult::ShuttingDown => Ok(()),
            ResponseResult::Error(error) => {
                if error.message.is_empty() {
                    return Err(ProtocolError::InvalidMessage("error message is empty"));
                }
                check_limit(
                    Limit::ErrorMessageBytes,
                    error.message.len(),
                    MAX_ERROR_MESSAGE_BYTES,
                )
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ResponseResult {
    DocumentOpened(DocumentOpened),
    PageMetadata(PageMetadata),
    PageRendered(RenderedPage),
    DocumentClosed,
    ShuttingDown,
    Error(RendererError),
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DocumentOpened {
    pub page_count: u32,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PageMetadata {
    pub page_index: u32,
    pub width_millipoints: u32,
    pub height_millipoints: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RenderedPage {
    pub page_index: u32,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub format: PixelFormat,
    pub pixels: Vec<u8>,
}

impl Drop for RenderedPage {
    fn drop(&mut self) {
        self.pixels.zeroize();
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum PixelFormat {
    Rgba8,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RendererError {
    pub code: RendererErrorCode,
    pub message: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum RendererErrorCode {
    InvalidRequest,
    UnsupportedVersion,
    DocumentTooLarge,
    InvalidPdf,
    EncryptedPdf,
    DocumentNotOpen,
    PageOutOfRange,
    InvalidDimensions,
    RenderTooLarge,
    RenderFailed,
    Internal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Limit {
    FrameBytes,
    DocumentBytes,
    PageCount,
    PageIndex,
    RenderDimension,
    RenderPixels,
    RenderOutputBytes,
    ErrorMessageBytes,
}

#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("renderer protocol I/O failed")]
    Io(#[from] io::Error),
    #[error("renderer protocol frame is truncated")]
    Truncated,
    #[error("renderer protocol version {actual} is unsupported")]
    UnsupportedVersion { actual: u16 },
    #[error("renderer protocol {limit:?} limit exceeded: {actual} > {maximum}")]
    LimitExceeded {
        limit: Limit,
        actual: usize,
        maximum: usize,
    },
    #[error("renderer protocol message is invalid: {0}")]
    InvalidMessage(&'static str),
    #[error("renderer protocol serialization failed")]
    Serialize(#[source] postcard::Error),
    #[error("renderer protocol deserialization failed")]
    Deserialize(#[source] postcard::Error),
}

/// Serializes and writes one bounded request frame, then flushes the writer.
///
/// # Errors
///
/// Returns an error for an invalid request, serialization failure, oversized
/// frame, or I/O failure.
pub fn write_request<W: Write>(writer: &mut W, request: &Request) -> Result<(), ProtocolError> {
    request.validate()?;
    write_frame(writer, request, MAX_REQUEST_FRAME_BYTES)
}

/// Reads and validates one bounded request frame. Clean EOF returns `None`.
///
/// # Errors
///
/// Returns an error for truncation, oversize, invalid postcard data, invalid
/// request semantics, or I/O failure.
pub fn read_request<R: Read>(reader: &mut R) -> Result<Option<Request>, ProtocolError> {
    let Some(request) = read_frame::<_, Request>(reader, MAX_REQUEST_FRAME_BYTES)? else {
        return Ok(None);
    };
    request.validate()?;
    Ok(Some(request))
}

/// Serializes and writes one bounded response frame, then flushes the writer.
///
/// # Errors
///
/// Returns an error for an invalid response, serialization failure, oversized
/// frame, or I/O failure.
pub fn write_response<W: Write>(writer: &mut W, response: &Response) -> Result<(), ProtocolError> {
    response.validate()?;
    write_frame(writer, response, MAX_RESPONSE_FRAME_BYTES)
}

/// Reads and validates one bounded response frame. Clean EOF returns `None`.
///
/// # Errors
///
/// Returns an error for truncation, oversize, invalid postcard data, invalid
/// response semantics, or I/O failure.
pub fn read_response<R: Read>(reader: &mut R) -> Result<Option<Response>, ProtocolError> {
    let Some(response) = read_frame::<_, Response>(reader, MAX_RESPONSE_FRAME_BYTES)? else {
        return Ok(None);
    };
    response.validate()?;
    Ok(Some(response))
}

fn write_frame<W: Write, T: Serialize>(
    writer: &mut W,
    message: &T,
    maximum: usize,
) -> Result<(), ProtocolError> {
    let payload = Zeroizing::new(postcard::to_allocvec(message).map_err(ProtocolError::Serialize)?);
    check_limit(Limit::FrameBytes, payload.len(), maximum)?;
    let length = u32::try_from(payload.len()).map_err(|_| ProtocolError::LimitExceeded {
        limit: Limit::FrameBytes,
        actual: payload.len(),
        maximum,
    })?;
    writer.write_all(&length.to_le_bytes())?;
    writer.write_all(&payload)?;
    writer.flush()?;
    Ok(())
}

fn read_frame<R: Read, T: for<'de> Deserialize<'de>>(
    reader: &mut R,
    maximum: usize,
) -> Result<Option<T>, ProtocolError> {
    let Some(length) = read_length(reader)? else {
        return Ok(None);
    };
    check_limit(Limit::FrameBytes, length, maximum)?;
    let mut payload = Zeroizing::new(vec![0_u8; length]);
    reader.read_exact(&mut payload).map_err(map_truncation)?;
    postcard::from_bytes(&payload)
        .map(Some)
        .map_err(ProtocolError::Deserialize)
}

fn read_length<R: Read>(reader: &mut R) -> Result<Option<usize>, ProtocolError> {
    let mut prefix = [0_u8; 4];
    let mut read = 0;
    while read < prefix.len() {
        match reader.read(&mut prefix[read..]) {
            Ok(0) if read == 0 => return Ok(None),
            Ok(0) => return Err(ProtocolError::Truncated),
            Ok(count) => read += count,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(ProtocolError::Io(error)),
        }
    }
    Ok(Some(u32::from_le_bytes(prefix) as usize))
}

fn map_truncation(error: io::Error) -> ProtocolError {
    if error.kind() == io::ErrorKind::UnexpectedEof {
        ProtocolError::Truncated
    } else {
        ProtocolError::Io(error)
    }
}

fn validate_version(version: u16) -> Result<(), ProtocolError> {
    if version == PROTOCOL_VERSION {
        Ok(())
    } else {
        Err(ProtocolError::UnsupportedVersion { actual: version })
    }
}

fn validate_page_index(page_index: u32) -> Result<(), ProtocolError> {
    check_limit(
        Limit::PageIndex,
        page_index as usize,
        (MAX_PAGE_COUNT - 1) as usize,
    )
}

fn validate_dimensions(width: u32, height: u32) -> Result<(), ProtocolError> {
    if width == 0 || height == 0 {
        return Err(ProtocolError::InvalidMessage(
            "render dimensions must be nonzero",
        ));
    }
    check_limit(
        Limit::RenderDimension,
        width as usize,
        MAX_RENDER_DIMENSION as usize,
    )?;
    check_limit(
        Limit::RenderDimension,
        height as usize,
        MAX_RENDER_DIMENSION as usize,
    )?;
    let pixels =
        (width as usize)
            .checked_mul(height as usize)
            .ok_or(ProtocolError::LimitExceeded {
                limit: Limit::RenderPixels,
                actual: usize::MAX,
                maximum: MAX_RENDER_PIXELS,
            })?;
    check_limit(Limit::RenderPixels, pixels, MAX_RENDER_PIXELS)
}

fn validate_rendered_page(rendered: &RenderedPage) -> Result<(), ProtocolError> {
    validate_page_index(rendered.page_index)?;
    validate_dimensions(rendered.width, rendered.height)?;
    let expected_stride = rendered
        .width
        .checked_mul(4)
        .ok_or(ProtocolError::InvalidMessage("render stride overflows"))?;
    if rendered.stride != expected_stride {
        return Err(ProtocolError::InvalidMessage("render stride is invalid"));
    }
    let expected_bytes = (rendered.stride as usize)
        .checked_mul(rendered.height as usize)
        .ok_or(ProtocolError::LimitExceeded {
            limit: Limit::RenderOutputBytes,
            actual: usize::MAX,
            maximum: MAX_RENDER_OUTPUT_BYTES,
        })?;
    check_limit(
        Limit::RenderOutputBytes,
        expected_bytes,
        MAX_RENDER_OUTPUT_BYTES,
    )?;
    if rendered.pixels.len() != expected_bytes {
        return Err(ProtocolError::InvalidMessage(
            "render output length does not match dimensions",
        ));
    }
    Ok(())
}

fn check_limit(limit: Limit, actual: usize, maximum: usize) -> Result<(), ProtocolError> {
    if actual <= maximum {
        Ok(())
    } else {
        Err(ProtocolError::LimitExceeded {
            limit,
            actual,
            maximum,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    #[test]
    fn every_request_command_roundtrips() {
        let commands = [
            RequestCommand::OpenDocument(OpenDocument {
                document: b"%PDF-1.7 test".to_vec(),
            }),
            RequestCommand::PageMetadata(PageMetadataRequest { page_index: 2 }),
            RequestCommand::RenderPage(RenderPageRequest {
                page_index: 2,
                width: 16,
                height: 8,
            }),
            RequestCommand::CloseDocument,
            RequestCommand::Shutdown,
        ];

        for (request_id, command) in (1_u64..).zip(commands) {
            let request = Request::new(request_id, command);
            let mut wire = Vec::new();
            write_request(&mut wire, &request).expect("request should encode");
            let decoded = read_request(&mut Cursor::new(wire))
                .expect("request should decode")
                .expect("request should exist");
            assert_eq!(decoded, request);
        }
    }

    #[test]
    fn every_response_result_roundtrips() {
        let results = [
            ResponseResult::DocumentOpened(DocumentOpened { page_count: 3 }),
            ResponseResult::PageMetadata(PageMetadata {
                page_index: 1,
                width_millipoints: 612_000,
                height_millipoints: 792_000,
            }),
            ResponseResult::PageRendered(RenderedPage {
                page_index: 1,
                width: 2,
                height: 2,
                stride: 8,
                format: PixelFormat::Rgba8,
                pixels: vec![0x7f; 16],
            }),
            ResponseResult::DocumentClosed,
            ResponseResult::ShuttingDown,
            ResponseResult::Error(RendererError {
                code: RendererErrorCode::InvalidPdf,
                message: "PDF structure is invalid".to_owned(),
            }),
        ];

        for (request_id, result) in (1_u64..).zip(results) {
            let response = Response::new(request_id, result);
            let mut wire = Vec::new();
            write_response(&mut wire, &response).expect("response should encode");
            let decoded = read_response(&mut Cursor::new(wire))
                .expect("response should decode")
                .expect("response should exist");
            assert_eq!(decoded, response);
        }
    }

    #[test]
    fn frame_prefix_is_little_endian_payload_length() {
        let mut wire = Vec::new();
        write_request(&mut wire, &Request::new(7, RequestCommand::Shutdown))
            .expect("request should encode");

        let length = usize::try_from(u32::from_le_bytes(
            wire[..4].try_into().expect("prefix should be four bytes"),
        ))
        .expect("u32 should fit usize");
        assert_eq!(length, wire.len() - 4);
    }

    #[test]
    fn clean_eof_is_distinct_from_truncated_prefix_and_payload() {
        assert_eq!(
            read_request(&mut Cursor::new(Vec::new())).expect("EOF should be clean"),
            None
        );

        for prefix in [vec![1], vec![1, 0], vec![1, 0, 0]] {
            assert!(matches!(
                read_request(&mut Cursor::new(prefix)),
                Err(ProtocolError::Truncated)
            ));
        }

        let mut wire = Vec::new();
        write_request(&mut wire, &Request::new(9, RequestCommand::CloseDocument))
            .expect("request should encode");
        wire.pop();
        assert!(matches!(
            read_request(&mut Cursor::new(wire)),
            Err(ProtocolError::Truncated)
        ));
    }

    #[test]
    fn oversized_frame_prefixes_are_rejected_before_payload_reads() {
        let request_length = u32::try_from(MAX_REQUEST_FRAME_BYTES + 1)
            .expect("request frame limit should fit u32")
            .to_le_bytes();
        assert!(matches!(
            read_request(&mut Cursor::new(request_length)),
            Err(ProtocolError::LimitExceeded {
                limit: Limit::FrameBytes,
                ..
            })
        ));

        let response_length = u32::try_from(MAX_RESPONSE_FRAME_BYTES + 1)
            .expect("response frame limit should fit u32")
            .to_le_bytes();
        assert!(matches!(
            read_response(&mut Cursor::new(response_length)),
            Err(ProtocolError::LimitExceeded {
                limit: Limit::FrameBytes,
                ..
            })
        ));
    }

    #[test]
    fn document_limit_is_inclusive_and_oversize_is_rejected() {
        let maximum = Request::new(
            1,
            RequestCommand::OpenDocument(OpenDocument {
                document: vec![0; MAX_DOCUMENT_BYTES],
            }),
        );
        maximum.validate().expect("maximum document should pass");

        let oversized = Request::new(
            1,
            RequestCommand::OpenDocument(OpenDocument {
                document: vec![0; MAX_DOCUMENT_BYTES + 1],
            }),
        );
        assert!(matches!(
            oversized.validate(),
            Err(ProtocolError::LimitExceeded {
                limit: Limit::DocumentBytes,
                ..
            })
        ));
        assert!(matches!(
            write_request(&mut Vec::new(), &oversized),
            Err(ProtocolError::LimitExceeded {
                limit: Limit::DocumentBytes,
                ..
            })
        ));
    }

    #[test]
    fn dimensions_pixels_and_output_are_strictly_bounded() {
        let too_wide = Request::new(
            1,
            RequestCommand::RenderPage(RenderPageRequest {
                page_index: 0,
                width: MAX_RENDER_DIMENSION + 1,
                height: 1,
            }),
        );
        assert!(matches!(
            too_wide.validate(),
            Err(ProtocolError::LimitExceeded {
                limit: Limit::RenderDimension,
                ..
            })
        ));

        let too_many_pixels = Request::new(
            1,
            RequestCommand::RenderPage(RenderPageRequest {
                page_index: 0,
                width: MAX_RENDER_DIMENSION,
                height: MAX_RENDER_DIMENSION,
            }),
        );
        assert!(matches!(
            too_many_pixels.validate(),
            Err(ProtocolError::LimitExceeded {
                limit: Limit::RenderPixels,
                ..
            })
        ));

        let wrong_output = Response::new(
            1,
            ResponseResult::PageRendered(RenderedPage {
                page_index: 0,
                width: 2,
                height: 2,
                stride: 8,
                format: PixelFormat::Rgba8,
                pixels: vec![0; 15],
            }),
        );
        assert!(matches!(
            wrong_output.validate(),
            Err(ProtocolError::InvalidMessage(_))
        ));
    }

    #[test]
    fn decoded_messages_are_validated_for_version_and_semantics() {
        let unsupported = Request {
            version: PROTOCOL_VERSION + 1,
            request_id: 1,
            command: RequestCommand::Shutdown,
        };
        let payload = postcard::to_allocvec(&unsupported).expect("request should serialize");
        let mut wire = Vec::new();
        wire.extend_from_slice(
            &u32::try_from(payload.len())
                .expect("payload should fit u32")
                .to_le_bytes(),
        );
        wire.extend_from_slice(&payload);
        assert!(matches!(
            read_request(&mut Cursor::new(wire)),
            Err(ProtocolError::UnsupportedVersion { .. })
        ));

        let invalid = Response::new(
            1,
            ResponseResult::Error(RendererError {
                code: RendererErrorCode::Internal,
                message: String::new(),
            }),
        );
        assert!(matches!(
            write_response(&mut Vec::new(), &invalid),
            Err(ProtocolError::InvalidMessage(_))
        ));
    }

    #[test]
    fn invalid_postcard_payload_is_rejected() {
        let wire = vec![1, 0, 0, 0, 0xff];
        assert!(matches!(
            read_response(&mut Cursor::new(wire)),
            Err(ProtocolError::Deserialize(_))
        ));
    }
}
