//! Authentication and framing for current-user local IPC.
//!
//! The transport supplies a connected byte stream (a current-user-only named
//! pipe on Windows). This layer authenticates each direction independently and
//! rejects modified, replayed, reordered, or oversized frames.

use std::{
    fmt,
    io::{self, Read, Write},
};

use hmac::{Hmac, Mac as _};
#[cfg(unix)]
use interprocess::local_socket::{GenericFilePath, ToFsName as _};
#[cfg(windows)]
use interprocess::local_socket::{GenericNamespaced, ToNsName as _};
use interprocess::local_socket::{
    Listener, ListenerOptions, Name, RecvHalf, SendHalf, Stream,
    traits::{Listener as _, Stream as _},
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::Sha256;
use thiserror::Error;
use zeroize::Zeroize as _;

pub const MAX_IPC_MESSAGE_BYTES: usize = 128 * 1024 * 1024;

const VERSION: u8 = 1;
const HEADER_BYTES: usize = 1 + 8;
const TAG_BYTES: usize = 32;
const HOST_TO_DESKTOP: &[u8] = b"inkwell-ipc-v1:host-to-desktop";
const DESKTOP_TO_HOST: &[u8] = b"inkwell-ipc-v1:desktop-to-host";
const HANDSHAKE_MAGIC: &[u8; 8] = b"INKWELL1";

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Deserialize, Serialize)]
pub enum HostCommand {
    Request(IpcRequest),
    Disconnect { request_id: String },
}

#[derive(Debug, Deserialize, Serialize)]
pub enum DesktopCommand {
    Terminal(inkwell_protocol::TerminalResponse),
}

#[derive(Debug, Deserialize, Serialize)]
pub struct IpcRequest {
    pub request_id: String,
    pub website_origin: String,
    pub document_name: String,
    pub preview_pdf: Vec<u8>,
    pub byte_range_content: Vec<u8>,
}

impl IpcRequest {
    pub fn clear(&mut self) {
        self.request_id.zeroize();
        self.website_origin.zeroize();
        self.document_name.zeroize();
        self.preview_pdf.zeroize();
        self.byte_range_content.zeroize();
    }
}

/// Ephemeral authentication material for one host-to-desktop connection.
///
/// It must be transferred through the current-user pipe handshake, never a
/// process argument, environment variable, log, or persistent file.
pub struct SessionKey([u8; 32]);

impl SessionKey {
    /// Creates a key from the operating system random source.
    ///
    /// # Errors
    ///
    /// Returns an error if the operating system random source is unavailable.
    pub fn generate() -> Result<Self, getrandom::Error> {
        let mut key = [0_u8; 32];
        getrandom::fill(&mut key)?;
        Ok(Self(key))
    }

    #[must_use]
    pub const fn from_bytes(key: [u8; 32]) -> Self {
        Self(key)
    }
}

impl fmt::Debug for SessionKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SessionKey([REDACTED])")
    }
}

impl Drop for SessionKey {
    fn drop(&mut self) {
        self.0.fill(0);
    }
}

/// A current-user-only desktop rendezvous endpoint.
pub struct CurrentUserListener {
    inner: Listener,
}

impl CurrentUserListener {
    /// Binds a namespaced local endpoint with current-user-only permissions.
    ///
    /// On Windows this creates a named pipe protected by a DACL for SYSTEM and
    /// the pipe owner. The OS assigns ownership to the desktop process user.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid or occupied name or an OS setup failure.
    pub fn bind(name: &str) -> io::Result<Self> {
        let name = local_name(name)?;
        let options = ListenerOptions::new().name(name);
        #[cfg(windows)]
        let options = current_user_options(options)?;
        #[cfg(unix)]
        let options = current_user_options(options);
        options.create_sync().map(|inner| Self { inner })
    }

    /// Accepts one host and establishes fresh in-pipe authentication material.
    ///
    /// # Errors
    ///
    /// Returns an error when accepting, generating randomness, or writing the
    /// handshake fails.
    pub fn accept(&self) -> Result<AuthenticatedStream<Stream>, HandshakeError> {
        let mut stream = self.inner.accept()?;
        let key = SessionKey::generate()?;
        stream.write_all(HANDSHAKE_MAGIC)?;
        stream.write_all(&key.0)?;
        stream.flush()?;
        Ok(AuthenticatedStream::new(stream, key, Endpoint::Desktop))
    }
}

