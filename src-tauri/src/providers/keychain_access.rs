//! Platform policy for provider-owned credential stores.
//!
//! macOS external Keychain reads are disabled. Windows Credential Manager and Linux Secret
//! Service reads remain available to preserve their existing provider flows.

#[cfg(any(not(target_os = "macos"), test))]
use std::collections::HashSet;

#[cfg(not(target_os = "macos"))]
use std::sync::{OnceLock, RwLock};

#[cfg(any(not(target_os = "macos"), test))]
use super::provider_family;

#[cfg(not(target_os = "macos"))]
static GRANTED: OnceLock<RwLock<HashSet<String>>> = OnceLock::new();

#[cfg(not(target_os = "macos"))]
fn granted() -> &'static RwLock<HashSet<String>> {
    GRANTED.get_or_init(|| RwLock::new(HashSet::new()))
}

#[cfg(target_os = "macos")]
pub fn is_granted(_provider_id: &str) -> bool {
    false
}

#[cfg(not(target_os = "macos"))]
pub fn is_granted(provider_id: &str) -> bool {
    granted()
        .read()
        .map(|ids| access_grant_matches(&ids, provider_id))
        .unwrap_or(false)
}

#[cfg(not(target_os = "macos"))]
pub fn sync(provider_ids: impl IntoIterator<Item = String>) {
    let next = provider_ids
        .into_iter()
        .map(|provider_id| provider_family(&provider_id).to_owned())
        .collect::<HashSet<_>>();
    match granted().write() {
        Ok(mut ids) => *ids = next,
        Err(_) => crate::app_warn!(
            "providers",
            "external credential grants could not be updated"
        ),
    }
}

#[cfg(any(not(target_os = "macos"), test))]
fn access_grant_matches(grants: &HashSet<String>, provider_id: &str) -> bool {
    grants.contains(provider_family(provider_id))
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::access_grant_matches;

    #[test]
    fn external_credential_access_is_default_denied_and_family_scoped() {
        assert!(!access_grant_matches(&HashSet::new(), "claude"));

        let granted = HashSet::from(["claude".to_owned()]);
        assert!(access_grant_matches(&granted, "claude"));
        assert!(access_grant_matches(&granted, "claude@1234abcd"));
        assert!(!access_grant_matches(&granted, "cursor"));
    }
}
