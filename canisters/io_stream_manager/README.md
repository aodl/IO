# IO Stream Manager

## Role in IO

`io_stream_manager` is IO's launch monetary canister. It owns direct ICRC-2
redemption, the protocol IO reserve and liquid ICP Account, proof-bound liquid
receipts, daily reward-entitlement accumulation, one pending backed batch, and
serialized IO settlement. It does not own an intake scanner or a user-selected
payout destination.

IO is pre-launch and not live. The production reservation remains inert, and
the repository contains no production activation transition.

## Dependencies and authority

The manager is configured with exact IO and ICP Ledger principals, its own
reserve Accounts, the NNS Manager, and an exact SNS Root/Governance pair. SNS
Governance alone controls `set_paused`. The NNS Manager alone prepares and
completes liquid receipts. Redemption rejects anonymous callers and binds the
IO source and ICP destination to that caller's exact principal and subaccount.

Ledgers are canonical for balances, fees, supply, and transfer blocks. SNS
Root/Governance are canonical for reviewed Governance identity, module, system
parameters, neurons, and reward events. The Stream Manager recomputes monetary
facts and never trusts frontend arithmetic. It does not maintain a second IO
supply ledger, and protected NNS principal is not part of liquid ICP backing.

## Value flow

For redemption, a user approves a short-lived allowance for the Stream Manager.
The manager durably records the operation, pulls IO directly from the user's
Account into the protocol reserve with `icrc2_transfer_from`, and pays ICP from
the liquid reserve to the same Account. There is no intake Account, return leg,
automatic refund, or caller-selected destination.

For NNS receipts, the NNS Manager first obtains a sequence-bound permit. After
the exact ICP receipt block is proved, Jupiter receipts back IO to Jupiter and
two-week receipts settle an immutable entitlement batch from the IO reserve.

## Production API

The checked-in [production Candid](io_stream_manager.did) exposes:

- `redeem`
- `prepare_liquid_receipt`
- `complete_liquid_receipt`
- `resume`
- `prove_active_transfer`
- `resume_reward_work`
- `resume_reward_backing`
- `set_paused`
- `validate_set_paused` (query)
- `get_status` (query)
- `get_caller_redemption_state` (query)

`get_caller_redemption_state` rejects anonymous callers and returns the
caller's next nonce, optional last request fingerprint, and optional last
completed `RedemptionResult`. The redemption frontend needs this state to
construct the next exact request, recognize an identical replay, and recover a
completed result without inventing client-side nonce or completion state.

`validate_set_paused` renders the reviewed SNS generic-function payload without
changing state. `set_paused` independently enforces the configured SNS
Governance caller.

## Lifecycle and readiness

Install and upgrade leave the manager `Paused`. `Paused` prevents new
redemption, receipt, reward-event, and reward-backing preparation. It does not
discard immutable work: an already active redemption or liquid-receipt
operation remains resumable, and an already frozen entitlement batch remains
bound while new daily credit accumulates separately.

The asynchronous activation preflight verifies at least:

- the exact SNS Root and Governance identities;
- the exact expected Governance module hash reported through Root;
- native initial and final reward rates both equal zero;
- the round/reward-event duration equals the reviewed configuration and exactly
  86,400 seconds;
- the maximum neuron count is nonzero and no greater than 1,000;
- maximum dissolve-delay and age bonuses both equal zero;
- the IO Ledger advertises ICRC-1, ICRC-2, and ICRC-3;
- the ICP Ledger advertises ICRC-1;
- current IO and ICP fees equal the reviewed configuration; and
- protocol reserve plus configured exclusions do not exceed total IO supply.

The preflight uses a captured control epoch and activates only if configuration,
lifecycle, and active-operation state are unchanged when all asynchronous reads
finish. First activation records the current reward event as a zero-credit
baseline rather than awarding unobserved history.

## Redemption economics

All values are integer e8s. The implementation uses checked `u128` arithmetic,
multiplies before dividing, and therefore rounds ratio results down:

```text
redeemable_io = total_io_supply - reserve_io - excluded_io
gross_icp = redeemed_io * liquid_icp / redeemable_io
net_icp = gross_icp - current_icp_payout_fee
```

`excluded_io` is the checked sum of the configured excluded Accounts. A request
also requires `redeemed_io + current_io_transfer_from_fee <= redeemable_io`, a
nonzero redeemable denominator, a positive net payout, the caller's minimum
output, and both caller fee maxima. Overflow, underflow, or a changed canonical
fee fails closed.

The SNS Ledger supplies `total_io_supply`; reserve and excluded Accounts are
separate denominator roles rather than generic liabilities. IO settlement uses
explicit reserve transfers, never arbitrary Stream Manager mint authority.
Ordinary IO transfer fees follow the configured ledger fee/burn policy.

