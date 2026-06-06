# IO Official SNS Compatibility Package

This directory contains the official SNS compatibility package for IO. It is separate from the IO-owned mock/PocketIC SNS-shaped harness. The package is not production launch configuration and must not depend on `dfx` in required CI, must not use `--network ic`, and must not call mainnet.

## Four Layers

Layer 1: IO mock/PocketIC SNS-shaped harness.

This is the fast internal safety layer. It uses mock governance/root/ledger/index canisters plus PocketIC where useful. It tests IO-specific ledger/index, governance-read, root/controller, stable-state, and DID assumptions. It does not run official SNS launch, SNS-W, decentralization swap, or mainnet testflight.

Layer 2: PocketIC NNS/SNS/application subnet topology.

This checks canister placement and constructor wiring against NNS, SNS, and application subnet shapes. It is closer to production topology, but still not official SNS launch unless real SNS canisters are installed.

Layer 3: Official local SNS launch rehearsal.

This is optional and heavier. It uses `dfinity/sns-testing` and `dfx sns` to rehearse the official launch mechanics locally. It is outside required CI and requires developer-local tooling.

Layer 4: Mainnet SNS testflight.

This is a future manual/mainnet rehearsal using a mock SNS. It tests governance and upgrade operations after handoff, but it is not the real SNS launch and has no real swap.

## Files

- `sns_init.io.template.yaml`: official-shape IO SNS candidate template with unresolved production decisions marked as placeholders.
- `sns_init.io.local.yaml`: local-only candidate for `sns-testing` rehearsal; all local canister IDs and controllers are placeholders.
- `sns_init.io.testflight.template.yaml`: mainnet testflight planning template; it is not executable by CI.
- `launch-readiness.toml`: machine-checkable readiness checklist.
- `testflight/`: proposal and handoff planning package for the future manual testflight.

The templates intentionally contain placeholder principals because final controllers and canister IDs are not locked.

IO's canonical IO ledger should be the SNS ledger; any IO_TEST ledger is non-canonical and only useful for local/mock compatibility tests.

The existing canister that owns IO NNS neuron 6345890886899317159 is not touched by these templates, scripts, or tests.

Validate the package without `dfx`:

```bash
cargo run -p xtask -- sns_config_validate
cargo run -p xtask -- sns_official_testing_check
cargo run -p xtask -- sns_launch_readiness_check
```

Optional official validation is opt-in and skips by default:

```bash
IO_RUN_DFX_SNS_VALIDATE=1 cargo run -p xtask -- sns_config_validate_official
```
