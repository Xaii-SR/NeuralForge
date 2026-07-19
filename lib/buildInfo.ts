import { invoke } from "@tauri-apps/api/core";

export interface BuildInfo {
  version: string;
  commit: string;
  build_time: string;
}

// Passive build-identity metadata (app version, git commit, build
// timestamp) - display only, no update-check, no network call. See
// core::build_info on the backend.
export function getBuildInfo(): Promise<BuildInfo> {
  return invoke("get_build_info");
}
