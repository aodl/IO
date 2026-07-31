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

## Remaining vertical work

The safety and topology invariants above are implemented. The NNS manager has a
typed proof-bound Jupiter 40/60 executor and an executable direct maturity
command/Mint-proof path. The browser implements the explicit redemption flow
against the stream manager and IO ledger while keeping historian reads
observation-only. Installed official-Governance execution, two-week serialized
reward fan-out, the direct unwind child, and complete historian status
projection remain required before final P0 completion. NNS readiness
deliberately returns `ImplementationIncomplete` rather than exposing Ready
until those effects and their upgrade matrix exist.
