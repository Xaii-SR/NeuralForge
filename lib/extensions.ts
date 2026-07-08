import { invoke } from "@tauri-apps/api/core";

export interface ExtensionManifest {
  name: string;
  version: string;
  author: string;
  description: string;
  entry_point: string;
  runtime: string;
  permissions: string[];
}

export interface InstalledExtension {
  manifest: ExtensionManifest;
  dir: string;
  enabled: boolean;
}

export interface ExtensionResult {
  success: boolean;
  output: unknown;
  error: string | null;
}

export function listExtensions(): Promise<InstalledExtension[]> {
  return invoke("list_extensions");
}

export function setExtensionEnabled(name: string, enabled: boolean): Promise<void> {
  return invoke("set_extension_enabled", { name, enabled });
}

export function uninstallExtension(name: string): Promise<void> {
  return invoke("uninstall_extension", { name });
}

export function runExtension(name: string, request: unknown): Promise<ExtensionResult> {
  return invoke("run_extension", { name, request });
}
