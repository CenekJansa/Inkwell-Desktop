use std::{sync::mpsc, thread, time::Instant};

use inkwell_app_core::REQUEST_TIMEOUT;
use inkwell_local_ipc::{CurrentUserListener, DesktopCommand, HostCommand, IpcStream};
use tauri::{AppHandle, Emitter as _, Manager as _};

use crate::{NATIVE_HOST_NAME, walking_skeleton::WalkingSkeletonState};

pub fn start(app: AppHandle, state: WalkingSkeletonState) -> std::io::Result<()> {
    let listener = CurrentUserListener::bind(NATIVE_HOST_NAME)?;
    thread::spawn(move || {
        while let Ok(stream) = listener.accept() {
            let app = app.clone();
            let state = state.clone();
            thread::spawn(move || handle_connection(stream, &app, &state));
        }
    });
    Ok(())
}

fn handle_connection(mut stream: IpcStream, app: &AppHandle, state: &WalkingSkeletonState) {
    let Ok(Some(command)) = stream.receive_message::<HostCommand>() else {
        return;
    };
    let response = match command {
        HostCommand::Request(request) => {
            let request_id = request.request_id.clone();
            match state.accept(request) {
                Ok(terminal) => {
                    activate(app);
                    let (mut receiver, mut sender) = stream.split();
                    let (host_tx, host_rx) = mpsc::channel();
                    thread::spawn(move || {
                        let command = receiver.receive_message::<HostCommand>();
                        let _ = host_tx.send(command);
                    });
                    let deadline = Instant::now() + REQUEST_TIMEOUT;
                    loop {
                        match terminal.recv_timeout(std::time::Duration::from_millis(50)) {
                            Ok(response) => {
                                let _ = sender.send_message(&DesktopCommand::Terminal(response));
                                return;
                            }
                            Err(mpsc::RecvTimeoutError::Disconnected) => return,
                            Err(mpsc::RecvTimeoutError::Timeout) => {}
                        }
                        if let Ok(Ok(Some(HostCommand::Disconnect {
                            request_id: disconnected_id,
                        }))) = host_rx.try_recv()
                        {
                            if disconnected_id == request_id {
                                let _ = state.disconnect(&request_id);
                            }
                            return;
                        }
                        if Instant::now() >= deadline {
                            let Ok(response) = state.timeout(&request_id) else {
                                return;
                            };
                            let _ = sender.send_message(&DesktopCommand::Terminal(response));
                            return;
                        }
                    }
                }
                Err(busy) => *busy,
            }
        }
        HostCommand::Disconnect { request_id } => match state.disconnect(&request_id) {
            Ok(response) => response,
            Err(_) => return,
        },
    };
    let _ = stream.send_message(&DesktopCommand::Terminal(response));
}

fn activate(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
    let _ = app.emit("signing-request-available", ());
}
