pub use inkwell_deployment_config::{DEPLOYMENT_PROFILE, EXTENSION_ID, NATIVE_HOST_NAME};

mod ipc_server;
mod walking_skeleton;

use tauri::Manager as _;
use walking_skeleton::{
    WalkingSkeletonState, cancel_signing_request, continue_signing_request, pdf_review_state,
    render_pdf_review_page,
};

/// Starts the Inkwell desktop application.
///
/// # Panics
///
/// Panics when Tauri cannot initialize or run the application.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let state = WalkingSkeletonState::default();
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(
            |app, _arguments, _directory| {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.unminimize();
                    let _ = window.set_focus();
                }
            },
        ))
        .manage(state.clone())
        .setup(move |app| {
            ipc_server::start(app.handle().clone(), state.clone())?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
                let state = window.app_handle().state::<WalkingSkeletonState>();
                if let Some(request_id) = state.close_active_window() {
                    ipc_server::emit_request_invalidated(window.app_handle(), &request_id);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            pdf_review_state,
            render_pdf_review_page,
            continue_signing_request,
            cancel_signing_request
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Inkwell Desktop");
}
