//! Passive build-identity metadata: app version, git commit, build
//! timestamp. Exists so a user/developer can look at Settings and tell
//! which commit an installed build actually came from, instead of
//! comparing file timestamps by hand (the diagnostic this session had to
//! do manually to explain why AI Council appeared "missing" from an
//! installed executable that simply predated the commits that added it).
//!
//! Deliberately passive: no update-checking, no telemetry, no network
//! calls. `NF_GIT_COMMIT`/`NF_BUILD_TIME` are set at compile time by
//! `build.rs` via `cargo:rustc-env`, with a graceful "unknown" fallback if
//! `git` isn't available in the build environment - `env!()` always
//! resolves to a real string either way, never a build failure.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct BuildInfo {
    pub version: String,
    pub commit: String,
    pub build_time: String,
}

#[tauri::command]
pub fn get_build_info() -> BuildInfo {
    BuildInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        commit: env!("NF_GIT_COMMIT").to_string(),
        build_time: env!("NF_BUILD_TIME").to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cross-checks the compile-time-embedded commit against a fresh,
    /// independent `git rev-parse --short HEAD` run at test time - proves
    /// the displayed value genuinely reflects the real build-time HEAD,
    /// not just that the env var is present. Skips (rather than fails) if
    /// git truly isn't available, matching build.rs's own graceful
    /// degradation - this asserts correctness when git IS available, which
    /// is every environment this crate is actually built in today.
    #[test]
    fn commit_matches_a_fresh_independent_git_rev_parse() {
        let output = std::process::Command::new("git")
            .args(["rev-parse", "--short", "HEAD"])
            .output();

        let Ok(output) = output else {
            eprintln!("skipping: git not available to independently verify against");
            return;
        };
        if !output.status.success() {
            eprintln!("skipping: git rev-parse failed");
            return;
        }

        let expected = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let info = get_build_info();
        assert_eq!(info.commit, expected, "get_build_info()'s embedded commit must match the real HEAD at build time");
        assert_ne!(info.commit, "unknown", "git was available at test time, so build.rs should not have fallen back");
    }

    #[test]
    fn version_and_build_time_are_present_and_non_empty() {
        let info = get_build_info();
        assert!(!info.version.is_empty());
        assert!(!info.build_time.is_empty());
    }
}
