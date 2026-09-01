//! Chrome native messaging host entry point.
//!
//! Standard output is reserved exclusively for framed protocol messages. The
//! implementation must use the safe logging crate for diagnostics.

use std::{io::Read as _, process::ExitCode, sync::mpsc, thread, time::Duration};

use inkwell_local_ipc::{DesktopCommand, HostCommand, IpcRequest, connect};
use inkwell_native_host::{UiAdapter, UiUnavailable, serve_one};
use inkwell_protocol::TerminalResponse;
use inkwell_request_validation::ValidatedRequest;

struct IpcUi;

impl UiAdapter for IpcUi {
    fn handle_request(
        &mut self,
        request: ValidatedRequest,
    ) -> Result<TerminalResponse, UiUnavailable> {
        let parts = request.into_parts();
        let request_id = parts.request_id.clone();
        let mut stream = connect_or_launch()?;
        stream
            .send_message(&HostCommand::Request(IpcRequest {
                request_id: parts.request_id,
                website_origin: parts.website_origin,
                document_name: parts.document_name,
                preview_pdf: parts.preview_pdf,
                byte_range_content: parts.byte_range_content,
            }))
            .map_err(|_| UiUnavailable)?;
        let (mut receiver, mut sender) = stream.split();
        let (desktop_tx, desktop_rx) = mpsc::channel();
        thread::spawn(move || {
            let result = receiver.receive_message::<DesktopCommand>();
            let _ = desktop_tx.send(result);
        });
        let (disconnect_tx, disconnect_rx) = mpsc::channel();
        thread::spawn(move || {
            let mut byte = [0_u8; 1];
            let disconnected = std::io::stdin()
                .read(&mut byte)
                .is_ok_and(|count| count == 0);
            if disconnected {
                let _ = disconnect_tx.send(());
            }
        });

        loop {
            match desktop_rx.recv_timeout(Duration::from_millis(50)) {
                Ok(Ok(Some(DesktopCommand::Terminal(response)))) => return Ok(response),
                Ok(Ok(None) | Err(_)) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(UiUnavailable);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
            if disconnect_rx.try_recv().is_ok() {
                let _ = sender.send_message(&HostCommand::Disconnect { request_id });
                return Err(UiUnavailable);
            }
        }
    }
}

fn connect_or_launch() -> Result<inkwell_local_ipc::IpcStream, UiUnavailable> {
    if let Ok(stream) = connect(inkwell_deployment_config::NATIVE_HOST_NAME) {
        return Ok(stream);
    }
    let executable = std::env::current_exe()
        .map_err(|_| UiUnavailable)?
        .with_file_name(desktop_executable_name());
    std::process::Command::new(executable)
        .spawn()
        .map_err(|_| UiUnavailable)?;
    for _ in 0..50 {
        if let Ok(stream) = connect(inkwell_deployment_config::NATIVE_HOST_NAME) {
            return Ok(stream);
        }
        thread::sleep(Duration::from_millis(100));
    }
    Err(UiUnavailable)
}

const fn desktop_executable_name() -> &'static str {
    if cfg!(windows) {
        "inkwell-desktop.exe"
    } else {
        "inkwell-desktop"
    }
}

fn main() -> ExitCode {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    match serve_one(stdin.lock(), stdout.lock(), &mut IpcUi) {
        Ok(_) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}
