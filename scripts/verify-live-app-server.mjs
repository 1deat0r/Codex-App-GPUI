#!/usr/bin/env node

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawn } from "node:child_process";
import readline from "node:readline";

const codexHome = fs.mkdtempSync(path.join(os.tmpdir(), "codex-app-gpui-codex-home-"));
const stateHome = fs.mkdtempSync(path.join(os.tmpdir(), "codex-app-gpui-state-"));
const child = spawn("codex", ["app-server", "--stdio"], {
  env: { ...process.env, CODEX_HOME: codexHome, CODEX_APP_GPUI_HOME: stateHome },
  stdio: ["pipe", "pipe", "pipe"],
});
const pending = new Map();
let nextId = 1;
let stderr = "";
child.stderr.on("data", (chunk) => { stderr += chunk.toString(); });
const input = readline.createInterface({ input: child.stdout });
input.on("line", (line) => {
  if (!line.trim().startsWith("{")) return;
  const message = JSON.parse(line);
  if (typeof message.id === "number" && !message.method) {
    const waiter = pending.get(message.id);
    if (!waiter) return;
    pending.delete(message.id);
    if (message.error) waiter.reject(new Error(JSON.stringify(message.error)));
    else waiter.resolve(message.result ?? null);
  }
});

function send(method, params = {}) {
  const id = nextId++;
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      pending.delete(id);
      reject(new Error(`timeout waiting for ${method}`));
    }, 8000);
    pending.set(id, {
      resolve: (value) => { clearTimeout(timer); resolve(value); },
      reject: (error) => { clearTimeout(timer); reject(error); },
    });
    child.stdin.write(`${JSON.stringify({ id, method, params })}\n`);
  });
}

try {
  const initialized = await send("initialize", {
    clientInfo: { name: "codex_app_gpui_parity", title: "Codex App GPUI parity verifier", version: "0.1.0" },
    capabilities: { experimentalApi: true },
  });
  child.stdin.write(`${JSON.stringify({ method: "initialized", params: {} })}\n`);
  const listed = await send("thread/list", { limit: 10, archived: false });
  if (!initialized || !listed || !Array.isArray(listed.data)) throw new Error("unexpected app-server response shape");
  const [models, permissions, modes, apps, installed, plugins, skills, mcp, account, config, hooks] = await Promise.all([
    send("model/list", { limit: 10 }),
    send("permissionProfile/list", { cwd: stateHome, limit: 10 }),
    send("collaborationMode/list"),
    send("app/list", { limit: 10 }),
    send("app/installed"),
    send("plugin/list"),
    send("skills/list", { cwds: [stateHome] }),
    send("mcpServerStatus/list"),
    send("account/read"),
    send("config/read", { cwd: stateHome, includeLayers: false }),
    send("hooks/list", { cwds: [stateHome] }),
  ]);
  if (!Array.isArray(models?.data) || !Array.isArray(permissions?.data) || !Array.isArray(modes?.data)) {
    throw new Error("catalog responses were not arrays");
  }
  if (!Array.isArray(apps?.data) || !Array.isArray(installed?.apps) || !plugins || !skills?.data || !mcp?.data || !account || !config?.config || !Array.isArray(hooks?.data)) {
    throw new Error("secondary app-server catalog response shape was incomplete");
  }
  if (apps.data.length > 0) {
    const details = await send("app/read", { appIds: [apps.data[0].id], includeTools: true });
    if (!Array.isArray(details?.apps)) throw new Error("app/read did not return apps");
  }
  const started = await send("thread/start", { cwd: stateHome });
  const threadId = started?.thread?.id;
  if (typeof threadId !== "string" || !threadId) throw new Error("thread/start did not return a thread id");
  child.stdin.end();
  await new Promise((resolve) => child.once("close", resolve));
  console.log(`PARITY_100_LIVE_OK thread=${threadId} isolated=${codexHome}`);
} catch (error) {
  console.error(`PARITY_100_LIVE_FAIL ${error.message}${stderr ? ` stderr=${stderr.trim()}` : ""}`);
  child.kill("SIGTERM");
  process.exit(1);
}
