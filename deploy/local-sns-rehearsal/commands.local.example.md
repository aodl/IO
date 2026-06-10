# Local SNS Rehearsal Command Templates

Local-only. These templates are for an operator running an official local SNS rehearsal. Do not use `--network ic`; do not hardcode mainnet canister IDs.

```bash
export IO_LOCAL_SNS_REHEARSAL_ACK=local-only
EVIDENCE="deploy/local-sns-rehearsal/canister-ids.local.toml"
SNS_LEDGER="$(awk -F '=' '/^ledger = / { gsub(/[ \"[:space:]]/, "", $2); print $2; exit }' "$EVIDENCE")"
SNS_INDEX="$(awk -F '=' '/^index = / { gsub(/[ \"[:space:]]/, "", $2); print $2; exit }' "$EVIDENCE")"
SNS_GOVERNANCE="$(awk -F '=' '/^governance = / { gsub(/[ \"[:space:]]/, "", $2); print $2; exit }' "$EVIDENCE")"
SNS_ROOT="$(awk -F '=' '/^root = / { gsub(/[ \"[:space:]]/, "", $2); print $2; exit }' "$EVIDENCE")"
RESERVE_OWNER="$(awk -F '=' '/^protocol_reserve_account_owner = / { gsub(/[ \"[:space:]]/, "", $2); print $2; exit }' "$EVIDENCE")"
```

## Ledger Reads

```bash
dfx canister call --network local "$SNS_LEDGER" icrc1_symbol "()"
dfx canister call --network local "$SNS_LEDGER" icrc1_fee "()"
dfx canister call --network local "$SNS_LEDGER" icrc1_total_supply "()"
dfx canister call --network local "$SNS_LEDGER" icrc1_balance_of "(record { owner = principal \"$RESERVE_OWNER\"; subaccount = null })"
```

## Transfer Observations

Use locally controlled accounts from the official local SNS rehearsal. Record block indexes and errors in `canister-ids.local.toml`.

```bash
# reserve-to-user issuance rehearsal
dfx canister call --network local "$SNS_LEDGER" icrc1_transfer '(record { to = record { owner = principal "TODO_LOCAL_USER_PRINCIPAL"; subaccount = null }; amount = 100000000 : nat; fee = opt (10000 : nat); memo = opt blob "IO local issuance rehearsal"; from_subaccount = null; created_at_time = opt (TODO_LOCAL_CREATED_AT_TIME_NANOS : nat64) })'

# user-to-reserve redemption return rehearsal
dfx canister call --network local "$SNS_LEDGER" icrc1_transfer '(record { to = record { owner = principal "TODO_LOCAL_PROTOCOL_RESERVE_OWNER"; subaccount = null }; amount = 100000000 : nat; fee = opt (10000 : nat); memo = opt blob "IO local redemption return"; from_subaccount = null; created_at_time = opt (TODO_LOCAL_CREATED_AT_TIME_NANOS : nat64) })'

# bad-fee transfer
dfx canister call --network local "$SNS_LEDGER" icrc1_transfer '(record { to = record { owner = principal "TODO_LOCAL_USER_PRINCIPAL"; subaccount = null }; amount = 100000000 : nat; fee = opt (1 : nat); memo = opt blob "IO local bad fee"; from_subaccount = null; created_at_time = opt (TODO_LOCAL_CREATED_AT_TIME_NANOS : nat64) })'

# insufficient-funds transfer
dfx canister call --network local "$SNS_LEDGER" icrc1_transfer '(record { to = record { owner = principal "TODO_LOCAL_USER_PRINCIPAL"; subaccount = null }; amount = 999999999999999999 : nat; fee = opt (10000 : nat); memo = opt blob "IO local insufficient funds"; from_subaccount = null; created_at_time = opt (TODO_LOCAL_CREATED_AT_TIME_NANOS : nat64) })'

# duplicate transfer: repeat the same transfer with the same created_at_time and memo
dfx canister call --network local "$SNS_LEDGER" icrc1_transfer '(record { to = record { owner = principal "TODO_LOCAL_USER_PRINCIPAL"; subaccount = null }; amount = 100000000 : nat; fee = opt (10000 : nat); memo = opt blob "IO local duplicate"; from_subaccount = null; created_at_time = opt (TODO_LOCAL_DUPLICATE_CREATED_AT_TIME_NANOS : nat64) })'
```

## Index, Governance, And Root

```bash
dfx canister call --network local "$SNS_INDEX" get_account_transactions '(record { account = record { owner = principal "TODO_LOCAL_PROTOCOL_RESERVE_OWNER"; subaccount = null }; start = null; max_results = 10 : nat })'
dfx canister call --network local "$SNS_GOVERNANCE" get_nervous_system_parameters "()"
dfx canister call --network local "$SNS_ROOT" list_sns_canisters "(record {})"
```

Optional governance-controlled dapp upgrade tests are local-only and depend on the official local SNS tooling available in the operator environment. If not tested, record a concrete `governance_upgrade_gap` in the evidence file.
