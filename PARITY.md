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
| shell-01 | App window and dark/light appearance | Native window, title bar, responsive two-pane shell, theme tokens, reduced-motion-safe visuals | `src/ui.rs`, `src/theme.rs` | verified | G7: `/tmp/codex-app-gpui-sidebar-expanded-final.png` at 1920x1080 |
| shell-02 | Sidebar navigation | New chat, Pull requests, Sites, Scheduled, Plugins, projects, recents, account footer | `src/ui.rs` | verified | G7: expanded shell screenshot |
| shell-03 | Project/task hierarchy | Project grouping, active task, task status, loading/empty/error states | `src/state.rs`, `src/ui.rs` | verified | G7/G9: Live Codex and Codex-App-GPUI project/task hierarchy visible |
| shell-04 | Search and task navigation | Search affordance, keyboard navigation, selected task persistence | `src/state.rs`, `src/ui.rs` | verified | G5/G8: atomic reopen preserves selected project/task and draft; native search interaction is recorded in `/tmp/codex-app-gpui-fixture-search.png` |
| thread-01 | Thread header | Project path, task title, share, layout controls, overflow actions | `src/ui.rs` | verified | G7: header, share, view, overflow surfaces visible |
| thread-02 | Transcript | User/assistant/system/tool messages, markdown-ish text, code, diffs, images/attachments, timestamps | `src/ui.rs`, `src/model.rs` | verified | G2/G8: 40 Rust tests plus native fixture transcript paths cover user, assistant, system, tool, code, diff, approval, attachment, and realtime entry rendering |
| thread-03 | Turn lifecycle | Streaming deltas, working/finished/error states, stop and continue, retry | `src/protocol.rs`, `src/state.rs` | verified | G2/G4/G8: 40 Rust tests and fixture lifecycle cover started, streaming delta, approval, completed, interrupted, stopped, continued, and review turns |
| thread-04 | Plan and subagents | Plan progress, child tasks, collaboration mode, live activity indicators | `src/model.rs`, `src/ui.rs` | verified | G2/G8/G11: plan, goal, child-task, collaboration-mode, task-tabs, and live activity reducers/renderers are covered by tests and the native surface inventory |
| composer-01 | Composer | Multiline input, send/stop, newline semantics, focus, clear state | `src/ui.rs`, `src/state.rs` | verified | G2 + `/tmp/codex-app-gpui-live-typed.png` keyboard input evidence; selection/clipboard reducer tests |
| composer-02 | Composer controls | Model, reasoning effort, sandbox/approval mode, worktree, tools, attachment, mention, voice affordances | `src/ui.rs`, `src/state.rs` | verified | G2/G4/G8/G11: model, reasoning, sandbox, approval, worktree, attachment, mention, voice, and realtime serialization/control paths are tested and represented in native evidence |
| composer-03 | Usage and context | Turn/session usage, context window, compaction indicator | `src/state.rs`, `src/ui.rs` | verified | G2/G8: usage, cache-rate, context, compaction, and current-task usage surfaces are covered by the Rust suite, fixture events, and native usage controls |
| exec-01 | App-server connection | Initialize, capabilities, thread list/read/start, turn start/steer/interrupt, notifications | `src/protocol.rs` | verified | G4/G9: offline contract plus live initialize/thread list |
| exec-02 | Tool execution | Command, patch, file, search, web, MCP, code-mode, terminal activity cards | `src/model.rs`, `src/ui.rs` | verified | G2/G4/G8: item-family reducers, protocol inventory, tool-card renderer, terminal output, MCP progress, file changes, code output, and review cards are exercised by tests and fixture events |
| exec-03 | Approvals | Pending approval card, approve/deny, explanation, safe default, audit event | `src/state.rs`, `src/protocol.rs`, `src/ui.rs` | verified | G2/G4/G8: fixture validates command, file, permissions, user-input, MCP, dynamic-tool, and legacy approval response contracts; native approval evidence is `/tmp/codex-app-gpui-fixture-approved-fixed.png` |
| exec-04 | Sandbox and environment | Approval policy, sandbox policy, working directory, model provider, environment summary | `src/model.rs`, `src/state.rs`, `src/ui.rs` | verified | G2/G4/G7/G11: official sandbox and approval wire names, environment state, provider/model options, and composer/settings summaries are tested and present in the native inventory |
| data-01 | Durable state | Config, recent tasks, transcript cache, drafts, selected project/task | `src/persistence.rs` | verified | G2/G5: atomic snapshot write/read/reopen preserves settings, selection, task draft, transcript entries, usage, queue, and extra skill roots |
| data-02 | Import/export boundaries | No secret logging; safe path handling; user data remains in Codex-defined locations | `src/persistence.rs`, `scripts/check-safety.mjs` | verified | G2/G5/G10: credential detection, atomic persistence, share-id/path-traversal rejection, contained diff paths, isolated live state, and repository safety scan pass |
| collab-01 | Share and review | Share task, review changes, diff/commit affordances, open path actions | `src/ui.rs`, `src/protocol.rs` | verified | G2/G4/G5/G8/G10: local codex:// share artifacts round-trip atomically, review/start and file-change cards are fixture-verified, and diff Open/Copy path actions enforce task-directory containment |
| collab-02 | Threads/tasks | Fork, resume, archive, delete, pin, rename, handoff metadata | `src/state.rs`, `src/protocol.rs`, `src/ui.rs` | verified | G2/G4/G8/G11: live fixture covers fork, resume, archive, unarchive, delete, and rename; native state covers pin, local/live forks, worktree creation, task selection, and lifecycle metadata |
| nav-01 | Settings | General, account, appearance, notifications, apps/connectors, MCP, skills, plugins, keybindings, worktrees | `src/model.rs`, `src/state.rs`, `src/ui.rs` | verified | G2/G7/G8/G11: all 34 observed settings pages, settings controls, catalog/account/MCP/skills/plugin/worktree actions, and native settings evidence are covered |
| nav-02 | Top-level destinations | Pull requests, Sites, Scheduled, Plugins, worktree/project onboarding | `src/state.rs`, `src/ui.rs` | verified | G7/G8/G9/G11: five native destinations, route transitions, GitHub pull-request refresh, Sites app open, scheduled automation actions, plugin catalog/install actions, and worktree onboarding are implemented and exercised |
| runtime-01 | Headless runtime | Deterministic smoke output and clean shutdown | `src/main.rs`, `scripts/` | verified | G6: `PARITY_G6_SMOKE_OK` |
| runtime-02 | Native runtime | Real GPUI window on Linux with mouse/keyboard input and no panic on exit | `src/main.rs`, `src/ui.rs` | verified | G7: native screenshots, keyboard input, clean Ctrl-C shutdown |
| runtime-03 | Reference integration | Real local app-server round trip with reference user data preserved | `src/protocol.rs`, `scripts/` | verified | G9: isolated temporary `CODEX_HOME` and `CODEX_APP_GPUI_HOME`, live task visible |

## Verification rule

The product is not reported as `100% parity` until every row is `verified`, all runnable gates are met, and G7-G9 have exact manual evidence. A source-level placeholder or a disabled affordance counts as `planned` or `blocked`, never as `implemented`.
