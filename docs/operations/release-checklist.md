# Release Checklist

Use this before every release-oriented commit or artifact proposal.

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo check --workspace --all-targets`
- [ ] `cargo test --workspace`
- [ ] `cargo check -p io-stream-manager -p io-nns-neuron-manager --target wasm32-unknown-unknown`
- [ ] `cargo run -p xtask -- did_surface`
- [ ] `cargo run -p xtask -- build_canisters`
- [ ] `cargo run -p xtask -- verify_artifacts`
- [ ] `cargo run -p xtask -- validate_install_args`
- [ ] `cargo run -p xtask -- security_scan_required`
- [ ] `cargo run -p xtask -- sns_harness_check`
- [ ] `cargo run -p xtask -- sns_governance_read_tests`
- [ ] `cargo run -p xtask -- sns_ledger_index_tests`
- [ ] `cargo run -p xtask -- sns_root_lifecycle_tests`
- [ ] `cargo run -p xtask -- sns_pocketic_smoke`
- [ ] `POCKET_IC_BIN=/home/codexdev/.local/bin/pocket-ic-server cargo run -p xtask -- test_pocketic_required`
- [ ] `POCKET_IC_BIN=/home/codexdev/.local/bin/pocket-ic-server cargo run -p xtask -- sns_pocketic_required`
- [ ] `POCKET_IC_BIN=/home/codexdev/.local/bin/pocket-ic-server cargo run -p xtask -- sns_root_lifecycle_required`
- [ ] `POCKET_IC_BIN=/home/codexdev/.local/bin/pocket-ic-server cargo run -p xtask -- test_ci`
- [ ] `cargo run -p xtask -- verify_release`
- [ ] `git diff --check`
- [ ] Review artifact diffs and `release-artifacts/manifest.json`.
- [ ] Confirm no production API expansion on value-moving canisters.
- [ ] Confirm local SNS fixture and mock root lifecycle remain local-only and not production launch config.
- [ ] Confirm upgrade proposal hashes match `release-artifacts/manifest.json`.
- [ ] Confirm no deployment/mainnet calls were made.
