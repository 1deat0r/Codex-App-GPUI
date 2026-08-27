#!/usr/bin/env node

import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawn, spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const executable = path.join(repositoryRoot, "target", "debug", "codex-app-gpui");
const inputProtocol = path.join(repositoryRoot, "scripts", "native-input.xml");
const inputSource = path.join(repositoryRoot, "scripts", "native-input.c");
const requestedOutputIndex = process.argv.indexOf("--output");
const requestedOutput = requestedOutputIndex >= 0 ? process.argv[requestedOutputIndex + 1] : null;
const stateHome = fs.mkdtempSync(path.join(os.tmpdir(), "codex-app-gpui-native-state-"));
const inputHome = fs.mkdtempSync(path.join(os.tmpdir(), "codex-app-gpui-native-input-"));
const statePath = path.join(stateHome, "state.json");
const outputPath = requestedOutput
  ? path.resolve(repositoryRoot, requestedOutput)
  : path.join(os.tmpdir(), `codex-app-gpui-native-window-${process.pid}.png`);
let app;
let appStdout = "";
let appStderr = "";
let client;

function fail(message) {
  throw new Error(message);
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: repositoryRoot,
    encoding: "utf8",
    ...options,
  });
  if (result.error) fail(`${command} could not start: ${result.error.message}`);
  if (result.status !== 0) {
    const details = `${result.stdout ?? ""}${result.stderr ?? ""}`.trim();
    fail(`${command} ${args.join(" ")} failed${details ? `: ${details}` : ""}`);
  }
  return result.stdout ?? "";
}

function executableOnPath(command) {
  return (process.env.PATH ?? "").split(path.delimiter).some((directory) => {
    const candidate = path.join(directory, command);
    try {
      const stats = fs.statSync(candidate);
      return stats.isFile() && (stats.mode & 0o111) !== 0;
    } catch {
      return false;
    }
  });
}

function ensureExecutable() {
  if (fs.existsSync(executable)) return;
  run(process.execPath, ["scripts/run-cargo.mjs", "build", "--locked"], { stdio: "inherit" });
  if (!fs.existsSync(executable)) fail("native GPUI executable was not produced");
}

