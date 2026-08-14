# Simplified production wiring

Production wiring remains a non-runnable plan. Stream reserve and liquid Accounts must be owned by the installed stream manager. NNS staging and fee Accounts must be owned by the installed manager, while the stream liquid destination is owned by the configured stream manager.

The NNS implementation is intended for the existing controller canister `oae4c-3iaaa-aaaar-qb5qq-cai`; `tatch-ciaaa-aaaar-qb7wq-cai` remains unused and inert. Any mainnet audit, installation or controller action requires separate explicit approval.

## Reserved mapping record

- `io_stream_manager` `thset-pqaaa-aaaar-qb7wa-cai`
- `io_nns_neuron_manager` `tatch-ciaaa-aaaar-qb7wq-cai` — reserved only, not the planned neuron authority
- `io_historian` `tjqj3-uaaaa-aaaar-qb7xa-cai`
- `frontend` `torpp-zyaaa-aaaar-qb7xq-cai`

This remains dry-run/config validation only. No production execution is active; IO protocol remains not live; SNS IO ledger is not launched. Production activation is a later audited milestone. Required workflows use `icp-cli` convention and required workflows do not use `dfx`. The IO_TEST ledger is non-canonical. These are planned wiring placeholders only: ReservedNotLive, reserved, empty/inert, not live, with no value-moving Wasm installed. No production activation has happened and no IO issuance/redemption is enabled.

## Production Wiring Checklist

Confirm the protected controller `oae4c-3iaaa-aaaar-qb5qq-cai` and neuron `6345890886899317159` are never mutation targets without explicit approval.
