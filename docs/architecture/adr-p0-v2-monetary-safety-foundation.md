# ADR: P0 V2 Monetary Safety Foundation

Status: accepted for P0 v2 implementation

This ADR records the monetary safety decisions that must guide implementation. It is not evidence that production behavior is complete.

## Decisions

IO uses the standard SNS ledger fee model. There is no IO fee collector and no zero-fee launch. When the SNS ledger has no fee collector, transfer fees are observed as supply burn.

Canonical IO total supply is `icrc1_total_supply` from the configured IO SNS ledger. Canonical protocol reserve is `icrc1_balance_of` for one exact configured Account. Excluded and non-redeemable IO supply is the checked sum of exact configured Accounts, each represented as `(owner, optional 32-byte subaccount)`. It is not a caller-provided scalar.

The protocol reserve is funded after SNS finalization and before activation by an SNS-governance treasury-transfer proposal. For desired reserve `R`, remaining treasury `T`, and SNS transfer fee `f`, genesis treasury must contain at least `R + T + f`. The reserve destination is:

- owner: the local or production `io_stream_manager` canister
- subaccount: the exact configured protocol reserve subaccount

Ordinary IO issuance is a transfer from that reserve Account. It is never arbitrary minting.

The canonical snapshot for monetary settlement must bracket all queried ledger values with coherent ledger/index tips. Free-form strings, historian state, and debug methods are not monetary authority.

The 40% NNS staking allocation must not reduce the 60% liquid backing by a hidden transfer fee. Any ICP fee required to realize the staking leg is an operational liability funded from a separate ICP fee-buffer Account that is excluded from redemption NAV. If the fee buffer is insufficient, the operation fails closed.

Redemption payout destination is the exact owner and exact optional 32-byte subaccount that sent the accepted IO redemption transfer. The incoming redemption transfer fee has already affected the IO ledger before the protocol observes the redemption and must not be applied twice.

For redemption observed after IO intake:

- gross ICP claim is based on the canonical post-intake snapshot
- net ICP transfer amount is gross claim minus authoritative ICP fee
- liquid ICP source debit is exactly the gross claim
- IO return amount is redeemed IO minus authoritative IO fee
- redemption account debit is exactly the redeemed IO amount

The system rejects redemptions where redeemed IO is less than or equal to the IO fee, or gross ICP claim is less than or equal to the ICP fee.

Until concurrent reservation accounting is separately proved, each value-moving canister may have at most one globally incomplete monetary operation that can progress through external settlement. Source events may still be durably journaled, but no second value-moving operation may start settlement while an ambiguous transfer, externally completed uncommitted leg, unresolved proof, or manual reconciliation remains.

ProductionActive remains unavailable. No production monetary timer is registered. Production value-moving DIDs remain constructor-only.

The exact two-week reward policy is unchanged: 1,209,600 seconds, non-dissolving, positive stake, frozen cohort stake, direct and followed participation, late stake/top-up exclusion, forfeiture to dust, no redistribution, and full backing target.
