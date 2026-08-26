# Gates: Codex App GPUI parity

OWNS: Cargo.toml, Cargo.lock, src/**, scripts/**, assets/**, docs/**, GATES.md, PARITY.md, README.md

Scope: deliver a native GPUI Codex desktop client whose shell, state model, protocol boundary, persistence, and primary interaction paths are measurable against the local Codex Desktop reference.

- [x] G1: the native GPUI project compiles from a clean checkout
  CHECK: node scripts/run-cargo.mjs check --locked
  EXPECT: PARITY_G1_BUILD_OK
  EVIDENCE: verified 2026-08-27; exit=0; stable Rust toolchain; output marker `PARITY_G1_BUILD_OK`; existing warnings are non-fatal dead-code and future-compatibility warnings.

- [x] G2: domain state, reducer, protocol, persistence, and UI helper tests pass
  CHECK: node scripts/run-cargo.mjs test --locked
  EXPECT: PARITY_G2_TESTS_OK
  EVIDENCE: verified 2026-08-27; exit=0; 15 tests passed; output marker `PARITY_G2_TESTS_OK`.

- [x] G3: the parity ledger is internally consistent and every required reference avenue has an implementation owner
  CHECK: node scripts/verify-parity.mjs
  EXPECT: PARITY_G3_LEDGER_OK
  EVIDENCE: verified 2026-08-27; exit=0; output `PARITY_G3_LEDGER_OK rows=24`.

- [x] G4: the app-server JSONL adapter round-trips the supported initialize, thread, turn, event, and approval contracts against an offline fixture server
  CHECK: node scripts/verify-protocol.mjs
  EXPECT: PARITY_G4_PROTOCOL_OK
  EVIDENCE: verified 2026-08-27; exit=0; offline protocol fixture passed initialize, thread start/list, turn start/interrupt, archive/unarchive/resume, review, and approval response coverage; output marker `PARITY_G4_PROTOCOL_OK`.

- [x] G5: configuration and transcript persistence survive a write/read/reopen cycle without leaking credentials
  CHECK: node scripts/verify-persistence.mjs
  EXPECT: PARITY_G5_PERSISTENCE_OK
  EVIDENCE: verified 2026-08-27; exit=0; atomic snapshot round-trip passed with credential detector coverage; output marker `PARITY_G5_PERSISTENCE_OK`.

- [x] G6: the executable supports a deterministic headless smoke path and reports the full primary-surface inventory
  CHECK: node scripts/run-cargo.mjs run --locked -- --smoke
  EXPECT: PARITY_G6_SMOKE_OK
  EVIDENCE: verified 2026-08-27; exit=0; output `target/debug/codex-app-gpui --smoke`, `models=4 reasoning=4 destinations=5 settings=11`, and marker `PARITY_G6_SMOKE_OK`.

- [x] G7: the native window renders the Codex shell at desktop dimensions with the reference navigation, thread, composer, account, and project surfaces visible and usable
  EVIDENCE: verified 2026-08-27 on Linux at 1920x1080; `/tmp/codex-app-gpui-sidebar-expanded-final.png` shows the expanded native shell with menu bar, navigation, Live Codex/Codex-App-GPUI projects, task, header actions, composer, account footer; `/tmp/codex-app-gpui-sidebar-collapsed.png` shows the functional compact shell; keyboard input and Ctrl/Cmd+Shift+B were exercised.

- [x] G8: primary user flows work end to end in the native window: new task, task selection/search, message send/stop, model/reasoning selection, attachment/mention affordances, approval decision, archive/delete, share, and settings navigation
  EVIDENCE: verified 2026-08-27 with the repository-owned `scripts/live-fixture-server.mjs` and isolated `CODEX_APP_GPUI_HOME=/tmp/codex-app-gpui-fixture-state.tuXpsb`; native screenshots `/tmp/codex-app-gpui-fixture-search.png`, `/tmp/codex-app-gpui-fixture-new-task-fixed.png`, `/tmp/codex-app-gpui-fixture-stopped-fixed.png`, `/tmp/codex-app-gpui-fixture-model-reasoning.png`, `/tmp/codex-app-gpui-fixture-attach-mention-fixed.png`, `/tmp/codex-app-gpui-fixture-approved-fixed.png`, `/tmp/codex-app-gpui-fixture-archived.png`, `/tmp/codex-app-gpui-fixture-deleted.png`, `/tmp/codex-app-gpui-fixture-share-fixed.png`, and `/tmp/codex-app-gpui-fixture-settings-fixed.png` show the exercised flows. The fixture performs no network or model calls.

- [x] G9: the client can attach to a real locally installed Codex app-server and display a live thread without replacing or mutating the reference app's user data
  EVIDENCE: verified 2026-08-27 with the locally installed `codex app-server --stdio`, `CODEX_HOME=/tmp/codex-app-gpui-live.jJeDIx`, and `CODEX_APP_GPUI_HOME=/tmp/codex-app-gpui-state.zYp43q`; the native window displayed a Live Codex task and the process was stopped without sending a turn; the reference desktop history remained outside those temporary paths.

- [x] G10: repository-owned checks contain no hard-coded credentials, private tokens, or destructive data commands
  CHECK: node scripts/check-safety.mjs
  EXPECT: PARITY_G10_SAFETY_OK
  EVIDENCE: verified 2026-08-27; exit=0; output `PARITY_G10_SAFETY_OK files=24`.
