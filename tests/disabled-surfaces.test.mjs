import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const read = (path) => readFile(new URL(`../${path}`, import.meta.url), "utf8");

test("unsafe execution commands are absent from the Tauri registry", async () => {
  const source = await read("src-tauri/src/lib.rs");
  const registry = source.match(/generate_handler!\[([\s\S]*?)\]\)/)?.[1] ?? "";
  const disabled = [
    "start_agent_task",
    "approve_agent_task",
    "reject_agent_task",
    "create_and_plan_code_task",
    "run_extension",
    "send_composer_message",
    "execute_composer_command_stream",
    "execute_sandboxed_command",
    "allowlist_add",
    "denylist_add",
    "orchestrator_create_task",
    "orchestrator_approve_task",
  ];

  for (const command of disabled) {
    assert.doesNotMatch(registry, new RegExp(`\\b${command}\\b`));
  }

  for (const preserved of ["spawn_shell", "create_and_plan_task", "run_council_pass"]) {
    assert.match(registry, new RegExp(`\\b${preserved}\\b`));
  }
});

test("unsafe and deceptive controls are absent from production panels", async () => {
  const [page, agent, terminal, extensions] = await Promise.all([
    read("app/page.tsx"),
    read("components/AgentPanel.tsx"),
    read("components/Terminal.tsx"),
    read("components/ExtensionsPanel.tsx"),
  ]);

  assert.doesNotMatch(page, /AgentWorkbench|workbench/);
  assert.doesNotMatch(agent, /Run V2 Agent|createAndPlanCodeTask|start_agent_task/);
  assert.doesNotMatch(terminal, /FixWithAiButton|useComposer/);
  assert.doesNotMatch(extensions, /runExtension|Test this extension directly/);
});

test("legacy run-code tasks are rejected before approval side effects", async () => {
  const source = await read("src-tauri/src/agent/mod.rs");

  assert.match(source, /ensure_task_type_is_approvable\(&task\)\?/);
  assert.match(source, /task\.task_type != task_type::EDIT_FILE/);
  assert.match(source, /CommandRejected/);
});
