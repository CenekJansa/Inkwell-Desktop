//! Validated deployment identity embedded into Inkwell binaries at build time.

mod defaults;
#[cfg(test)]
mod validation;

/// Build profile selected through `INKWELL_DEPLOYMENT_PROFILE`.
pub const DEPLOYMENT_PROFILE: &str = env!("INKWELL_DEPLOYMENT_PROFILE");

/// Chrome native messaging host name selected for this build.
pub const NATIVE_HOST_NAME: &str = env!("INKWELL_NATIVE_HOST_NAME");

/// Chrome extension identifier permitted by this build.
pub const EXTENSION_ID: &str = env!("INKWELL_EXTENSION_ID");

/// Checked-in native host name reserved for local development.
pub const DEVELOPMENT_NATIVE_HOST_NAME: &str = defaults::NATIVE_HOST_NAME;

/// Checked-in extension identifier reserved for the unpacked test extension.
pub const DEVELOPMENT_EXTENSION_ID: &str = defaults::EXTENSION_ID;

#[cfg(test)]
mod tests {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use sha2::{Digest as _, Sha256};

    use super::{
        DEPLOYMENT_PROFILE, DEVELOPMENT_EXTENSION_ID, DEVELOPMENT_NATIVE_HOST_NAME, EXTENSION_ID,
        NATIVE_HOST_NAME, validation,
    };

    #[test]
    fn development_configuration_is_valid() {
        assert!(validation::validate_native_host_name(DEVELOPMENT_NATIVE_HOST_NAME).is_ok());
        assert!(validation::validate_extension_id(DEVELOPMENT_EXTENSION_ID).is_ok());
    }

    #[test]
    fn compiled_configuration_is_valid() {
        assert!(matches!(DEPLOYMENT_PROFILE, "development" | "production"));
        assert!(validation::validate_native_host_name(NATIVE_HOST_NAME).is_ok());
        assert!(validation::validate_extension_id(EXTENSION_ID).is_ok());
    }

    #[test]
    fn development_extension_id_matches_public_key() {
        let encoded_key =
            include_str!("../../../extensions/test-extension/development-public-key.base64").trim();
        let public_key = STANDARD
            .decode(encoded_key)
            .expect("development extension public key must be Base64 DER");
        let digest = Sha256::digest(public_key);
        let extension_id: String = digest[..16]
            .iter()
            .flat_map(|byte| [byte >> 4, byte & 0x0f])
            .map(|nibble| char::from(b'a' + nibble))
            .collect();

        assert_eq!(extension_id, DEVELOPMENT_EXTENSION_ID);
    }

    #[test]
    fn rejects_invalid_deployment_values() {
        assert!(validation::validate_native_host_name("Com.Inkwell").is_err());
        assert!(validation::validate_native_host_name("com..inkwell").is_err());
        assert!(validation::validate_extension_id("not-an-extension-id").is_err());
    }
}
