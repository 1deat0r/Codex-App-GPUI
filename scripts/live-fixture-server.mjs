#!/usr/bin/env node

import readline from "node:readline";

const cwd = process.cwd();
const threads = new Map();
const approvals = new Map();
let nextThread = 1;
let nextTurn = 1;
let nextItem = 1;
let nextApproval = 900;

function send(message) {
  process.stdout.write(`${JSON.stringify(message)}\n`);
}

function response(id, result) {
  send({ id, result });
}

function notify(method, params) {
  send({ method, params });
}

function request(id, method, params) {
  send({ id, method, params });
}

function createThread(name = "Untitled task") {
  const id = `fixture-thread-${nextThread++}`;
  const thread = {
    id,
    name,
    cwd,
    status: "idle",
    model: "5.6 Luna Max",
    updatedAt: "Fixture",
    turns: [],
  };
  threads.set(id, thread);
  return thread;
}

function completeTurn(thread, turn, item, status = "completed") {
  turn.status = status;
  if (status === "completed") {
    thread.turns.push({
      id: turn.id,
      status,
      items: [item],
    });
  }
  thread.status = "idle";
  notify("turn/completed", {
    threadId: thread.id,
    turn: {
      id: turn.id,
      threadId: thread.id,
      status,
      items: status === "completed" ? [item] : [],
    },
  });
}

function startTurn(thread, text) {
  const turn = {
    id: `fixture-turn-${nextTurn++}`,
    threadId: thread.id,
    status: "inProgress",
  };
  const item = {
    id: `fixture-item-${nextItem++}`,
    type: "agentMessage",
    text: `Fixture received: ${text}`,
  };
  thread.status = "running";
  notify("turn/started", { threadId: thread.id, turn });
  notify("item/started", { threadId: thread.id, item: { ...item, text: "" } });
  notify("item/agentMessage/delta", {
    threadId: thread.id,
    itemId: item.id,
    delta: `Fixture received: ${text}`,
  });

  const requestId = nextApproval++;
  approvals.set(requestId, { thread, turn, item });
  request(requestId, "item/commandExecution/requestApproval", {
    threadId: thread.id,
    itemId: item.id,
    command: "fixture --safe-check",
    cwd,
    reason: "The fixture is checking the approval reducer without touching the filesystem.",
  });
}

function handle(message) {
  if (!message || typeof message !== "object") return;
  if (message.method) {
    const { id, method, params = {} } = message;
    switch (method) {
      case "initialize":
        response(id, {
          userAgent: "codex-app-gpui-fixture",
          capabilities: { experimentalApi: true },
        });
        break;
      case "thread/list":
        response(id, { data: [...threads.values()] });
        break;
      case "thread/start": {
        const thread = createThread();
        response(id, { thread });
        notify("thread/started", { thread });
        break;
      }
      case "thread/read": {
        const thread = threads.get(params.threadId) ?? createThread();
        response(id, { thread });
        break;
      }
      case "thread/fork": {
        const source = threads.get(params.threadId);
        const thread = createThread(source?.name ? `${source.name} (fork)` : "Forked task");
        response(id, { thread });
        notify("thread/started", { thread });
        break;
      }
      case "thread/resume":
        response(id, { thread: threads.get(params.threadId) ?? createThread() });
        break;
      case "thread/name/set": {
        const thread = threads.get(params.threadId);
        if (thread) {
          thread.name = params.name || thread.name;
          response(id, { thread });
          notify("thread/name/updated", {
            threadId: thread.id,
            name: thread.name,
          });
        } else {
          response(id, {});
        }
        break;
      }
      case "thread/archive": {
        response(id, {});
        notify("thread/archived", { threadId: params.threadId });
        break;
      }
      case "thread/unarchive": {
        response(id, {});
        notify("thread/unarchived", { threadId: params.threadId });
        break;
      }
      case "thread/delete":
        threads.delete(params.threadId);
        response(id, {});
        notify("thread/deleted", { threadId: params.threadId });
        break;
      case "thread/unsubscribe":
      case "thread/compact/start":
      case "thread/shellCommand":
        response(id, {});
        break;
      case "turn/start": {
        const thread = threads.get(params.threadId) ?? createThread();
        const text = params.input?.[0]?.text ?? "";
        response(id, {
          turn: {
            id: `fixture-turn-${nextTurn}`,
            threadId: thread.id,
            status: "inProgress",
          },
        });
        startTurn(thread, text);
        break;
      }
      case "turn/steer":
        response(id, {});
        break;
      case "turn/interrupt": {
        response(id, {});
        const thread = threads.get(params.threadId);
        if (thread) {
          for (const [requestId, approval] of approvals) {
            if (approval.thread.id === thread.id && approval.turn.id === params.turnId) {
              approvals.delete(requestId);
            }
          }
          thread.status = "idle";
          notify("turn/completed", {
            threadId: thread.id,
            turn: {
              id: params.turnId,
              threadId: thread.id,
              status: "interrupted",
              items: [],
            },
          });
        }
        break;
      }
      default:
        response(id, {});
        break;
    }
    return;
  }

  const approval = approvals.get(message.id);
  if (approval) {
    approvals.delete(message.id);
    notify("serverRequest/resolved", { requestId: message.id });
    completeTurn(approval.thread, approval.turn, approval.item);
  }
}

const input = readline.createInterface({ input: process.stdin, crlfDelay: Infinity });
input.on("line", (line) => {
  try {
    handle(JSON.parse(line));
  } catch {
    // Ignore non-JSON launcher noise, matching the real adapter's boundary.
  }
});
