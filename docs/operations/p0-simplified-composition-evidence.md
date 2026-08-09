# P0 simplified composition evidence

This document records the deterministic composition defects reproduced from the simplified-execution baseline and the launch invariant that replaces each defect. It is normative for the simplified protocol; scanner-era recovery remains non-normative research history.

| Item | Baseline reproduction | Positive invariant and regression |
| --- | --- | --- |
| A | Canonical pricing ran before any active-operation reservation, so two `redeem` messages could interleave and an older callback could install its operation after a newer redemption. | Caller balance and allowance are read first; the exact caller nonce and fingerprint are re-read; `RedemptionPreparation` then exclusively reserves the one operation slot before global pricing. Only the byte-for-byte matching preparation may become executable. |
| B | Completion wrote `CallerRedemptionState` before rechecking the active sequence, permitting two completion callbacks to advance caller state. | `CompletionPrepared` durably stores the exact result. Caller state accepts only `nonce` once or the exact already-applied `nonce + 1` replay. `CallerResultApplied` is persisted before the exact operation is cleared. |
| C | A failed postcondition callback called an unconditional persistence helper and could overwrite a newer operation. | The callback compares sequence, variant, phase and request fingerprint after the await and before mutation. A stale callback returns `Busy` without writing. |
| D | The ICP payout intent inherited the initial `redeem` timestamp and could be `TooOld` before its first submission. | Preparation creates only the IO pull. The ICP intent and checked deduplication deadline are created with the current time only on the first resume from `IoInReserve`. |
| E | The request fingerprint encoded the caller's raw optional subaccount, so `null` and 32 zero bytes differed. | `CanonicalRedeemRequestV1` fingerprints one fixed effective `[u8; 32]` subaccount under a domain-separated hash. `null_and_zero_subaccounts_have_one_request_identity` proves equality. |
| F | An unpause preflight re-read only lifecycle/work state, so a governance pause delivered during the awaits could be overwritten. | Every governance pause/unpause increments `control_epoch`; Ready may be written only for the captured epoch while the latest lifecycle is still Paused. |
| G | `post_upgrade` reopened stable state without changing Ready. | Both launch canisters fully validate V1 and then force lifecycle to Paused while preserving typed work. Installed redemption upgrades exercise Paused replay/resume semantics. |
| H | `complete_liquid_receipt` used SNS-style `get_transactions` against the configured ICP ledger. | Receipt completion calls the official ICP `query_blocks` interface and its exact returned archive callback, matching account identifiers, amount, fee, memo, timestamp and absence of spender. |
| I | One `nns_receipt_source` could not represent distinct Jupiter and two-week staging accounts. | Stream configuration contains exact distinct `jupiter_receipt_source` and `two_week_receipt_source` accounts; the receipt operation persists the source selected by kind. |
| J | Clearing a completed receipt discarded all exact replay evidence. | `LastCompletedReceipt` stores the request fingerprint, permit, exact ICP block, settlement result and completion time. Exact preparation replay returns the durable permit; a conflicting sequence is rejected. |
| K | `two_week_target` returned eligible IO e8s directly. | The pure target is `floor(active eligible IO * liquid ICP / redeemable IO supply)`, without subtracting a payout fee. Tests cover backing rates below, equal to and above one. |
| L | Stable reopening validated only configuration and accepted inconsistent active work. | `StreamStateV1::validate` and `NnsStateV1::validate` validate typed active work, transfer attempts, fingerprints, bounded fields and pending maturity identities before reopening. Caller replay records validate whenever read or written. |
| M | `progress_for` mapped durable `Stuck` to `PayoutSubmitted`. | Public progress has an explicit `Stuck(text)` state and preparation has explicit `Preparing`. |
| N | A transport rejection returned `ApiError::Stuck` while the durable transfer remained safely retryable `Submitted`. | Transport and ledger ambiguity return `ApiError::Pending`; only an explicit durable transition may report `Stuck` and pause.

## Remaining stream composition reproductions