/// Connects the host to the desktop rendezvous endpoint.
///
/// # Errors
///
/// Returns an error when connecting or receiving the in-pipe handshake fails.
pub fn connect(name: &str) -> Result<AuthenticatedStream<Stream>, HandshakeError> {
    let name = local_name(name)?;
    let mut stream = Stream::connect(name)?;
    let mut handshake = [0_u8; HANDSHAKE_MAGIC.len() + 32];
    stream
        .read_exact(&mut handshake)
        .map_err(map_handshake_io)?;
    if &handshake[..HANDSHAKE_MAGIC.len()] != HANDSHAKE_MAGIC {
        return Err(HandshakeError::InvalidHandshake);
    }
    let mut key = [0_u8; 32];
    key.copy_from_slice(&handshake[HANDSHAKE_MAGIC.len()..]);
    handshake.fill(0);
    Ok(AuthenticatedStream::new(
        stream,
        SessionKey::from_bytes(key),
        Endpoint::Host,
    ))
}

#[derive(Debug, Error)]
pub enum HandshakeError {
    #[error("current-user IPC setup failed")]
    Io(#[from] io::Error),
    #[error("the operating system random source is unavailable")]
    Random(#[from] getrandom::Error),
    #[error("the current-user IPC handshake is invalid")]
    InvalidHandshake,
}

#[cfg(windows)]
fn current_user_options(options: ListenerOptions<'_>) -> io::Result<ListenerOptions<'_>> {
    use interprocess::os::windows::{
        local_socket::ListenerOptionsExt as _, security_descriptor::SecurityDescriptor,
    };
    use widestring::U16CString;

    // OW is the Windows Owner Rights SID. The pipe owner is the current user;
    // the protected DACL prevents inherited or default broad pipe access.
    let sddl = U16CString::from_str("D:P(A;;GA;;;SY)(A;;GA;;;OW)").map_err(io::Error::other)?;
    let descriptor = SecurityDescriptor::deserialize(&sddl)?;
    Ok(options.security_descriptor(descriptor))
}

#[cfg(unix)]
fn current_user_options(options: ListenerOptions<'_>) -> ListenerOptions<'_> {
    use interprocess::os::unix::local_socket::ListenerOptionsExt as _;

    options.mode(0o600)
}

fn map_handshake_io(error: io::Error) -> HandshakeError {
    if error.kind() == io::ErrorKind::UnexpectedEof {
        HandshakeError::InvalidHandshake
    } else {
        HandshakeError::Io(error)
    }
}

fn validate_name(name: &str) -> io::Result<()> {
    if name.is_empty()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "local IPC name contains unsupported characters",
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn local_name(name: &str) -> io::Result<Name<'static>> {
    validate_name(name)?;
    name.to_owned().to_ns_name::<GenericNamespaced>()
}

#[cfg(unix)]
fn local_name(name: &str) -> io::Result<Name<'static>> {
    validate_name(name)?;
    format!("/tmp/{name}.sock").to_fs_name::<GenericFilePath>()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Endpoint {
    Host,
    Desktop,
}

impl Endpoint {
    const fn outgoing_domain(self) -> &'static [u8] {
        match self {
            Self::Host => HOST_TO_DESKTOP,
            Self::Desktop => DESKTOP_TO_HOST,
        }
    }

    const fn incoming_domain(self) -> &'static [u8] {
        match self {
            Self::Host => DESKTOP_TO_HOST,
            Self::Desktop => HOST_TO_DESKTOP,
        }
    }
}

#[derive(Debug, Error)]
pub enum IpcError {
    #[error("local IPC I/O failed")]
    Io(#[from] io::Error),
    #[error("the local IPC frame exceeds the configured limit")]
    TooLarge,
    #[error("the local IPC frame is truncated")]
    Truncated,
    #[error("the local IPC frame has an unsupported version")]
    UnsupportedVersion,
    #[error("local IPC authentication failed")]
    AuthenticationFailed,
    #[error("the local IPC frame is replayed or out of order")]
    UnexpectedSequence,
    #[error("the local IPC sequence is exhausted")]
    SequenceExhausted,
    #[error("the local IPC message is invalid")]
    InvalidMessage,
}

pub struct AuthenticatedStream<S> {
    stream: S,
    key: SessionKey,
    endpoint: Endpoint,
    outgoing_sequence: u64,
    incoming_sequence: u64,
}

pub type IpcStream = AuthenticatedStream<Stream>;

impl AuthenticatedStream<Stream> {
    #[must_use]
    pub fn split(self) -> (AuthenticatedStream<RecvHalf>, AuthenticatedStream<SendHalf>) {
        let key = self.key.0;
        let endpoint = self.endpoint;
        let outgoing_sequence = self.outgoing_sequence;
        let incoming_sequence = self.incoming_sequence;
        let (receiver, sender) = self.stream.split();
        (
            AuthenticatedStream {
                stream: receiver,
                key: SessionKey::from_bytes(key),
                endpoint,
                outgoing_sequence,
                incoming_sequence,
            },
            AuthenticatedStream {
                stream: sender,
                key: SessionKey::from_bytes(key),
                endpoint,
                outgoing_sequence,
                incoming_sequence,
            },
        )
    }
}

impl<S> AuthenticatedStream<S> {
    #[must_use]
    pub const fn new(stream: S, key: SessionKey, endpoint: Endpoint) -> Self {
        Self {
            stream,
            key,
            endpoint,
            outgoing_sequence: 0,
            incoming_sequence: 0,
        }
    }

    #[must_use]
    pub fn into_inner(self) -> S {
        self.stream
    }
}

impl<S: Write> AuthenticatedStream<S> {
    /// Writes and flushes one authenticated length-prefixed frame.
    ///
    /// # Errors
    ///
    /// Returns an error for oversized payloads, sequence exhaustion, or I/O.
    pub fn send(&mut self, payload: &[u8]) -> Result<(), IpcError> {
        if payload.len() > MAX_IPC_MESSAGE_BYTES {
            return Err(IpcError::TooLarge);
        }
        let sequence = self.outgoing_sequence;
        let next = sequence.checked_add(1).ok_or(IpcError::SequenceExhausted)?;
        let frame_len = HEADER_BYTES + payload.len() + TAG_BYTES;
        let frame_len = u32::try_from(frame_len).map_err(|_| IpcError::TooLarge)?;
        let sequence_bytes = sequence.to_le_bytes();
        let tag = authenticate(
            &self.key,
            self.endpoint.outgoing_domain(),
            sequence_bytes,
            payload,
        );

        self.stream.write_all(&frame_len.to_le_bytes())?;
        self.stream.write_all(&[VERSION])?;
        self.stream.write_all(&sequence_bytes)?;
        self.stream.write_all(payload)?;
        self.stream.write_all(&tag)?;
        self.stream.flush()?;
        self.outgoing_sequence = next;
        Ok(())
    }

    /// Serializes and sends one typed authenticated message.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization, framing, authentication setup, or I/O fails.
    pub fn send_message<T: Serialize>(&mut self, message: &T) -> Result<(), IpcError> {
        let mut payload = postcard::to_allocvec(message).map_err(|_| IpcError::InvalidMessage)?;
        let result = self.send(&payload);
        payload.zeroize();
        result
    }
}

impl<S: Read> AuthenticatedStream<S> {
    /// Reads one authenticated frame. Clean EOF is distinct from truncation.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid framing, authentication, sequence, or I/O.
    pub fn receive(&mut self) -> Result<Option<Vec<u8>>, IpcError> {
        let Some(frame_len) = read_length(&mut self.stream)? else {
            return Ok(None);
        };
        if frame_len < HEADER_BYTES + TAG_BYTES {
            return Err(IpcError::Truncated);
        }
        let payload_len = frame_len - HEADER_BYTES - TAG_BYTES;
        if payload_len > MAX_IPC_MESSAGE_BYTES {
            return Err(IpcError::TooLarge);
        }

        let mut frame = vec![0_u8; frame_len];
        self.stream.read_exact(&mut frame).map_err(map_truncation)?;
        if frame[0] != VERSION {
            return Err(IpcError::UnsupportedVersion);
        }
        let mut sequence_bytes = [0_u8; 8];
        sequence_bytes.copy_from_slice(&frame[1..HEADER_BYTES]);
        let sequence = u64::from_le_bytes(sequence_bytes);
        if sequence != self.incoming_sequence {
            return Err(IpcError::UnexpectedSequence);
        }
        let payload_end = HEADER_BYTES + payload_len;
        let payload = &frame[HEADER_BYTES..payload_end];
        let received_tag = &frame[payload_end..];
        let verification = verify(
            &self.key,
            self.endpoint.incoming_domain(),
            sequence_bytes,
            payload,
            received_tag,
        );
        if let Err(error) = verification {
            frame.zeroize();
            return Err(error);
        }
        self.incoming_sequence = self
            .incoming_sequence
            .checked_add(1)
            .ok_or(IpcError::SequenceExhausted)?;
        let payload = payload.to_vec();
        frame.zeroize();
        Ok(Some(payload))
    }

    /// Receives and deserializes one typed authenticated message.
    ///
    /// # Errors
    ///
    /// Returns an error if framing, authentication, deserialization, or I/O fails.
    pub fn receive_message<T: DeserializeOwned>(&mut self) -> Result<Option<T>, IpcError> {
        self.receive()?
            .map(|mut payload| {
                let message = postcard::from_bytes(&payload).map_err(|_| IpcError::InvalidMessage);
                payload.zeroize();
                message
            })
            .transpose()
    }
}

fn authenticate(
    key: &SessionKey,
    domain: &[u8],
    sequence: [u8; 8],
    payload: &[u8],
) -> [u8; TAG_BYTES] {
    let mut mac = HmacSha256::new_from_slice(&key.0).expect("HMAC accepts a 32-byte key");
    mac.update(domain);
    mac.update(&sequence);
    mac.update(payload);
    mac.finalize().into_bytes().into()
}

fn verify(
    key: &SessionKey,
    domain: &[u8],
    sequence: [u8; 8],
    payload: &[u8],
    tag: &[u8],
) -> Result<(), IpcError> {
    let mut mac = HmacSha256::new_from_slice(&key.0).expect("HMAC accepts a 32-byte key");
    mac.update(domain);
    mac.update(&sequence);
    mac.update(payload);
    mac.verify_slice(tag)
        .map_err(|_| IpcError::AuthenticationFailed)
}

fn read_length<R: Read>(reader: &mut R) -> Result<Option<usize>, IpcError> {
    let mut prefix = [0_u8; 4];
    let mut read = 0;
    while read < prefix.len() {
        match reader.read(&mut prefix[read..]) {
            Ok(0) if read == 0 => return Ok(None),
            Ok(0) => return Err(IpcError::Truncated),
            Ok(count) => read += count,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(IpcError::Io(error)),
        }
    }
    Ok(Some(u32::from_le_bytes(prefix) as usize))
}

fn map_truncation(error: io::Error) -> IpcError {
    if error.kind() == io::ErrorKind::UnexpectedEof {
        IpcError::Truncated
    } else {
        IpcError::Io(error)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    #[cfg(any(windows, target_os = "linux"))]
    use std::thread;

    use super::*;

    const KEY: [u8; 32] = [0x5a; 32];

    #[cfg(any(windows, target_os = "linux"))]
    #[test]
    fn current_user_transport_performs_handshake_inside_the_pipe() {
        let name = format!(
            "inkwell-local-ipc-test-{}-{}",
            std::process::id(),
            getrandom::u64().expect("test name randomness should be available")
        );
        let listener = CurrentUserListener::bind(&name).expect("listener should bind");
        let server = thread::spawn(move || {
            let mut desktop = listener.accept().expect("desktop should accept host");
            assert_eq!(
                desktop.receive().expect("request should read"),
                Some(b"request".to_vec())
            );
            desktop.send(b"terminal").expect("response should write");
        });

        let mut host = connect(&name).expect("host should connect");
        host.send(b"request").expect("request should write");
        assert_eq!(
            host.receive().expect("response should read"),
            Some(b"terminal".to_vec())
        );
        server.join().expect("server should finish");
    }

    #[test]
    fn typed_commands_keep_binary_request_fields_compact() {
        let command = HostCommand::Request(IpcRequest {
            request_id: "123e4567-e89b-42d3-a456-426614174000".to_owned(),
            website_origin: "https://example.com".to_owned(),
            document_name: "contract.pdf".to_owned(),
            preview_pdf: vec![1; 1024],
            byte_range_content: vec![2; 1024],
        });
        let mut wire = Vec::new();
        AuthenticatedStream::new(&mut wire, SessionKey::from_bytes(KEY), Endpoint::Host)
            .send_message(&command)
            .expect("command should encode");

        assert!(wire.len() < 2_200);
        let decoded: HostCommand = AuthenticatedStream::new(
            Cursor::new(wire),
            SessionKey::from_bytes(KEY),
            Endpoint::Desktop,
        )
        .receive_message()
        .expect("command should decode")
        .expect("command should exist");
        let HostCommand::Request(request) = decoded else {
            panic!("expected request command");
        };
        assert_eq!(request.preview_pdf.len(), 1024);
        assert_eq!(request.byte_range_content.len(), 1024);
    }

    #[test]
    fn endpoints_exchange_multiple_authenticated_frames() {
        let mut wire = Vec::new();
        let mut host =
            AuthenticatedStream::new(&mut wire, SessionKey::from_bytes(KEY), Endpoint::Host);
        host.send(b"request").expect("first frame should write");
        host.send(b"disconnect").expect("second frame should write");
        drop(host);

        let mut desktop = AuthenticatedStream::new(
            Cursor::new(wire),
            SessionKey::from_bytes(KEY),
            Endpoint::Desktop,
        );
        assert_eq!(
            desktop.receive().expect("frame should read"),
            Some(b"request".to_vec())
        );
        assert_eq!(
            desktop.receive().expect("frame should read"),
            Some(b"disconnect".to_vec())
        );
        assert_eq!(desktop.receive().expect("EOF should be clean"), None);
    }

    #[test]
    fn rejects_tampering_wrong_keys_and_wrong_directions() {
        let frame = encode(Endpoint::Host, b"request");

        let mut tampered = frame.clone();
        tampered[HEADER_BYTES + 4] ^= 1;
        assert!(matches!(
            decode(Endpoint::Desktop, KEY, tampered),
            Err(IpcError::AuthenticationFailed)
        ));

        let mut wrong_key = KEY;
        wrong_key[0] ^= 1;
        assert!(matches!(
            decode(Endpoint::Desktop, wrong_key, frame.clone()),
            Err(IpcError::AuthenticationFailed)
        ));
        assert!(matches!(
            decode(Endpoint::Host, KEY, frame),
            Err(IpcError::AuthenticationFailed)
        ));
    }

    #[test]
    fn rejects_replayed_and_out_of_order_frames() {
        let first = encode(Endpoint::Host, b"request");
        let mut duplicated = first.clone();
        duplicated.extend_from_slice(&first);
        let mut desktop = AuthenticatedStream::new(
            Cursor::new(duplicated),
            SessionKey::from_bytes(KEY),
            Endpoint::Desktop,
        );
        assert!(
            desktop
                .receive()
                .expect("first frame should pass")
                .is_some()
        );
        assert!(matches!(
            desktop.receive(),
            Err(IpcError::UnexpectedSequence)
        ));
    }

    #[test]
    fn distinguishes_clean_eof_truncation_and_oversize() {
        let mut empty = AuthenticatedStream::new(
            Cursor::new(Vec::new()),
            SessionKey::from_bytes(KEY),
            Endpoint::Desktop,
        );
        assert_eq!(empty.receive().expect("EOF should be clean"), None);

        let mut truncated = AuthenticatedStream::new(
            Cursor::new(vec![1, 0]),
            SessionKey::from_bytes(KEY),
            Endpoint::Desktop,
        );
        assert!(matches!(truncated.receive(), Err(IpcError::Truncated)));

        let too_large = u32::try_from(MAX_IPC_MESSAGE_BYTES + HEADER_BYTES + TAG_BYTES + 1)
            .expect("configured frame limit fits u32")
            .to_le_bytes();
        let mut oversized = AuthenticatedStream::new(
            Cursor::new(too_large),
            SessionKey::from_bytes(KEY),
            Endpoint::Desktop,
        );
        assert!(matches!(oversized.receive(), Err(IpcError::TooLarge)));
    }

    #[test]
    fn debug_output_never_exposes_authentication_material() {
        let key = SessionKey::from_bytes(KEY);
        let output = format!("{key:?}");
        assert_eq!(output, "SessionKey([REDACTED])");
        assert!(!output.contains("5a"));
    }

    fn encode(endpoint: Endpoint, payload: &[u8]) -> Vec<u8> {
        let mut wire = Vec::new();
        AuthenticatedStream::new(&mut wire, SessionKey::from_bytes(KEY), endpoint)
            .send(payload)
            .expect("frame should encode");
        wire
    }

    fn decode(
        endpoint: Endpoint,
        key: [u8; 32],
        frame: Vec<u8>,
    ) -> Result<Option<Vec<u8>>, IpcError> {
        AuthenticatedStream::new(Cursor::new(frame), SessionKey::from_bytes(key), endpoint)
            .receive()
    }
}
