# Parity status: Codex App GPUI

Reference: Codex Desktop 26.820.60940

**Parity coverage:** 0/24 avenues verified (0%). 0 implemented, 24 planned, 0 blocked.

**Acceptance gates:** 0/10 checked. See [GATES.md](GATES.md) for runnable and manual evidence.

## Status counts

| Status | Count |
| --- | ---: |
| planned | 24 |
| implemented | 0 |
| verified | 0 |
| blocked | 0 |

## Ledger snapshot

| ID | Avenue | Owner | Status | Evidence |
| --- | --- | --- | --- | --- |
| shell-01 | App window and dark/light appearance | `src/ui.rs`, `src/theme.rs` | planned | G7 |
| shell-02 | Sidebar navigation | `src/ui.rs` | planned | G7/G8 |
| shell-03 | Project/task hierarchy | `src/state.rs`, `src/ui.rs` | planned | G2/G7 |
| shell-04 | Search and task navigation | `src/state.rs`, `src/ui.rs` | planned | G2/G8 |
| thread-01 | Thread header | `src/ui.rs` | planned | G7/G8 |
| thread-02 | Transcript | `src/ui.rs`, `src/model.rs` | planned | G2/G7 |
| thread-03 | Turn lifecycle | `src/protocol.rs`, `src/state.rs` | planned | G2/G4/G8 |
| thread-04 | Plan and subagents | `src/model.rs`, `src/ui.rs` | planned | G2/G7 |
| composer-01 | Composer | `src/ui.rs`, `src/state.rs` | planned | G2/G8 |
| composer-02 | Composer controls | `src/ui.rs`, `src/state.rs` | planned | G2/G8 |
| composer-03 | Usage and context | `src/state.rs`, `src/ui.rs` | planned | G2/G7 |
| exec-01 | App-server connection | `src/protocol.rs` | planned | G4/G9 |
| exec-02 | Tool execution | `src/model.rs`, `src/ui.rs` | planned | G2/G7 |
| exec-03 | Approvals | `src/state.rs`, `src/protocol.rs`, `src/ui.rs` | planned | G2/G4/G8 |
| exec-04 | Sandbox and environment | `src/model.rs`, `src/state.rs`, `src/ui.rs` | planned | G7/G8 |
| data-01 | Durable state | `src/persistence.rs` | planned | G2/G5 |
| data-02 | Import/export boundaries | `src/persistence.rs`, `scripts/check-safety.mjs` | planned | G5/G10 |
| collab-01 | Share and review | `src/ui.rs`, `src/protocol.rs` | planned | G8/G9 |
| collab-02 | Threads/tasks | `src/state.rs`, `src/protocol.rs`, `src/ui.rs` | planned | G2/G4/G8 |
| nav-01 | Settings | `src/model.rs`, `src/state.rs`, `src/ui.rs` | planned | G7/G8 |
| nav-02 | Top-level destinations | `src/state.rs`, `src/ui.rs` | planned | G7/G8 |
| runtime-01 | Headless runtime | `src/main.rs`, `scripts/` | planned | G6 |
| runtime-02 | Native runtime | `src/main.rs`, `src/ui.rs` | planned | G7/G8 |
| runtime-03 | Reference integration | `src/protocol.rs`, `scripts/` | planned | G9 |

This file is generated from [PARITY.md](../PARITY.md) by the pre-commit hook.
