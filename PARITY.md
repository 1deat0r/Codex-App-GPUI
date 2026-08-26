# Codex App GPUI parity ledger

Reference baseline: locally installed Codex Desktop `26.820.60940` on Linux, observed in the shared desktop on 2026-08-27. The target is a native GPUI client, not a claim that a screenshot alone proves behavioral parity.

Status vocabulary:

- `planned`: not implemented or not verified.
- `implemented`: present in the target source and covered by a local test or deterministic smoke path.
- `verified`: exercised against the native window or a real Codex app-server and recorded in `GATES.md` evidence.
- `blocked`: the reference contract or required external capability is unavailable; retain the handoff and do not call the product complete.

## Product axes

| ID | Reference avenue | Required target behavior | Owner | Status | Evidence |
| --- | --- | --- | --- | --- | --- |
| shell-01 | App window and dark/light appearance | Native window, title bar, responsive two-pane shell, theme tokens, reduced-motion-safe visuals | `src/ui.rs`, `src/theme.rs` | planned | G7 |
| shell-02 | Sidebar navigation | New chat, Pull requests, Sites, Scheduled, Plugins, projects, recents, account footer | `src/ui.rs` | planned | G7/G8 |
| shell-03 | Project/task hierarchy | Project grouping, active task, task status, loading/empty/error states | `src/state.rs`, `src/ui.rs` | planned | G2/G7 |
| shell-04 | Search and task navigation | Search affordance, keyboard navigation, selected task persistence | `src/state.rs`, `src/ui.rs` | planned | G2/G8 |
| thread-01 | Thread header | Project path, task title, share, layout controls, overflow actions | `src/ui.rs` | planned | G7/G8 |
| thread-02 | Transcript | User/assistant/system/tool messages, markdown-ish text, code, diffs, images/attachments, timestamps | `src/ui.rs`, `src/model.rs` | planned | G2/G7 |
| thread-03 | Turn lifecycle | Streaming deltas, working/finished/error states, stop and continue, retry | `src/protocol.rs`, `src/state.rs` | planned | G2/G4/G8 |
| thread-04 | Plan and subagents | Plan progress, child tasks, collaboration mode, live activity indicators | `src/model.rs`, `src/ui.rs` | planned | G2/G7 |
| composer-01 | Composer | Multiline input, send/stop, newline semantics, focus, clear state | `src/ui.rs`, `src/state.rs` | planned | G2/G8 |
| composer-02 | Composer controls | Model, reasoning effort, sandbox/approval mode, worktree, tools, attachment, mention, voice affordances | `src/ui.rs`, `src/state.rs` | planned | G2/G8 |
| composer-03 | Usage and context | Turn/session usage, context window, compaction indicator | `src/state.rs`, `src/ui.rs` | planned | G2/G7 |
| exec-01 | App-server connection | Initialize, capabilities, thread list/read/start, turn start/steer/interrupt, notifications | `src/protocol.rs` | planned | G4/G9 |
| exec-02 | Tool execution | Command, patch, file, search, web, MCP, code-mode, terminal activity cards | `src/model.rs`, `src/ui.rs` | planned | G2/G7 |
| exec-03 | Approvals | Pending approval card, approve/deny, explanation, safe default, audit event | `src/state.rs`, `src/protocol.rs`, `src/ui.rs` | planned | G2/G4/G8 |
| exec-04 | Sandbox and environment | Approval policy, sandbox policy, working directory, model provider, environment summary | `src/model.rs`, `src/state.rs`, `src/ui.rs` | planned | G7/G8 |
| data-01 | Durable state | Config, recent tasks, transcript cache, drafts, selected project/task | `src/persistence.rs` | planned | G2/G5 |
| data-02 | Import/export boundaries | No secret logging; safe path handling; user data remains in Codex-defined locations | `src/persistence.rs`, `scripts/check-safety.mjs` | planned | G5/G10 |
| collab-01 | Share and review | Share task, review changes, diff/commit affordances, open path actions | `src/ui.rs`, `src/protocol.rs` | planned | G8/G9 |
| collab-02 | Threads/tasks | Fork, resume, archive, delete, pin, rename, handoff metadata | `src/state.rs`, `src/protocol.rs`, `src/ui.rs` | planned | G2/G4/G8 |
| nav-01 | Settings | General, account, appearance, notifications, apps/connectors, MCP, skills, plugins, keybindings, worktrees | `src/model.rs`, `src/state.rs`, `src/ui.rs` | planned | G7/G8 |
| nav-02 | Top-level destinations | Pull requests, Sites, Scheduled, Plugins, worktree/project onboarding | `src/state.rs`, `src/ui.rs` | planned | G7/G8 |
| runtime-01 | Headless runtime | Deterministic smoke output and clean shutdown | `src/main.rs`, `scripts/` | planned | G6 |
| runtime-02 | Native runtime | Real GPUI window on Linux with mouse/keyboard input and no panic on exit | `src/main.rs`, `src/ui.rs` | planned | G7/G8 |
| runtime-03 | Reference integration | Real local app-server round trip with reference user data preserved | `src/protocol.rs`, `scripts/` | planned | G9 |

## Verification rule

The product is not reported as `100% parity` until every row is `verified`, all runnable gates are met, and G7-G9 have exact manual evidence. A source-level placeholder or a disabled affordance counts as `planned` or `blocked`, never as `implemented`.
