# Official Local SNS Rehearsal

This runbook describes how to prove IO assumptions against a real SNS-created ledger stack in a local environment. It is optional/manual, local-only, and outside required CI because the official SNS path may require `dfx sns`.

It must not use `--network ic`, must not call mainnet, must not touch `oae4c-3iaaa-aaaar-qb5qq-cai`, and must not touch IO neuron `6345890886899317159`.

## Package

- `deploy/local-sns-rehearsal/README.md`
- `deploy/local-sns-rehearsal/sns_init.local.template.yaml`
- `deploy/local-sns-rehearsal/local-vars.example.toml`
- `deploy/local-sns-rehearsal/canister-ids.local.example.toml`
- `deploy/local-sns-rehearsal/commands.local.example.md`
- `deploy/local-sns-rehearsal/runbook.sh`
- `deploy/local-sns-rehearsal/scripts/00-check-prereqs.sh`
- `deploy/local-sns-rehearsal/scripts/01-render-sns-init.sh`
- `deploy/local-sns-rehearsal/scripts/02-record-canister-ids.sh`
- `deploy/local-sns-rehearsal/scripts/03-capture-ledger-evidence.sh`
- `deploy/local-sns-rehearsal/scripts/04-render-local-wiring.sh`
- `deploy/local-sns-rehearsal/scripts/05-validate-evidence.sh`

The rendered local `generated/sns_init.local.yaml` is not final tokenomics and is not a mainnet SNS proposal. It exists only to create a real local SNS ledger/index/governance/root stack for integration testing.

IO_TEST remains a non-canonical staging ledger label and must not be confused with the real SNS-created local IO ledger created by this rehearsal.

## Current SNS Tooling

Follow the current official ICP/DFINITY SNS testing documentation as the source of truth. The historical standalone `dfinity/sns-testing` repository is deprecated; if the official docs reference successor tooling or a new repository/location, use that current official location.

Local SNS rehearsal may require `dfx sns`. That remains optional/manual, local-only, and outside required CI. Required repository workflows must not depend on `dfx`.

The committed package is executable scaffolding and evidence validation: it includes a renderable local `sns_init` candidate, a local variables template, evidence capture helpers, no-network validators, and this manual runbook. It does not prove IO against a real SNS ledger until an operator completes the local rehearsal, records `deploy/local-sns-rehearsal/canister-ids.local.toml`, and validates that evidence.

## Manual Flow

1. Prepare a clean local SNS testing environment using the current official ICP/DFINITY SNS testing documentation.
2. Run `IO_LOCAL_SNS_REHEARSAL_ACK=local-only deploy/local-sns-rehearsal/runbook.sh check`.
3. Copy `deploy/local-sns-rehearsal/local-vars.example.toml` to ignored `local-vars.toml` and fill only local principals.
4. Run `runbook.sh render-sns-init` to write ignored `generated/sns_init.local.yaml`.
5. Deploy IO app canisters locally.
6. Add local NNS root as co-controller where the official SNS launch tooling requires it.
7. Validate `deploy/local-sns-rehearsal/generated/sns_init.local.yaml` with local SNS tooling.
8. Submit the local SNS proposal through the local SNS testing flow.
9. Let SNS-W deploy local SNS canisters.
10. Run `runbook.sh record-ids` and record root, governance, ledger, index, swap, and archive IDs in ignored `deploy/local-sns-rehearsal/canister-ids.local.toml`.
11. Run `runbook.sh capture-evidence` and the command templates to observe ledger/index/governance/root behavior.
12. Run no-network repository validation:

```bash
cargo run -p xtask -- validate_local_sns_rehearsal
cargo run -p xtask -- validate_local_sns_ledger
```

The second command checks only the recorded local evidence file. It does not call canisters.

If `deploy/local-sns-rehearsal/canister-ids.local.toml` is absent, `validate_local_sns_ledger` skips clearly. In that state no local SNS canister IDs are recorded and no real SNS ledger/index/governance/root behavior has been observed.

## Ledger Assumptions to Prove Manually

Run local canister calls against the local SNS ledger/index principals recorded in `canister-ids.local.toml`:

- `icrc1_fee` returns the fee configured in `sns_init.local.yaml`.
- `icrc1_total_supply` matches the local total supply configuration.
- `icrc1_balance_of` for the protocol reserve account is non-zero and sufficient for rehearsal issuance.
- `icrc1_transfer` supports reserve-to-user transfers using IO's account/subaccount encoding.
- `icrc1_transfer` returns `BadFee` for an intentionally wrong fee.
- `icrc1_transfer` returns `InsufficientFunds` for an unfunded source subaccount.
- Repeating a transfer with the same created-at time/memo produces duplicate behavior that IO can prove against the duplicate block.
- The SNS index `get_account_transactions` endpoint returns the expected reserve/user account history in a stable order for IO cursor handling.
- Index lag or archive-required behavior is either observed and recorded or explicitly marked as future work in the local evidence file.
- SNS governance exposes nervous-system parameters.
- SNS root is available and can report controlled dapp canisters or support the corresponding official local query.
- A governance-controlled dapp upgrade proposal is tested if supported by the local tooling.

## Issuance Model

IO issuance is resolved conservatively as a transfer from a protocol reserve account/subaccount funded at SNS genesis.

Redemption returns IO to the protocol reserve. IO must not assume arbitrary post-launch minting unless final SNS ledger configuration and governance policy explicitly support it and a later audited milestone changes this model.

The local rehearsal must prove:

- the protocol reserve account exists on the SNS ledger;
- the reserve balance is funded at genesis;
- stream-manager local wiring can construct the reserve-to-user transfer intent;
- redemption local wiring can construct the user-to-reserve return intent;
- total supply remains constant across issuance/redemption rehearsal transfers.

## What Remains Unproven

Until a local evidence file is produced from a completed local rehearsal, this package also does not prove local SNS ledger behavior, local SNS index behavior, local SNS governance/root behavior, or SNS-W-created canister IDs.

This rehearsal does not prove final SNS launch readiness, mainnet NNS proposal acceptance, final tokenomics, final fallback controllers, production adapter activation, archive traversal completeness, or external audit readiness.

IO protocol remains not live. The canonical SNS IO ledger remains not launched on mainnet.

## Completion Checklist

The rehearsal is complete only when official local SNS tooling was run locally; local SNS root/governance/ledger/index/swap IDs were recorded; local SNS ledger fee, total supply, and reserve balance were observed; reserve-to-user and user-to-reserve transfers were observed; bad fee, insufficient funds, duplicate behavior, duplicate block proof, and index account history were observed; SNS governance/root/swap availability and dapp controller state were checked; and `cargo run -p xtask -- validate_local_sns_ledger` passes against the filled evidence file.

Passing this local evidence gate still does not prove mainnet SNS launch readiness, final tokenomics, final SNS config, mainnet testflight, audit readiness, or production adapter activation.
