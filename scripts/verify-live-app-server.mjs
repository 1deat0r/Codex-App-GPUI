#!/usr/bin/env node

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawn, spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const executable = path.join(repositoryRoot, "target", "debug", "codex-app-gpui");
const codexHome = fs.mkdtempSync(path.join(os.tmpdir(), "codex-app-gpui-codex-home-"));
const stateHome = fs.mkdtempSync(path.join(os.tmpdir(), "codex-app-gpui-state-"));
let child;
let stdout = "";
let stderr = "";

function ensureExecutable() {
  if (fs.existsSync(executable)) return;
  const result = spawnSync(process.execPath, ["scripts/run-cargo.mjs", "build", "--locked"], {
    cwd: repositoryRoot,
    stdio: "inherit",
  });
  if (result.status !== 0 || !fs.existsSync(executable)) {
    throw new Error("could not build the native live-smoke executable");
  }
}

function waitForClose(processHandle) {
  return new Promise((resolve, reject) => {
    processHandle.once("error", reject);
    processHandle.once("close", (code, signal) => resolve({ code, signal }));
  });
}

try {
  ensureExecutable();
  child = spawn(executable, ["--live-smoke"], {
    cwd: repositoryRoot,
    env: {
      ...process.env,
      CODEX_HOME: codexHome,
      CODEX_APP_GPUI_HOME: stateHome,
      CODEX_APP_GPUI_CREATE_LIVE_THREAD: "1",
      CODEX_APP_SERVER_COMMAND: "codex app-server --stdio",
    },
    stdio: ["ignore", "pipe", "pipe"],
  });
  child.stdout.on("data", (chunk) => { stdout += chunk.toString(); });
  child.stderr.on("data", (chunk) => { stderr += chunk.toString(); });
  const closed = await waitForClose(child);
  const marker = stdout.match(/PARITY_100_LIVE_CLIENT_OK[^\r\n]*/)?.[0];
  if (closed.code !== 0 || !marker) {
    const details = [stdout.trim(), stderr.trim()].filter(Boolean).join(" ");
    throw new Error(`native live client failed with status ${closed.code}: ${details}`.trim());
  }
  if (!/thread=\S+/.test(marker)) throw new Error("native live client did not report a thread id");
  console.log(`${marker} isolated=true`);
  console.log("PARITY_100_LIVE_OK");
} catch (error) {
  if (child && child.exitCode === null) child.kill("SIGTERM");
  console.error(`PARITY_100_LIVE_FAIL ${error.message}`);
  process.exitCode = 1;
} finally {
  fs.rmSync(codexHome, { recursive: true, force: true });
  fs.rmSync(stateHome, { recursive: true, force: true });
}
