# Dependency And Supply Chain

## Cargo.lock Policy

`Cargo.lock` is authoritative for this workspace. Dependency changes should be narrow, reviewed, and justified by the task. Tooling should run against the locked graph rather than floating versions.

## Security Scanning

The baseline command is:

```bash
cargo run -p xtask -- security_scan
```

The strict command is:

```bash
cargo run -p xtask -- security_scan_required
```

`tools/scripts/security-scan` runs the locally available checks:

- `cargo deny check`
- `cargo audit`
- `cargo tree -d`

Permissive mode reports missing optional tools. Required mode fails if `cargo-deny` or `cargo-audit` is missing, or if any configured check fails.

## cargo-deny Baseline

`deny.toml` starts with:

- advisories as hard failures, with yanked crates reported;
- unknown registries and unknown git sources denied;
- common Rust ecosystem licenses allowed;
- duplicate dependency versions reported as warnings while the graph is reviewed.

Duplicate versions should be reduced when doing so is low-risk. They should not be flattened through broad upgrades during unrelated work.

## Release Artifact Provenance

Release artifacts live under `release-artifacts/`:

```text
<canister>.wasm
<canister>.wasm.gz
<canister>.wasm.sha256
<canister>.wasm.gz.sha256
manifest.json
```

`cargo run -p xtask -- verify_artifacts` checks the SHA sidecars, manifest content, byte sizes, and stale release files. The manifest records the git commit if available but does not include a build timestamp.

## Build Host And Tool Assumptions

The repository uses the Rust version from `rust-toolchain.toml`, the `wasm32-unknown-unknown` target, `gzip -n` for deterministic gzip metadata, `sha256sum`, and `icp-cli` configuration. It intentionally does not use `dfx`.

Live PocketIC tests require `POCKET_IC_BIN`. The known local path used in development examples is:

```bash
POCKET_IC_BIN=/home/codexdev/.local/bin/pocket-ic-server
```

No production deployment should depend on mock canisters or debug APIs.
