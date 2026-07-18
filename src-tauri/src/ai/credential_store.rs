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

/// Stores `api_key` in the OS keychain under `provider_id`. A no-op that
/// succeeds if `api_key` is empty (nothing to protect) rather than writing
/// an empty credential entry.
pub fn store_api_key(provider_id: &str, api_key: &str) -> Result<(), String> {
    if api_key.is_empty() {
        return Ok(());
    }
    let entry = keyring::Entry::new(SERVICE, provider_id).map_err(|e| e.to_string())?;
    entry.set_password(api_key).map_err(|e| e.to_string())
}

/// Reads `provider_id`'s API key back from the OS keychain. Returns an
/// empty string (not an error) if no entry exists - a provider with no key
/// configured yet (e.g. local Ollama) is a normal, expected state, not a
/// failure.
pub fn load_api_key(provider_id: &str) -> String {
    let Ok(entry) = keyring::Entry::new(SERVICE, provider_id) else {
        return String::new();
    };
    entry.get_password().unwrap_or_default()
}

/// Removes `provider_id`'s stored API key, if any. A no-op, not an error,
/// if no entry exists - matches `delete_provider_config`'s idempotent
/// intent.
pub fn delete_api_key(provider_id: &str) {
    if let Ok(entry) = keyring::Entry::new(SERVICE, provider_id) {
        let _ = entry.delete_password();
    }
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
