import { invoke } from "@tauri-apps/api/core";

export interface FileEntry {
  name: string;
  path: string;
  is_dir: boolean;
}

export function openWorkspace(path: string): Promise<string> {
  return invoke("open_workspace", { path });
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
