import assert from "node:assert/strict";
import test from "node:test";
import {
  acceptSavedReplacement,
  acknowledgeSavedRevision,
  createBuffer,
  dirtyBufferPaths,
  isCurrentWorkspaceGeneration,
  savedSnapshotsAreCurrent,
  saveDirtySnapshotsForDecision,
  shouldAcceptWorkspaceResponse,
  updateBuffer,
} from "../lib/workspace-buffers.ts";

test("a delayed save cannot mark a newer edit clean", () => {
  let buffers = [createBuffer("main.ts", "one")];
  buffers = updateBuffer(buffers, "main.ts", "two");
  const revisionBeingSaved = buffers[0].revision;
  buffers = updateBuffer(buffers, "main.ts", "three");
  buffers = acknowledgeSavedRevision(buffers, "main.ts", revisionBeingSaved);

  assert.equal(buffers[0].content, "three");
  assert.equal(buffers[0].isDirty, true);
  assert.deepEqual(dirtyBufferPaths(buffers), ["main.ts"]);
});

test("Save approval rejects a buffer changed during persistence", () => {
  let buffers = [createBuffer("main.ts", "one")];
  buffers = updateBuffer(buffers, "main.ts", "two");
  const snapshot = { path: buffers[0].path, revision: buffers[0].revision };
  buffers = updateBuffer(buffers, "main.ts", "three");
  buffers = acknowledgeSavedRevision(buffers, snapshot.path, snapshot.revision);

  assert.equal(savedSnapshotsAreCurrent(buffers, [snapshot]), false);
});

test("a deferred filesystem save cannot approve a newer edit", async () => {
  let buffers = updateBuffer([createBuffer("main.ts", "one")], "main.ts", "two");
  let release;
  const gate = new Promise((resolve) => {
    release = resolve;
  });
  const saving = saveDirtySnapshotsForDecision(
    ["main.ts"],
    () => buffers,
    async (snapshot) => {
      await gate;
      buffers = acknowledgeSavedRevision(buffers, snapshot.path, snapshot.revision);
    }
  );

  buffers = updateBuffer(buffers, "main.ts", "three");
  release();

  assert.equal(await saving, false);
  assert.equal(buffers[0].content, "three");
  assert.equal(buffers[0].isDirty, true);
});

test("a delayed operation result is rejected after a generation switch", async () => {
  let currentGeneration = 1;
  let release;
  const delayed = new Promise((resolve) => {
    release = () => resolve("workspace A result");
  });
  const operation = delayed.then((value) => ({
    accepted: isCurrentWorkspaceGeneration(1, currentGeneration),
    value,
  }));

  currentGeneration = 2;
  release();

  assert.deepEqual(await operation, {
    accepted: false,
    value: "workspace A result",
  });
});

test("only the latest workspace request may publish its response", () => {
  assert.equal(shouldAcceptWorkspaceResponse(1, 2, 3, 2), false);
  assert.equal(shouldAcceptWorkspaceResponse(2, 2, 1, 2), false);
  assert.equal(shouldAcceptWorkspaceResponse(2, 2, 3, 2), true);
});

test("the exact saved revision becomes clean", () => {
  let buffers = [createBuffer("main.ts", "one")];
  buffers = updateBuffer(buffers, "main.ts", "two");
  buffers = acknowledgeSavedRevision(buffers, "main.ts", buffers[0].revision);

  assert.equal(buffers[0].content, "two");
  assert.equal(buffers[0].isDirty, false);
});

test("a reviewed replacement cannot overwrite a changed buffer", () => {
  let buffers = [createBuffer("main.ts", "base")];
  buffers = updateBuffer(buffers, "main.ts", "newer user edit");
  buffers = acceptSavedReplacement(buffers, "main.ts", "base", "proposal");

  assert.equal(buffers[0].content, "newer user edit");
  assert.equal(buffers[0].isDirty, true);
});

test("a matching reviewed replacement becomes the clean saved revision", () => {
  const buffers = acceptSavedReplacement(
    [createBuffer("main.ts", "base")],
    "main.ts",
    "base",
    "proposal"
  );

  assert.equal(buffers[0].content, "proposal");
  assert.equal(buffers[0].isDirty, false);
  assert.equal(buffers[0].revision, 1);
});
