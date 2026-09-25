//! Platform policy for provider-owned credential stores.
//!
//! macOS external Keychain reads are disabled. Windows Credential Manager and Linux Secret
//! Service reads remain available to preserve their existing provider flows.

#[cfg(target_os = "macos")]
pub fn is_granted(_provider_id: &str) -> bool {
    false
}

#[cfg(not(target_os = "macos"))]
pub fn is_granted(_provider_id: &str) -> bool {
    true
}