| Item | Baseline reproduction at `ce496d5` | Positive invariant and regression |
| --- | --- | --- |
| A | Governance pause could arrive while `RedemptionPreparation` awaited the global snapshot, after which the older callback still promoted the preparation. | Preparation captures `control_epoch`; promotion rechecks the exact preparation, Ready lifecycle, control epoch, caller nonce and expiry. Pause clears only the exact no-effect preparation, so the older callback observes no match and writes nothing. |
| B | Same-Wasm upgrade preserved a Paused preparation with no executable resume path. | `post_upgrade` validates V1, forces Paused and clears only `RedemptionPreparation`; caller nonce remains unchanged. Any operation whose transfer may have been submitted is preserved. |
| C | Transfer retry expiry was derived from operational `first_submitted_at`. | Every retry deadline is checked from immutable `intent.created_at_time + ledger_deduplication_window`; first-submission time remains telemetry. |
| D | Two Jupiter `resume` calls could both observe `ReceiptProved` and create settlement transfers. | Canonical facts are queried first, then exact operation CAS installs one immutable settlement attempt. The other callback sees a phase mismatch and cannot submit or mutate. |
| E | A Jupiter transport rejection left `Submitted` but had no identical-intent retry transition. | Ambiguity returns `Pending`; after the configured delay, resume reuses amount, fee, memo and created-at time while incrementing only the dispatch epoch. |
| F | Jupiter settlement could age past the ledger window without a durable proof state. | Expiry moves the exact attempt to `Stuck`, pauses, and accepts only an exact ICRC current/archive block proof. No absence proof is attempted. |
| G | `complete_liquid_receipt` could not return the result after active state was cleared. | Typed `LastCompletedReceipt` returns the exact completed progress for the same sequence and block and rejects a conflicting block. |
| H | `jupiter_io_account` validation allowed unsafe or excluded destinations. | Every monetary Account has a safe owner; the Jupiter IO Account is neither reserve nor excluded. |
| I | The target calculation accepted active eligible IO larger than redeemable supply. | The target rejects `active_eligible_io > redeemable_io`; regressions cover below-one, one and above-one rates plus equal/excess active supply. |

## Installed composition evidence

`installed_stream_real_sns_icrc2_redemption` installs the stream-manager debug Wasm with the pinned real SNS ledger as IO and PocketIC's official ICP ledger canister as ICP. It proves Paused installation, readiness, excluded/reserve Account rejection, ICRC-2 approval/pull, exact IO fee burn, upgrade after the pull, a delayed first payout whose timestamp is not inherited from the redeem request, exact ICP movement, upgrade after payout, a separate canonical commit, null/zero normalization, durable exact redemption replay, and conflicting-nonce rejection.

The same installed test prepares a Jupiter receipt, transfers ICP from the exact configured NNS source, proves that block through the official ledger's `query_blocks` shape, settles backed IO from reserve to the fixed Jupiter Account, checks the exact fee burn, upgrades, and replays the durable receipt permit while Paused. The test therefore composes both payout and receipt proof against the production-shaped ICP interface rather than a second SNS ledger substitute.

## Exact reward, receipt, and target regressions

| Item | Reproduced defect | Durable correction |
| --- | --- | --- |
| A | A separate two-week participation cohort duplicated SNS accounting and required exact alignment with one NNS receipt. | Each canonical daily SNS reward event contributes once to a non-overlapping entitlement accumulator. Actual NNS maturity receipt and distribution are asynchronous. |
| B | Ballot reconstruction could diverge from Governance voting-power policy. | Proposal-bearing events consume only current-event candidate `reward_shares`; absent, stale, zero, or malformed current data fails closed without ballot or maturity fallback. |
| C | Silence could be confused with zero eligible shares. | Only `settled_proposals.is_empty()` selects the no-proposal fallback. Every currently eligible neuron receives its exact stake as one virtual unanimous-proposal weight. A proposal-bearing zero-share event adds no credit. |
| D | Protocol-owned and Jupiter-governance staking Accounts could enter membership and inflate the target. | The exact staking Account is SNS Governance plus the 32-byte neuron ID subaccount; configured excluded Accounts, dissolving neurons, wrong delays and zero stake are removed before event weighting and target input. |
| E | Missing daily observations could be fabricated as no-proposal days. | Round gaps and catch-up spans produce one bounded typed skipped-event record, add no entitlement credit, advance the checkpoint, and preserve undistributed backing. |
| F | A liquid donation after permit preparation changed issuance. | `ReceiptPreparation` captures one immutable canonical backing snapshot before returning the permit. Jupiter and reward pricing use that snapshot, while conservative postconditions permit only donations and extra fee burn. |
| G | Backing could claim the wrong entitlement work. | Only the configured stream manager may prepare maturity, and the immutable entitlement-batch generation, exact target, and NNS baseline must match. Later events continue in the live accumulator. |
| H | A target generation could be started twice. | Stable latest-started and latest-completed generation counters plus exact active/passive plan matching make replay idempotent and conflicting reuse invalid. |
| I | A dissolving unwind child inflated active capacity. | Capacity is the canonical non-dissolving parent stake only; child principal is exposed separately. |
| J | Same-generation UnderTarget replay returned stale state. | Every exact replay queries the parent again and replaces the observed target status by full-state compare-and-swap. |
| K | A target written while another NNS operation was active never created its later unwind. | An idle `resume` first reconciles the latest target and creates at most one direct child when the parent is materially OverTarget. |
| L | Two-week staging ambiguity had no exact-proof route. | NNS `prove_active_transfer` dispatches by typed operation. Only `AmbiguousPossibleEffect` accepts a single exact ICP block matching the immutable intent. |
| M | Stable reward state accepted duplicate neuron IDs or mismatched destinations. | V1 validation requires at most 1,000 sorted unique 32-byte IDs, unique canonical destinations, checked totals, exclusions, and exact SNS-Governance ownership/subaccount derivation. |

