# io_stream_manager

The launch monetary canister owns direct-reserve redemption, liquid ICP and IO
reserve roles, proof-bound NNS receipts, exact reward cohorts, serialized reward
settlement, and local lifecycle.

Two-week entitlement uses canonical SNS Governance reward shares as the complete
weight for a proposal-bearing latest event. When no proposal settled, exact
eligible captured stake is the fallback; settled proposals with zero eligible
shares issue no reward. Readiness verifies exact Root, Governance principal and
module hash, the approved event duration, and zero current native reward rates.

IO is not live. The production canister remains inert and this repository does
not contain a production activation transition.

## Production API

The production DID contains only:

- `redeem`
- `prepare_liquid_receipt`
- `complete_liquid_receipt`
- `resume`
- `prove_active_transfer`
- `set_paused`
- `get_status`

Every update checks authority in the method. Redemption rejects anonymous
callers, binds both token source and ICP payout to the caller's exact
`Account`, enforces the per-caller nonce, and rejects `Busy` before moving
funds. There is no caller-selected destination.

## Redemption

The frontend first creates an exact, short-lived ICRC-2 allowance for the stream
manager. The allowance normally covers `io_amount + transfer_from fee`; the
approval itself burns a separate IO fee. It should use `expected_allowance`,
clear an incompatible prior allowance when necessary, and set min-output and
fee maxima.

`redeem` queries canonical fees, total supply, reserve, excluded balances, and
liquid ICP. It persists the complete operation, pulls IO directly from the user
to reserve with `icrc2_transfer_from`, then pays ICP to the same caller and
subaccount. There is no intake account, scanner, IO return leg, or automatic
refund.

## Stable state

Launch state is `StableCell<StreamStateV1>` plus
`StableBTreeMap<Principal, CallerRedemptionState>`. Only V1 is supported.
Prelaunch migration chains are research history, not runtime code.
One `StreamOperation` slot serializes redemption, liquid receipt, cohort capture,
and cohort close. Capture and close each have only Prepared and Submitted phases;
their exact NNS generation is the replay boundary.

## Unsupported activity

Direct transfers that do not correspond to an authenticated command create no
protocol claim and are not automatically refunded. Rare unresolved transfer
ambiguity safely pauses for exact proof or an SNS-governed forward fix.
