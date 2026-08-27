#!/usr/bin/env node

import { spawn } from "node:child_process";
import readline from "node:readline";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const server = spawn(process.execPath, ["scripts/live-fixture-server.mjs"], {
  cwd: repositoryRoot,
  stdio: ["pipe", "pipe", "pipe"],
});
const pending = new Map();
const events = [];
const requests = [];
let nextId = 1;
let closed = false;
const input = readline.createInterface({ input: server.stdout });
input.on("line", (line) => {
  const message = JSON.parse(line);
  if (typeof message.id === "number" && !message.method) {
    const waiter = pending.get(message.id);
    if (!waiter) return;
    pending.delete(message.id);
    if (message.error) waiter.reject(new Error(JSON.stringify(message.error)));
    else waiter.resolve(message.result ?? null);
    return;
  }
  if (message.method && message.id !== undefined) {
    requests.push(message);
    if (message.method === "item/commandExecution/requestApproval") {
      server.stdin.write(`${JSON.stringify({ id: message.id, result: { decision: "accept" } })}\n`);
    }
    return;
  }
  if (message.method) events.push(message);
});

function send(method, params = {}) {
  const id = nextId++;
  return new Promise((resolve, reject) => {
    pending.set(id, { resolve, reject });
    server.stdin.write(`${JSON.stringify({ id, method, params })}\n`);
  });
}

function waitFor(predicate, label) {
  const start = Date.now();
  return new Promise((resolve, reject) => {
    const check = () => {
      if (predicate()) return resolve();
      if (Date.now() - start > 3000) return reject(new Error(`timeout waiting for ${label}`));
      setTimeout(check, 10);
    };
    check();
  });
}

function respondToServerRequest(message, result) {
  server.stdin.write(JSON.stringify({ id: message.id, result }) + "\n");
}

try {
  await send("initialize");
  server.stdin.write(`${JSON.stringify({ method: "initialized", params: {} })}\n`);
  const listed = await send("thread/list", { limit: 100 });
  if (!Array.isArray(listed.data)) throw new Error("thread/list did not return data");
  const catalog = await Promise.all([
    send("model/list", { limit: 100 }),
    send("permissionProfile/list", { cwd: process.cwd() }),
    send("collaborationMode/list"),
    send("app/list", { limit: 100 }),
    send("app/installed"),
    send("plugin/list"),
    send("skills/list", { cwds: [process.cwd()] }),
    send("mcpServerStatus/list"),
    send("account/read"),
    send("config/read", { includeLayers: false }),
  ]);
  if (!Array.isArray(catalog[0].data) || !Array.isArray(catalog[1].data)) {
    throw new Error("catalog methods did not return data");
  }
  const started = await send("thread/start", {});
  const threadId = started.thread.id;
  await send("thread/name/set", { threadId, name: "Fixture parity task" });
  await send("thread/read", { threadId, includeTurns: true });
  await send("thread/fork", { threadId });
  await send("thread/resume", { threadId });
  await send("turn/start", { threadId, input: [{ type: "text", text: "acceptance" }] });
  await waitFor(() => requests.some((request) => request.method === "item/commandExecution/requestApproval"), "approval request");
  await waitFor(() => events.some((event) => event.method === "turn/completed"), "completed turn");
  await send("turn/steer", {
    threadId,
    expectedTurnId: "fixture-turn-1",
    input: [{ type: "text", text: "steer" }],
  });
  await send("turn/start", { threadId, input: [{ type: "text", text: "interrupt" }] });
  await waitFor(() => requests.filter((request) => request.method === "item/commandExecution/requestApproval").length >= 2, "second approval request");
  await send("turn/interrupt", { threadId, turnId: "fixture-turn-2" });
  await waitFor(() => events.some((event) => event.method === "turn/completed" && event.params.turn.status === "interrupted"), "interrupted turn");
  await send("thread/archive", { threadId });
  const archived = await send("thread/list", { limit: 100, archived: true });
  if (!archived.data.some((thread) => thread.id === threadId)) throw new Error("archived thread missing");
  await send("thread/unarchive", { threadId });
  await send("thread/realtime/start", { threadId, outputModality: "text" });
  await send("thread/realtime/appendText", { threadId, text: "voice fixture" });
  await send("thread/realtime/stop", { threadId });
  await send("review/start", { threadId, target: { type: "uncommittedChanges" }, delivery: "inline" });
  await waitFor(() => events.some((event) => event.method === "item/completed" && event.params.item.type === "fileChange"), "review file change");
  await send("thread/compact/start", { threadId });
  await send("thread/shellCommand", { threadId, command: "true" });
  await send("thread/unsubscribe", { threadId });
  await send("thread/delete", { threadId });
  await waitFor(() => events.some((event) => event.method === "thread/deleted"), "deleted thread");
  const contractStart = requests.length;
  await send("fixture/emitServerRequests", { threadId });
  await waitFor(() => requests.length >= contractStart + 7, "server request contract suite");
  for (const request of requests.slice(contractStart)) {
    switch (request.method) {
      case "item/fileChange/requestApproval":
        respondToServerRequest(request, { decision: "acceptForSession" });
        break;
      case "item/permissions/requestApproval":
        respondToServerRequest(request, {
          permissions: {
            fileSystem: {
              entries: [{
                access: "write",
                path: { type: "path", path: process.cwd() },
              }],
            },
          },
          scope: "session",
        });
        break;
      case "item/tool/requestUserInput":
        respondToServerRequest(request, {
          answers: { "fixture-question": { answers: ["Yes"] } },
        });
        break;
      case "mcpServer/elicitation/request":
        respondToServerRequest(request, { action: "decline" });
        break;
      case "item/tool/call":
        respondToServerRequest(request, { success: false, contentItems: [] });
        break;
      case "execCommandApproval":
        respondToServerRequest(request, { decision: "approved" });
        break;
      case "applyPatchApproval":
        respondToServerRequest(request, {
          decision: { denied: { rejection: "fixture refusal" } },
        });
        break;
      default:
        throw new Error("unexpected contract request " + request.method);
    }
  }
  await waitFor(
    () => events.filter((event) => event.method === "fixture/serverRequestValidated").length >= 7,
    "validated server request responses",
  );
  const contractResult = await send("fixture/assertServerRequests");
  if (contractResult.count !== 7 || contractResult.valid !== true) {
    throw new Error("server request contracts were not valid: " + JSON.stringify(contractResult));
  }
  const normalApprovalCount = requests.filter(
    (request) => request.method === "item/commandExecution/requestApproval",
  ).length;
  if (normalApprovalCount !== 2) {
    throw new Error(`expected 2 turn approval requests, found ${normalApprovalCount}`);
  }
  if (requests.length !== 9) throw new Error(`expected 9 total server requests, found ${requests.length}`);
  if (!events.some((event) => event.method === "thread/archived")) throw new Error("archive event missing");
  if (!events.some((event) => event.method === "thread/unarchived")) throw new Error("unarchive event missing");
  console.log(`PARITY_100_FIXTURE_OK events=${events.length} requests=${requests.length}`);
} catch (error) {
  console.error(`PARITY_100_FIXTURE_FAIL ${error.message}`);
  process.exitCode = 1;
} finally {
  if (!closed) {
    closed = true;
    server.kill("SIGTERM");
  }
}
