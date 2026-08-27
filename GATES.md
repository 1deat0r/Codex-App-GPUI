# Gates: Codex App GPUI parity

OWNS: Cargo.toml, Cargo.lock, src/**, scripts/**, assets/**, docs/**, GATES.md, PARITY.md, README.md

Scope: deliver a native GPUI Codex desktop client whose shell, state model, protocol boundary, persistence, and primary interaction paths are measurable against the local Codex Desktop reference.

- [x] G1: the native GPUI project compiles from a clean checkout
  CHECK: node scripts/run-cargo.mjs check --locked
  EXPECT: PARITY_G1_BUILD_OK
  EVIDENCE: exit=0; output=PARITY_G1_BUILD_OK; verified at HEAD 2874e66 on 2026-08-27

- [x] G2: domain state, reducer, protocol, persistence, and UI helper tests pass
  CHECK: node scripts/run-cargo.mjs test --locked
  EXPECT: PARITY_G2_TESTS_OK
  EVIDENCE: exit=0; tests=40 passed, 0 failed; output=PARITY_G2_TESTS_OK; verified at HEAD 2874e66 on 2026-08-27

- [x] G3: the parity ledger is internally consistent and every required reference avenue has an implementation owner
  CHECK: node scripts/verify-parity.mjs
  EXPECT: PARITY_G3_LEDGER_OK
  EVIDENCE: exit=0; output=PARITY_G3_LEDGER_OK rows=24; verified at HEAD 2874e66 on 2026-08-27

- [x] G4: the app-server JSONL adapter round-trips the supported initialize, thread, turn, event, and approval contracts against an offline fixture server
  CHECK: node scripts/verify-protocol.mjs
  EXPECT: PARITY_G4_PROTOCOL_OK
  EVIDENCE: exit=0; tests=10 passed, 0 failed; output=PARITY_G2_TESTS_OK and PARITY_G4_PROTOCOL_OK; verified at HEAD 2874e66 on 2026-08-27

- [x] G5: configuration and transcript persistence survive a write/read/reopen cycle without leaking credentials
  CHECK: node scripts/verify-persistence.mjs
  EXPECT: PARITY_G5_PERSISTENCE_OK
  EVIDENCE: exit=0; tests=5 passed, 0 failed; output=PARITY_G2_TESTS_OK and PARITY_G5_PERSISTENCE_OK; verified at HEAD 2874e66 on 2026-08-27

- [x] G6: the executable supports a deterministic headless smoke path and reports the full primary-surface inventory
  CHECK: node scripts/run-cargo.mjs run --locked -- --smoke
  EXPECT: PARITY_G6_SMOKE_OK
  EVIDENCE: exit=0; output=destinations=5 settings_pages=34 official_client_requests=150 PARITY_G6_SMOKE_OK; verified at HEAD 2874e66 on 2026-08-27

- [x] G7: the native window renders the Codex shell at desktop dimensions with the reference navigation, thread, composer, account, and project surfaces visible and usable
  EVIDENCE: verified 2026-08-27 on Linux at 1920x1080; `/tmp/codex-app-gpui-native-evidence-current.png` shows the current native fullscreen shell with menu bar, navigation, settings inventory, account, projects, and tasks; `/tmp/codex-app-gpui-sidebar-expanded-final.png` and `/tmp/codex-app-gpui-sidebar-collapsed.png` show expanded and compact shells; keyboard input, focus, and Ctrl/Cmd+Shift+B were exercised.

- [x] G8: primary user flows work end to end in the native window: new task, task selection/search, message send/stop, model/reasoning selection, attachment/mention affordances, approval decision, archive/delete, share, and settings navigation
  EVIDENCE: verified 2026-08-27 with the repository-owned `scripts/live-fixture-server.mjs` and isolated `CODEX_APP_GPUI_HOME=/tmp/codex-app-gpui-fixture-state.tuXpsb`; native screenshots `/tmp/codex-app-gpui-fixture-search.png`, `/tmp/codex-app-gpui-fixture-new-task-fixed.png`, `/tmp/codex-app-gpui-fixture-stopped-fixed.png`, `/tmp/codex-app-gpui-fixture-model-reasoning.png`, `/tmp/codex-app-gpui-fixture-attach-mention-fixed.png`, `/tmp/codex-app-gpui-fixture-approved-fixed.png`, `/tmp/codex-app-gpui-fixture-archived.png`, `/tmp/codex-app-gpui-fixture-deleted.png`, `/tmp/codex-app-gpui-fixture-share-fixed.png`, and `/tmp/codex-app-gpui-fixture-settings-fixed.png` show the exercised flows; the fixture contract reports `PARITY_100_FIXTURE_OK events=35 requests=9` and performs no network or model calls.

- [x] G9: the client can attach to a real locally installed Codex app-server and display a live thread without replacing or mutating the reference app's user data
  EVIDENCE: verified 2026-08-27 with the locally installed `codex app-server --stdio`, `CODEX_HOME=/tmp/codex-app-gpui-codex-home-F7ssrz`, and an isolated client state directory; output=`PARITY_100_LIVE_OK thread=01a0415b-5414-7cf2-9e86-a6393210db83`; the live task was created and read without changing the reference desktop data directory.

- [x] G10: repository-owned checks contain no hard-coded credentials, private tokens, or destructive data commands
  CHECK: node scripts/check-safety.mjs
  EXPECT: PARITY_G10_SAFETY_OK
  EVIDENCE: exit=0; output=PARITY_G10_SAFETY_OK files=32; verified at HEAD 2874e66 on 2026-08-27

- [x] G11: the native source inventory covers the observed reference labels, handlers, settings pages, and official request families
  CHECK: node scripts/verify-native-surface.mjs
  EXPECT: PARITY_100_NATIVE_SURFACE_OK
  EVIDENCE: exit=0; output=PARITY_100_NATIVE_SURFACE_OK labels=94 handlers=26 methods=36 settings=34; verified at HEAD 2874e66 on 2026-08-27
