pub use inkwell_deployment_config::{DEPLOYMENT_PROFILE, EXTENSION_ID, NATIVE_HOST_NAME};

mod walking_skeleton;

use walking_skeleton::{WalkingSkeletonState, cancel_signing_request, pending_signing_request};

/// Starts the Inkwell desktop application.
///
/// # Panics
///
/// Panics when Tauri cannot initialize or run the application.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(WalkingSkeletonState::default())
        .invoke_handler(tauri::generate_handler![
            pending_signing_request,
            cancel_signing_request
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Inkwell Desktop");
}
