# Controller And Recovery

IO is not production-mainnet-ready. This document defines the intended controller posture for the current canister set and records open governance questions before real ledger/NNS/SNS integrations.

## Controller Model

Pre-SNS launch:

- `io_stream_manager`: controlled by the IO launch operator set until an IO SNS root handoff is approved. This controller can upgrade the stream manager and must not use that power to add broad production APIs, alter IO economics, or bypass ledger/index-driven classification.
- `io_nns_neuron_manager`: controlled by the operator set that can manage the existing IO NNS neuron until SNS handoff. It must preserve the known 2-year NNS neuron id `6345890886899317159` and controller canister principal `oae4c-3iaaa-aaaar-qb5qq-cai`.
- `io_historian`: controlled by the operator set. It is a public read-model canister and must not become a value-moving authority.
- `frontend`: controlled by the operator set. It can inform users but must not be treated as a source of protocol truth.

Post-SNS launch:

- IO SNS root is expected to become the upgrade/settings authority for IO protocol canisters.
- The SNS governance process should own upgrade proposals, emergency settings changes, and controller rotations.
- Any remaining Jupiter Faucet governance role should be explicit, time-bounded, and documented in the handoff proposal.

> TODO / open question: Finalize whether an emergency committee, SNS root only, or another time-locked mechanism controls emergency upgrades before and after IO SNS launch.

## Local SNS Root Harness

The repository includes a mock/PocketIC SNS root/controller lifecycle harness. It registers IO dapp canisters with a mock SNS root, routes mock governance upgrade proposals into root-approved upgrade intents, executes the actual PocketIC upgrade from the test harness with the mock root as controller, and records the outcome. This validates proposal status, controller authorization, artifact hashes, and stable-state preservation without using mainnet, `dfx`, SNS-W, or the official launch/swap flow.

This local harness is not production SNS root/governance wiring. Production fallback controllers, emergency handoff, and official proposal payloads remain launch work.

## Permission Expectations

Controllers may:

- propose and execute reviewed upgrades;
- rotate controllers through the approved governance process;
- recover from failed upgrades using the exact artifact/hash process in `docs/operations/emergency-runbook.md`;
- pause future integrations only through a documented mechanism once one exists.

Controllers must not:

- deploy unreviewed Wasm;
- change production canister IDs, install args, or mainnet wiring without explicit approval;
- introduce production methods such as `get_state`, `redeem`, `tick`, `debug_*`, or event dumps on value-moving canisters;
- run install/upgrade/settings commands against `--network ic` during normal development.

## Emergency Upgrade Process

1. Confirm the incident class and affected canister.
2. Stop non-essential automation that could submit repeated upgrade proposals or builds.
3. Build artifacts with `cargo run -p xtask -- build_canisters`.
4. Verify artifacts with `cargo run -p xtask -- verify_artifacts`.
5. Verify release readiness with `cargo run -p xtask -- verify_release`.
6. Compare raw and gzipped SHA-256 values against `release-artifacts/manifest.json` and the proposal payload.
7. Use the approved governance path to upgrade. This repo does not perform the deployment.

> TODO / open question: Define a production pause mechanism. Current production DIDs are install-args-only, so there is no public pause/open method.

## Failure Recovery

If `io_stream_manager` stalls:

- Inspect ledger/index lag and scheduler cursor state in debug/local environments.
- Confirm no broad production API was added to force stream classification.
- Recover stuck pending journal entries through a reviewed upgrade that preserves journal ids and transfer status.

If `io_nns_neuron_manager` stalls:

- Inspect pending maturity/unwind journal entries in debug/local environments.
- Confirm downstream ICP transfer state before retrying.
- Avoid double-disbursing maturity or principal. A completed downstream block must not be repeated.

If `io_historian` is wrong:

- Treat it as a read-model divergence unless value-moving canister state or ledger source data is also wrong.
- Rebuild historian state from source ledgers/governance data where possible.
- Do not add query APIs to value-moving canisters to patch historian gaps.
- Confirm public histories remain bounded/paginated and debug ingestion remains absent from the production DID.

If `frontend` is compromised:

- Remove or replace the frontend artifact through the controller/governance process.
- Warn users to verify ledger/governance facts from canonical sources.
- Do not treat frontend text as protocol state.

If an upgrade fails:

- Compare the installed module hash with the intended artifact hash.
- Rebuild and verify artifacts from the same commit.
- Prepare a rollback or forward-fix proposal using reviewed artifacts.
- Preserve stable state and journal compatibility.

If pending journal operations are stuck:

- Identify the operation kind, phase, retry count, downstream block, and last error in debug/local state.
- Verify downstream ledger effects before changing retry logic.
- Use the smallest upgrade that resumes the pending phase without marking incomplete work complete.

If an artifact/hash mismatch is found:

- Stop the release.
- Rebuild with `cargo run -p xtask -- build_canisters`.
- Rerun `cargo run -p xtask -- verify_artifacts`.
- Compare `release-artifacts/manifest.json` and SHA sidecars between builders.

## Governance Handoff

The handoff path from Jupiter Faucet governance to IO SNS governance should include:

- exact canister list: `io_stream_manager`, `io_nns_neuron_manager`, `io_historian`, `frontend`;
- controller principal before and after handoff;
- artifact hashes for the handoff upgrade, if any;
- install args and known live principal checks;
- emergency rollback expectations;
- confirmation that value-moving production DIDs remain `service : (InitArgs) -> {}`.
- confirmation that frontend reads historian state and does not treat frontend text as protocol truth.

> TODO / open question: Define the final governance proposal templates and who signs off on real ledger/NNS/SNS principal values.