Candidate-Governance PocketIC evidence observes direct Yes, direct No and
followed votes through canonical event shares, unequal voting power, multiple
proposal counts, stale participation tags, and empty settled-proposal events.
The installed IO profile proves stake-proportional no-proposal entitlement,
actual-receipt-backed allocation, excluded-neuron omission, zero-share
proposal behavior, reserve/dust reconciliation, and upgrade-safe serialized
fan-out.

## Normalized daily-credit correction reproductions

These deterministic regressions record the reviewed `55907f6` behavior before
the normalized-credit correction. They are evidence of the gap, not the target
policy.

| Item | Baseline reproduction | Required correction |
| --- | --- | --- |
| A | One 100-share proposal day followed by a 10,000-share many-proposal day accumulates 5,100 for A and 5,000 for B. Raw proposal volume makes the two-day result approximately equal instead of the equal-day 75%/25% result. | Give every successfully observed day one fixed policy credit and normalize current-event shares within that day. |
| B | A proposal event with eligible share 50 and excluded protocol share 50 removes the excluded neuron before allocation. The eligible neuron becomes the only denominator entry and receives the full backed pool. | Keep every current-event canonical share in the event denominator and forfeit excluded/ineligible credit. |
| C | The freeze gate checks only `processed_event_count != last_frozen_event_count`. One observed event is therefore separable even when the 60% NNS liquid leg is below the canonical minimum. | Query no-effect NNS backing readiness before the accumulator can be frozen. |
| D | `classify_event_sequence(None, event)` returns `First`, and the current observation path computes and merges that event's weights. | Seed the current canonical event as a zero-credit activation baseline during first successful unpause. |
| E | An absent or zero-stake neuron remains in mandatory pre-transfer stake observation. After an exact successful transfer, persisted `refresh_submitted` still requires a later stake-increase observation; rejection or transport failure leaves the recipient index unchanged and the monetary slot occupied. | Make the exact transfer the completion condition and bound refresh to one best-effort attempt. |
| F | The reader accepts ten 100-neuron pages and rejects any nonempty eleventh page: exactly 1,000 total neurons fit, while 1,001 fails. Governance readiness does not expose or pin this product bound. | Require the reviewed Governance `max_number_of_neurons` parameter to be at most 1,000 before pagination. |
| G | The next observation is scheduled only one second after the nominal event boundary. If Governance has not advanced, `Pending` leaves work due but installs no replacement timer. | Use a documented safety margin and one bounded replacement one-shot timer for retryable latest-event reads. |

## Normalized correction evidence

The corrected path gives every successfully observed non-skipped event exactly
`1,000,000,000,000,000,000` policy-credit units. Current-event canonical SNS
shares are normalized over all tagged neurons, while no-proposal events are
normalized over current eligible stake. Eligible entries may sum below the
policy total; excluded, ineligible, zero-share, no-eligible, and fixed-point
remainder fractions are forfeited and remain in reserve.

First readiness stores the already completed event only as a zero-credit
baseline. A new batch cannot freeze until authenticated no-effect NNS evidence
says the exact target is ready and its liquid 60% leg meets the canonical
minimum. An exact recipient transfer is monetary completion; one persisted
best-effort refresh attempt cannot strand the active monetary slot. Governance
readiness pins a maximum of 1,000 total neurons. Observation uses a 300-second
margin and one 60-second replacement one-shot after `Pending` or a retryable
read.

## Protected NNS backing lifecycle reproductions

These deterministic regressions record the reviewed `11ee4ee` composition
before the protected-NNS lifecycle correction. They do not alter daily
entitlement economics.

