use std::{
    sync::{Arc, Mutex, mpsc},
    time::Duration,
};

use inkwell_app_core::{ClearRequest, REQUEST_TIMEOUT, RequestMachine, RequestToken};
use inkwell_local_ipc::IpcRequest;
use inkwell_protocol::{CancellationReason, SignCancelled, TerminalResponse};
use serde::Serialize;
use tauri::State;
use zeroize::Zeroize as _;

#[derive(Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingSigningRequest {
    request_id: String,
    website_origin: String,
    document_name: String,
}

struct RequestData(IpcRequest);

impl ClearRequest for RequestData {
    fn clear(&mut self) {
        self.0.clear();
    }
}

struct ActiveUiRequest {
    token: RequestToken,
    display: PendingSigningRequest,
    terminal: mpsc::Sender<TerminalResponse>,
}

impl Drop for ActiveUiRequest {
    fn drop(&mut self) {
        self.display.request_id.zeroize();
        self.display.website_origin.zeroize();
        self.display.document_name.zeroize();
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
        let display = PendingSigningRequest {
            request_id: request.request_id.clone(),
            website_origin: request.website_origin.clone(),
            document_name: request.document_name.clone(),
        };
        let mut inner = self.inner.lock().map_err(|_| Box::new(internal_error()))?;
        let token = inner
            .machine
            .accept(
                request.request_id.clone(),
                RequestData(request),
                Duration::ZERO,
            )
            .map_err(|busy| Box::new(busy.response))?;
        let (terminal, receiver) = mpsc::channel();
        inner.active_ui = Some(ActiveUiRequest {
            token,
            display,
            terminal,
        });
        Ok(receiver)
    }

    fn pending(&self) -> Result<Option<PendingSigningRequest>, &'static str> {
        self.inner
            .lock()
            .map(|inner| {
                inner
                    .active_ui
                    .as_ref()
                    .map(|active| active.display.clone())
            })
            .map_err(|_| "request state is unavailable")
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

    pub fn close_active_window(&self) {
        let request_id = self
            .pending()
            .ok()
            .flatten()
            .map(|request| request.request_id);
        if let Some(request_id) = request_id {
            let _ = self.cancel(&request_id, CancellationReason::WindowClosed);
        }
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
            .filter(|active| active.display.request_id == request_id)
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
pub fn pending_signing_request(
    state: State<'_, WalkingSkeletonState>,
) -> Result<Option<PendingSigningRequest>, &'static str> {
    state.pending()
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn cancel_signing_request(
    request_id: String,
    state: State<'_, WalkingSkeletonState>,
) -> Result<SignCancelled, &'static str> {
    state.cancel(&request_id, CancellationReason::UserCancelled)
}

#[cfg(test)]
mod tests {
    use inkwell_protocol::ErrorCode;

    use super::*;

    const REQUEST_ID: &str = "123e4567-e89b-42d3-a456-426614174000";

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
        assert!(
            state
                .pending()
                .expect("state should be available")
                .is_none()
        );
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
