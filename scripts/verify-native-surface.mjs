#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const read = (relativePath) => fs.readFileSync(path.join(repositoryRoot, relativePath), "utf8");
const failures = [];

const ui = read("src/ui.rs");
const model = read("src/model.rs");
const state = read("src/state.rs");
const protocol = read("src/protocol.rs");

const requireAll = (source, values, label) => {
  const missing = values.filter((value) => !source.includes(value));
  if (missing.length > 0) failures.push(`${label} missing: ${missing.join(", ")}`);
  return values.length - missing.length;
};

const referenceNavigation = [
  "Pull requests", "Sites", "Scheduled", "Plugins", "Change content layout",
  "Chat", "Task tabs", "Files", "Side chat", "Browser", "Review", "Detail", "Terminal",
  "Fullscreen", "Compact on the right", "Bottom panel", "Split view",
  "Fork chat from here", "Fork from this message in a new worktree",
  "Fork from this message in the current workspace", "Fork from this message in the same worktree",
  "Open subagents", "Review changes", "Pull request associated with this task",
  "Current task usage", "Background terminal", "Stop all background terminals",
];
const settingsLabels = [
  "General", "Import", "Profile", "Account", "Appearance", "Voice", "Agent",
  "Personalization", "Pets", "Notifications", "Usage", "Analytics", "Debug",
  "Keyboard shortcuts", "Teams", "Apps & Connectors", "Computer use", "Chronicle",
  "Appshots", "Codex Micro", "MCP", "Plugins", "Skills", "Browser use", "Hooks",
  "Connections", "Cloud", "Cloud environments", "Code review", "Worktrees", "Git",
  "Local environments", "Environments", "Data controls",
];
const settingsControls = [
  "UI font size", "Code font size", "Reduce motion", "Show context window usage in the composer",
  "Show the bottom panel control in the app header", "Show Full access in the composer",
  "Show educational tips", "Enable ambient suggestions", "Queue follow-ups while a task runs",
  "Projectless task folder", "Language for the app UI", "Choose when Enter sends a prompt or inserts a new line",
  "Disable Git-Based Review", "Always force push", "Create draft pull requests",
  "Pull request merge method", "Commit instructions", "Pull request instructions",
  "Watch and fix pull requests", "Auto-merge when ready", "Review delivery", "Worktree root",
  "Always fetch upstream before creating worktrees", "Automatically delete old worktrees",
  "Auto-delete limit", "New chat in this worktree", "Add server", "Extra skill folders",
  "Add skill folder", "Refresh servers", "Search catalog", "Marketplaces", "Voice input",
];
const stateHandlers = [
  "set_route", "refresh_pull_requests", "open_app", "create_automation", "run_automation",
  "toggle_automation", "delete_automation", "install_plugin", "uninstall_plugin",
  "refresh_marketplaces", "refresh_worktrees", "delete_worktree", "new_chat_in_worktree",
  "fork_current_in_new_worktree", "share_current", "review_current", "open_diff_path",
  "copy_diff_path", "begin_mcp_server_edit", "add_mcp_server_from_json", "pick_skill_root",
  "pick_projectless_task_folder", "start_account_login", "logout_account", "refresh_account",
  "stop_all_background_terminals",
];
const protocolMethods = [
  "thread/list", "thread/read", "thread/start", "thread/fork", "thread/resume",
  "thread/archive", "thread/unarchive", "thread/delete", "thread/name/set", "turn/start",
  "turn/steer", "turn/interrupt", "review/start", "thread/queue/add", "thread/queue/list",
  "thread/queue/update", "thread/queue/reorder", "thread/queue/delete", "thread/realtime/start",
  "thread/realtime/appendText", "thread/realtime/stop", "app/list", "app/read", "skills/list",
  "skills/extraRoots/set", "hooks/list", "config/value/write", "config/mcpServer/reload",
  "plugin/list", "plugin/install", "plugin/uninstall", "account/read", "account/login/start",
  "account/login/cancel", "account/logout", "thread/backgroundTerminals/clean",
];

const labels = requireAll(ui, referenceNavigation, "reference navigation")
  + requireAll(model, settingsLabels, "settings inventory")
  + requireAll(ui, settingsControls, "settings controls");
const handlers = requireAll(state, stateHandlers, "state handlers");
const methods = requireAll(protocol, protocolMethods, "protocol methods");

const enumBody = model.match(/pub enum SettingsPage \{([\s\S]*?)\n\}/)?.[1] ?? "";
const pageNames = [...enumBody.matchAll(/^\s{4}([A-Z][A-Za-z0-9]+),/gm)].map((match) => match[1]);
const allBody = model.match(/pub const ALL: &\[Self\] = &\[([\s\S]*?)\n\s*\];/)?.[1] ?? "";
const allPages = [...allBody.matchAll(/Self::([A-Z][A-Za-z0-9]+)/g)].map((match) => match[1]);
if (pageNames.length !== 34) failures.push(`expected 34 settings enum pages, found ${pageNames.length}`);
if (allPages.length !== 34) failures.push(`expected 34 settings ALL entries, found ${allPages.length}`);
if (JSON.stringify([...new Set(pageNames)].sort()) !== JSON.stringify([...new Set(allPages)].sort())) {
  failures.push("settings enum and ALL inventory differ");
}

const smoke = spawnSync(process.execPath, ["scripts/run-cargo.mjs", "run", "--locked", "--", "--smoke"], {
  cwd: repositoryRoot,
  encoding: "utf8",
});
const smokeOutput = `${smoke.stdout}${smoke.stderr}`;
if (smoke.status !== 0) {
  failures.push("compiled smoke inventory failed");
} else {
  for (const expected of ["destinations=5", "settings_pages=34", "official_client_requests=150", "PARITY_G6_SMOKE_OK"]) {
    if (!smokeOutput.includes(expected)) failures.push(`compiled smoke output missing ${expected}`);
  }
}

if (failures.length > 0) {
  console.error("PARITY_100_NATIVE_SURFACE_FAIL");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}
console.log(`PARITY_100_NATIVE_SURFACE_OK labels=${labels} handlers=${handlers} methods=${methods} settings=${allPages.length}`);