| Item | Baseline reproduction | Required correction |
| --- | --- | --- |
| A | Zero maturity makes the read-only readiness calculation treat generation zero as reconciled, but the durable `two_week_maturity_baseline_reconciled` flag remains false. Once maturity becomes nonzero, readiness reports `BaselineUnreconciled`; no pending batch exists from which `prepare_two_week_maturity` could make the flag durable. | Prove the exact zero-maturity seeded parent during NNS unpause and persist the baseline before Ready. |
| B | A lower target is classified `OverTarget` by the read-only readiness method, but the accepted target and generation remain unchanged and no unwind operation is created. Idle resume therefore reconciles only the older target. | Replace observation with authenticated idempotent target reconciliation that persists a changed target and creates at most one direct unwind. |
| C | A Ready stream update with no pending batch invokes only `freeze_batch` and returns `BatchFrozen`; `prepare_two_week_maturity` is reachable only on a later update. Maturity and later daily credits can therefore advance while an older subset remains frozen. | Bind the exact freeze CAS and immediate maturity preparation in one normal stream update, retaining the immutable batch only for exact replay after ambiguity. |
| D | Freeze checks the cached `governance_parameters_fresh` Boolean but does not call the reviewed Root/Governance verification boundary. A module or parameter change after the last daily observation is therefore not re-read before target calculation. | Reverify Root, Governance hash and reviewed parameters immediately before target calculation and again after any neuron pagination used by that calculation. |

## Controlled protected-NNS vertical result

The controlled pinned-NNS PocketIC path proves the exact zero-maturity baseline,
UnderTarget without implicit funding, three independent target generations, a
single direct child unwind and exact disbursement block, entitlement generation
one, StakeMaturity 40%, DisburseMaturity 100% of the remainder, delayed Mint,
actual staging receipt, backed IO recipient settlement and fresh live credit for
the next generation. Same-Wasm upgrades force Paused after baseline, child
creation, dissolution, disbursement, maturity phases, Mint proof, staging
delivery and recipient transfer while immutable work resumes without duplicate
effects. IO remains Paused, inert, prelaunch and not live; no mainnet execution
is authorized.

## Protected-NNS policy and liveness gaps at `6266557`

These deterministic reproductions describe the reviewed starting behavior
before the final protected-NNS correction. They do not change daily SNS
entitlement economics.

| Item | Baseline reproduction | Required correction |
| --- | --- | --- |
| A | The controlled pinned-NNS harness creates the protected reward-backing parent at 252,288,000 seconds (eight years). Pinned NNS Governance requires roughly six months of dissolve delay to vote, while a genuine 1,209,600-second neuron is below that boundary and therefore earns no voting maturity. Normative IO prose nevertheless calls the protected parent an exact-two-week NNS position. | Separate the ordinary SNS/user two-week product rule from the NNS-voting-eligible protected backing neuron and record the reviewed launch delay explicitly. |
| B | After an over-target split and `StartDissolving`, the child remains the immediate `active_operation` until its long dissolve timestamp. Readiness stays `Busy`/`OverTarget`; another maturity command cannot occupy the slot. Daily SNS credit remains able to accumulate in the stream manager, but the next batch cannot receive backing. | Retain one exact child as passive unwind evidence after canonical `StartDissolving`, clear the immediate slot, and permit maturity work on the reduced parent. |
| C | First readiness checks the parent ID through its query, seeded cached principal, ordinary maturity and pending maturity disbursements. Its validation signature cannot distinguish nonzero staked maturity, `auto_stake_maturity = true`, a dissolving parent or the wrong dissolve delay. With auto-stake enabled, a real NNS reward event moves reward into staked maturity rather than the ordinary maturity consumed by IO's 40/60 pipeline. | Prove the complete launch baseline once, persist that proof, and recheck auto-stake plus exact non-dissolving delay before every later maturity start. |
| D | A frozen entitlement batch retains its immutable target, but pending-batch resume invokes `prepare_two_week_maturity` directly. If the protected parent principal drifts before preparation, the NNS manager returns `Pending`; the stream retries preparation without reconciling the frozen target. | Reconcile the batch's exact stored target before every preparation replay, retaining the batch and all newer live credit in every waiting state. |
| E | The controlled real-NNS harness does not call `notify_jupiter_deposit`; the stream side uses mock SNS Governance; all-real two-year stream accounting and the real SNS trigger remain absent; and no real merge-back interruption fixture exists. | Add controlled production-API Jupiter and two-year verticals, the SNS-governed trigger, the combined real SNS/NNS lifecycle, and the remaining ambiguity matrix. |
