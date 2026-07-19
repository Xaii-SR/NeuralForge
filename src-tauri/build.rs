use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
  // Passive build-identity metadata only (app version + git commit + build
  // timestamp) - see NF_GIT_COMMIT/NF_BUILD_TIME below, read via env! in
  // core::build_info. No update-checking, no telemetry, no network calls.
  let commit = Command::new("git")
    .args(["rev-parse", "--short", "HEAD"])
    .output()
    .ok()
    .filter(|o| o.status.success())
    .and_then(|o| String::from_utf8(o.stdout).ok())
    .map(|s| s.trim().to_string())
    .filter(|s| !s.is_empty())
    .unwrap_or_else(|| "unknown".to_string());
  println!("cargo:rustc-env=NF_GIT_COMMIT={commit}");

  let build_time = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map(|d| d.as_secs().to_string())
    .unwrap_or_else(|_| "unknown".to_string());
  println!("cargo:rustc-env=NF_BUILD_TIME={build_time}");

  // Re-run this script (and thus refresh the commit hash) whenever HEAD
  // moves, not just when build.rs itself changes.
  println!("cargo:rerun-if-changed=../.git/HEAD");

  tauri_build::build()
}
