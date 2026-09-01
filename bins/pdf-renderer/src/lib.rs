//! Strict request loop for the isolated PDF renderer process.

use std::error::Error;
use std::fmt;
use std::io::{Read, Write};

use inkwell_renderer_protocol::{
    DocumentOpened, PageMetadata, ProtocolError, RenderPageRequest, RenderedPage, RendererError,
    RendererErrorCode, Request, RequestCommand, Response, ResponseResult, read_request,
    write_response,
};

const NOT_CONFIGURED: &str = "renderer distribution is not configured";
const INVALID_BACKEND_OUTPUT: &str = "renderer backend returned invalid output";

/// Errors a rendering backend may report without exposing backend diagnostics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EngineError {
    InvalidPdf,
    EncryptedPdf,
    PageOutOfRange,
    RenderFailed,
    Internal(&'static str),
}

/// Backend boundary for document parsing and rasterization.
pub trait RenderEngine {
    /// Opens one document.
    ///
    /// # Errors
    ///
    /// Returns a structured engine error when the document cannot be opened.
    fn open_document(&mut self, document: &[u8]) -> Result<DocumentOpened, EngineError>;

    /// Reads metadata for one page in the open document.
    ///
    /// # Errors
    ///
    /// Returns a structured engine error when metadata cannot be read.
    fn page_metadata(&mut self, page_index: u32) -> Result<PageMetadata, EngineError>;

    /// Renders one page in the open document.
    ///
    /// # Errors
    ///
    /// Returns a structured engine error when the page cannot be rendered.
    fn render_page(&mut self, request: RenderPageRequest) -> Result<RenderedPage, EngineError>;

    /// Closes the open document and releases its resources.
    ///
    /// # Errors
    ///
    /// Returns a structured engine error when cleanup fails.
    fn close_document(&mut self) -> Result<(), EngineError>;
}

/// Production backend placeholder until a pinned `PDFium` distribution is packaged.
#[derive(Debug, Default)]
pub struct DistributionEngine;

impl RenderEngine for DistributionEngine {
    fn open_document(&mut self, _document: &[u8]) -> Result<DocumentOpened, EngineError> {
        Err(EngineError::Internal(NOT_CONFIGURED))
    }

    fn page_metadata(&mut self, _page_index: u32) -> Result<PageMetadata, EngineError> {
        Err(EngineError::Internal(NOT_CONFIGURED))
    }

    fn render_page(&mut self, _request: RenderPageRequest) -> Result<RenderedPage, EngineError> {
        Err(EngineError::Internal(NOT_CONFIGURED))
    }

