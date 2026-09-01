use std::{
    sync::{Arc, Mutex, mpsc},
    time::Duration,
};

use inkwell_app_core::{ClearRequest, REQUEST_TIMEOUT, RequestMachine, RequestState, RequestToken};
use inkwell_local_ipc::IpcRequest;
use inkwell_protocol::{CancellationReason, SignCancelled, TerminalResponse};
use serde::Serialize;
use tauri::{AppHandle, State};
use zeroize::Zeroize as _;

const PDF_REVIEW_PENDING_MESSAGE: &str =
    "PDF review is unavailable until the PDFium renderer is configured.";
const MAX_REVIEW_SCALE: f64 = 2.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
enum PdfReviewStatus {
    Preparing,
    Ready,
    Failed,
}

#[derive(Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfPageMetadata {
    page_number: u32,
    width: u32,
    height: u32,
}

#[derive(Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfReview {
    request_id: String,
    website_origin: String,
    document_name: String,
    document_size_bytes: u64,
    status: PdfReviewStatus,
    status_message: Option<String>,
    pages: Vec<PdfPageMetadata>,
}

struct ReviewPage {
    metadata: PdfPageMetadata,
    png: Vec<u8>,
}

struct ReviewData {
    display: PdfReview,
    rendered_pages: Vec<ReviewPage>,
}

impl ReviewData {
    fn preparing(request: &IpcRequest) -> Self {
        Self {
            display: PdfReview {
                request_id: request.request_id.clone(),
                website_origin: request.website_origin.clone(),
                document_name: request.document_name.clone(),
                document_size_bytes: request.preview_pdf.len() as u64,
                status: PdfReviewStatus::Preparing,
                status_message: None,
                pages: Vec::new(),
            },
            rendered_pages: Vec::new(),
        }
    }

    fn fail_pending_configuration(&mut self) {
        self.display.status = PdfReviewStatus::Failed;
        self.display.status_message = Some(PDF_REVIEW_PENDING_MESSAGE.to_owned());
        self.display.pages.clear();
        self.rendered_pages.clear();
    }
}

impl Drop for ReviewData {
    fn drop(&mut self) {
        self.display.request_id.zeroize();
        self.display.website_origin.zeroize();
        self.display.document_name.zeroize();
        if let Some(message) = &mut self.display.status_message {
            message.zeroize();
        }
        for page in &mut self.rendered_pages {
            page.png.zeroize();
        }
    }
}

struct RequestData(IpcRequest);

impl ClearRequest for RequestData {
    fn clear(&mut self) {
        self.0.clear();
    }
}

struct ActiveUiRequest {
    token: RequestToken,
    request_id: String,
    review: Option<ReviewData>,
    terminal: mpsc::Sender<TerminalResponse>,
}

impl Drop for ActiveUiRequest {
    fn drop(&mut self) {
        self.request_id.zeroize();
    }
}

#[derive(Default)]
struct Inner {
    machine: RequestMachine<RequestData>,
    active_ui: Option<ActiveUiRequest>,
}

#[derive(Clone, Default)]
pub struct WalkingSkeletonState {
    inner: Arc<Mutex<Inner>>,
}

impl WalkingSkeletonState {
    pub fn accept(
        &self,
        request: IpcRequest,
    ) -> Result<mpsc::Receiver<TerminalResponse>, Box<TerminalResponse>> {
        let mut review = ReviewData::preparing(&request);
        review.fail_pending_configuration();
        let request_id = request.request_id.clone();
        let mut inner = self.inner.lock().map_err(|_| Box::new(internal_error()))?;
        let token = inner
            .machine
            .accept(request_id.clone(), RequestData(request), Duration::ZERO)
            .map_err(|busy| Box::new(busy.response))?;
        let (terminal, receiver) = mpsc::channel();
        inner.active_ui = Some(ActiveUiRequest {
            token,
            request_id,
            review: Some(review),
            terminal,
        });
        Ok(receiver)
    }

