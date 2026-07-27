//! OS-keychain-backed storage for provider API keys.
//!
//! Remediates the plaintext-SQLite exposure documented on
//! `provider_registry::ProviderConfig`: `api_key` used to be persisted as
//! plain JSON in the `settings` table, readable by anything that can read
//! the workspace's `index.db`. This module stores the actual secret in the
//! OS credential store (Windows Credential Manager / macOS Keychain /
//! Linux libsecret, via the `keyring` crate) instead, keyed by provider id.
//! `provider_registry` persists only the provider id in its JSON blob and
//! reads the real key from here at load time.

const SERVICE: &str = "neuralforge-provider-api-key";

pub trait CredentialBackend {
    fn store(&self, provider_id: &str, api_key: &str) -> Result<(), String>;
    fn load(&self, provider_id: &str) -> Result<Option<String>, String>;
    fn delete(&self, provider_id: &str) -> Result<(), String>;
}

pub struct KeyringCredentialBackend;

impl CredentialBackend for KeyringCredentialBackend {
    fn store(&self, provider_id: &str, api_key: &str) -> Result<(), String> {
        if api_key.is_empty() {
            return Ok(());
        }
        let entry = keyring::Entry::new(SERVICE, provider_id).map_err(|error| error.to_string())?;
        entry.set_password(api_key).map_err(|error| error.to_string())
    }

    fn load(&self, provider_id: &str) -> Result<Option<String>, String> {
        let entry = keyring::Entry::new(SERVICE, provider_id).map_err(|error| error.to_string())?;
        match entry.get_password() {
            Ok(api_key) => Ok(Some(api_key)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(error.to_string()),
        }
    }

    fn delete(&self, provider_id: &str) -> Result<(), String> {
        let entry = keyring::Entry::new(SERVICE, provider_id).map_err(|error| error.to_string())?;
        match entry.delete_password() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(error.to_string()),
        }
    }
}

/// Stores `api_key` in the OS keychain under `provider_id`. A no-op that
/// succeeds if `api_key` is empty (nothing to protect) rather than writing
/// an empty credential entry.
pub fn store_api_key(provider_id: &str, api_key: &str) -> Result<(), String> {
    KeyringCredentialBackend.store(provider_id, api_key)
}

pub fn load_api_key_result(provider_id: &str) -> Result<Option<String>, String> {
    KeyringCredentialBackend.load(provider_id)
}

/// Missing credentials are a normal state for local providers. Backend
/// failures are intentionally collapsed only for internal routing callers;
/// migrations use `load_api_key_result` and never scrub on an error.
pub fn load_api_key(provider_id: &str) -> String {
    load_api_key_result(provider_id)
        .ok()
        .flatten()
        .unwrap_or_default()
}

/// Removes `provider_id`'s stored API key, if any. A no-op, not an error,
/// if no entry exists - matches `delete_provider_config`'s idempotent
/// intent.
pub fn delete_api_key(provider_id: &str) {
    let _ = delete_api_key_result(provider_id);
}

pub fn delete_api_key_result(provider_id: &str) -> Result<(), String> {
    KeyringCredentialBackend.delete(provider_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Exercises the real OS keychain - ignored by default since CI/sandboxed
    /// environments may not have one available. Run explicitly via
    /// `cargo test credential_store -- --ignored` on a machine with a real
    /// keychain (Windows Credential Manager, macOS Keychain, or libsecret).
    #[test]
    #[ignore = "requires a real OS keychain; run explicitly with --ignored"]
    fn store_then_load_round_trips_the_real_key() {
        let id = "neuralforge-test-provider-round-trip";
        store_api_key(id, "sk-test-12345").unwrap();
        assert_eq!(load_api_key(id), "sk-test-12345");
        delete_api_key(id);
        assert_eq!(load_api_key(id), "");
    }

    #[test]
    fn storing_an_empty_key_is_a_no_op() {
        // Does not touch the keychain at all, so this is safe to run
        // unconditionally in CI.
        assert!(store_api_key("neuralforge-test-provider-empty", "").is_ok());
    }

    #[test]
    #[ignore = "requires a real OS keychain; run explicitly with --ignored"]
    fn loading_a_missing_key_returns_empty_string_not_an_error() {
        assert_eq!(load_api_key("neuralforge-test-provider-never-registered"), "");
    }

    #[test]
    #[ignore = "requires a real OS keychain; run explicitly with --ignored"]
    fn deleting_a_missing_key_does_not_panic() {
        delete_api_key("neuralforge-test-provider-never-registered-2");
    }
}
