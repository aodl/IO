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

`io_stream_manager::clients::sns_governance::MockSnsGovernanceClient` implements the boundary for mock neuron pages. Proposal reads remain unsupported in that adapter until the mock proposal canister exposes production-shaped proposal records.

The local SNS harness can install IO canisters with SNS-shaped local governance principals, but it does not read local SNS governance canisters yet. Official SNS testing tools are optional reference material and are not part of required IO workflows.

## Limitations

No code in this boundary calls live NNS or live SNS governance canisters. Real NNS/SNS adapters, audited Candid mappings, pagination, retry policy, local SNS governance reads, and final canister principal wiring remain future work. Production DIDs for value-moving canisters remain constructor-only.
