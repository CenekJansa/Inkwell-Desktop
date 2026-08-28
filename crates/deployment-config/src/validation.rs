pub fn validate_native_host_name(value: &str) -> Result<(), &'static str> {
    if value.is_empty() {
        return Err("must not be empty");
    }
    if value.starts_with('.') || value.ends_with('.') || value.contains("..") {
        return Err("must not start or end with a dot or contain consecutive dots");
    }
    if !value.bytes().all(|byte| {
        byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'.')
    }) {
        return Err("must contain only lowercase ASCII letters, digits, underscores, and dots");
    }
    Ok(())
}

pub fn validate_extension_id(value: &str) -> Result<(), &'static str> {
    if value.len() != 32 {
        return Err("must contain exactly 32 characters");
    }
    if !value.bytes().all(|byte| matches!(byte, b'a'..=b'p')) {
        return Err("must contain only lowercase letters from 'a' through 'p'");
    }
    Ok(())
}
