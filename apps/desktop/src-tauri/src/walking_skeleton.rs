use std::sync::Mutex;

use inkwell_protocol::{CancellationReason, SignCancelled};
use serde::Serialize;
use tauri::State;

#[derive(Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingSigningRequest {
    request_id: String,
    website_origin: String,
    document_name: String,
}

#[derive(Default)]
pub struct WalkingSkeletonState {
    pending: Mutex<Option<PendingSigningRequest>>,
}

impl WalkingSkeletonState {
    fn pending(&self) -> Result<Option<PendingSigningRequest>, &'static str> {
        self.pending
            .lock()
            .map(|pending| pending.clone())
            .map_err(|_| "request state is unavailable")
    }

    fn cancel(&self, request_id: &str) -> Result<SignCancelled, &'static str> {
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| "request state is unavailable")?;
        let matches = pending
            .as_ref()
            .is_some_and(|request| request.request_id == request_id);
        if !matches {
            return Err("request is no longer active");
        }
        let request = pending.take().expect("matching request must be present");
        Ok(SignCancelled::new(
            request.request_id,
            CancellationReason::UserCancelled,
        ))
    }

    #[cfg(test)]
    fn display(&self, request: PendingSigningRequest) -> Result<(), &'static str> {
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| "request state is unavailable")?;
        if pending.is_some() {
            return Err("another request is already active");
        }
        *pending = Some(request);
        Ok(())
    }
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
    state.cancel(&request_id)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use base64::Engine as _;
    use inkwell_native_host::{
        DisplayRequest, MAX_REQUEST_FRAME_BYTES, MAX_RESPONSE_FRAME_BYTES, UiAdapter,
        UiUnavailable, read_frame, serve_one, write_frame,
    };
    use inkwell_protocol::CancellationReason;
    use serde_json::json;
    use sha2::{Digest, Sha256};

    use super::*;

    #[test]
    fn cancellation_consumes_the_request_and_builds_a_terminal_response() {
        let state = WalkingSkeletonState {
            pending: Mutex::new(Some(PendingSigningRequest {
                request_id: "123e4567-e89b-42d3-a456-426614174000".to_owned(),
                website_origin: "https://example.com".to_owned(),
                document_name: "contract.pdf".to_owned(),
            })),
        };

        let response = state
            .cancel("123e4567-e89b-42d3-a456-426614174000")
            .expect("request should cancel");

        assert_eq!(
            serde_json::to_value(response).expect("response should serialize"),
            json!({
                "version": 1,
                "type": "sign_cancelled",
                "requestId": "123e4567-e89b-42d3-a456-426614174000",
                "reason": "user_cancelled"
            })
        );
        assert!(
            state
                .pending()
                .expect("state should remain available")
                .is_none()
        );
        assert!(
            state
                .cancel("123e4567-e89b-42d3-a456-426614174000")
                .is_err()
        );
    }

    struct StateBackedUi<'a> {
        state: &'a WalkingSkeletonState,
    }

    impl UiAdapter for StateBackedUi<'_> {
        fn display_and_cancel(
            &mut self,
            request: DisplayRequest<'_>,
        ) -> Result<CancellationReason, UiUnavailable> {
            self.state
                .display(PendingSigningRequest {
                    request_id: request.request_id.to_owned(),
                    website_origin: request.website_origin.to_owned(),
                    document_name: request.document_name.to_owned(),
                })
                .map_err(|_| UiUnavailable)?;

            let displayed = self.state.pending().map_err(|_| UiUnavailable)?;
            let displayed = displayed.as_ref().ok_or(UiUnavailable)?;
            assert_eq!(displayed.website_origin, "https://example.com");
            assert_eq!(displayed.document_name, "contract.pdf");

            self.state
                .cancel(request.request_id)
                .map(|response| response.reason)
                .map_err(|_| UiUnavailable)
        }
    }

    #[test]
    fn framed_request_crosses_the_temporary_ui_state_and_returns_cancellation() {
        let preview = b"%PDF-1.7\n";
        let byte_range = b"byte range";
        let request = json!({
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
        });
        let mut input = Vec::new();
        write_frame(
            &mut input,
            &serde_json::to_vec(&request).expect("fixture should serialize"),
            MAX_REQUEST_FRAME_BYTES,
        )
        .expect("request frame should write");
        let mut output = Vec::new();
        let state = WalkingSkeletonState::default();

        serve_one(
            Cursor::new(input),
            &mut output,
            &mut StateBackedUi { state: &state },
        )
        .expect("walking skeleton should complete");

        let body = read_frame(&mut Cursor::new(output), MAX_RESPONSE_FRAME_BYTES)
            .expect("response frame should read")
            .expect("response should exist");
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&body).expect("response should be JSON"),
            json!({
                "version": 1,
                "type": "sign_cancelled",
                "requestId": "123e4567-e89b-42d3-a456-426614174000",
                "reason": "user_cancelled"
            })
        );
        assert!(
            state
                .pending()
                .expect("state should remain available")
                .is_none()
        );
    }
}
