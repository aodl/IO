# Emergency Runbook

This runbook describes safe investigation and containment. It does not authorize deployment or mainnet operations.

## Stuck Stream-Manager Journal Operation

- Detection: repeated retry errors, no cursor progress, or pending journal phase in debug/local state.
- Immediate containment: stop release activity and avoid manual reclassification.
- Investigation commands: `cargo run -p xtask -- test_unit`, `cargo run -p xtask -- did_surface`, inspect relevant mock/PocketIC test failure.
- Safe actions: prepare a reviewed retry fix that preserves operation ids and downstream transfer status.
- Unsafe actions: mark incomplete transfers completed or add production `process_stream_event`.
- Escalation: governance proposal expected for production upgrade.

## Stuck NNS-Manager Disbursement

- Detection: pending maturity/unwind operation with failed ICP transfer status.
- Immediate containment: verify downstream transfer did not already land.
- Investigation commands: run NNS unit/PocketIC tests and inspect journal phase.
- Safe actions: retry from the recorded phase after confirming idempotency.
- Unsafe actions: disburse maturity twice or change the 2-year neuron id.
- Escalation: governance proposal with artifact hash comparison.

## Failed Upgrade

- Detection: upgrade rejection, trap, or post-upgrade behavior mismatch.
- Immediate containment: stop further proposals for the same canister.
- Investigation commands: `cargo run -p xtask -- build_canisters`, `cargo run -p xtask -- verify_artifacts`, `cargo run -p xtask -- sns_root_lifecycle_tests`, `cargo run -p xtask -- test_ci`.
- Safe actions: forward-fix or rollback proposal using verified artifacts and manifest-matched raw/gz SHA-256 values.
- Unsafe actions: use unverified Wasm or bypass stable-state compatibility review.
- Escalation: controller/governance incident channel.

## Bad Artifact Generated

- Detection: SHA sidecar mismatch, manifest mismatch, stale artifact, or multi-builder mismatch.
- Immediate containment: stop release.
- Investigation commands: `cargo run -p xtask -- build_canisters`, `cargo run -p xtask -- verify_artifacts`.
- Safe actions: rebuild from a clean checkout and compare manifest.
- Unsafe actions: edit SHA sidecars by hand.
- Escalation: release owner review.

## DID Guardrail Failure

- Detection: `cargo run -p xtask -- did_surface` fails.
- Immediate containment: block release.
- Investigation commands: inspect changed `.did` files and release Wasm exact method strings.
- Safe actions: remove unintended production methods or document an approved architecture exception.
- Unsafe actions: weaken `did_surface` to pass.
- Escalation: architecture/security review.

## Suspected Unbacked Issuance

- Detection: IO issuance without matching authorized ICP flow or backing math mismatch.
- Immediate containment: pause release work and preserve logs/artifacts.
- Investigation commands: targeted accounting tests and e2e tests.
- Safe actions: fix classification/accounting and add regression tests.
- Unsafe actions: patch state without ledger reconciliation.
- Escalation: governance and external audit review.

## Suspected Redemption Double-Pay

- Detection: two ICP payouts for one IO redemption source block.
- Immediate containment: stop retry automation if possible through approved controls.
- Investigation commands: inspect redemption journal phase, payout block, and IO return block.
- Safe actions: preserve completed transfer markers and add idempotency tests.
- Unsafe actions: reset the journal cursor.
- Escalation: governance proposal and public incident report expectation.

## Suspected Cursor Corruption

- Detection: replayed source blocks, skipped blocks, or archive gap.
- Immediate containment: avoid broad rescans until duplicate guards are understood.
- Investigation commands: cursor unit/PocketIC tests and ledger history comparison.
- Safe actions: repair cursor logic while preserving processed transaction ids.
- Unsafe actions: delete processed transaction history.
- Escalation: governance proposal for production.

## Mock-vs-Production Client Confusion

- Detection: production path depends on `debug_*` mock APIs or mock principals.
- Immediate containment: block release.
- Investigation commands: `cargo run -p xtask -- did_surface`, `cargo run -p xtask -- validate_install_args`.
- Safe actions: isolate mock-only code behind debug/test paths and add guardrail tests.
- Unsafe actions: deploy mock client assumptions to mainnet.
- Escalation: security review before real-client work resumes.

## Historian Read-Model Divergence

- Detection: dashboard values disagree with ledger/index/governance/release artifact sources or historian ingestion status is stale.
- Immediate containment: treat the frontend/historian value as suspect and do not alter value-moving canister state to make the dashboard match.
- Investigation commands: `cargo run -p xtask -- historian_tests`, `cargo run -p xtask -- did_surface`, and targeted ledger/index/governance fixture tests for the source being reconstructed.
- Safe actions: correct the observation-to-read-model conversion, ingest corrected local/test fixtures, or rebuild historian state from canonical sources through a reviewed upgrade or future production ingestion path.
- Unsafe actions: add `get_state`, `get_events`, `tick`, `process_stream_event`, `redeem`, or `debug_*` production methods to value-moving canisters to patch a dashboard gap.
- Escalation: architecture/security review if the divergence suggests a canonical ledger, governance, release artifact, or value-moving state issue.
