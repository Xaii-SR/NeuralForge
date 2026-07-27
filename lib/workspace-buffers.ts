export interface WorkspaceBuffer {
  path: string;
  content: string;
  isDirty: boolean;
  revision: number;
}

export function createBuffer(path: string, content: string): WorkspaceBuffer {
  return { path, content, isDirty: false, revision: 0 };
}

export function updateBuffer(
  buffers: WorkspaceBuffer[],
  path: string,
  content: string
): WorkspaceBuffer[] {
  return buffers.map((buffer) =>
    buffer.path === path
      ? { ...buffer, content, isDirty: true, revision: buffer.revision + 1 }
      : buffer
  );
}

export function acknowledgeSavedRevision(
  buffers: WorkspaceBuffer[],
  path: string,
  savedRevision: number
): WorkspaceBuffer[] {
  return buffers.map((buffer) =>
    buffer.path === path && buffer.revision === savedRevision
      ? { ...buffer, isDirty: false }
      : buffer
  );
}

export function acceptSavedReplacement(
  buffers: WorkspaceBuffer[],
  path: string,
  expectedContent: string,
  content: string
): WorkspaceBuffer[] {
  return buffers.map((buffer) =>
    buffer.path === path && buffer.content === expectedContent
      ? { ...buffer, content, isDirty: false, revision: buffer.revision + 1 }
      : buffer
  );
}

export function dirtyBufferPaths(buffers: WorkspaceBuffer[]): string[] {
  return buffers.filter((buffer) => buffer.isDirty).map((buffer) => buffer.path);
}

export function savedSnapshotsAreCurrent(
  buffers: WorkspaceBuffer[],
  snapshots: Pick<WorkspaceBuffer, "path" | "revision">[]
): boolean {
  return snapshots.every((snapshot) => {
    const current = buffers.find((buffer) => buffer.path === snapshot.path);
    return !!current && !current.isDirty && current.revision === snapshot.revision;
  });
}

export function shouldAcceptWorkspaceResponse(
  responseRequest: number,
  latestRequest: number,
  responseGeneration: number,
  currentGeneration: number
): boolean {
  return (
    responseRequest === latestRequest &&
    responseGeneration >= currentGeneration
  );
}

export async function saveDirtySnapshotsForDecision(
  paths: string[],
  getBuffers: () => WorkspaceBuffer[],
  persist: (snapshot: WorkspaceBuffer) => Promise<void>
): Promise<boolean> {
  const snapshots: WorkspaceBuffer[] = [];
  for (const path of paths) {
    const file = getBuffers().find((candidate) => candidate.path === path);
    if (file?.isDirty) {
      snapshots.push(file);
      await persist(file);
    }
  }
  return savedSnapshotsAreCurrent(getBuffers(), snapshots);
}

export function isCurrentWorkspaceGeneration(
  expected: number,
  current: number
): boolean {
  return expected !== 0 && expected === current;
}
