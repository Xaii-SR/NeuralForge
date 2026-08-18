import { invoke } from "@tauri-apps/api/core";

export interface FileEntry {
  name: string;
  path: string;
  is_dir: boolean;
}

export interface WorkspaceInfo {
  root: string;
  generation: number;
}

export interface WorkspaceProject {
  root: string;
  name: string;
  last_session_id: string | null;
  last_opened_at: number;
  available: boolean;
}

export function openWorkspace(path: string): Promise<WorkspaceInfo> {
  return invoke("open_workspace", { path });
}

/** Last successfully opened workspace path, if it still exists on disk (v1.4.0 restoration). */
export function getLastWorkspace(): Promise<string | null> {
  return invoke("get_last_workspace");
}

export function listWorkspaceProjects(): Promise<WorkspaceProject[]> {
  return invoke("list_workspace_projects");
}

export function renameWorkspaceProject(path: string, name: string): Promise<void> {
  return invoke("rename_workspace_project", { path, name });
}

export function setWorkspaceActiveSession(path: string, sessionId: string | null): Promise<void> {
  return invoke("set_workspace_active_session", { path, sessionId });
}

export function readDir(path: string): Promise<FileEntry[]> {
  return invoke("read_dir", { path });
}

export function readFile(path: string): Promise<string> {
  return invoke("read_file", { path });
}

export function writeFile(path: string, contents: string): Promise<void> {
  return invoke("write_file", { path, contents });
}

export function writeFileIfUnchanged(path: string, expectedContents: string, contents: string): Promise<void> {
  return invoke("write_file_if_unchanged", { path, expectedContents, contents });
}

export function createFile(path: string): Promise<void> {
  return invoke("create_file", { path });
}

export function createDir(path: string): Promise<void> {
  return invoke("create_dir", { path });
}

export function deletePath(path: string): Promise<void> {
  return invoke("delete_path", { path });
}

export function renamePath(from: string, to: string): Promise<void> {
  return invoke("rename_path", { from, to });
}
