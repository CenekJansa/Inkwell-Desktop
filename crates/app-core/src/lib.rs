//! Platform-independent signing request lifecycle.
//!
//! UI and Windows integrations drive this state machine. It owns the active
//! request slot and returns terminal responses to its caller instead of keeping
//! completed results for retry.

use std::time::Duration;

use inkwell_protocol::{CancellationReason, ErrorCode, SignCancelled, SignError, TerminalResponse};
use thiserror::Error;

pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(15 * 60);

/// Request-owned values implement this to erase sensitive buffers before the
/// active slot is released.
pub trait ClearRequest {
    fn clear(&mut self);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequestState {
    Reviewing,
    DiscoveringCertificates,
    Confirming,
    Signing,
    Responding,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RequestToken(u64);

#[derive(Debug, Eq, PartialEq)]
pub struct Busy {
    pub response: TerminalResponse,
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum TransitionError {
    #[error("there is no matching active request")]
    NoActiveRequest,
    #[error("the request state transition is not legal")]
    IllegalTransition,
}

struct ActiveRequest<T> {
    token: RequestToken,
    request_id: String,
    data: T,
    state: RequestState,
    accepted_at: Duration,
}

pub struct RequestMachine<T: ClearRequest> {
    next_token: u64,
    active: Option<ActiveRequest<T>>,
}

impl<T: ClearRequest> Default for RequestMachine<T> {
    fn default() -> Self {
        Self {
            next_token: 1,
            active: None,
        }
    }
}

impl<T: ClearRequest> RequestMachine<T> {
    #[must_use]
    pub fn state(&self) -> Option<RequestState> {
        self.active.as_ref().map(|active| active.state)
    }

    #[must_use]
    pub fn request_id(&self) -> Option<&str> {
        self.active
            .as_ref()
            .map(|active| active.request_id.as_str())
    }

    /// Claims the only active request slot.
    ///
    /// # Errors
    ///
    /// Returns a `BUSY` response for the incoming request when another request
    /// owns the slot. The incoming data is cleared before returning.
    pub fn accept(
        &mut self,
        request_id: String,
        mut data: T,
        now: Duration,
    ) -> Result<RequestToken, Box<Busy>> {
        if self.active.is_some() {
            data.clear();
            return Err(Box::new(Busy {
                response: error_response(
                    Some(request_id),
                    ErrorCode::Busy,
                    "Another signing request is already active.",
                ),
            }));
        }

        let token = RequestToken(self.next_token);
        self.next_token = self.next_token.wrapping_add(1).max(1);
        self.active = Some(ActiveRequest {
            token,
            request_id,
            data,
            state: RequestState::Reviewing,
            accepted_at: now,
        });
        Ok(token)
    }

    /// Advances through a legal non-terminal lifecycle transition.
    ///
    /// # Errors
    ///
    /// Returns an error for stale request tokens or skipped/backward states.
    pub fn transition(
        &mut self,
        token: RequestToken,
        next: RequestState,
    ) -> Result<(), TransitionError> {
        let active = self.active_mut(token)?;
        let legal = matches!(
            (active.state, next),
            (
                RequestState::Reviewing,
                RequestState::DiscoveringCertificates
            ) | (
                RequestState::DiscoveringCertificates,
                RequestState::Confirming
            ) | (RequestState::Confirming, RequestState::Signing)
                | (RequestState::Signing, RequestState::Responding)
        );
        if !legal {
            return Err(TransitionError::IllegalTransition);
        }
        active.state = next;
        Ok(())
    }

    /// Cancels before provider signing starts and clears the request.
    ///
    /// # Errors
    ///
    /// Returns an error for a stale token or once signing has started.
    pub fn cancel(
        &mut self,
        token: RequestToken,
        reason: CancellationReason,
    ) -> Result<TerminalResponse, TransitionError> {
        let state = self.active_ref(token)?.state;
        if matches!(state, RequestState::Signing | RequestState::Responding) {
            return Err(TransitionError::IllegalTransition);
        }
        let request_id = self.clear_active(token)?;
        Ok(TerminalResponse::Cancelled(SignCancelled::new(
            request_id,
            request_id_reason(reason),
        )))
    }

    /// Times out an unsigned request when the fifteen-minute deadline expires.
    /// Signing and response delivery are deliberately not interrupted.
    ///
    /// # Errors
    ///
    /// Returns an error when the token does not identify the active request.
    pub fn poll_timeout(
        &mut self,
        token: RequestToken,
        now: Duration,
    ) -> Result<Option<TerminalResponse>, TransitionError> {
        let active = self.active_ref(token)?;
        if matches!(
            active.state,
            RequestState::Signing | RequestState::Responding
        ) || now.saturating_sub(active.accepted_at) < REQUEST_TIMEOUT
        {
            return Ok(None);
        }
        let request_id = self.clear_active(token)?;
        Ok(Some(error_response(
            Some(request_id),
            ErrorCode::RequestTimeout,
            "The signing request timed out.",
        )))
    }

    /// Clears unsigned work after the browser connection disappears.
    ///
    /// The returned response may be shown locally, but must not be retained for
    /// later browser delivery.
    ///
    /// # Errors
    ///
    /// Returns an error when the token does not identify the active request.
    pub fn extension_disconnected(
        &mut self,
        token: RequestToken,
    ) -> Result<Option<TerminalResponse>, TransitionError> {
        let state = self.active_ref(token)?.state;
        if matches!(state, RequestState::Signing | RequestState::Responding) {
            return Ok(None);
        }
        let request_id = self.clear_active(token)?;
        Ok(Some(error_response(
            Some(request_id),
            ErrorCode::ExtensionDisconnected,
            "The browser extension disconnected.",
        )))
    }

    /// Completes response delivery and clears all request-owned data.
    ///
    /// # Errors
    ///
    /// Returns an error unless the matching request is responding.
    pub fn response_delivered(&mut self, token: RequestToken) -> Result<(), TransitionError> {
        if self.active_ref(token)?.state != RequestState::Responding {
            return Err(TransitionError::IllegalTransition);
        }
        self.clear_active(token)?;
        Ok(())
    }

    fn active_ref(&self, token: RequestToken) -> Result<&ActiveRequest<T>, TransitionError> {
        self.active
            .as_ref()
            .filter(|active| active.token == token)
            .ok_or(TransitionError::NoActiveRequest)
    }

    fn active_mut(
        &mut self,
        token: RequestToken,
    ) -> Result<&mut ActiveRequest<T>, TransitionError> {
        self.active
            .as_mut()
            .filter(|active| active.token == token)
            .ok_or(TransitionError::NoActiveRequest)
    }

    fn clear_active(&mut self, token: RequestToken) -> Result<String, TransitionError> {
        self.active_ref(token)?;
        let mut active = self
            .active
            .take()
            .expect("matching active request must exist");
        active.data.clear();
        Ok(active.request_id)
    }
}

impl<T: ClearRequest> Drop for RequestMachine<T> {
    fn drop(&mut self) {
        if let Some(active) = &mut self.active {
            active.data.clear();
        }
    }
}

const fn request_id_reason(reason: CancellationReason) -> CancellationReason {
    reason
}

fn error_response(request_id: Option<String>, code: ErrorCode, message: &str) -> TerminalResponse {
    TerminalResponse::Error(SignError::new(request_id, code, message.to_owned()))
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, rc::Rc};

    use super::*;

    const FIRST_ID: &str = "123e4567-e89b-42d3-a456-426614174000";
    const SECOND_ID: &str = "123e4567-e89b-42d3-a456-426614174001";

    struct Secret {
        cleared: Rc<Cell<bool>>,
    }

    impl ClearRequest for Secret {
        fn clear(&mut self) {
            self.cleared.set(true);
        }
    }

    fn secret() -> (Secret, Rc<Cell<bool>>) {
        let cleared = Rc::new(Cell::new(false));
        (
            Secret {
                cleared: Rc::clone(&cleared),
            },
            cleared,
        )
    }

    #[test]
    fn enforces_legal_states_and_exactly_once_completion() {
        let (data, cleared) = secret();
        let mut machine = RequestMachine::default();
        let token = machine
            .accept(FIRST_ID.to_owned(), data, Duration::ZERO)
            .expect("slot should be free");

        assert_eq!(machine.state(), Some(RequestState::Reviewing));
        assert_eq!(
            machine.transition(token, RequestState::Confirming),
            Err(TransitionError::IllegalTransition)
        );
        for state in [
            RequestState::DiscoveringCertificates,
            RequestState::Confirming,
            RequestState::Signing,
            RequestState::Responding,
        ] {
            machine
                .transition(token, state)
                .expect("transition is legal");
        }
        machine
            .response_delivered(token)
            .expect("response should complete once");

        assert!(cleared.get());
        assert_eq!(machine.state(), None);
        assert_eq!(
            machine.response_delivered(token),
            Err(TransitionError::NoActiveRequest)
        );
    }

    #[test]
    fn rejects_concurrent_work_without_disturbing_the_active_request() {
        let (first, first_cleared) = secret();
        let (second, second_cleared) = secret();
        let mut machine = RequestMachine::default();
        machine
            .accept(FIRST_ID.to_owned(), first, Duration::ZERO)
            .expect("first request should be accepted");

        let busy = machine
            .accept(SECOND_ID.to_owned(), second, Duration::ZERO)
            .expect_err("second request must be busy");

        assert_eq!(machine.request_id(), Some(FIRST_ID));
        assert!(!first_cleared.get());
        assert!(second_cleared.get());
        let TerminalResponse::Error(error) = busy.response else {
            panic!("busy outcome must be an error");
        };
        assert_eq!(error.request_id.as_deref(), Some(SECOND_ID));
        assert_eq!(error.error.code, ErrorCode::Busy);
    }

    #[test]
    fn user_and_window_cancellation_clear_data_and_are_terminal() {
        for reason in [
            CancellationReason::UserCancelled,
            CancellationReason::WindowClosed,
        ] {
            let (data, cleared) = secret();
            let mut machine = RequestMachine::default();
            let token = machine
                .accept(FIRST_ID.to_owned(), data, Duration::ZERO)
                .expect("request should be accepted");

            let response = machine
                .cancel(token, reason)
                .expect("request should cancel");

            assert!(cleared.get());
            let TerminalResponse::Cancelled(cancelled) = response else {
                panic!("expected cancellation");
            };
            assert_eq!(cancelled.reason, reason);
            assert_eq!(
                machine.cancel(token, reason),
                Err(TransitionError::NoActiveRequest)
            );
        }
    }

    #[test]
    fn timeout_clears_waiting_work_but_stops_when_signing_begins() {
        let (data, cleared) = secret();
        let mut machine = RequestMachine::default();
        let token = machine
            .accept(FIRST_ID.to_owned(), data, Duration::from_secs(10))
            .expect("request should be accepted");
        assert_eq!(
            machine.poll_timeout(token, Duration::from_secs(909)),
            Ok(None)
        );
        let response = machine
            .poll_timeout(token, Duration::from_secs(910))
            .expect("token should match")
            .expect("deadline should expire");
        assert!(cleared.get());
        let TerminalResponse::Error(error) = response else {
            panic!("timeout must be an error");
        };
        assert_eq!(error.error.code, ErrorCode::RequestTimeout);

        let (data, signing_cleared) = secret();
        let token = machine
            .accept(FIRST_ID.to_owned(), data, Duration::ZERO)
            .expect("new request should be accepted");
        for state in [
            RequestState::DiscoveringCertificates,
            RequestState::Confirming,
            RequestState::Signing,
        ] {
            machine
                .transition(token, state)
                .expect("transition is legal");
        }
        assert_eq!(machine.poll_timeout(token, REQUEST_TIMEOUT * 2), Ok(None));
        assert!(!signing_cleared.get());
    }

    #[test]
    fn disconnect_clears_unsigned_work_without_retaining_a_result() {
        let (data, cleared) = secret();
        let mut machine = RequestMachine::default();
        let token = machine
            .accept(FIRST_ID.to_owned(), data, Duration::ZERO)
            .expect("request should be accepted");

        let response = machine
            .extension_disconnected(token)
            .expect("token should match")
            .expect("unsigned work should terminate");

        assert!(cleared.get());
        assert_eq!(machine.request_id(), None);
        let TerminalResponse::Error(error) = response else {
            panic!("disconnect must be an error");
        };
        assert_eq!(error.error.code, ErrorCode::ExtensionDisconnected);
        assert_eq!(
            machine.extension_disconnected(token),
            Err(TransitionError::NoActiveRequest)
        );
    }

    #[test]
    fn dropping_the_machine_clears_an_active_request() {
        let (data, cleared) = secret();
        let mut machine = RequestMachine::default();
        machine
            .accept(FIRST_ID.to_owned(), data, Duration::ZERO)
            .expect("request should be accepted");
        drop(machine);
        assert!(cleared.get());
    }
}