function sleep(milliseconds) {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

async function waitFor(predicate, label, timeout = 20000) {
  const started = Date.now();
  while (Date.now() - started < timeout) {
    if (predicate()) return;
    await sleep(100);
  }
  fail(`timeout waiting for ${label}`);
}

function hyprClients() {
  const output = run("hyprctl", ["clients", "-j"]);
  try {
    return JSON.parse(output);
  } catch (error) {
    fail(`hyprctl clients returned invalid JSON: ${error.message}`);
  }
}

function activeWindow() {
  const output = run("hyprctl", ["activewindow", "-j"]);
  try {
    return JSON.parse(output);
  } catch (error) {
    fail(`hyprctl activewindow returned invalid JSON: ${error.message}`);
  }
}

function geometry(client) {
  return {
    x: client.at[0],
    y: client.at[1],
    w: client.size[0],
    h: client.size[1],
  };
}

function cursorPosition() {
  const output = run("hyprctl", ["cursorpos"]);
  const match = output.match(/(-?\d+)\s*,\s*(-?\d+)/);
  if (!match) fail(`could not parse cursor position: ${output.trim()}`);
  return { x: Number(match[1]), y: Number(match[2]) };
}

function compilePointerHelper() {
  const header = path.join(inputHome, "codex-app-gpui-native-input-client-protocol.h");
  const protocol = path.join(inputHome, "codex-app-gpui-native-input-protocol.c");
  const binary = path.join(inputHome, "codex-app-gpui-native-input");
  run("wayland-scanner", ["client-header", inputProtocol, header]);
  run("wayland-scanner", ["private-code", inputProtocol, protocol]);
  run("cc", [
    "-std=c11",
    "-O2",
    "-Wall",
    "-Wextra",
    "-I",
    inputHome,
    inputSource,
    protocol,
    "-o",
    binary,
    "-lwayland-client",
    "-lm",
  ]);
  return binary;
}

function clickAt(pointerHelper, x, y) {
  const cursor = cursorPosition();
  run(pointerHelper, [String(Math.round(x - cursor.x)), String(Math.round(y - cursor.y))]);
}

function readState() {
  if (!fs.existsSync(statePath)) return null;
  try {
    return JSON.parse(fs.readFileSync(statePath, "utf8"));
  } catch {
    return null;
  }
}

function stateText() {
  const state = readState();
  return state ? JSON.stringify(state) : "";
}

function descendantPids(rootPid) {
  const rows = run("ps", ["-eo", "pid=,ppid="]).trim().split("\n").filter(Boolean);
  const children = new Map();
  for (const row of rows) {
    const [pid, parent] = row.trim().split(/\s+/).map(Number);
    if (!Number.isNaN(pid) && !Number.isNaN(parent)) {
      const siblings = children.get(parent) ?? [];
      siblings.push(pid);
      children.set(parent, siblings);
    }
  }
  const found = new Set();
  const visit = (pid) => {
    for (const childPid of children.get(pid) ?? []) {
      if (found.has(childPid)) continue;
      found.add(childPid);
      visit(childPid);
    }
  };
  visit(rootPid);
  return found;
}

function processExists(pid) {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

async function stopApp() {
  if (!app) return;
  const descendants = processExists(app.pid) ? descendantPids(app.pid) : new Set();
  const isStopped = () => app.exitCode !== null || app.signalCode !== null;
  if (!isStopped()) {
    app.kill("SIGINT");
    const stopped = await Promise.race([
      new Promise((resolve) => app.once("close", () => resolve(true))),
      sleep(5000).then(() => false),
    ]);
    if (!stopped && !isStopped()) app.kill("SIGTERM");
  }
  await waitFor(isStopped, "native process shutdown", 5000);
  await waitFor(
    () => [...descendants].every((pid) => !processExists(pid)),
    "app-server child shutdown",
    5000,
  );
}

function capture(window) {
  fs.mkdirSync(path.dirname(outputPath), { recursive: true });
  run("grim", ["-g", `${window.x},${window.y} ${window.w}x${window.h}`, outputPath]);
  if (!fs.existsSync(outputPath)) fail("grim did not create the native window artifact");
}

try {
  if (!process.env.WAYLAND_DISPLAY) fail("native window gate requires WAYLAND_DISPLAY");
  for (const command of ["hyprctl", "wtype", "grim", "wayland-scanner", "cc"]) {
    if (!executableOnPath(command)) fail(`native window gate requires ${command}`);
  }
  ensureExecutable();
  const pointerHelper = compilePointerHelper();
  app = spawn(executable, [], {
    cwd: repositoryRoot,
    env: {
      ...process.env,
      CODEX_HOME: stateHome,
      CODEX_APP_GPUI_HOME: stateHome,
      CODEX_APP_SERVER_COMMAND: "node scripts/live-fixture-server.mjs",
      CODEX_APP_GPUI_CREATE_LIVE_THREAD: "1",
    },
    stdio: ["ignore", "pipe", "pipe"],
  });
  app.stdout.on("data", (chunk) => { appStdout += chunk.toString(); });
  app.stderr.on("data", (chunk) => { appStderr += chunk.toString(); });
  await waitFor(() => {
    client = hyprClients().find((candidate) => candidate.pid === app.pid && candidate.mapped);
    return Boolean(client);
  }, "native GPUI window");
  run("hyprctl", ["dispatch", `hl.dsp.focus({ window = "address:${client.address}" })`]);
  await waitFor(() => activeWindow().pid === app.pid, "native window focus", 5000);
  run("hyprctl", ["dispatch", "hl.dsp.window.fullscreen({})"]);
  await waitFor(() => {
    client = hyprClients().find((candidate) => candidate.pid === app.pid && candidate.mapped);
    return Boolean(client && client.size[0] > 2000);
  }, "native fullscreen window", 5000);
  run("hyprctl", ["dispatch", `hl.dsp.focus({ window = "address:${client.address}" })`]);
  await waitFor(() => activeWindow().pid === app.pid, "native fullscreen focus", 5000);
  await waitFor(() => stateText().includes("fixture-thread-1"), "fixture thread import");

  const clientGeometry = geometry(client);
  const inputX = clientGeometry.x + clientGeometry.w * 0.65;
  const inputY = clientGeometry.y + clientGeometry.h * 0.89;
  clickAt(pointerHelper, inputX, inputY);
  await sleep(150);
  run("wtype", ["-d", "20", "--", "native fixture interaction"]);
  run("wtype", ["-k", "Return"]);
  await waitFor(
    () => stateText().includes("item/commandExecution/requestApproval"),
    "native approval request",
  );
  run("wtype", ["-M", "ctrl", "-M", "shift", "-k", "a", "-m", "shift", "-m", "ctrl"]);
  await waitFor(
    () => stateText().includes("Fixture received: native fixture interaction")
      && stateText().includes("native fixture interaction")
      && stateText().includes("Approved by user"),
    "native fixture turn and approval",
    20000,
  );

  const finalClient = hyprClients().find((candidate) => candidate.pid === app.pid && candidate.mapped) ?? client;
  const finalGeometry = geometry(finalClient);
  capture(finalGeometry);
  const screenshotHash = crypto.createHash("sha256").update(fs.readFileSync(outputPath)).digest("hex");
  const stateHash = crypto.createHash("sha256").update(fs.readFileSync(statePath)).digest("hex");
  console.log(
    `PARITY_100_NATIVE_WINDOW_OK pid=${app.pid} geometry=${finalGeometry.w}x${finalGeometry.h}`
      + ` state_sha256=${stateHash} screenshot=${outputPath} screenshot_sha256=${screenshotHash}`,
  );
} catch (error) {
  const details = [appStdout.trim(), appStderr.trim()].filter(Boolean).join(" ");
  const stateDetails = stateText().slice(-1800);
  let failureScreenshot = "";
  if (client) {
    const failureGeometry = geometry(client);
    failureScreenshot = path.join(os.tmpdir(), `codex-app-gpui-native-failure-${process.pid}.png`);
    spawnSync("grim", ["-g", `${failureGeometry.x},${failureGeometry.y} ${failureGeometry.w}x${failureGeometry.h}`, failureScreenshot]);
  }
  console.error(
    `PARITY_100_NATIVE_WINDOW_FAIL ${error.message}`
      + `${details ? ` output=${details}` : ""}`
      + `${stateDetails ? ` state_tail=${stateDetails}` : ""}`
      + `${failureScreenshot ? ` failure_screenshot=${failureScreenshot}` : ""}`,
  );
  process.exitCode = 1;
} finally {
  await stopApp();
  fs.rmSync(inputHome, { recursive: true, force: true });
  fs.rmSync(stateHome, { recursive: true, force: true });
}
