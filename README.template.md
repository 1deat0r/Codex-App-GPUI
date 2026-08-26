# Codex App GPUI

Native GPU-accelerated Codex desktop client built with Rust and GPUI.

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

The native client can use fixture data for offline development and can be pointed at a local Codex app-server with `CODEX_APP_SERVER_COMMAND` or `CODEX_APP_SERVER_URL`.

## Verification

```sh
cargo check --locked
cargo test --locked
node scripts/verify-parity.mjs
```

The full acceptance contract is in [GATES.md](GATES.md), and the living feature ledger is in [PARITY.md](PARITY.md). The pre-commit hook regenerates this README and [docs/PARITY-STATUS.md](docs/PARITY-STATUS.md), then synchronizes the GitHub repository description when an authenticated `gh` remote is configured.

## License

{{LICENSE}}
