# IO Official SNS Compatibility Package

This directory contains the official SNS compatibility package for IO. It is separate from the IO-owned mock/PocketIC SNS-shaped harness. The package is not production launch configuration and must not depend on `dfx` in required CI, must not use `--network ic`, and must not call mainnet.

## Four Layers

Layer 1: Local fixture IO mock/PocketIC SNS-shaped harness.

This is the fast internal safety layer. It uses mock governance/root/ledger/index canisters plus PocketIC where useful. It tests IO-specific ledger/index, governance-read, root/controller, stable-state, and DID assumptions. It does not run official SNS launch, SNS-W, decentralization swap, or mainnet testflight.

Layer 2: Local fixture PocketIC NNS/SNS/application subnet topology.

This checks canister placement and constructor wiring against NNS, SNS, and
application subnet shapes. It remains local fixture evidence unless the
specific profile supplies checksum-pinned real canisters.

Layer 3: Candidate-upstream local SNS launch rehearsal.

This is optional and heavier. It follows the current official ICP/DFINITY SNS
testing documentation and uses the maintained source-built `sns-testing-init`,
`sns-testing`, and `sns` flow. The historical standalone
`dfinity/sns-testing` repository is deprecated. This layer is outside required
CI and requires developer-local tooling.

The concrete IO package for this layer lives under
`deploy/local-sns-rehearsal/`. It is local-only and provides scaffolding and
evidence validation for creating an SNS-created IO
ledger/index/governance/root stack without claiming mainnet readiness.
Historical packages remain bound to their own releases; the explicit
`current-canonical.toml` selector is the source of truth for the selected
release/package identity. A source-built candidate proves compatibility but is
not an official capability-bearing SNS release or production configuration.

Layer 4: Production-configuration mainnet SNS testflight.

This is a future manual/mainnet rehearsal using a mock SNS. It tests governance and upgrade operations after handoff, but it is not the real SNS launch and has no real swap.

## Files

- `sns_init.io.template.yaml`: official-shape IO SNS candidate template with unresolved production decisions marked as placeholders.
- `sns_init.io.local.yaml`: local-only candidate for official local SNS rehearsal; all local canister IDs and controllers are placeholders.
- `sns_init.io.testflight.template.yaml`: mainnet testflight planning template; it is not executable by CI.
- `launch-readiness.toml`: machine-checkable readiness checklist.
- `testflight/`: proposal and handoff planning package for the future manual testflight.

The templates intentionally contain placeholder principals because final controllers and canister IDs are not locked.
Their Governance maximum dissolve delay is nevertheless fixed at the reviewed
1,296,060-second IO user-neuron duration so the local, testflight-planning, and
production-shape inputs cannot cap an eligible neuron at the separate 14-day
NNS Dynamic-neuron duration.

IO's canonical IO ledger should be the SNS ledger; any IO_TEST ledger is non-canonical and only useful for local/mock compatibility tests.

NNS Manager execution canister `oae4c-3iaaa-aaaar-qb5qq-cai` and protected IO
NNS neuron `10292412127977304661` are not touched by these templates, scripts,
or tests.

Validate the package without `dfx`:

```bash
cargo run -p xtask -- sns_config_validate
cargo run -p xtask -- sns_official_testing_check
cargo run -p xtask -- sns_launch_readiness_check
cargo run -p xtask -- validate_local_sns_rehearsal
```

Optional official validation is opt-in and skips by default:

```bash
IO_RUN_SOURCE_BUILT_SNS_VALIDATE=1 cargo run -p xtask -- sns_config_validate_official
cargo run -p xtask -- validate_local_sns_ledger
```