    fn review(&self) -> Result<Option<PdfReview>, &'static str> {
        self.inner
            .lock()
            .map(|inner| {
                inner
                    .active_ui
                    .as_ref()
                    .and_then(|active| active.review.as_ref())
                    .map(|review| review.display.clone())
            })
            .map_err(|_| "request state is unavailable")
    }

    fn render_page(
        &self,
        request_id: &str,
        page_number: u32,
        scale: f64,
    ) -> Result<Vec<u8>, &'static str> {
        if !scale.is_finite() || scale <= 0.0 || scale > MAX_REVIEW_SCALE {
            return Err("page scale must be between zero and two");
        }
        let inner = self
            .inner
            .lock()
            .map_err(|_| "request state is unavailable")?;
        let active = inner
            .active_ui
            .as_ref()
            .filter(|active| active.request_id == request_id)
            .ok_or("request is no longer active")?;
        let review = active
            .review
            .as_ref()
            .ok_or("PDF review is no longer active")?;
        if review.display.status != PdfReviewStatus::Ready {
            return Err("PDF review is not ready");
        }
        review
            .rendered_pages
            .iter()
            .find(|page| page.metadata.page_number == page_number)
            .map(|page| page.png.clone())
            .ok_or("PDF review page is unavailable")
    }

    fn continue_signing(&self, request_id: &str) -> Result<(), &'static str> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| "request state is unavailable")?;
        let active = inner
            .active_ui
            .as_ref()
            .filter(|active| active.request_id == request_id)
            .ok_or("request is no longer active")?;
        if active.review.as_ref().map(|review| review.display.status)
            != Some(PdfReviewStatus::Ready)
        {
            return Err("PDF review is not ready");
        }
        let token = active.token;
        inner
            .machine
            .transition(token, RequestState::DiscoveringCertificates)
            .map_err(|_| "request transition failed")?;
        inner
            .active_ui
            .as_mut()
            .expect("matching UI request must be present")
            .review
            .take();
        Ok(())
    }

    fn cancel(
        &self,
        request_id: &str,
        reason: CancellationReason,
    ) -> Result<SignCancelled, &'static str> {
        let response = self.finish(request_id, |machine, token| {
            machine.cancel(token, reason).map(Some)
        })?;
        let TerminalResponse::Cancelled(cancelled) = response else {
            return Err("request cancellation failed");
        };
        Ok(cancelled)
    }

    pub fn timeout(&self, request_id: &str) -> Result<TerminalResponse, &'static str> {
        self.finish(request_id, |machine, token| {
            machine.poll_timeout(token, REQUEST_TIMEOUT)
        })
    }

    pub fn disconnect(&self, request_id: &str) -> Result<TerminalResponse, &'static str> {
        self.finish(request_id, |machine, token| {
            machine.extension_disconnected(token)
        })
    }

    pub fn close_active_window(&self) -> Option<String> {
        let request_id = self
            .inner
            .lock()
            .ok()?
            .active_ui
            .as_ref()
            .map(|active| active.request_id.clone())?;
        self.cancel(&request_id, CancellationReason::WindowClosed)
            .ok()?;
        Some(request_id)
    }

    fn finish(
        &self,
        request_id: &str,
        transition: impl FnOnce(
            &mut RequestMachine<RequestData>,
            RequestToken,
        )
            -> Result<Option<TerminalResponse>, inkwell_app_core::TransitionError>,
    ) -> Result<TerminalResponse, &'static str> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| "request state is unavailable")?;
        let active = inner
            .active_ui
            .as_ref()
            .filter(|active| active.request_id == request_id)
            .ok_or("request is no longer active")?;
        let token = active.token;
        let response = transition(&mut inner.machine, token)
            .map_err(|_| "request transition failed")?
            .ok_or("request cannot terminate in its current state")?;
        let active = inner
            .active_ui
            .take()
            .expect("matching UI request must be present");
        let _ = active.terminal.send(response.clone());
        Ok(response)
    }

    #[cfg(test)]
    fn install_ready_pages(
        &self,
        request_id: &str,
        pages: Vec<(PdfPageMetadata, Vec<u8>)>,
    ) -> Result<(), &'static str> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| "request state is unavailable")?;
        let active = inner
            .active_ui
            .as_mut()
            .filter(|active| active.request_id == request_id)
            .ok_or("request is no longer active")?;
        let review = active
            .review
            .as_mut()
            .ok_or("PDF review is no longer active")?;
        review.display.status = PdfReviewStatus::Ready;
        review.display.status_message = None;
        review.display.pages = pages.iter().map(|(metadata, _)| metadata.clone()).collect();
        review.rendered_pages = pages
            .into_iter()
            .map(|(metadata, png)| ReviewPage { metadata, png })
            .collect();
        Ok(())
    }
}

fn internal_error() -> TerminalResponse {
    TerminalResponse::Error(inkwell_protocol::SignError::new(
        None,
        inkwell_protocol::ErrorCode::InternalError,
        "The request state is unavailable.".to_owned(),
    ))
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn pdf_review_state(
    state: State<'_, WalkingSkeletonState>,
) -> Result<Option<PdfReview>, &'static str> {
    state.review()
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn render_pdf_review_page(
    request_id: String,
    page_number: u32,
    scale: f64,
    state: State<'_, WalkingSkeletonState>,
) -> Result<tauri::ipc::Response, &'static str> {
    state
        .render_page(&request_id, page_number, scale)
        .map(tauri::ipc::Response::new)
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn continue_signing_request(
    request_id: String,
    state: State<'_, WalkingSkeletonState>,
    app: AppHandle,
) -> Result<(), &'static str> {
    state.continue_signing(&request_id)?;
    crate::ipc_server::emit_request_invalidated(&app, &request_id);
    Ok(())
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn cancel_signing_request(
    request_id: String,
    state: State<'_, WalkingSkeletonState>,
    app: AppHandle,
) -> Result<(), &'static str> {
    state.cancel(&request_id, CancellationReason::UserCancelled)?;
    crate::ipc_server::emit_request_invalidated(&app, &request_id);
    Ok(())
}

#[cfg(test)]
mod tests {
    use inkwell_protocol::ErrorCode;

