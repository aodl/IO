# Deployment

This repository is not production-mainnet-ready. The current flow builds and verifies artifacts without deploying them.

## Prerequisites

- Rust from `rust-toolchain.toml`
- `wasm32-unknown-unknown` target
- `icp-cli` for local configuration validation
- `POCKET_IC_BIN` for strict PocketIC tests
- `cargo-deny` and `cargo-audit` for strict security scans

Example PocketIC environment:

```bash
export POCKET_IC_BIN=/home/codexdev/.local/bin/pocket-ic-server
```

## Non-Mainnet Flow

```bash
cargo run -p xtask -- build_canisters
cargo run -p xtask -- verify_artifacts
cargo run -p xtask -- did_surface
cargo run -p xtask -- validate_install_args
cargo run -p xtask -- test_ci
```

`build_canisters` writes raw and gzipped Wasm under `release-artifacts/`. `verify_artifacts` checks SHA sidecars, manifest content, byte sizes, and stale files.

## Future Mainnet Flow

Future mainnet work must be proposal-driven and explicitly requested. Normal development must not call mainnet, update settings, install, reinstall, or upgrade canisters on `--network ic`.

Before any future mainnet proposal:

```bash
cargo run -p xtask -- verify_release
tools/scripts/validate-mainnet-install-args
```

Compare module hashes and manifest SHA values between independent builders. Confirm the proposal payload references the intended gzipped artifact and controller target.

## Policies

- No `dfx`.
- No direct mainnet calls in normal development.
- No production API expansion on value-moving canisters.
- No real ledger/NNS/SNS calls until production clients are implemented and audited.