    fn close_document(&mut self) -> Result<(), EngineError> {
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum State {
    Closed,
    Open,
}

struct Renderer<E> {
    engine: E,
    state: State,
}

impl<E: RenderEngine> Renderer<E> {
    const fn new(engine: E) -> Self {
        Self {
            engine,
            state: State::Closed,
        }
    }

    fn handle(&mut self, request: Request) -> Handled {
        let request_id = request.request_id;
        let (result, shutdown) = match request.command {
            RequestCommand::OpenDocument(open) => (self.open(&open.document), false),
            RequestCommand::PageMetadata(metadata) => (self.metadata(metadata.page_index), false),
            RequestCommand::RenderPage(render) => (self.render(render), false),
            RequestCommand::CloseDocument => (self.close(), false),
            RequestCommand::Shutdown => (self.shutdown(), true),
        };

        Handled {
            response: Response::new(request_id, result),
            shutdown,
        }
    }

    fn open(&mut self, document: &[u8]) -> ResponseResult {
        if self.state == State::Open {
            return renderer_error(
                RendererErrorCode::InvalidRequest,
                "a document is already open",
            );
        }

        match self.engine.open_document(document) {
            Ok(opened) => {
                let result = ResponseResult::DocumentOpened(opened);
                if valid_result(&result) {
                    self.state = State::Open;
                    result
                } else {
                    let _ = self.engine.close_document();
                    renderer_error(RendererErrorCode::Internal, INVALID_BACKEND_OUTPUT)
                }
            }
            Err(error) => engine_error(error),
        }
    }

    fn metadata(&mut self, page_index: u32) -> ResponseResult {
        if self.state == State::Closed {
            return document_not_open();
        }

        match self.engine.page_metadata(page_index) {
            Ok(metadata) if metadata.page_index == page_index => {
                checked_result(ResponseResult::PageMetadata(metadata))
            }
            Ok(_) => renderer_error(RendererErrorCode::Internal, INVALID_BACKEND_OUTPUT),
            Err(error) => engine_error(error),
        }
    }

    fn render(&mut self, request: RenderPageRequest) -> ResponseResult {
        if self.state == State::Closed {
            return document_not_open();
        }

        match self.engine.render_page(request) {
            Ok(rendered)
                if rendered.page_index == request.page_index
                    && rendered.width == request.width
                    && rendered.height == request.height =>
            {
                checked_result(ResponseResult::PageRendered(rendered))
            }
            Ok(_) => renderer_error(RendererErrorCode::Internal, INVALID_BACKEND_OUTPUT),
            Err(error) => engine_error(error),
        }
    }

    fn close(&mut self) -> ResponseResult {
        if self.state == State::Closed {
            return document_not_open();
        }

        match self.engine.close_document() {
            Ok(()) => {
                self.state = State::Closed;
                ResponseResult::DocumentClosed
            }
            Err(error) => engine_error(error),
        }
    }

    fn shutdown(&mut self) -> ResponseResult {
        if self.state == State::Open {
            match self.engine.close_document() {
                Ok(()) => self.state = State::Closed,
                Err(error) => return engine_error(error),
            }
        }
        ResponseResult::ShuttingDown
    }

    fn cleanup(&mut self) {
        if self.state == State::Open {
            let _ = self.engine.close_document();
            self.state = State::Closed;
        }
    }
}

struct Handled {
    response: Response,
    shutdown: bool,
}

/// A terminal framing or protocol I/O failure.
#[derive(Debug)]
pub struct ServeError(ProtocolError);

impl fmt::Display for ServeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("renderer sidecar protocol failed")
    }
}

impl Error for ServeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.0)
    }
}

/// Serves framed requests until shutdown, clean EOF, or a protocol failure.
///
/// The writer receives only protocol frames. Callers must not print diagnostics
/// to the same stream.
///
/// # Errors
///
/// Returns an error when a request cannot be read or a response cannot be
/// written. Any open document is closed before returning.
pub fn serve<R: Read, W: Write, E: RenderEngine>(
    reader: &mut R,
    writer: &mut W,
    engine: E,
) -> Result<(), ServeError> {
    let mut renderer = Renderer::new(engine);
    loop {
        let request = match read_request(reader) {
            Ok(Some(request)) => request,
            Ok(None) => {
                renderer.cleanup();
                return Ok(());
            }
            Err(error) => {
                renderer.cleanup();
                return Err(ServeError(error));
            }
        };
        let handled = renderer.handle(request);
        if let Err(error) = write_response(writer, &handled.response) {
            renderer.cleanup();
            return Err(ServeError(error));
        }
        if handled.shutdown {
            renderer.cleanup();
            return Ok(());
        }
    }
}

fn checked_result(result: ResponseResult) -> ResponseResult {
    if valid_result(&result) {
        result
    } else {
        renderer_error(RendererErrorCode::Internal, INVALID_BACKEND_OUTPUT)
    }
}

fn valid_result(result: &ResponseResult) -> bool {
    Response::new(0, result.clone()).validate().is_ok()
}

fn document_not_open() -> ResponseResult {
    renderer_error(RendererErrorCode::DocumentNotOpen, "no document is open")
}

fn engine_error(error: EngineError) -> ResponseResult {
    match error {
        EngineError::InvalidPdf => {
            renderer_error(RendererErrorCode::InvalidPdf, "PDF structure is invalid")
        }
        EngineError::EncryptedPdf => renderer_error(
            RendererErrorCode::EncryptedPdf,
            "encrypted PDFs are not supported",
        ),
        EngineError::PageOutOfRange => renderer_error(
            RendererErrorCode::PageOutOfRange,
            "page index is out of range",
        ),
        EngineError::RenderFailed => {
            renderer_error(RendererErrorCode::RenderFailed, "page rendering failed")
        }
        EngineError::Internal(message) => renderer_error(RendererErrorCode::Internal, message),
    }
}

