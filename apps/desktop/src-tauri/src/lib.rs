pub use inkwell_deployment_config::{DEPLOYMENT_PROFILE, EXTENSION_ID, NATIVE_HOST_NAME};

/// Starts the Inkwell desktop application.
///
/// # Panics
///
/// Panics when Tauri cannot initialize or run the application.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("failed to run Inkwell Desktop");
}
