# Codex App GPUI

Native GPU-accelerated Codex desktop client built with Rust and GPUI.

Public repository: [github.com/1deat0r/Codex-App-GPUI](https://github.com/1deat0r/Codex-App-GPUI)

This project is an independent native client for the Codex app-server protocol. It is being developed against the locally observed Codex Desktop reference and keeps the parity claim evidence-based: a surface is not called complete until it is implemented and verified.

## Current status

{{PARITY_SUMMARY}}

{{GATE_SUMMARY}}

## Product surface

{{FEATURE_GRID}}

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

## Verification

```sh
cargo check --locked
cargo test --locked
node scripts/verify-parity.mjs
```

The full acceptance contract is in [GATES.md](GATES.md), and the living feature ledger is in [PARITY.md](PARITY.md). The pre-commit hook regenerates this README and [docs/PARITY-STATUS.md](docs/PARITY-STATUS.md), then synchronizes the GitHub repository description when an authenticated `gh` remote is configured.

## License

{{LICENSE}}
