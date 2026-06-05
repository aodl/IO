# io_historian

Placeholder public read-model canister.

## Role

- Public read model.
- Dashboard/query surface.
- Intended to reconstruct state from ledgers/indexes/governance/management-canister observations.

Historian should not depend on broad public query APIs from value-moving canisters. Historian may query observable/public sources and ledgers/indexes.

The current placeholder exposes a small `get_public_status` query and Candid-shaped observation structs. These are deliberately modeled around external observations rather than private core-canister events or broad core-canister query APIs.

Future historian state should be derived from observable sources where possible:

- ICP ledger/index.
- IO/SNS ledger/index.
- NNS governance where possible.
- SNS governance where possible.
- Management canister status where possible.
- Canister metadata, install args, and governance proposal records.
