import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const read = (path) => readFile(new URL(`../${path}`, import.meta.url), "utf8");

test("dirty buffers share one save discard cancel decision path", async () => {
  const [workspace, dialog, page] = await Promise.all([
    read("hooks/useWorkspace.ts"),
    read("components/UnsavedChangesDialog.tsx"),
    read("app/page.tsx"),
  ]);

  assert.match(workspace, /requestUnsavedDecision/);
  assert.match(workspace, /"close_file"/);
  assert.match(workspace, /"switch_workspace"/);
  assert.match(workspace, /"close_app"/);
  assert.match(workspace, /onCloseRequested/);
  assert.match(dialog, /onResolve\("save"\)/);
  assert.match(dialog, /onResolve\("discard"\)/);
  assert.match(dialog, /onResolve\("cancel"\)/);
  assert.match(page, /UnsavedChangesDialog/);
});

test("failed or conflicting diff application keeps the proposal", async () => {
  const editor = await read("components/EditorPane.tsx");
  const accept = editor.slice(
    editor.indexOf("const handleDiffAccept"),
    editor.indexOf("const handleDiffReject")
  );

  assert.match(accept, /writeFileIfUnchanged/);
  assert.match(accept, /removeActiveDiff\(\)/);
  assert.match(accept, /catch \(error\)/);
  assert.doesNotMatch(
    accept.slice(accept.indexOf("catch (error)")),
    /removeActiveDiff\(\)/
  );
});

test("workspace changes invalidate active chat and session initialization", async () => {
  const [chat, sessions] = await Promise.all([
    read("components/ChatPane.tsx"),
    read("components/SessionTabs.tsx"),
  ]);

  assert.match(chat, /workspaceGenerationRef/);
  assert.match(chat, /activeRequestId\.current = null/);
  assert.match(chat, /getSessionMessages\(generation,/);
  assert.match(sessions, /workspaceGenerationRef/);
  assert.match(sessions, /listSessions\(generation\)/);
  assert.match(sessions, /createSession\(generation,/);
});

test("Agent and Council operations are generation-bound and clear stale UI", async () => {
  const [agentPanel, councilPanel, agentApi, councilApi] = await Promise.all([
    read("components/AgentPanel.tsx"),
    read("components/CouncilPanel.tsx"),
    read("lib/agent.ts"),
    read("lib/council.ts"),
  ]);

  assert.match(agentPanel, /\[workspaceOpen, workspaceGeneration\]/);
  assert.match(agentPanel, /setTasks\(\[\]\)/);
  assert.match(agentApi, /workspaceGeneration/);
  assert.match(councilPanel, /\[workspaceGeneration\]/);
  assert.match(councilApi, /workspaceGeneration/);
});

test("diff navigation resets loading and uses stable proposal identity", async () => {
  const [editor, composer] = await Promise.all([
    read("components/EditorPane.tsx"),
    read("hooks/useComposer.ts"),
  ]);

  assert.match(editor, /currentDiff\?\.id/);
  assert.match(editor, /setDiffLoading\(false\)/);
  assert.match(composer, /id: string/);
});

test("unmount resolves any pending unsaved decision", async () => {
  const workspace = await read("hooks/useWorkspace.ts");
  assert.match(workspace, /unsavedResolver\.current = null/);
  assert.match(workspace, /resolve\?\.\(false\)/);
});