fn renderer_error(code: RendererErrorCode, message: &'static str) -> ResponseResult {
    ResponseResult::Error(RendererError {
        code,
        message: message.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::io::Cursor;
    use std::rc::Rc;

    use inkwell_renderer_protocol::{
        OpenDocument, PageMetadataRequest, PixelFormat, read_response, write_request,
    };

    use super::*;

    #[derive(Clone, Debug, Eq, PartialEq)]
    enum Call {
        Open(Vec<u8>),
        Metadata(u32),
        Render(RenderPageRequest),
        Close,
    }

    struct FakeEngine {
        calls: Rc<RefCell<Vec<Call>>>,
        open_results: VecDeque<Result<DocumentOpened, EngineError>>,
        metadata_result: Result<PageMetadata, EngineError>,
        render_result: Result<RenderedPage, EngineError>,
    }

    impl FakeEngine {
        fn successful(calls: Rc<RefCell<Vec<Call>>>) -> Self {
            Self {
                calls,
                open_results: [Ok(DocumentOpened { page_count: 2 })].into(),
                metadata_result: Ok(PageMetadata {
                    page_index: 1,
                    width_millipoints: 612_000,
                    height_millipoints: 792_000,
                }),
                render_result: Ok(RenderedPage {
                    page_index: 1,
                    width: 2,
                    height: 1,
                    stride: 8,
                    format: PixelFormat::Rgba8,
                    pixels: vec![7; 8],
                }),
            }
        }
    }

    impl RenderEngine for FakeEngine {
        fn open_document(&mut self, document: &[u8]) -> Result<DocumentOpened, EngineError> {
            self.calls.borrow_mut().push(Call::Open(document.to_vec()));
            self.open_results
                .pop_front()
                .expect("fake open result should be configured")
        }

        fn page_metadata(&mut self, page_index: u32) -> Result<PageMetadata, EngineError> {
            self.calls.borrow_mut().push(Call::Metadata(page_index));
            self.metadata_result
        }

        fn render_page(&mut self, request: RenderPageRequest) -> Result<RenderedPage, EngineError> {
            self.calls.borrow_mut().push(Call::Render(request));
            self.render_result.clone()
        }

        fn close_document(&mut self) -> Result<(), EngineError> {
            self.calls.borrow_mut().push(Call::Close);
            Ok(())
        }
    }

    #[test]
    fn serves_open_metadata_render_close_and_shutdown_in_order() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let engine = FakeEngine::successful(Rc::clone(&calls));
        let requests = [
            Request::new(
                1,
                RequestCommand::OpenDocument(OpenDocument {
                    document: b"test document".to_vec(),
                }),
            ),
            Request::new(
                2,
                RequestCommand::PageMetadata(PageMetadataRequest { page_index: 1 }),
            ),
            Request::new(
                3,
                RequestCommand::RenderPage(RenderPageRequest {
                    page_index: 1,
                    width: 2,
                    height: 1,
                }),
            ),
            Request::new(4, RequestCommand::CloseDocument),
            Request::new(5, RequestCommand::Shutdown),
        ];

        let responses = run(engine, &requests);

        assert!(matches!(
            responses[0].result,
            ResponseResult::DocumentOpened(_)
        ));
        assert!(matches!(
            responses[1].result,
            ResponseResult::PageMetadata(_)
        ));
        assert!(matches!(
            responses[2].result,
            ResponseResult::PageRendered(_)
        ));
        assert_eq!(responses[3].result, ResponseResult::DocumentClosed);
        assert_eq!(responses[4].result, ResponseResult::ShuttingDown);
        assert_eq!(
            *calls.borrow(),
            [
                Call::Open(b"test document".to_vec()),
                Call::Metadata(1),
                Call::Render(RenderPageRequest {
                    page_index: 1,
                    width: 2,
                    height: 1,
                }),
                Call::Close,
            ]
        );
    }

    #[test]
    fn rejects_commands_that_violate_document_sequencing() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let mut engine = FakeEngine::successful(Rc::clone(&calls));
        engine
            .open_results
            .push_back(Ok(DocumentOpened { page_count: 2 }));
        let requests = [
            Request::new(
                1,
                RequestCommand::PageMetadata(PageMetadataRequest { page_index: 0 }),
            ),
            Request::new(2, RequestCommand::CloseDocument),
            Request::new(
                3,
                RequestCommand::OpenDocument(OpenDocument {
                    document: b"first".to_vec(),
                }),
            ),
            Request::new(
                4,
                RequestCommand::OpenDocument(OpenDocument {
                    document: b"second".to_vec(),
                }),
            ),
            Request::new(5, RequestCommand::Shutdown),
        ];

        let responses = run(engine, &requests);

        assert_error(&responses[0], RendererErrorCode::DocumentNotOpen);
        assert_error(&responses[1], RendererErrorCode::DocumentNotOpen);
        assert!(matches!(
            responses[2].result,
            ResponseResult::DocumentOpened(_)
        ));
        assert_error(&responses[3], RendererErrorCode::InvalidRequest);
        assert_eq!(responses[4].result, ResponseResult::ShuttingDown);
        assert_eq!(
            *calls.borrow(),
            [Call::Open(b"first".to_vec()), Call::Close]
        );
    }

    #[test]
    fn maps_engine_failures_to_structured_errors() {
        for (failure, expected) in [
            (EngineError::InvalidPdf, RendererErrorCode::InvalidPdf),
            (EngineError::EncryptedPdf, RendererErrorCode::EncryptedPdf),
        ] {
            let calls = Rc::new(RefCell::new(Vec::new()));
            let mut engine = FakeEngine::successful(calls);
            engine.open_results = [Err(failure)].into();
            let responses = run(
                engine,
                &[open_request(1), Request::new(2, RequestCommand::Shutdown)],
            );
            assert_error(&responses[0], expected);
        }

        let calls = Rc::new(RefCell::new(Vec::new()));
        let mut engine = FakeEngine::successful(calls);
        engine.render_result = Err(EngineError::RenderFailed);
        let responses = run(
            engine,
            &[
                open_request(1),
                Request::new(
                    2,
                    RequestCommand::RenderPage(RenderPageRequest {
                        page_index: 1,
                        width: 2,
                        height: 1,
                    }),
                ),
                Request::new(3, RequestCommand::Shutdown),
            ],
        );
        assert_error(&responses[1], RendererErrorCode::RenderFailed);
    }

    #[test]
    fn clean_eof_closes_an_open_document() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let engine = FakeEngine::successful(Rc::clone(&calls));
        let responses = run(engine, &[open_request(1)]);

        assert_eq!(responses.len(), 1);
        assert_eq!(*calls.borrow(), [Call::Open(b"pdf".to_vec()), Call::Close]);
    }

    #[test]
    fn production_engine_fails_closed_without_parsing_input() {
        let responses = run(
            DistributionEngine,
            &[open_request(1), Request::new(2, RequestCommand::Shutdown)],
        );

        let ResponseResult::Error(error) = &responses[0].result else {
            panic!("open should return a structured error");
        };
        assert_eq!(error.code, RendererErrorCode::Internal);
        assert_eq!(error.message, NOT_CONFIGURED);
    }

    fn open_request(request_id: u64) -> Request {
        Request::new(
            request_id,
            RequestCommand::OpenDocument(OpenDocument {
                document: b"pdf".to_vec(),
            }),
        )
    }

    fn run<E: RenderEngine>(engine: E, requests: &[Request]) -> Vec<Response> {
        let mut input = Vec::new();
        for request in requests {
            write_request(&mut input, request).expect("request should encode");
        }
        let mut output = Vec::new();
        serve(&mut Cursor::new(input), &mut output, engine).expect("server should complete");

        let mut reader = Cursor::new(output);
        let mut responses = Vec::new();
        while let Some(response) = read_response(&mut reader).expect("response should decode") {
            responses.push(response);
        }
        responses
    }

    fn assert_error(response: &Response, expected: RendererErrorCode) {
        let ResponseResult::Error(error) = &response.result else {
            panic!("response should be an error");
        };
        assert_eq!(error.code, expected);
        assert!(!error.message.is_empty());
    }
}
