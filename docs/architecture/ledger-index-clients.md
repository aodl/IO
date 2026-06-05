# Ledger and Index Client Boundaries

IO uses shared production-shaped ledger and index domain types in `io-ledger-types`.
The types model accounts, subaccounts, block indexes, memos, e8s-denominated token amounts, transfer fees, transfer responses, ledger errors, index scan pages, archive-required states, and index lag.

This boundary is intentionally separate from the mock canisters under `tests/mocks/`.
Mock adapters may implement or map into these types, but production-shaped client code must not depend on mock `debug_*` methods.

## Accounts and Amounts

`Account` stores an owner `Principal` plus an optional 32-byte `Subaccount`.
Amounts and fees remain `u128` at the IO boundary so model accounting does not lose precision when real ledger interfaces use Nat-like values.
Legacy ICP transfer arguments are converted only at the adapter edge and reject values that do not fit the ICP ledger's e8s shape.

## Transfers

`LedgerTransferRequest` carries an optional source subaccount, destination account, amount, optional fee, optional memo, and optional created-at timestamp.

`LedgerTransferError` models insufficient funds, bad fee, temporarily unavailable, duplicate transfer, too old, created in future, generic ledger errors, canister call failures, decode failures, and unsupported paths.

Duplicate transfer results are an idempotency signal only after the caller proves that the duplicate block matches the expected amount, destination account, and memo. A duplicate block with mismatched values must not be treated as success.

## ICP Ledger Boundary

The crate contains production-shaped ICP transfer argument and error models. These are fixture-tested for Candid encoding and conversion behavior, but they are not a live audited ICP ledger client. No tests call the real ICP ledger or mainnet.

## ICRC IO/SNS Ledger Boundary

The crate contains ICRC account, transfer argument, Nat conversion, and transfer error mapping models for future IO/SNS ledger work. Fixture tests cover account/subaccount encoding, bad fee, insufficient funds, duplicate transfer, and generic error mapping.

## Index Boundary

`IndexScanRequest` and `IndexScanResult` model account transaction scans with pagination, optional account filters, last-seen block, index tip, and archive-required status.
The stream-manager scheduler has boundary-level cursor tests for empty pages, duplicate blocks, gaps, archive-required pages, and index lag.

The current implementation does not fetch archives. Archive-required and lag states are modelled as explicit retryable scheduler boundary errors so later production adapters cannot silently skip ranges.

## Fee and Dust Policy

Fees are represented explicitly at the ledger boundary.
Current mock-ledger mode still ignores fees for economic state transitions.
Before mainnet, production fee policy must be finalized for ICP payouts, IO transfers, tiny redemptions below payout fee, and tiny deposits whose split or issuance rounds to zero.

## Remaining Production Gaps

- Real ICP ledger/index canister adapters are production-shaped but not audited or wired to mainnet.
- Real IO/SNS ledger/index canister adapters are production-shaped but not audited or wired to mainnet.
- Archive traversal is not implemented.
- Fee policy is represented but not final.
- Production value-moving DIDs remain install-args-only.
