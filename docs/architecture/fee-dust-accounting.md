# Fee, Dust, and Accounting Policy

This document defines IO monetary accounting in e8s. It is a pre-activation policy for the pure model, stream-manager journal, historian read model, and ledger transfer boundaries. It does not imply IO issuance, redemption, a canonical SNS IO ledger, or production adapters are live.

## Canonical Sources

The stream manager state and its pending operation journal are the protocol accounting source once a production adapter is intentionally activated. Ledger and index canisters prove external transfers. The historian is a rebuildable read model and may show missing, incomplete, retryable, or observed-only fields; it is not protocol truth.

Normal ledger history reads use index canisters. Raw ledger/archive traversal is not the default account-history design.

## Stream Deposits

The minimum accepted authorized ICP stream deposit is `3` e8s. Deposits below `3` e8s, including zero, are rejected before protocol state or processed-transaction state changes. Unknown or unauthorized deposits remain non-mutating and are journaled or ignored according to the stream scanner convention for rejected ICP deposits.

Authorized deposits split by floor rounding:

- `stake_e8s = floor(amount_e8s * 4000 / 10000)`
- `liquid_e8s = amount_e8s - stake_e8s`

The remainder therefore stays liquid. The split never creates or loses ICP e8s.

## IO Issuance

Jupiter Faucet and two-week maturity streams calculate IO issuance from the pre-deposit redemption rate and `liquid_e8s`. Issuance rounds down with integer division. Rounding favors solvency: IO is never over-issued. If calculated issuance is zero, the stream is economically invalid and is rejected before state mutation or downstream IO transfer.

Two-year maturity streams issue no IO. Two-week reward allocation dust remains unissued in the protocol reserve and is reported as `dust_e8s` / `dust_unissued_e8s`; it is not silently lost.

No zero-value downstream IO transfer may be attempted.

## Redemption

The minimum accepted redemption input is `1` IO e8. A zero redemption is rejected before state mutation.

For a redemption:

- `gross_icp_payout_e8s = floor(io_redeemed_e8s * liquid_icp_e8s / redeemable_io_e8s)` using the pre-redemption rate.
- `icp_ledger_fee_e8s` is explicit in the fee policy.
- `net_user_icp_payout_e8s = gross_icp_payout_e8s - icp_ledger_fee_e8s`.
- `io_returned_to_reserve_e8s = io_redeemed_e8s`.

`io_ledger_transfer_fee_e8s` is a transfer-boundary/protocol-paid IO ledger cost for returning redeemed IO to the protocol reserve. It is not deducted from `io_returned_to_reserve_e8s`, which remains the gross redeemed IO amount credited back to reserve after successful IO return proof.

If `gross_icp_payout_e8s <= icp_ledger_fee_e8s`, the redemption is rejected as unpayable. If gross payout exceeds liquid reserve, the redemption is rejected. State mutates only after ICP payout and IO return are both proven by success or matching duplicate proof. Failed ICP payout remains retryable without mutating protocol state. Failed IO return remains retryable without paying ICP again.

Partial redemption removes the gross ICP payout from liquid reserve and returns the redeemed IO to protocol reserve only at safe commit. Rounding favors solvency and can never overdraw liquid reserve or overpay a user.

## Ledger Transfer Boundaries

Transfer requests preserve expected amount, fee intent, memo, source subaccount, destination account, and operation kind in the journal or reconstructed request. Current mock/local flows use zero explicit fees; production activation must set explicit ICP and IO ledger fees or document intentional delegation to ledger defaults before activation.

Bad-fee responses do not mutate accounting. They are retryable with the expected fee surfaced. Insufficient-funds responses do not mutate accounting and surface the available balance.

Duplicate transfer responses complete safely only when the duplicate block matches expected amount, destination account, memo, and transfer operation kind. When ledger kind is available, it must also match. Mismatched or unavailable duplicate proof remains retryable and does not complete accounting.

## Reserve and Supply Invariants

`redeemable_io_supply_e8s = total_io_supply_e8s - protocol_reserve_io_e8s - non_redeemable_governance_io_e8s`.

Excluded supply must not exceed total supply. IO issuance decrements protocol reserve only for actually issued IO. Unissued dust remains in protocol reserve. Redemption returns gross IO to protocol reserve only after safe completion. Liquid ICP reserve decreases by gross payout only after safe completion.

Historian snapshots may display gross IO redeemed, gross/net ICP payout, payout fee, IO returned to reserve, dust, and retry status when observed. Missing fields mean unavailable read-model observation, not zero protocol value.

This policy is pre-production. No live value-moving stream-manager stable state exists that requires a compatibility migration for newly added fee fields. Local/defaulted pending redemption records must still retry with their legacy gross payout amount when explicit net payout fields are absent or zero.
