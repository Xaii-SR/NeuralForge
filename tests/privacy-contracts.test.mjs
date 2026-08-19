import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const read = (path) => readFile(new URL(`../${path}`, import.meta.url), "utf8");

test("legacy browser credentials migrate before plaintext is removed", async () => {
  const store = await read("lib/store.ts");
  const migration = store.slice(
    store.indexOf("export async function migrateConfig"),
    store.indexOf("export async function getAppConfig"),
  );

  assert.match(migration, /await migrateLegacyApiKey/);
  assert.match(migration, /credentialMigrated = true/);
  assert.match(migration, /localStorage\.removeItem\("nf_api_key_backup"\)/);
  assert.doesNotMatch(migration, /localStorage\.setItem\("nf_api_key_backup"/);
  assert.ok(
    migration.indexOf("await migrateLegacyApiKey")
      < migration.indexOf('localStorage.removeItem("nf_api_key_backup")'),
  );
});

test("provider list DTO and provider actions never consume a stored key", async () => {
  const [providers, manager] = await Promise.all([
    read("lib/providers.ts"),
    read("components/ProviderManager.tsx"),
  ]);
  const publicProvider = providers.slice(
    providers.indexOf("export interface ProviderConfig"),
    providers.indexOf("export interface ProviderCapabilities"),
  );

  assert.match(publicProvider, /has_api_key: boolean/);
  assert.doesNotMatch(publicProvider, /^\s*api_key:/m);
  assert.match(manager, /testProviderConnection\(config\.id\)/);
  assert.match(manager, /listProviderModels\(config\.id\)/);
  assert.doesNotMatch(manager, /config\.api_key/);
});

test("release IPC registry excludes dormant privileged commands", async () => {
  const registry = await read("src-tauri/src/lib.rs");
  const handler = registry.slice(
    registry.indexOf(".invoke_handler"),
    registry.indexOf("])", registry.indexOf(".invoke_handler")),
  );

  for (const command of [
    "test_openai_compatible_connection",
    "list_openai_compatible_models",
    "fetch_and_cache_doc",
    "get_git_status",
    "get_git_diff",
    "propose_self_improvement",
    "apply_self_improvement",
    "build_local_index",
    "generate_local_embeddings",
    "query_codebase_semantic",
    "database::search_workspace",
  ]) {
    assert.doesNotMatch(handler, new RegExp(`\\b${command}\\b`));
  }
});

test("retained workspace search derives its root in Rust", async () => {
  const [search, inline, composer] = await Promise.all([
    read("src-tauri/src/workspace/search.rs"),
    read("components/editor/InlinePromptWidget.tsx"),
    read("components/composer/ComposerWindow.tsx"),
  ]);

  assert.match(search, /State<'_, crate::core::state::AppState>/);
  assert.match(search, /workspace_root/);
  assert.doesNotMatch(inline, /workspaceRoot:/);
  assert.doesNotMatch(composer, /workspaceRoot:/);
});

test("cloud workspace context sharing is explicit and defaults off", async () => {
  const [store, settings, chat, team, inline] = await Promise.all([
    read("lib/store.ts"),
    read("components/SettingsPanel.tsx"),
    read("components/ChatPane.tsx"),
    read("components/AITeamPanel.tsx"),
    read("src-tauri/src/ai/inline.rs"),
  ]);

  assert.match(store, /shareWorkspaceContextWithCloud: false/);
  assert.match(settings, /Share workspace context with cloud providers/);
  assert.match(settings, /Off by default/);
  assert.match(chat, /appConfig\.shareWorkspaceContextWithCloud/);
  assert.match(team, /config\.shareWorkspaceContextWithCloud/);
  assert.match(inline, /AdapterKind::Ollama/);
  assert.doesNotMatch(inline, /resolve_provider_for_model/);
});

test("official sign-in launchers are explicit and allowlisted", async () => {
  const [signInPanel, launcher] = await Promise.all([
    read("components/OfficialSignInPanel.tsx"),
    read("src-tauri/src/official_signin.rs"),
  ]);

  assert.match(signInPanel, /Official account sign-in/);
  assert.match(signInPanel, /Add Provider remains the explicit API-credential path/);
  assert.match(signInPanel, /GitHub Copilot additionally requires a registered GitHub App/);
  assert.match(launcher, /Unknown official sign-in client/);
  assert.match(launcher, /No renderer-provided text is passed through to the OS command line/);
  assert.doesNotMatch(launcher, /Command::new\(&client_id\)/);
});
