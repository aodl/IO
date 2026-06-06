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

The crate also contains Candid fixture records for the `manage_neuron` command shapes used by those operations. The fixtures are production-shaped and covered by encode/decode tests, but they are not audited real-client wiring.

`io_nns_neuron_manager::clients::nns_governance::MockNnsGovernanceClient` implements the boundary for PocketIC/debug tests by calling the mock governance canister `debug_*` methods. That debug dependency is intentionally isolated to the mock adapter.

## SNS Boundary

The SNS boundary models neurons, permissions, dissolve states, proposals, ballots, votes, eligibility snapshots, and participation summaries. The client trait covers:

- paginated neuron listing;
- single-neuron reads;
- paginated proposal listing;
- single-proposal reads.

`io_stream_manager::clients::sns_governance::MockSnsGovernanceClient` implements the boundary for mock neuron and proposal pages. The mock SNS governance canister stores production-shaped `SnsNeuron` and `SnsProposal` records, exposes debug-only page/get methods, and supports deterministic proposal pagination with `before_proposal` cursors.

`io_stream_manager::governance_snapshot` fetches all local/mock SNS governance pages through the trait, applies SNS eligibility and participation policies, and converts valid eight-byte local/mock SNS neuron IDs into `NeuronSnapshot` values. Invalid SNS neuron IDs are excluded and surfaced as conversion errors rather than coerced to `0`.

The local SNS harness can install IO canisters with SNS-shaped local governance principals and includes a read-only PocketIC governance read test. Official SNS testing tools are optional reference material and are not part of required IO workflows.

## Limitations

No code in this boundary calls live NNS or live SNS governance canisters. Real NNS/SNS adapters, audited Candid mappings, retry policy, local SNS ledger/index wiring, SNS root/controller lifecycle testing, and final canister principal wiring remain future work. Production DIDs for value-moving canisters remain constructor-only.
