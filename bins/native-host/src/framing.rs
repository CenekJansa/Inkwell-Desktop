use std::io::{self, Read, Write};

use thiserror::Error;

pub const MAX_REQUEST_FRAME_BYTES: usize = 140 * 1024 * 1024;
pub const MAX_RESPONSE_FRAME_BYTES: usize = 1024 * 1024;

#[derive(Debug, Error)]
pub enum FrameError {
    #[error("native message length prefix is truncated")]
    TruncatedPrefix,
    #[error("native message body is truncated")]
    TruncatedBody,
    #[error("native message exceeds its configured limit")]
    TooLarge,
    #[error("native message I/O failed")]
    Io(#[from] io::Error),
}

/// Reads one Chrome native-messaging frame, or `None` for a clean EOF.
///
/// # Errors
///
/// Returns a framing or I/O error without allocating a body above `max_bytes`.
pub fn read_frame<R: Read>(
    reader: &mut R,
    max_bytes: usize,
) -> Result<Option<Vec<u8>>, FrameError> {
    let mut prefix = [0_u8; 4];
    let mut prefix_read = 0;
    while prefix_read < prefix.len() {
        match reader.read(&mut prefix[prefix_read..]) {
            Ok(0) if prefix_read == 0 => return Ok(None),
            Ok(0) => return Err(FrameError::TruncatedPrefix),
            Ok(count) => prefix_read += count,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(FrameError::Io(error)),
        }
    }

    let length = u32::from_ne_bytes(prefix) as usize;
    if length > max_bytes {
        return Err(FrameError::TooLarge);
    }

    let mut body = vec![0_u8; length];
    let mut body_read = 0;
    while body_read < body.len() {
        match reader.read(&mut body[body_read..]) {
            Ok(0) => return Err(FrameError::TruncatedBody),
            Ok(count) => body_read += count,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(FrameError::Io(error)),
        }
    }
    Ok(Some(body))
}

/// Writes exactly one Chrome native-messaging frame.
///
/// # Errors
///
/// Returns an error when the body exceeds either the configured limit or the
/// protocol's 32-bit length field, or when writing fails.
pub fn write_frame<W: Write>(
    writer: &mut W,
    body: &[u8],
    max_bytes: usize,
) -> Result<(), FrameError> {
    if body.len() > max_bytes || body.len() > u32::MAX as usize {
        return Err(FrameError::TooLarge);
    }
    let length = u32::try_from(body.len()).map_err(|_| FrameError::TooLarge)?;
    writer.write_all(&length.to_ne_bytes())?;
    writer.write_all(body)?;
    writer.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    #[test]
    fn round_trips_a_browser_compatible_frame() {
        let mut encoded = Vec::new();
        write_frame(&mut encoded, br#"{"version":1}"#, 100).expect("frame should write");

        let decoded = read_frame(&mut Cursor::new(encoded), 100)
            .expect("frame should read")
            .expect("frame should exist");

        assert_eq!(decoded, br#"{"version":1}"#);
    }

    #[test]
    fn reads_a_literal_little_endian_chrome_frame_on_supported_targets() {
        let input = [3, 0, 0, 0, b'a', b'b', b'c'];
        assert_eq!(
            read_frame(&mut Cursor::new(input), 3).expect("literal frame should read"),
            Some(b"abc".to_vec())
        );
    }

    #[test]
    fn distinguishes_clean_eof_from_truncation() {
        assert!(
            read_frame(&mut Cursor::new([]), 10)
                .expect("clean EOF")
                .is_none()
        );
        assert!(matches!(
            read_frame(&mut Cursor::new([1, 0]), 10),
            Err(FrameError::TruncatedPrefix)
        ));

        let mut truncated_body = 3_u32.to_ne_bytes().to_vec();
        truncated_body.extend_from_slice(b"ab");
        assert!(matches!(
            read_frame(&mut Cursor::new(truncated_body), 10),
            Err(FrameError::TruncatedBody)
        ));
    }

    #[test]
    fn rejects_large_frames_before_body_allocation() {
        let input = 11_u32.to_ne_bytes();
        assert!(matches!(
            read_frame(&mut Cursor::new(input), 10),
            Err(FrameError::TooLarge)
        ));
        assert!(matches!(
            write_frame(&mut Vec::new(), b"123", 2),
            Err(FrameError::TooLarge)
        ));
    }

    #[test]
    fn accepts_empty_message_bodies() {
        let mut encoded = Vec::new();
        write_frame(&mut encoded, b"", 0).expect("empty frame should write");
        assert_eq!(
            read_frame(&mut Cursor::new(encoded), 0).expect("empty frame should read"),
            Some(Vec::new())
        );
    }
}
