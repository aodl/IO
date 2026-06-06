# SNS Root Lifecycle

IO has a local SNS root/controller lifecycle harness for upgrade-path testing. It is mock/PocketIC only, does not run the official SNS launch or decentralization swap flow, and does not call mainnet.

## Local Model

The local lifecycle uses:

- `tests/mocks/mock_sns_root`: tracks registered dapp canisters, records expected module hashes, enforces that only the configured mock SNS governance can request upgrades, and records upgrade attempts and outcomes.
- `tests/mocks/mock_sns_governance`: stores proposal-shaped upgrade requests, votes, adopted/rejected decisions, and finalization.
- `crates/io_sns_lifecycle`: shared Candid/debug types plus release-artifact manifest helpers used by tests and `xtask`.

The mock root records an approved upgrade intent. The test harness executes the PocketIC upgrade with the mock SNS root as the local controller, then records the outcome back on the root. This two-layer design avoids adding production management-canister upgrade code to the mock while still exercising controller authorization, proposal status, artifact hashes, and stable-state behavior.

## Proposal Artifact Checks

Upgrade proposals must reference `release-artifacts/manifest.json`. The lifecycle helpers resolve the canister entry and compare:

- raw Wasm path;
- raw Wasm SHA-256;
- gzipped Wasm SHA-256;
- recorded byte sizes for stale-manifest detection;
- git commit metadata when present.

PocketIC lifecycle tests use debug Wasm for local upgrade execution, but proposal payloads still verify the intended release artifact manifest hashes. The mock root also checks the expected debug module hash recorded for the local canister under test.

## Tested Paths

The local tests cover:

- stream-manager upgrade through mock SNS governance/root;
- NNS-neuron-manager upgrade through mock SNS governance/root;
- controller handoff to mock SNS root in PocketIC;
- non-controller upgrade rejection;
- stable pending stream reward retry across root-style upgrade;
- stable pending NNS maturity retry across root-style upgrade;
- rejected, open, wrong-hash, wrong-target, and unauthorized caller paths;
- production DID guardrails remaining constructor-only after the local upgrade tests.

## Relationship to Official SNS

We currently run SNS-shaped mock/PocketIC tests. We do not currently run the official SNS launch locally in required CI.

Official `sns-testing` is optional and heavier. The official SNS launch path uses `dfx sns`; this is not part of required IO workflows. SNS testflight is a future manual/mainnet rehearsal.

Production SNS root/governance wiring remains future work. The harness does not submit live proposals, does not invoke SNS-W, does not run decentralization swap, does not call NNS/SNS mainnet canisters, and does not use `dfx` in required workflows.

Fallback controllers, emergency controller rotation, final SNS root handoff, proposal templates, and official SNS launch validation remain production launch work.
