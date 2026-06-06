# Governance Client Boundaries

IO defines production-shaped governance records and client traits in `crates/io_governance_types`.

## NNS Boundary

The NNS boundary models neuron identity, controller, stake, maturity, dissolve state, command results, and explicit governance errors. The client trait covers the operations IO expects to need:

- read a neuron;
- disburse maturity;
- split a neuron;
- start dissolving;
- stop dissolving;
- disburse principal from a dissolved neuron.

The crate also contains production-shaped Candid records for the NNS governance read and lifecycle-command shapes IO expects to need: `list_neurons`, proposal listing records, `manage_neuron` requests, and command responses for split, dissolve configuration, start/stop dissolving, merge, merge maturity, stake maturity, disburse maturity, disburse principal, and refresh voting power. Fixture tests encode/decode those official-shaped records and map successful/error command responses into IO command results. Pagination fields are modelled explicitly with NNS page number/page size fields; no scheduler checkpoint is advanced by this boundary.

`NnsGovernanceCanisterClient` is a production-shaped adapter behind `NnsGovernanceClient`. Its host build returns explicit `Unsupported` errors; its Wasm path uses bounded governance canister calls and production-shaped DTO decoding. The adapter is not audited, not wired into default production execution, and is not exercised against live NNS governance in tests.

`io_nns_neuron_manager::clients::nns_governance::MockNnsGovernanceClient` implements the boundary for PocketIC/debug tests by calling the mock governance canister `debug_*` methods. That debug dependency is intentionally isolated to the mock adapter.

## SNS Boundary

The SNS boundary models neurons, permissions, dissolve states, proposals, ballots, votes, eligibility snapshots, and participation summaries. The client trait covers:

- paginated neuron listing;
- single-neuron reads;
- paginated proposal listing;
- single-proposal reads.

The crate contains production-shaped SNS governance records for `list_neurons`, `get_neuron`, `list_proposals`, `get_proposal`, governance-level errors, neuron permissions, dissolve state, followees, ballots, topics, proposal timestamps, reward eligibility, and pagination. Mapping tests feed official-shaped SNS neuron/proposal records into the existing eligibility and participation models without changing exclusion or reward policy. Pagination helpers reject duplicate IDs and non-progressing cursors.

`SnsGovernanceCanisterClient` is a production-shaped adapter behind `SnsGovernanceClient`. Its host build returns explicit `Unsupported` errors; its Wasm path uses bounded governance canister calls and production-shaped DTO decoding. The adapter is not audited, not wired into default production execution, and is not exercised against live SNS governance in tests.

`io_stream_manager::clients::sns_governance::MockSnsGovernanceClient` implements the boundary for mock neuron and proposal pages. The mock SNS governance canister stores production-shaped `SnsNeuron` and `SnsProposal` records, exposes debug-only page/get methods, and supports deterministic proposal pagination with `before_proposal` cursors.

`io_stream_manager::governance_snapshot` fetches all local/mock SNS governance pages through the trait, applies SNS eligibility and participation policies, and converts valid eight-byte local/mock SNS neuron IDs into `NeuronSnapshot` values. Invalid SNS neuron IDs are excluded and surfaced as conversion errors rather than coerced to `0`.

The local SNS harness can install IO canisters with SNS-shaped local governance principals, includes read-only PocketIC governance read tests, and combines those snapshots with local SNS-ledger-shaped reward transfers. We currently run SNS-shaped mock/PocketIC tests; they are not official SNS launch tests, not SNS-W, not decentralization swap, and not mainnet testflight. Official `sns-testing` and `dfx sns` flows are optional reference material and are not part of required IO workflows.

## Limitations

No tests call live NNS or live SNS governance canisters. The production-shaped adapters are fixture-tested only and remain unwired from default production execution. Audited retry policy, final canister principal wiring, production lifecycle reconciliation, and live-governance rollout remain future work. Production DIDs for value-moving canisters remain constructor-only.
