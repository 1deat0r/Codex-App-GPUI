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
| shell-04 | Search and task navigation | Search affordance, keyboard navigation, selected task persistence | `src/state.rs`, `src/ui.rs` | implemented | G5/G8: persisted selection plus native search screenshot `/tmp/codex-app-gpui-fixture-search.png` |
| thread-01 | Thread header | Project path, task title, share, layout controls, overflow actions | `src/ui.rs` | verified | G7: header, share, view, overflow surfaces visible |
| thread-02 | Transcript | User/assistant/system/tool messages, markdown-ish text, code, diffs, images/attachments, timestamps | `src/ui.rs`, `src/model.rs` | implemented | G2: entry model and native entry renderer compile/test |
| thread-03 | Turn lifecycle | Streaming deltas, working/finished/error states, stop and continue, retry | `src/protocol.rs`, `src/state.rs` | implemented | G2/G4: reducer and offline lifecycle fixture |
| thread-04 | Plan and subagents | Plan progress, child tasks, collaboration mode, live activity indicators | `src/model.rs`, `src/ui.rs` | implemented | G2: plan/child status helpers and renderer |
| composer-01 | Composer | Multiline input, send/stop, newline semantics, focus, clear state | `src/ui.rs`, `src/state.rs` | verified | G2 + `/tmp/codex-app-gpui-live-typed.png` keyboard input evidence; selection/clipboard reducer tests |
| composer-02 | Composer controls | Model, reasoning effort, sandbox/approval mode, worktree, tools, attachment, mention, voice affordances | `src/ui.rs`, `src/state.rs` | implemented | G2/G8: native model/reasoning and attachment/mention screenshots; native picker and app-server attachment serialization are implemented, realtime voice remains staged |
| composer-03 | Usage and context | Turn/session usage, context window, compaction indicator | `src/state.rs`, `src/ui.rs` | implemented | G2/G7: usage/cache surface and compaction entry handling |
| exec-01 | App-server connection | Initialize, capabilities, thread list/read/start, turn start/steer/interrupt, notifications | `src/protocol.rs` | verified | G4/G9: offline contract plus live initialize/thread list |
| exec-02 | Tool execution | Command, patch, file, search, web, MCP, code-mode, terminal activity cards | `src/model.rs`, `src/ui.rs` | implemented | G2/G4: item mapping and tool-card renderer |
| exec-03 | Approvals | Pending approval card, approve/deny, explanation, safe default, audit event | `src/state.rs`, `src/protocol.rs`, `src/ui.rs` | implemented | G4/G8: safe decline contract plus native fixture approval screenshot `/tmp/codex-app-gpui-fixture-approved-fixed.png`; both decisions and provider-specific approval variants remain to be exercised |
| exec-04 | Sandbox and environment | Approval policy, sandbox policy, working directory, model provider, environment summary | `src/model.rs`, `src/state.rs`, `src/ui.rs` | implemented | G2/G7: settings/options model and composer environment controls |
| data-01 | Durable state | Config, recent tasks, transcript cache, drafts, selected project/task | `src/persistence.rs` | implemented | G5: atomic snapshot write/read/reopen |
| data-02 | Import/export boundaries | No secret logging; safe path handling; user data remains in Codex-defined locations | `src/persistence.rs`, `scripts/check-safety.mjs` | implemented | G5/G10: persistence and repository safety checks |
| collab-01 | Share and review | Share task, review changes, diff/commit affordances, open path actions | `src/ui.rs`, `src/protocol.rs` | blocked | G8/G9: share is currently a local codex:// affordance; external review/share provider unavailable |
| collab-02 | Threads/tasks | Fork, resume, archive, delete, pin, rename, handoff metadata | `src/state.rs`, `src/protocol.rs`, `src/ui.rs` | implemented | G2/G4/G8: live thread lifecycle adapter plus native task creation, archive, delete, and rename paths; fork/resume/handoff coverage remains incomplete |
| nav-01 | Settings | General, account, appearance, notifications, apps/connectors, MCP, skills, plugins, keybindings, worktrees | `src/model.rs`, `src/state.rs`, `src/ui.rs` | implemented | G2/G8: 11-page settings model and native General settings screenshot `/tmp/codex-app-gpui-fixture-settings-fixed.png`; external account/connectors remain unavailable |
| nav-02 | Top-level destinations | Pull requests, Sites, Scheduled, Plugins, worktree/project onboarding | `src/state.rs`, `src/ui.rs` | blocked | G7/G8: navigation cards exist; connector/hosting integrations are unavailable in this isolated client |
| runtime-01 | Headless runtime | Deterministic smoke output and clean shutdown | `src/main.rs`, `scripts/` | verified | G6: `PARITY_G6_SMOKE_OK` |
| runtime-02 | Native runtime | Real GPUI window on Linux with mouse/keyboard input and no panic on exit | `src/main.rs`, `src/ui.rs` | verified | G7: native screenshots, keyboard input, clean Ctrl-C shutdown |
| runtime-03 | Reference integration | Real local app-server round trip with reference user data preserved | `src/protocol.rs`, `scripts/` | verified | G9: isolated temporary `CODEX_HOME` and `CODEX_APP_GPUI_HOME`, live task visible |

## Verification rule

The product is not reported as `100% parity` until every row is `verified`, all runnable gates are met, and G7-G9 have exact manual evidence. A source-level placeholder or a disabled affordance counts as `planned` or `blocked`, never as `implemented`.
