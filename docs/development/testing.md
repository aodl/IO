# Testing simplified execution

Run focused crate tests before workspace gates. `io-core-model` covers stateless checked economics; `io-reward-policy` owns the unchanged 18 allocation tests. Stream/NNS unit tests cover typed state and idempotency. PocketIC tests install Paused canisters, and ignored real-source tests use pinned local Wasms without network access.

`cargo run -p xtask -- test_all` is the broad local suite. `verify_release` rebuilds validation artifacts, which must be restored rather than committed. Set `POCKET_IC_BIN=/home/codexdev/.local/bin/pocket-ic-server` for required local canister tests. Do not run value-moving PocketIC suites concurrently.

Ordinary CI installs PocketIC under a versioned parent directory while keeping
the executable basename `pocket-ic-server`. It provisions the required `icp`
0.2.7 executable with `tools/scripts/provision-icp-cli`; the local integration
gate requires both `icp project show` and `icp build` to succeed.

Use [SNS framework sources](../testing/sns-framework-sources.md) for the single
official/local/bundle artifact workflow. Unless a task explicitly names the
sibling IC checkout or a prepared bundle, run
`tools/scripts/test-sns-framework --source official`.

The deterministic SNS root/controller compatibility gate is `cargo run -p xtask -- sns_root_lifecycle_tests`. The strict local PocketIC form is `POCKET_IC_BIN=/home/codexdev/.local/bin/pocket-ic-server cargo run -p xtask -- sns_root_lifecycle_required`. This compatibility path does not use `dfx` and is not launch evidence.

Scanner-era tests are historical. Current tests must exercise explicit commands, separate effect invocations, exact proofs and V1 upgrade behavior.
