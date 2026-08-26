# Gates: Codex App GPUI parity

OWNS: Cargo.toml, Cargo.lock, src/**, scripts/**, assets/**, docs/**, GATES.md, PARITY.md, README.md

Scope: deliver a native GPUI Codex desktop client whose shell, state model, protocol boundary, persistence, and primary interaction paths are measurable against the local Codex Desktop reference.

- [ ] G1: the native GPUI project compiles from a clean checkout
  CHECK: node scripts/run-cargo.mjs check --locked
  EXPECT: PARITY_G1_BUILD_OK
  EVIDENCE: pending

- [ ] G2: domain state, reducer, protocol, persistence, and UI helper tests pass
  CHECK: node scripts/run-cargo.mjs test --locked
  EXPECT: PARITY_G2_TESTS_OK
  EVIDENCE: pending

- [ ] G3: the parity ledger is internally consistent and every required reference avenue has an implementation owner
  CHECK: node scripts/verify-parity.mjs
  EXPECT: PARITY_G3_LEDGER_OK
  EVIDENCE: pending

- [ ] G4: the app-server JSONL adapter round-trips the supported initialize, thread, turn, event, and approval contracts against an offline fixture server
  CHECK: node scripts/verify-protocol.mjs
  EXPECT: PARITY_G4_PROTOCOL_OK
  EVIDENCE: pending

- [ ] G5: configuration and transcript persistence survive a write/read/reopen cycle without leaking credentials
  CHECK: node scripts/verify-persistence.mjs
  EXPECT: PARITY_G5_PERSISTENCE_OK
  EVIDENCE: pending

- [ ] G6: the executable supports a deterministic headless smoke path and reports the full primary-surface inventory
  CHECK: node scripts/run-cargo.mjs run --locked -- --smoke
  EXPECT: PARITY_G6_SMOKE_OK
  EVIDENCE: pending

- [ ] G7: the native window renders the Codex shell at desktop dimensions with the reference navigation, thread, composer, account, and project surfaces visible and usable
  EVIDENCE: pending

- [ ] G8: primary user flows work end to end in the native window: new task, task selection/search, message send/stop, model/reasoning selection, attachment/mention affordances, approval decision, archive/delete, share, and settings navigation
  EVIDENCE: pending

- [ ] G9: the client can attach to a real locally installed Codex app-server and display a live thread without replacing or mutating the reference app's user data
  EVIDENCE: pending

- [ ] G10: repository-owned checks contain no hard-coded credentials, private tokens, or destructive data commands
  CHECK: node scripts/check-safety.mjs
  EXPECT: PARITY_G10_SAFETY_OK
  EVIDENCE: pending