    use super::*;

    const REQUEST_ID: &str = "123e4567-e89b-42d3-a456-426614174000";
    const PNG: &[u8] = b"\x89PNG\r\n\x1a\nfixture";

    #[test]
    fn accepted_request_reports_pdfium_configuration_failure() {
        let state = WalkingSkeletonState::default();
        let _receiver = state
            .accept(request(REQUEST_ID))
            .expect("request should be accepted");

        let review = state
            .review()
            .expect("state should be available")
            .expect("review should be present");
        assert_eq!(review.request_id, REQUEST_ID);
        assert_eq!(review.document_size_bytes, 9);
        assert_eq!(review.status, PdfReviewStatus::Failed);
        assert_eq!(
            review.status_message.as_deref(),
            Some(PDF_REVIEW_PENDING_MESSAGE)
        );
        assert!(review.pages.is_empty());
        assert!(state.continue_signing(REQUEST_ID).is_err());
        assert!(state.render_page(REQUEST_ID, 1, 1.0).is_err());
    }

    #[test]
    fn ready_backend_controls_pages_rendering_and_continuation() {
        let state = WalkingSkeletonState::default();
        let _receiver = state
            .accept(request(REQUEST_ID))
            .expect("request should be accepted");
        state
            .install_ready_pages(
                REQUEST_ID,
                vec![(
                    PdfPageMetadata {
                        page_number: 1,
                        width: 612,
                        height: 792,
                    },
                    PNG.to_vec(),
                )],
            )
            .expect("test backend should install pages");

        let review = state.review().unwrap().unwrap();
        assert_eq!(review.status, PdfReviewStatus::Ready);
        assert_eq!(review.pages.len(), 1);
        assert_eq!(state.render_page(REQUEST_ID, 1, 1.5).unwrap(), PNG.to_vec());
        assert!(state.render_page(REQUEST_ID, 2, 1.0).is_err());
        assert!(state.render_page(REQUEST_ID, 1, 0.0).is_err());

        state
            .continue_signing(REQUEST_ID)
            .expect("ready request should continue");
        assert!(state.review().unwrap().is_none());
        assert!(state.continue_signing(REQUEST_ID).is_err());
    }

    #[test]
    fn cancellation_consumes_the_request_and_notifies_the_ipc_handler() {
        let state = WalkingSkeletonState::default();
        let receiver = state
            .accept(request(REQUEST_ID))
            .expect("request should be accepted");

        let response = state
            .cancel(REQUEST_ID, CancellationReason::UserCancelled)
            .expect("request should cancel");

        assert_eq!(response.reason, CancellationReason::UserCancelled);
        assert!(state.review().expect("state should be available").is_none());
        assert!(matches!(
            receiver.recv().expect("IPC handler should be notified"),
            TerminalResponse::Cancelled(_)
        ));
        assert!(
            state
                .cancel(REQUEST_ID, CancellationReason::UserCancelled)
                .is_err()
        );
    }

    #[test]
    fn disconnect_consumes_the_request_exactly_once() {
        let state = WalkingSkeletonState::default();
        let receiver = state
            .accept(request(REQUEST_ID))
            .expect("request should be accepted");

        let response = state
            .disconnect(REQUEST_ID)
            .expect("request should disconnect");
        let TerminalResponse::Error(error) = response else {
            panic!("disconnect must be an error");
        };
        assert_eq!(error.error.code, ErrorCode::ExtensionDisconnected);
        assert!(matches!(receiver.recv(), Ok(TerminalResponse::Error(_))));
        assert!(state.review().unwrap().is_none());
        assert!(state.disconnect(REQUEST_ID).is_err());
    }

    #[test]
    fn concurrent_request_gets_busy_and_timeout_releases_the_slot() {
        let state = WalkingSkeletonState::default();
        let first = state
            .accept(request(REQUEST_ID))
            .expect("first request should be accepted");
        let busy = state
            .accept(request("123e4567-e89b-42d3-a456-426614174001"))
            .expect_err("second request should be busy");
        let TerminalResponse::Error(error) = *busy else {
            panic!("busy must be an error");
        };
        assert_eq!(error.error.code, ErrorCode::Busy);

        let timeout = state.timeout(REQUEST_ID).expect("request should time out");
        let TerminalResponse::Error(error) = timeout else {
            panic!("timeout must be an error");
        };
        assert_eq!(error.error.code, ErrorCode::RequestTimeout);
        assert!(matches!(first.recv(), Ok(TerminalResponse::Error(_))));
        assert!(state.review().unwrap().is_none());
        assert!(
            state
                .accept(request("123e4567-e89b-42d3-a456-426614174002"))
                .is_ok()
        );
    }

    fn request(request_id: &str) -> IpcRequest {
        IpcRequest {
            request_id: request_id.to_owned(),
            website_origin: "https://example.com".to_owned(),
            document_name: "contract.pdf".to_owned(),
            preview_pdf: b"%PDF-1.7\n".to_vec(),
            byte_range_content: b"byte range".to_vec(),
        }
    }
}
