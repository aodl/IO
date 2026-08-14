# PocketIC development

PocketIC validation is local-only. Set the pinned server explicitly:

```bash
export POCKET_IC_BIN=/home/codexdev/.local/bin/pocket-ic-server
```

Use `cargo run -p xtask -- test_pocketic_integration` for the repository integration layer and `cargo run -p xtask -- test_pocketic_required` when a missing server must fail. Do not run multiple debug-canister builders concurrently because they share `debug-artifacts/`.

The simplified installed redemption path is exercised with the pinned real SNS ledger as IO, PocketIC's official ICP ledger, and the installed stream-manager Wasm:

```bash
IO_REAL_SNS_WASM_DIR=.real-canister-wasms \
IO_REAL_SNS_WASM_MANIFEST=tests/e2e_real_canisters/wasms.local.toml \
tools/scripts/run-io-stream-manager-live-pocketic
```

That driver uses the module-qualified exact Rust test name, rebuilds the debug Wasm, scopes server cleanup to its run ID and retries only transport failures. The test proves direct ICRC-2 pull, separate payout and commit, Paused upgrades, canonical replay, official ICP `query_blocks` receipt proof and completed Jupiter reserve settlement.

Mocks remain useful for bounded boundary failures, but mock history is never monetary proof. Pinned real-source tests are opt-in and never download artifacts or call mainnet. Production execution has no timer; permissionless `resume` advances one persisted typed phase per call.
