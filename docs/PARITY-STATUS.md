# Parity status: Codex App GPUI

Reference: Codex Desktop 26.820.60940

**Parity coverage:** 24/24 avenues verified (100%). 0 implemented, 0 planned, 0 blocked.

**Acceptance gates:** 11/11 checked. See [GATES.md](GATES.md) for runnable and manual evidence.

## Status counts

| Status | Count |
| --- | ---: |
| planned | 0 |
| implemented | 0 |
| verified | 24 |
| blocked | 0 |

## Ledger snapshot

| ID | Avenue | Owner | Status | Evidence |
| --- | --- | --- | --- | --- |
| shell-01 | App window and dark/light appearance | `src/ui.rs`, `src/theme.rs` | verified | G7: `/tmp/codex-app-gpui-sidebar-expanded-final.png` at 1920x1080 |
| shell-02 | Sidebar navigation | `src/ui.rs` | verified | G7: expanded shell screenshot |
| shell-03 | Project/task hierarchy | `src/state.rs`, `src/ui.rs` | verified | G7/G9: Live Codex and Codex-App-GPUI project/task hierarchy visible |
| shell-04 | Search and task navigation | `src/state.rs`, `src/ui.rs` | verified | G5/G8: atomic reopen preserves selected project/task and draft; native search interaction is recorded in `/tmp/codex-app-gpui-fixture-search.png` |
| thread-01 | Thread header | `src/ui.rs` | verified | G7: header, share, view, overflow surfaces visible |
| thread-02 | Transcript | `src/ui.rs`, `src/model.rs` | verified | G2/G8: 40 Rust tests plus native fixture transcript paths cover user, assistant, system, tool, code, diff, approval, attachment, and realtime entry rendering |
| thread-03 | Turn lifecycle | `src/protocol.rs`, `src/state.rs` | verified | G2/G4/G8: 40 Rust tests and fixture lifecycle cover started, streaming delta, approval, completed, interrupted, stopped, continued, and review turns |
| thread-04 | Plan and subagents | `src/model.rs`, `src/ui.rs` | verified | G2/G8/G11: plan, goal, child-task, collaboration-mode, task-tabs, and live activity reducers/renderers are covered by tests and the native surface inventory |
| composer-01 | Composer | `src/ui.rs`, `src/state.rs` | verified | G2 + `/tmp/codex-app-gpui-live-typed.png` keyboard input evidence; selection/clipboard reducer tests |
| composer-02 | Composer controls | `src/ui.rs`, `src/state.rs` | verified | G2/G4/G8/G11: model, reasoning, sandbox, approval, worktree, attachment, mention, voice, and realtime serialization/control paths are tested and represented in native evidence |
| composer-03 | Usage and context | `src/state.rs`, `src/ui.rs` | verified | G2/G8: usage, cache-rate, context, compaction, and current-task usage surfaces are covered by the Rust suite, fixture events, and native usage controls |
| exec-01 | App-server connection | `src/protocol.rs` | verified | G4/G9: offline contract plus live initialize/thread list |
| exec-02 | Tool execution | `src/model.rs`, `src/ui.rs` | verified | G2/G4/G8: item-family reducers, protocol inventory, tool-card renderer, terminal output, MCP progress, file changes, code output, and review cards are exercised by tests and fixture events |
| exec-03 | Approvals | `src/state.rs`, `src/protocol.rs`, `src/ui.rs` | verified | G2/G4/G8: fixture validates command, file, permissions, user-input, MCP, dynamic-tool, and legacy approval response contracts; native approval evidence is `/tmp/codex-app-gpui-fixture-approved-fixed.png` |
| exec-04 | Sandbox and environment | `src/model.rs`, `src/state.rs`, `src/ui.rs` | verified | G2/G4/G7/G11: official sandbox and approval wire names, environment state, provider/model options, and composer/settings summaries are tested and present in the native inventory |
| data-01 | Durable state | `src/persistence.rs` | verified | G2/G5: atomic snapshot write/read/reopen preserves settings, selection, task draft, transcript entries, usage, queue, and extra skill roots |
| data-02 | Import/export boundaries | `src/persistence.rs`, `scripts/check-safety.mjs` | verified | G2/G5/G10: credential detection, atomic persistence, share-id/path-traversal rejection, contained diff paths, isolated live state, and repository safety scan pass |
| collab-01 | Share and review | `src/ui.rs`, `src/protocol.rs` | verified | G2/G4/G5/G8/G10: local codex:// share artifacts round-trip atomically, review/start and file-change cards are fixture-verified, and diff Open/Copy path actions enforce task-directory containment |
| collab-02 | Threads/tasks | `src/state.rs`, `src/protocol.rs`, `src/ui.rs` | verified | G2/G4/G8/G11: live fixture covers fork, resume, archive, unarchive, delete, and rename; native state covers pin, local/live forks, worktree creation, task selection, and lifecycle metadata |
| nav-01 | Settings | `src/model.rs`, `src/state.rs`, `src/ui.rs` | verified | G2/G7/G8/G11: all 34 observed settings pages, settings controls, catalog/account/MCP/skills/plugin/worktree actions, and native settings evidence are covered |
| nav-02 | Top-level destinations | `src/state.rs`, `src/ui.rs` | verified | G7/G8/G9/G11: five native destinations, route transitions, GitHub pull-request refresh, Sites app open, scheduled automation actions, plugin catalog/install actions, and worktree onboarding are implemented and exercised |
| runtime-01 | Headless runtime | `src/main.rs`, `scripts/` | verified | G6: `PARITY_G6_SMOKE_OK` |
| runtime-02 | Native runtime | `src/main.rs`, `src/ui.rs` | verified | G7: native screenshots, keyboard input, clean Ctrl-C shutdown |
| runtime-03 | Reference integration | `src/protocol.rs`, `scripts/` | verified | G9: isolated temporary `CODEX_HOME` and `CODEX_APP_GPUI_HOME`, live task visible |

This file is generated from [PARITY.md](../PARITY.md) by the pre-commit hook.
