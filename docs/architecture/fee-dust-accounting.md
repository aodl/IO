# Fee, Dust, and Accounting Policy

## Launch fee rule

IO uses standard SNS fee burn: there is no fee collector and no zero-fee launch.
Redemption queries current IO and ICP fees and enforces caller maxima. The
approval call burns its own IO fee; `transfer_from` burns another IO fee, so a
normal allowance covers the requested IO amount plus the transfer-from fee.
Intentional fee changes require pause, a drained active operation, governance
change, configuration upgrade, current-fee verification, and unpause.

This document defines IO monetary accounting in e8s. It is a pre-activation policy for the pure model, stream-manager journal, historian read model, and ledger transfer boundaries. It does not imply IO issuance, redemption, a canonical SNS IO ledger, or production adapters are live.

## Canonical Sources

Ledgers and governance canisters provide canonical asset and governance facts. The value-moving operation journal is canonical retry and operation-phase truth. The pure model deterministically interprets coherent canonical snapshots and authorised events; it does not replace the sources behind those snapshots. The historian is observation only: it is rebuildable, may show missing, incomplete, retryable, or observed-only fields, and is never monetary authority.

Normal ledger history reads use index canisters. Raw ledger/archive traversal is not the default account-history design.

## Stream Deposits

The minimum accepted authorized ICP stream deposit is `3` e8s. Deposits below `3` e8s, including zero, are rejected before protocol state or processed-transaction state changes. Unknown or unauthorized deposits remain non-mutating and are journaled or ignored according to the stream scanner convention for rejected ICP deposits.

Authorized deposits split by floor rounding:

- `stake_e8s = floor(amount_e8s * 4000 / 10000)`
- `liquid_e8s = amount_e8s - stake_e8s`

The remainder therefore stays liquid. The split never creates or loses ICP e8s.
Any fee needed to realize the 40% NNS staking leg is an operational ICP fee-buffer liability and is not deducted from the 60% liquid backing. If a direct staking operation cannot prove fee-free realization and the fee buffer cannot cover the fee, the operation fails closed.

## IO Issuance

Jupiter Faucet and two-week maturity streams calculate IO issuance from the pre-deposit redemption rate and `liquid_e8s`. Issuance rounds down with integer division. Rounding favors solvency: IO is never over-issued. If calculated issuance is zero, the stream is economically invalid and is rejected before state mutation or downstream IO transfer. For delivered IO `I` and each reserve-paid IO transfer fee `f`, reserve debit is `I + sum(f)`, total supply decreases by `sum(f)`, and redeemable supply increases only by `I`.

Two-year maturity streams issue no IO. For reward allocations `A_i` with per-recipient fees `f_i`, reserve debit is `sum(A_i) + sum(f_i)`. Reward dust is `pool - sum(A_i)`, remains in reserve, and is never redistributed. Each fee is accounted separately; one aggregate or default-ledger fee assumption is not sufficient.

No zero-value downstream IO transfer may be attempted.

## Redemption

The minimum accepted redemption input is `1` IO e8. A zero redemption is rejected before state mutation.

The canonical redemption pre-state is observed after the incoming IO redemption transfer has already occurred on the IO ledger. That incoming transfer fee must not be applied a second time by the protocol model.

For a redemption:

- `gross_icp_payout_e8s = floor(io_redeemed_e8s * liquid_icp_e8s / redeemable_io_e8s)` using the canonical post-intake snapshot.
- `icp_ledger_fee_e8s` is explicit in the fee policy.
- `net_user_icp_payout_e8s = gross_icp_payout_e8s - icp_ledger_fee_e8s`.
- `io_returned_to_reserve_e8s = io_redeemed_e8s - io_ledger_transfer_fee_e8s`.
- `io_redemption_intake_debit_e8s = io_redeemed_e8s`.

The incoming user-to-redemption fee has already burned before protocol processing and is not charged or counted a second time. The redemption intake is debited by the full redeemed amount; the IO return transfer delivers the redeemed amount minus its explicit IO fee to reserve.

The liquid ICP source account debit is exactly the gross ICP claim. If `gross_icp_payout_e8s <= icp_ledger_fee_e8s` or `io_redeemed_e8s <= io_ledger_transfer_fee_e8s`, the redemption is rejected as unpayable. If gross payout exceeds liquid reserve, the redemption is rejected. State mutates only after ICP payout and IO return are both proven by success or matching duplicate proof. Failed ICP payout remains retryable without mutating protocol state. Failed IO return remains retryable without paying ICP again.

Partial redemption removes the gross ICP payout from liquid reserve and returns the redeemed IO to protocol reserve only at safe commit. Rounding favors solvency and can never overdraw liquid reserve or overpay a user.

## Ledger Transfer Boundaries

Transfer requests preserve exact source and destination Accounts, expected amount, explicit fee, memo, created-at time, ledger principal, method, operation kind, and immutable attempt fingerprint. Every production monetary transfer uses an explicit fee. Production code has no permission to delegate monetary fees to a ledger default.

Bad-fee responses do not mutate accounting. They are retryable with the expected fee surfaced. Insufficient-funds responses do not mutate accounting and surface the available balance.

Duplicate transfer responses complete safely only when the duplicate block matches expected amount, destination account, memo, and transfer operation kind. When ledger kind is available, it must also match. Mismatched or unavailable duplicate proof remains retryable and does not complete accounting.

## Reserve and Supply Invariants

`redeemable_io_supply_e8s = icrc1_total_supply - protocol_reserve_io_e8s - sum(excluded_account_balances_e8s)`.

Excluded supply must be the checked sum of exact configured Accounts and must not exceed total supply. IO issuance decrements protocol reserve by delivered IO plus all reserve-paid IO fees. Unissued dust remains in protocol reserve. Redemption credits reserve only with the IO return amount after its IO fee while reducing redeemable supply by the full intake debit. Liquid ICP reserve decreases by the gross claim only after safe completion.

Historian snapshots may display gross IO redeemed, gross/net ICP payout, payout fee, IO returned to reserve, dust, and retry status when observed. Missing fields mean unavailable read-model observation, not zero protocol value.

This policy is pre-production. No live value-moving stream-manager stable state exists that requires a compatibility migration for newly added fee fields. Even so, migration is fail closed: an incomplete legacy monetary operation without an exact Account and immutable attempt/proof evidence enters `ManualReconciliationRequired`. It must never retry from a display string, inferred subaccount, assumed gross amount, or reconstructed transfer intent.
