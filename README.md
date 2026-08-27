# Codex App GPUI

Native GPU-accelerated Codex desktop client built with Rust and GPUI.

Public repository: [github.com/1deat0r/Codex-App-GPUI](https://github.com/1deat0r/Codex-App-GPUI)

This project is an independent native client for the Codex app-server protocol. It is being developed against the locally observed Codex Desktop reference and keeps the parity claim evidence-based: a surface is not called complete until it is implemented and verified.

## Current status

**Parity coverage:** 24/24 avenues verified (100%). 0 implemented, 0 planned, 0 blocked.

**Acceptance gates:** 11/11 checked. See [GATES.md](GATES.md) for runnable and manual evidence.

## Product surface

| Axis | Reference avenue | Status | Owner |
| --- | --- | --- | --- |
| shell-01 | App window and dark/light appearance | verified | `src/ui.rs`, `src/theme.rs` |
| shell-02 | Sidebar navigation | verified | `src/ui.rs` |
| shell-03 | Project/task hierarchy | verified | `src/state.rs`, `src/ui.rs` |
| shell-04 | Search and task navigation | verified | `src/state.rs`, `src/ui.rs` |
| thread-01 | Thread header | verified | `src/ui.rs` |
| thread-02 | Transcript | verified | `src/ui.rs`, `src/model.rs` |
| thread-03 | Turn lifecycle | verified | `src/protocol.rs`, `src/state.rs` |
| thread-04 | Plan and subagents | verified | `src/model.rs`, `src/ui.rs` |
| composer-01 | Composer | verified | `src/ui.rs`, `src/state.rs` |
| composer-02 | Composer controls | verified | `src/ui.rs`, `src/state.rs` |
| composer-03 | Usage and context | verified | `src/state.rs`, `src/ui.rs` |
| exec-01 | App-server connection | verified | `src/protocol.rs` |

[View the complete 24-avenue parity ledger](PARITY.md).

## Run locally

The project uses the stable Rust toolchain and GPUI. From the repository root:

```sh
cargo run
```

For a deterministic no-window check:

```sh
cargo run -- --smoke
```

The native client uses fixture data by default. To attach it to a local Codex app-server, set the command explicitly:

```sh
CODEX_APP_SERVER_COMMAND='codex app-server --stdio' node scripts/run-cargo.mjs run --locked
```

For isolated live-window validation, `CODEX_APP_GPUI_CREATE_LIVE_THREAD=1` asks the client to create an empty server-owned thread when the isolated `CODEX_HOME` has no history. Keep `CODEX_HOME` pointed at a temporary directory for that check so the reference desktop's user data is not changed.

For a deterministic native-flow exercise without network or model usage, use the repository-owned fixture server:

```sh
CODEX_APP_SERVER_COMMAND='node scripts/live-fixture-server.mjs' CODEX_APP_GPUI_CREATE_LIVE_THREAD=1 node scripts/run-cargo.mjs run --locked
```

## Verification

```sh
cargo check --locked
cargo test --locked
node scripts/verify-parity.mjs
```

The full acceptance contract is in [GATES.md](GATES.md), and the living feature ledger is in [PARITY.md](PARITY.md). The pre-commit hook regenerates this README and [docs/PARITY-STATUS.md](docs/PARITY-STATUS.md), then synchronizes the GitHub repository description when an authenticated `gh` remote is configured.

## License

MIT