The frontend allowance normally covers `io_amount + current IO
transfer_from fee`; the `icrc2_approve` transaction burns its own separate IO
fee. Clients should use `expected_allowance`, clear an incompatible allowance
when necessary, and use short expiries.

## Reward policy and backing

Each canonical daily event presents one fixed
`io_reward_policy::DAILY_EVENT_CREDIT` opportunity. Ordinary beneficiary
eligibility requires positive cached IO stake, `NotDissolving`, and an exact
1,209,600-second (14-day) dissolve delay. Protocol/Jupiter Accounts and other
configured exclusions are not recipients.

For a proposal-bearing event, meaning `settled_proposals` is nonempty, the
denominator is the sum of current-event canonical `reward_shares` across all
relevant Governance neurons. That denominator includes excluded and ineligible
classes, so their shares are forfeited rather than redistributed. A missing or
stale participation record contributes zero. Malformed participation for the
current event fails closed. If canonical shares are zero, or no eligible neuron
has positive current-event shares, the event gives no entitlement credit; it
does not fall back to stake.

Stake fallback is used only when `settled_proposals.is_empty()`. In that
genuine no-proposal case, the fixed daily opportunity is normalized across
current eligible cached IO stake. No proposal evidence is reconstructed or
inferred from missing participation data.

The target reconciled before entitlement freezing is:

```text
two_week_target = active_eligible_io * liquid_icp / redeemable_io_supply
```

This is also checked `u128` multiplication followed by floor division. The
manager rejects a zero redeemable supply or active eligible IO above that
supply. `UnderTarget`, every unwind state, and insufficient maturity keep all
credits in the live accumulator. `Ready` binds the exact freeze compare-and-
swap to immediate NNS maturity preparation. The frozen batch remains immutable
through maturity preparation and receipt proof; only actual proved ICP backing
can lead to backed IO settlement.

An event-sequence gap that cannot be attributed exactly is recorded as
`MissedSkipped`. It increments the missed count, receives zero policy credit,
and is not interpolated into a synthetic entitlement or fabricated
proposal-free day. Missing evidence is not interpreted as zero participation.

IO maintains one current live entitlement accumulator and at most one immutable
pending frozen batch; there is no general entitlement queue. Daily events may
continue accumulating while a frozen batch waits. The amount of ICP actually
received and exactly proved from the NNS maturity path determines the
economically backed IO settlement quantity.

## Stable state and upgrades

Launch state is `StableCell<StreamStateV1>` plus
`StableBTreeMap<Principal, CallerRedemptionState>`. Only V1 is supported;
pre-launch migration chains are not runtime code. Upgrade restores the durable
snapshot, preserves accumulator, pending batch, caller replay, and operation
state, forces `Paused`, and arms at most one future reward timer only after
reviewed activation.

One `StreamOperation` slot serializes external monetary effects. Reward
observation has no external value effect and does not occupy that slot.
Observation/backing waits likewise do not block redemption; actual
reserve-to-recipient transfers do share the monetary slot.

## Failure, ambiguity, resume, and proof

Every external effect is represented by a persisted intent and attempted at
most once per invocation. `resume` re-enters the active phase without creating
a second intent. A retry outside the configured ledger deduplication window
becomes `Stuck` and requires exact proof rather than a guess.

`prove_active_transfer(block_index)` handles four active proof slots:

- a Stuck redemption IO pull into the reserve;
- a Stuck redemption ICP payout to the caller Account;
- a Stuck Jupiter IO settlement from the reserve; and
- a Stuck two-week recipient IO reward transfer.

The supplied canonical block must match the active ledger, from/to Accounts,
spender where applicable, amount, fee, memo, creation timestamp, sequence, and
operation fingerprint. Proof only marks that exact persisted effect succeeded;
it cannot select a new transfer or rewrite monetary state.

Direct transfers without an authenticated command create no claim and are not
automatically refunded. Rare ambiguity stays visible for proof or a separately
reviewed SNS-governed forward fix.

## Commands and verification

```bash
cargo test -p io-stream-manager --lib
POCKET_IC_BIN=/home/codexdev/.local/bin/pocket-ic-server \
  cargo test -p io-stream-manager --test io_stream_manager_pocketic
cargo run -p xtask -- did_surface
cargo run -p xtask -- validate_install_args
cargo check -p io-stream-manager --target wasm32-unknown-unknown
```

Run PocketIC targets serially. See the repository [xtask guide](../../tools/xtask/README.md)
for aggregate required gates.

## Non-goals

The manager is not a general transfer scanner, custody wallet, arbitrary mint
authority, refund service, historical dashboard, or NNS neuron controller. It
does not make the Historian or frontend authoritative, and it exposes no debug
completion method in the production Candid.
