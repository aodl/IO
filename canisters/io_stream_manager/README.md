# io_stream_manager

The launch monetary canister owns direct-reserve redemption, liquid ICP and IO
reserve roles, proof-bound NNS receipts, daily entitlement accumulation, one
pending backed batch, serialized reward settlement, and local lifecycle.

Each daily proposal-bearing event normalizes canonical SNS Governance reward
shares into one fixed policy credit. The denominator includes current-event
shares from excluded and ineligible neurons, so their fraction is forfeited
rather than redistributed. An empty `settled_proposals` list normalizes current
eligible cached IO stake into the same daily credit; settled proposals with zero
canonical shares forfeit the complete opportunity.
Readiness verifies exact Root, Governance principal and module hash, the 86,400
It also requires Governance's configured maximum to be at most 1,000 total
neurons. First activation seeds the current event as a zero-credit baseline.

IO is not live. The production canister remains inert and this repository does
not contain a production activation transition.

## Production API

The production DID contains only:

- `redeem`
- `prepare_liquid_receipt`
- `complete_liquid_receipt`
- `resume`
- `resume_reward_work`
- `resume_reward_backing`
- `prove_active_transfer`
- `set_paused`
- `validate_set_paused`
- `get_status`

Every update checks authority in the method. Redemption rejects anonymous
callers, binds both token source and ICP payout to the caller's exact
`Account`, enforces the per-caller nonce, and rejects `Busy` before moving
funds. There is no caller-selected destination.

`validate_set_paused` is the read-only payload renderer for the SNS generic
function. The matching `set_paused` update remains callable only by the
configured SNS Governance canister.

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
One `StreamOperation` slot serializes monetary effects. Reward observation has
no external value effect and does not occupy that slot. A bounded live
accumulator persists across upgrades; at most one immutable pending entitlement
batch binds the NNS maturity request and recipient progress while later daily
events continue accumulating live.

Before a freeze, the stream revalidates reviewed SNS Governance and calls the
authenticated target-reconciliation boundary. UnderTarget and every unwind
leave all credits live. Ready binds the exact freeze CAS to immediate maturity
preparation. Every pending replay first reconciles the batch's stored target;
target drift or transport ambiguity retains that one immutable batch while
new daily credit remains live.

Observation and NNS backing waits do not occupy the monetary slot, so they do
not block redemption. Actual reserve-to-recipient transfers share that slot
with redemption. An exact ICRC transfer completes one recipient; the following
SNS neuron refresh is attempted at most once and cannot extend serialization
indefinitely.

## Unsupported activity

Direct transfers that do not correspond to an authenticated command create no
protocol claim and are not automatically refunded. Rare unresolved transfer
ambiguity safely pauses for exact proof or an SNS-governed forward fix.
