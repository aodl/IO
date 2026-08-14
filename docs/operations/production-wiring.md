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
mutation target. Protected IO NNS neuron `10292412127977304661` remains a
protected reference and is never a mutation target. This static plan
authorizes no inspection or mainnet action.
