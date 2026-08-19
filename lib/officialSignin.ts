import { invoke } from "@tauri-apps/api/core";

export interface OfficialSignInClient {
  id: "chatgpt_codex" | "claude_code" | "github_cli";
  title: string;
  detail: string;
  available: boolean;
}

export function listOfficialSignInClients() {
  return invoke<OfficialSignInClient[]>("list_official_signin_clients");
}

export function startOfficialSignIn(clientId: OfficialSignInClient["id"]) {
  return invoke<void>("start_official_signin", { clientId });
}
