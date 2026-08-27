#!/usr/bin/env node

import readline from "node:readline";

const cwd = process.cwd();
const threads = new Map();
const approvals = new Map();
const contractRequests = new Map();
const contractResponses = [];
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
    archived: false,
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
    turnId: turn.id,
    itemId: item.id,
    startedAtMs: Date.now(),
    command: "fixture --safe-check",
    cwd,
    reason: "The fixture is checking the approval reducer without touching the filesystem.",
  });
}

function emitServerRequestContracts(threadId) {
  const requests = [
    {
      method: "item/fileChange/requestApproval",
      params: {
        threadId,
        turnId: "fixture-contract-turn",
        itemId: "fixture-file-item",
        startedAtMs: Date.now(),
        reason: "fixture file approval",
      },
    },
    {
      method: "item/permissions/requestApproval",
      params: {
        threadId,
        turnId: "fixture-contract-turn",
        itemId: "fixture-permission-item",
        startedAtMs: Date.now(),
        cwd,
        permissions: {
          fileSystem: {
            entries: [
              {
                access: "write",
                path: { type: "path", path: cwd },
              },
            ],
          },
        },
        reason: "fixture permission approval",
      },
    },
    {
      method: "item/tool/requestUserInput",
      params: {
        threadId,
        turnId: "fixture-contract-turn",
        itemId: "fixture-input-item",
        isBlocking: true,
        questions: [
          {
            id: "fixture-question",
            header: "Fixture",
            question: "Continue the contract test?",
            options: [{ label: "Yes", description: "Continue" }],
          },
        ],
      },
    },
    {
      method: "mcpServer/elicitation/request",
      params: {
        threadId,
        serverName: "fixture-mcp",
        mode: "form",
        message: "Confirm the fixture MCP request",
        requestedSchema: {
          type: "object",
          properties: {},
        },
      },
    },
    {
      method: "item/tool/call",
      params: {
        threadId,
        turnId: "fixture-contract-turn",
        callId: "fixture-tool-call",
        tool: "fixture_tool",
        arguments: {},
      },
    },
    {
      method: "execCommandApproval",
      params: {
        conversationId: threadId,
        callId: "fixture-legacy-command",
        command: ["fixture", "--legacy"],
        cwd,
        parsedCmd: [],
      },
    },
    {
      method: "applyPatchApproval",
      params: {
        conversationId: threadId,
        callId: "fixture-legacy-patch",
        fileChanges: {},
      },
    },
  ];
  for (const { method, params } of requests) {
    const id = nextApproval++;
    contractRequests.set(id, { method });
    request(id, method, params);
  }
  return requests.length;
}

function validContractResponse(method, result) {
  if (!result || typeof result !== "object") return false;
  switch (method) {
    case "item/commandExecution/requestApproval":
    case "item/fileChange/requestApproval":
      return ["accept", "acceptForSession", "decline", "cancel"].includes(result.decision);
    case "item/permissions/requestApproval":
      return result.permissions && typeof result.permissions === "object" && !("decision" in result);
    case "item/tool/requestUserInput":
      return result.answers && typeof result.answers === "object";
    case "mcpServer/elicitation/request":
      return ["accept", "decline", "cancel"].includes(result.action);
    case "item/tool/call":
      return typeof result.success === "boolean" && Array.isArray(result.contentItems);
    case "execCommandApproval":
    case "applyPatchApproval":
      return result.decision === "approved"
        || (result.decision && typeof result.decision.denied === "object");
    default:
      return false;
  }
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
        response(id, {
          data: [...threads.values()].filter((thread) =>
            params.archived === true ? thread.archived : !thread.archived,
          ),
        });
        break;
      case "model/list":
        response(id, {
          data: [
            {
              id: "fixture-model",
              model: "fixture-model",
              displayName: "Fixture Model",
              supportedReasoningEfforts: [
                { reasoningEffort: "low", description: "Fast" },
                { reasoningEffort: "high", description: "Deep" },
              ],
            },
          ],
        });
        break;
      case "permissionProfile/list":
        response(id, { data: [{ id: ":workspace", name: "Workspace" }] });
        break;
      case "collaborationMode/list":
        response(id, { data: [{ mode: "default", name: "Default" }, { mode: "plan", name: "Plan" }] });
        break;
      case "app/list":
        response(id, { data: [{ id: "fixture-app", name: "Fixture App" }] });
        break;
      case "app/installed":
        response(id, { apps: [{ id: "fixture-app", name: "Fixture App" }] });
        break;
      case "plugin/list":
        response(id, { marketplaces: [{ name: "Fixture Marketplace", plugins: [{ id: "fixture-plugin", name: "Fixture Plugin" }] }] });
        break;
      case "skills/list":
        response(id, { data: [{ cwd, skills: [{ name: "fixture-skill" }] }] });
        break;
      case "mcpServerStatus/list":
        response(id, { data: [{ name: "fixture-mcp", status: "connected" }] });
        break;
      case "account/read":
        response(id, { account: { name: "Fixture account", type: "fixture" } });
        break;
      case "config/read":
        response(id, { config: { approval_policy: "on-request", sandbox_mode: "workspace-write" }, origins: {} });
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
        const thread = threads.get(params.threadId);
        if (thread) thread.archived = true;
        response(id, {});
        notify("thread/archived", { threadId: params.threadId });
        break;
      }
      case "thread/unarchive": {
        const thread = threads.get(params.threadId);
        if (thread) thread.archived = false;
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
      case "fixture/emitServerRequests": {
        response(id, { count: emitServerRequestContracts(params.threadId) });
        break;
      }
      case "fixture/assertServerRequests": {
        const invalid = contractResponses.filter((entry) => !entry.valid);
        response(id, {
          count: contractResponses.length,
          valid: invalid.length === 0,
          methods: contractResponses.map((entry) => entry.method),
        });
        break;
      }
      case "review/start": {
        const thread = threads.get(params.threadId) ?? createThread();
        const turn = {
          id: `fixture-review-${nextTurn++}`,
          threadId: thread.id,
          status: "inProgress",
        };
        const item = {
          id: `fixture-review-item-${nextItem++}`,
          type: "fileChange",
          status: "completed",
          changes: [
            {
              path: "fixture-review.txt",
              kind: "modified",
              diff: "+ fixture review passed\n- fixture review pending",
            },
          ],
        };
        response(id, { turn: { ...turn, status: "completed" } });
        thread.status = "running";
        notify("turn/started", { threadId: thread.id, turn });
        notify("item/started", { threadId: thread.id, item });
        notify("item/completed", { threadId: thread.id, item });
        completeTurn(thread, turn, item);
        break;
      }
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
      case "thread/realtime/start":
        response(id, { realtimeSessionId: "fixture-realtime" });
        notify("thread/realtime/started", {
          threadId: params.threadId,
          realtimeSessionId: "fixture-realtime",
          version: "fixture",
        });
        break;
      case "thread/realtime/appendText":
        response(id, {});
        notify("thread/realtime/transcript/delta", {
          threadId: params.threadId,
          role: "user",
          delta: params.text ?? "",
        });
        break;
      case "thread/realtime/stop":
        response(id, {});
        notify("thread/realtime/closed", { threadId: params.threadId, reason: "fixture stop" });
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

  const contract = contractRequests.get(message.id);
  if (contract) {
    contractRequests.delete(message.id);
    const valid = validContractResponse(contract.method, message.result);
    contractResponses.push({ method: contract.method, valid });
    notify("fixture/serverRequestValidated", { method: contract.method, valid });
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
