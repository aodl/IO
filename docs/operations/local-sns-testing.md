# Local SNS Testing

IO uses local SNS testing as an additional compatibility layer. It does not replace the existing accounting, journal, retry, artifact, or DID guardrails.

## Strategy

1. Pure model tests remain the main accounting guardrail.
2. Mock and PocketIC tests remain the main journal, retry, and upgrade guardrail.
3. Local SNS harness tests provide SNS topology, config, and controller compatibility.
4. Local SNS governance read tests exercise mock-backed SNS neuron and proposal pages through the `SnsGovernanceClient` boundary.
5. Later milestones will wire local SNS ledger and index flows.
6. Later milestones will test SNS root and controller upgrade lifecycle.

The local SNS harness is not production launch configuration. It must not call mainnet, must not use `--network ic`, and must not deploy, install, upgrade, reinstall, or update settings on mainnet.

## Official SNS Testing Flow

The Internet Computer ecosystem has official SNS local testing flows based on SNS launch configuration files, commonly represented as `sns_init.yaml`-style inputs. Those flows are useful reference material for future launch validation and local compatibility work.

Some official SNS steps require the `dfx sns` extension. There is no `icp-cli` equivalent for every operation, including `add-nns-root` and some SNS init validation paths. IO therefore does not add `dfx` to required workflows.

Any `dfx`-based SNS testing for IO is optional, local-only, and not part of `test_ci` or `verify_release`. It must remain outside required CI and release commands unless the repository intentionally adopts a reviewed replacement workflow.

## IO-Owned PocketIC SNS Harness

The IO-owned harness uses PocketIC where practical and stays inside the repository's normal Rust and xtask workflow. Required checks do not require `dfx` and do not call mainnet.

The harness includes topology, config, and read-only governance smoke tests:

- load and validate the local SNS fixture skeleton;
- install IO canisters with SNS-shaped constructor principals in PocketIC;
- preserve production value-moving DIDs as constructor-only;
- keep debug APIs confined to debug DIDs and debug Wasm tests;
- prove required scripts do not invoke `dfx` or `--network ic`.
- seed mock SNS governance neurons/proposals in PocketIC;
- read paginated governance records through a `SnsGovernanceClient` implementation;
- convert governance snapshots into TwoWeekMaturity allocation inputs without SNS ledger/index value movement.

Future milestones can extend this harness into local SNS ledger/index integration and SNS root/controller lifecycle tests after the required artifacts and launch process are established.
