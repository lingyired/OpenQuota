//! Tracks which providers the user has explicitly allowed to read the system credential
//! store entries owned by another application.
//!
//! macOS guards those reads with a Keychain authorization prompt, so Quota01 never touches
//! them until the user turns the provider on by hand. Every automatic path — startup
//! probing and fallback enablement — leaves the grant untouched, which keeps a freshly
//! installed build from prompting on behalf of an application the user never asked about.
//!
//! Credentials Quota01 owns (`com.lingyi.quota01.credentials`), its own config files and
//! inherited environment variables are not gated: they are read through
//! `credential_store::read_owned_password` / `api_key` and never prompt.

use std::{
    collections::HashSet,
    sync::{OnceLock, RwLock},
};

use super::provider_family;

static GRANTED: OnceLock<RwLock<HashSet<String>>> = OnceLock::new();

fn granted() -> &'static RwLock<HashSet<String>> {
    GRANTED.get_or_init(|| RwLock::new(HashSet::new()))
}

/// Whether `provider_id` may read the system credential store entries that belong to
/// another application. Defaults to `false`, and stays `false` if the registry is poisoned:
/// an unreadable policy must never widen access.
pub fn is_granted(provider_id: &str) -> bool {
    granted()
        .read()
        .map(|ids| ids.contains(provider_family(provider_id)))
        .unwrap_or(false)
}

/// Replaces the granted set with the providers the user enabled by hand. Account runtimes
/// (`claude@<hash>`) share their family grant, so a single toggle covers every account.
pub fn sync(provider_ids: impl IntoIterator<Item = String>) {
    let next = provider_ids
        .into_iter()
        .map(|provider_id| provider_family(&provider_id).to_owned())
        .collect::<HashSet<_>>();
    match granted().write() {
        Ok(mut ids) => *ids = next,
        Err(_) => crate::app_warn!("providers", "keychain access grants could not be updated"),
    }
}

/// Serializes tests that depend on the process-wide grant state.
#[cfg(test)]
pub(crate) static GRANT_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests {
    use super::{is_granted, sync, GRANT_TEST_LOCK};

    #[test]
    fn grants_default_to_denied_and_follow_the_manual_enablement() {
        let _guard = GRANT_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());

        sync(Vec::new());
        assert!(!is_granted("claude"));
        assert!(!is_granted("antigravity"));

        sync(vec!["claude".to_owned(), "cursor".to_owned()]);
        assert!(is_granted("claude"));
        assert!(is_granted("cursor"));
        assert!(!is_granted("codex"));

        sync(vec!["cursor".to_owned()]);
        assert!(!is_granted("claude"));
        assert!(is_granted("cursor"));
    }

    #[test]
    fn account_runtimes_share_their_family_grant() {
        let _guard = GRANT_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());

        sync(vec!["claude@1234abcd".to_owned()]);
        assert!(is_granted("claude"));
        assert!(is_granted("claude@1234abcd"));
        assert!(!is_granted("codex"));

        sync(vec!["claude".to_owned()]);
        assert!(is_granted("claude@1234abcd"));
    }
}
