# SNS Testing Layers

IO has four SNS-related test layers. They are intentionally separate because each proves a different claim.

## Layer 1: Mock and Local Ledger/Index/Governance Tests

This layer uses repository-owned mock canisters and host fixtures.

Examples:

- `cargo run -p xtask -- sns_governance_read_tests`
- `cargo run -p xtask -- sns_ledger_index_tests`
- `cargo run -p xtask -- sns_root_lifecycle_tests`
- unit tests in `io-ledger-types`, `io-governance-types`, `io-stream-manager`, and `io-nns-neuron-manager`
- mock canisters under `tests/mocks/mock_io_ledger`, `tests/mocks/mock_io_index`, `tests/mocks/mock_sns_governance`, and `tests/mocks/mock_sns_root`

What it proves:

- IO account/subaccount conversion, fee representation, transfer error mapping, duplicate proof checks, and index cursor handling are modelled at the boundary.
- IO scheduler and journal behavior can process SNS-shaped ledger/index pages.
- SNS governance and root records can be decoded or mocked into IO policy types.

What it does not prove:

- It does not use SNS-W.
- It does not create real SNS canisters.
- It does not prove official SNS ledger, index, governance, root, swap, or archive behavior.
- It does not prove IO launch readiness.

## Layer 2: PocketIC SNS-Shaped and Topology Tests

This layer uses PocketIC where supported by the pinned dependency and IO mock canisters.

Examples:

- `POCKET_IC_BIN=/home/codexdev/.local/bin/pocket-ic-server cargo run -p xtask -- sns_governance_read_required`
- `POCKET_IC_BIN=/home/codexdev/.local/bin/pocket-ic-server cargo run -p xtask -- sns_ledger_index_required`
- `POCKET_IC_BIN=/home/codexdev/.local/bin/pocket-ic-server cargo run -p xtask -- sns_root_lifecycle_required`
- `POCKET_IC_BIN=/home/codexdev/.local/bin/pocket-ic-server cargo run -p xtask -- sns_pocketic_required`

What it proves:

- IO canisters accept local SNS-shaped principals in constructor-only production DIDs.
- Mock SNS governance/root/ledger/index canisters can exercise IO value-flow and upgrade-lifecycle paths under PocketIC.
- NNS/SNS/application subnet topology assumptions can be smoke-tested locally.

What it does not prove:

- It does not run `dfx sns`.
- It does not run SNS-W.
- It does not create an official SNS ledger stack.
- It does not prove actual SNS ledger/index behavior.

## Layer 3: Official Local SNS Rehearsal

This layer is optional/manual and local-only. It follows the current official ICP/DFINITY SNS testing documentation as the source of truth and may use `dfx sns` to create a real local SNS root, governance, ledger, index, swap, and archive stack from a local `sns_init` candidate. The historical standalone `dfinity/sns-testing` repository is deprecated; if the official docs reference successor tooling or a new repository/location, use that current official location.

Package:

- `deploy/local-sns-rehearsal/README.md`
- `deploy/local-sns-rehearsal/sns_init.local.template.yaml`
- `deploy/local-sns-rehearsal/local-vars.example.toml`
- `deploy/local-sns-rehearsal/canister-ids.local.example.toml`
- `deploy/local-sns-rehearsal/commands.local.example.md`
- `deploy/local-sns-rehearsal/runbook.sh`
- `docs/operations/official-local-sns-rehearsal.md`

What it proves after a completed local run:

- A real SNS-created IO ledger exists locally.
- The SNS ledger exposes the ICRC calls IO plans to use.
- The SNS index can observe account history for the SNS ledger.
- SNS governance/root/swap canisters are available locally.
- IO can record local SNS canister IDs without confusing them for mainnet or IO_TEST values.
- IO issuance can be rehearsed as a protocol reserve transfer rather than arbitrary minting.

What it does not prove:

- Before a completed local run, it does not prove real SNS ledger/index/governance/root behavior.
- It does not prove final tokenomics.
- It does not prove mainnet launch readiness.
- It does not activate production adapters.
- It does not make IO issuance or redemption live.

Current package status:

- Package/scaffolding exists: renderable local `sns_init` template, local variables template, evidence capture helpers, local command templates, no-network validators, and operator runbook.
- Real proof is not completed: no local SNS ledger evidence file is committed, no local SNS canister IDs are recorded, no real SNS ledger/index/governance/root behavior has been observed, and `validate_local_sns_ledger` skips until evidence exists.

Done criteria for this layer are intentionally concrete: official local SNS tooling must run locally; local SNS root/governance/ledger/index/swap IDs must be recorded; ledger fee, total supply, reserve balance, reserve-to-user transfer, user-to-reserve transfer, bad fee, insufficient funds, duplicate proof, and index account history must be observed; governance/root/swap availability and dapp controller state must be checked; and `cargo run -p xtask -- validate_local_sns_ledger` must pass against the filled local evidence file.

## Layer 4: Mainnet SNS Testflight

This layer is future manual/mainnet work and remains incomplete. It is a testflight/mock SNS rehearsal, not the real SNS launch.

What it should prove later:

- Mainnet governance and proposal operations can be rehearsed without launching the real SNS.
- SNS-controlled dapp upgrade workflows and frontend/historian wiring can be checked before final launch execution.

What it does not prove:

- It does not run the final NNS SNS launch proposal.
- It does not run a real swap.
- It does not mean the canonical SNS IO ledger exists on mainnet.

## Current Gap Closed by the Local Rehearsal Package

Before the official local rehearsal package, IO had strong mock/PocketIC coverage but no required artifact describing how to create and validate a real SNS-created local IO ledger/index/governance/root stack. The new package closes the scaffolding and evidence-validation gap while keeping `dfx sns` optional/manual and outside required CI.

IO_TEST ledgers remain non-canonical staging tools. The canonical IO ledger is intended to be the SNS ledger, and that ledger has not launched on mainnet.
