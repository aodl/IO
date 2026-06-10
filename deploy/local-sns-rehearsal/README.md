# IO Official Local SNS Rehearsal

This package is local-only. It provides rehearsal scaffolding and evidence validation for creating a real SNS-created IO ledger/index/governance/root stack in a local rehearsal environment.

It is not production launch configuration, not final tokenomics, not a mainnet SNS proposal, not required CI, and not proof that IO is live.

Do not use `--network ic`. Do not call mainnet. Do not touch `oae4c-3iaaa-aaaar-qb5qq-cai` or IO neuron `6345890886899317159`.

## Files

- `sns_init.local.template.yaml`: local/dev candidate config template for official SNS tooling.
- `local-vars.example.toml`: local variables template for rendering the SNS init file.
- `generated/sns_init.local.yaml`: ignored local render output.
- `canister-ids.local.example.toml`: template for recording local SNS canister IDs and manually observed ledger evidence.
- `commands.local.example.md`: local-only command templates for ledger/index/governance/root evidence.
- `runbook.sh`: single operator entrypoint.
- `scripts/00-check-prereqs.sh`: prerequisite and guardrail check.
- `scripts/01-render-sns-init.sh`: renders `generated/sns_init.local.yaml` from `local-vars.toml`.
- `scripts/02-record-canister-ids.sh`: creates the ignored local evidence draft.
- `scripts/03-capture-ledger-evidence.sh`: prints local evidence capture commands from recorded IDs.
- `scripts/04-render-local-wiring.sh`: renders ignored local dry-run wiring from validated evidence.
- `scripts/05-validate-evidence.sh`: validates completed local evidence.

`canister-ids.local.toml` is the operator-filled local evidence file and should not be treated as production config.

`generated/local-production-wiring.toml` is local helper output only:

- Human-readable local evidence-derived wiring.
- Not accepted by production wiring validators.
- Do not use as install args.

All operator scripts require:

```bash
IO_LOCAL_SNS_REHEARSAL_ACK=local-only
```

They reject mainnet-like arguments, protected IO asset IDs, and `--network ic`/`-n ic` use. The scripts are optional/manual and not required CI.

## Official Local Flow

Follow the current official ICP/DFINITY SNS testing documentation as the source of truth for local NNS plus SNS-W setup. The historical standalone `dfinity/sns-testing` repository is deprecated; if the official docs reference successor tooling or a new repository/location, use that current official location.

The package provides a renderable local `sns_init` candidate, local evidence capture helpers, no-network validators, and an operator runbook. It does not itself prove IO against a real SNS ledger until an operator completes the local rehearsal, records `canister-ids.local.toml`, and validates that evidence.

Manual sequence:

1. Run `IO_LOCAL_SNS_REHEARSAL_ACK=local-only deploy/local-sns-rehearsal/runbook.sh check`.
2. Copy `local-vars.example.toml` to ignored `local-vars.toml` and fill local values once.
3. Run `runbook.sh render-sns-init` to produce ignored `generated/sns_init.local.yaml`.
4. Deploy IO dapp canisters locally.
5. Add local NNS root as co-controller where required by the local SNS launch tooling.
6. Validate the rendered SNS init file with local SNS tooling. This may require `dfx sns`, but that remains optional/manual and local-only.
7. Submit the local SNS proposal through the official local flow.
8. Let SNS-W deploy local SNS root, governance, ledger, index, swap, and archive canisters.
9. Run `runbook.sh record-ids` and record those local IDs in ignored `canister-ids.local.toml`.
10. Run `runbook.sh capture-evidence` and the command templates in `commands.local.example.md`.
11. Verify total supply, reserve balance, fee, bad-fee, insufficient-funds, duplicate, and account-history behavior.
12. Verify SNS governance/root/swap availability and dapp controller state.
13. Test an SNS-governance-controlled dapp upgrade proposal if the local tooling supports it; otherwise record a concrete gap.
14. Run `runbook.sh validate` and `cargo run -p xtask -- validate_local_sns_ledger`.

## Repository Validators

These validators do not call canisters and do not require `dfx`:

```bash
cargo run -p xtask -- validate_local_sns_rehearsal
cargo run -p xtask -- validate_local_sns_ledger
cargo run -p xtask -- validate_local_sns_scripts
```

`validate_local_sns_rehearsal` checks the package structure and local-only guardrails.

`validate_local_sns_ledger` checks the optional local evidence file. If `canister-ids.local.toml` is absent, it skips clearly. If present, it parses the evidence schema, rejects placeholders, known mainnet/prior canister IDs in local SNS/app wiring, protected IO IDs outside explicit reminders, invalid principals, live-protocol claims, minting assumptions, fee/supply mismatches, zero reserve balance, missing duplicate proof, and missing governance upgrade gap.

`validate_local_sns_scripts` copies the operator scripts to a temp directory, writes fixture local variables and completed local evidence, runs the no-network executable paths, and checks positive and negative guardrails. It does not call canisters and does not require `dfx sns`.

Until `canister-ids.local.toml` is produced from a completed local rehearsal, no local SNS canister IDs are recorded and no real SNS ledger/index/governance/root behavior has been observed.

## Issuance Model Under Test

IO issuance is modelled as reserve transfer, not arbitrary minting:

- reserve-to-user transfer for issuance;
- user-to-reserve transfer for redemption return;
- constant SNS ledger total supply during those flows.

The protocol reserve account/subaccount must be funded at SNS genesis in the local config. Any minting-based assumption is a blocker unless a later audited launch decision explicitly changes this model.

## Done Criteria

The local SNS rehearsal is complete only when:

- official local SNS tooling was run locally;
- local SNS root/governance/ledger/index/swap IDs were recorded;
- local SNS ledger fee, total supply, and reserve balance were observed;
- reserve-to-user transfer and user-to-reserve transfer were observed;
- bad fee, insufficient funds, and duplicate behavior were observed;
- duplicate block was verified;
- index account history was observed;
- SNS governance/root/swap availability was observed;
- dapp controller state was checked;
- `cargo run -p xtask -- validate_local_sns_ledger` passes against the filled local evidence file.

This still does not prove mainnet SNS launch readiness, final tokenomics, final SNS config, mainnet testflight, audit readiness, production adapter activation, or that IO is live.
