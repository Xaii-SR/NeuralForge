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

  // Re-run this script (and thus refresh the commit hash) whenever a new
  // commit lands on the current branch. `.git/HEAD` only changes on branch
  // checkout (it just holds `ref: refs/heads/<branch>`), not on ordinary
  // commits - `.git/logs/HEAD` (the reflog) gets a new line on every
  // commit/checkout/merge, which is what actually needs watching here.
  println!("cargo:rerun-if-changed=../.git/logs/HEAD");

  tauri_build::build()
}
