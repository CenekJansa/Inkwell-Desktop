#[path = "src/defaults.rs"]
mod defaults;
#[path = "src/validation.rs"]
mod validation;

use std::env::{self, VarError};

const PROFILE_ENV: &str = "INKWELL_DEPLOYMENT_PROFILE";
const HOST_NAME_ENV: &str = "INKWELL_NATIVE_HOST_NAME";
const EXTENSION_ID_ENV: &str = "INKWELL_EXTENSION_ID";

fn main() {
    for name in [PROFILE_ENV, HOST_NAME_ENV, EXTENSION_ID_ENV] {
        println!("cargo:rerun-if-env-changed={name}");
    }

    let profile = optional_value(PROFILE_ENV).unwrap_or_else(|| "development".to_owned());
    let host_name = configured_value(HOST_NAME_ENV, defaults::NATIVE_HOST_NAME);
    let extension_id = configured_value(EXTENSION_ID_ENV, defaults::EXTENSION_ID);

    assert!(
        matches!(profile.as_str(), "development" | "production"),
        "{PROFILE_ENV} must be either 'development' or 'production'"
    );
    assert!(
        profile != "production"
            || (host_name != defaults::NATIVE_HOST_NAME && extension_id != defaults::EXTENSION_ID),
        "production builds must provide non-development {HOST_NAME_ENV} and {EXTENSION_ID_ENV} values"
    );

    validation::validate_native_host_name(&host_name)
        .unwrap_or_else(|message| panic!("invalid {HOST_NAME_ENV}: {message}"));
    validation::validate_extension_id(&extension_id)
        .unwrap_or_else(|message| panic!("invalid {EXTENSION_ID_ENV}: {message}"));

    println!("cargo:rustc-env={PROFILE_ENV}={profile}");
    println!("cargo:rustc-env={HOST_NAME_ENV}={host_name}");
    println!("cargo:rustc-env={EXTENSION_ID_ENV}={extension_id}");
}

fn configured_value(name: &str, development_default: &str) -> String {
    optional_value(name).unwrap_or_else(|| development_default.to_owned())
}

fn optional_value(name: &str) -> Option<String> {
    match env::var(name) {
        Ok(value) => Some(value),
        Err(VarError::NotPresent) => None,
        Err(VarError::NotUnicode(_)) => panic!("{name} must contain valid Unicode"),
    }
}
