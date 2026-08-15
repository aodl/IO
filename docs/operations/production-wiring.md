# Simplified production wiring

Production wiring remains a non-runnable plan. Stream reserve and liquid Accounts must be owned by the installed stream manager. NNS staging and fee Accounts must be owned by the installed manager, while the stream liquid destination is owned by the configured stream manager.

The NNS implementation executes at existing protected-neuron controller
`oae4c-3iaaa-aaaar-qb5qq-cai`. Any mainnet audit, installation, or controller
action requires separate explicit approval.

## Role identity record

- `io_stream_manager` `thset-pqaaa-aaaar-qb7wa-cai`
- `io_nns_neuron_manager` `oae4c-3iaaa-aaaar-qb5qq-cai` — existing protected execution identity, not a reservation or general mutation target
- `io_historian` `tjqj3-uaaaa-aaaar-qb7xa-cai`
- `frontend` `torpp-zyaaa-aaaar-qb7xq-cai`

This remains dry-run/config validation only. No production execution is active;
IO protocol remains not live; SNS IO ledger is not launched. Production
activation is a later audited milestone. Required workflows use `icp-cli`
convention and required workflows do not use `dfx`. The IO_TEST ledger is
non-canonical. The Stream Manager, Historian, and frontend values are planned
reserved placeholders only: `ReservedNotLive`, empty/inert, and not live, with
no value-moving Wasm installed. The NNS Manager entry identifies its existing
protected authority rather than reserving a fourth canister. No production
activation has happened and no IO issuance/redemption is enabled.

## Production Wiring Checklist

The checked-in dry-run target for `io_nns_neuron_manager` is the existing
controller `oae4c-3iaaa-aaaar-qb5qq-cai`. Static validation allows `oae4c`
only in this one field and rejects it as the Stream Manager or a general
mutation target. Two-year protected NNS neuron `10292412127977304661` remains a
protected reference and is never a mutation target. This static plan
authorizes no inspection or mainnet action.

## NNS launch inventory

| Role | Neuron ID | Controller/executor | Expected launch stake | Maturity baseline | Dissolve configuration | Staging/destination |
| --- | --- | --- | --- | --- | --- | --- |
| Two-year protected NNS neuron | `10292412127977304661` | `oae4c-3iaaa-aaaar-qb5qq-cai` | audited `seeded_two_year_principal_e8s`; unresolved in the production template | ordinary and staked maturity zero; no pending disbursement | non-dissolving, approved 252,460,800-second delay, auto-stake off | canonical Mint proof to the Stream Manager liquid ICP Account |
| Two-week reward-backing NNS neuron | unresolved production neuron ID; configured separately | `oae4c-3iaaa-aaaar-qb5qq-cai` | audited `seeded_two_week_principal_e8s`; unresolved in the production template | ordinary and staked maturity zero; no pending disbursement or child ambiguity | non-dissolving, approved 252,460,800-second delay, auto-stake off | NNS Manager self-owned two-week staging Account, then proof-bound Stream receipt |

The Jupiter and two-week staging Accounts are distinct ICP-ledger Accounts
owned by the executing NNS Manager. Their final fee floats and subaccounts,
the Stream liquid destination, and the Jupiter activation block floor remain
explicit launch inputs; local fixture values are not production values.
