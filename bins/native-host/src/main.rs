//! Chrome native messaging host entry point.
//!
//! Standard output is reserved exclusively for framed protocol messages. The
//! implementation must use the safe logging crate for diagnostics.

use std::process::ExitCode;

use inkwell_native_host::{DisplayRequest, UiAdapter, UiUnavailable, serve_one};

struct DisconnectedUi;

impl UiAdapter for DisconnectedUi {
    fn display_and_cancel(
        &mut self,
        _request: DisplayRequest<'_>,
    ) -> Result<inkwell_protocol::CancellationReason, UiUnavailable> {
        Err(UiUnavailable)
    }
}

fn main() -> ExitCode {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    match serve_one(stdin.lock(), stdout.lock(), &mut DisconnectedUi) {
        Ok(_) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}
