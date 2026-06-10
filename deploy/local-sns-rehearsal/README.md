# IO Official Local SNS Rehearsal

This package is local-only. It provides rehearsal scaffolding and evidence validation for creating a real SNS-created IO ledger/index/governance/root stack in a local rehearsal environment.

It is not production launch configuration, not final tokenomics, not a mainnet SNS proposal, not required CI, and not proof that IO is live.

Do not use `--network ic`. Do not call mainnet. Do not touch `oae4c-3iaaa-aaaar-qb5qq-cai` or IO neuron `6345890886899317159`.

## Files

- `sns_init.local.yaml`: local/dev candidate config for official SNS tooling.
- `canister-ids.local.example.toml`: template for recording local SNS canister IDs and manually observed ledger evidence.
- `scripts/check-prereqs.sh`: no-network prerequisite and guardrail check.
- `scripts/render-local-wiring.sh`: renders local dry-run wiring from a completed local evidence file.

`canister-ids.local.toml` is the operator-filled local evidence file and should not be treated as production config.

## Official Local Flow

Follow the current official ICP/DFINITY SNS testing documentation as the source of truth for local NNS plus SNS-W setup. The historical standalone `dfinity/sns-testing` repository is deprecated; if the official docs reference successor tooling or a new repository/location, use that current official location.

The package currently provides a local `sns_init` candidate, a local evidence template, no-network validators, and a manual runbook. It does not itself prove IO against a real SNS ledger until an operator completes the local rehearsal, records `canister-ids.local.toml`, and validates that evidence.

Manual sequence:

1. Deploy IO dapp canisters locally.
2. Add local NNS root as co-controller where required by the local SNS launch tooling.
3. Validate `sns_init.local.yaml` with local SNS tooling. This may require `dfx sns`, but that remains optional/manual and local-only.
4. Submit the local SNS proposal.
5. Let SNS-W deploy local SNS root, governance, ledger, index, swap, and archive canisters.
6. Record those local IDs in `canister-ids.local.toml`.
7. Verify the SNS ledger exists and exposes ICRC methods.
8. Verify total supply, reserve balance, fee, bad-fee, insufficient-funds, duplicate, and account-history behavior.
9. Verify SNS governance/root availability.
10. Test an SNS-governance-controlled dapp upgrade proposal if the local tooling supports it.

## Repository Validators

These validators do not call canisters and do not require `dfx`:

```bash
cargo run -p xtask -- validate_local_sns_rehearsal
cargo run -p xtask -- validate_local_sns_ledger
```

`validate_local_sns_rehearsal` checks the package structure and local-only guardrails.

`validate_local_sns_ledger` checks the optional local evidence file. If `canister-ids.local.toml` is absent, it skips clearly. If present, it rejects placeholders, known mainnet system canister IDs in local SNS/app wiring, protected IO IDs used as local SNS/app wiring, and missing evidence fields. The protected IDs may appear only as explicit "must not touch" reminders.

Until `canister-ids.local.toml` is produced from a completed local rehearsal, no local SNS canister IDs are recorded and no real SNS ledger/index/governance/root behavior has been observed.

## Issuance Model Under Test

IO issuance is modelled as reserve transfer, not arbitrary minting:

- reserve-to-user transfer for issuance;
- user-to-reserve transfer for redemption return;
- constant SNS ledger total supply during those flows.

The protocol reserve account/subaccount must be funded at SNS genesis in the local config. Any minting-based assumption is a blocker unless a later audited launch decision explicitly changes this model.
