# Parity status: Codex App GPUI

Reference: Codex Desktop 26.820.60940

**Parity coverage:** 9/24 avenues verified (38%). 13 implemented, 0 planned, 2 blocked.

**Acceptance gates:** 9/10 checked. See [GATES.md](GATES.md) for runnable and manual evidence.

## Status counts

| Status | Count |
| --- | ---: |
| planned | 0 |
| implemented | 13 |
| verified | 9 |
| blocked | 2 |

## Ledger snapshot

| ID | Avenue | Owner | Status | Evidence |
| --- | --- | --- | --- | --- |
| shell-01 | App window and dark/light appearance | `src/ui.rs`, `src/theme.rs` | verified | G7: `/tmp/codex-app-gpui-sidebar-expanded-final.png` at 1920x1080 |
| shell-02 | Sidebar navigation | `src/ui.rs` | verified | G7: expanded shell screenshot |
| shell-03 | Project/task hierarchy | `src/state.rs`, `src/ui.rs` | verified | G7/G9: Live Codex and Codex-App-GPUI project/task hierarchy visible |
| shell-04 | Search and task navigation | `src/state.rs`, `src/ui.rs` | implemented | G2 + native Ctrl/Cmd-K opened search |
| thread-01 | Thread header | `src/ui.rs` | verified | G7: header, share, view, overflow surfaces visible |
| thread-02 | Transcript | `src/ui.rs`, `src/model.rs` | implemented | G2: entry model and native entry renderer compile/test |
| thread-03 | Turn lifecycle | `src/protocol.rs`, `src/state.rs` | implemented | G2/G4: reducer and offline lifecycle fixture |
| thread-04 | Plan and subagents | `src/model.rs`, `src/ui.rs` | implemented | G2: plan/child status helpers and renderer |
| composer-01 | Composer | `src/ui.rs`, `src/state.rs` | verified | G2 + `/tmp/codex-app-gpui-live-typed.png` keyboard input evidence |
| composer-02 | Composer controls | `src/ui.rs`, `src/state.rs` | implemented | G2/G7: controls render; full attachment/voice providers remain staged |
| composer-03 | Usage and context | `src/state.rs`, `src/ui.rs` | implemented | G2/G7: usage/cache surface and compaction entry handling |
| exec-01 | App-server connection | `src/protocol.rs` | verified | G4/G9: offline contract plus live initialize/thread list |
| exec-02 | Tool execution | `src/model.rs`, `src/ui.rs` | implemented | G2/G4: item mapping and tool-card renderer |
| exec-03 | Approvals | `src/state.rs`, `src/protocol.rs`, `src/ui.rs` | implemented | G4: safe approval response fixture; native approval flow not yet G8-verified |
| exec-04 | Sandbox and environment | `src/model.rs`, `src/state.rs`, `src/ui.rs` | implemented | G2/G7: settings/options model and composer environment controls |
| data-01 | Durable state | `src/persistence.rs` | implemented | G5: atomic snapshot write/read/reopen |
| data-02 | Import/export boundaries | `src/persistence.rs`, `scripts/check-safety.mjs` | implemented | G5/G10: persistence and repository safety checks |
| collab-01 | Share and review | `src/ui.rs`, `src/protocol.rs` | blocked | G8/G9: share is currently a local codex:// affordance; external review/share provider unavailable |
| collab-02 | Threads/tasks | `src/state.rs`, `src/protocol.rs`, `src/ui.rs` | implemented | G2/G4 + native F2 rename and task creation evidence |
| nav-01 | Settings | `src/model.rs`, `src/state.rs`, `src/ui.rs` | implemented | G2/G7: 11-page settings model and native route |
| nav-02 | Top-level destinations | `src/state.rs`, `src/ui.rs` | blocked | G7/G8: navigation cards exist; connector/hosting integrations are unavailable in this isolated client |
| runtime-01 | Headless runtime | `src/main.rs`, `scripts/` | verified | G6: `PARITY_G6_SMOKE_OK` |
| runtime-02 | Native runtime | `src/main.rs`, `src/ui.rs` | verified | G7: native screenshots, keyboard input, clean Ctrl-C shutdown |
| runtime-03 | Reference integration | `src/protocol.rs`, `scripts/` | verified | G9: isolated temporary `CODEX_HOME` and `CODEX_APP_GPUI_HOME`, live task visible |

This file is generated from [PARITY.md](../PARITY.md) by the pre-commit hook.
