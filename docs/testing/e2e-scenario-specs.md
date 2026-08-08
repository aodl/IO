# Simplified E2E scenario specifications

These scenarios describe explicit command execution against canonical ledgers and governance. No scenario authorizes mainnet work or uses account-history discovery as monetary authority.

## Serialized redemption

Install Paused with a pinned real SNS ledger as IO, the official ICP ledger interface as ICP, and the stream-manager Wasm. Governance readiness must validate standards, fees, supply/reserve/exclusions and liquid backing.

For multiple users and canonical subaccounts, prove exact ICRC-2 allowance and pull, separate payout resume, separate canonical commit, balances and fee burns. Cover null/zero normalization, excluded/reserve rejection, exact replay, conflicting nonce, overlapping preparation, concurrent resume, stale callbacks, upgrades at Preparation/IO-in-reserve/payout/completion, delayed first payout, duplicate/ambiguous payout, deduplication expiry with exact proof and adverse postconditions.

The official-ledger baseline is `installed_stream_real_sns_icrc2_redemption`. It proves Paused install, readiness, pinned SNS ICRC-2 pull, separate official ICP payout, separate completion, upgrades, exact balances, replay and nonce conflict. Deterministic response barriers and the remaining matrix are required before P0 completion.

## Jupiter 40/60

Prove one exact Jupiter Faucet raw-ICP block from the configured faucet default Account to the NNS-manager default Account. The block, not a callback caller or fabricated sequential memo, is authority. Persist the typed operation, stake floor(40% of gross), refresh the protected neuron, prove the exact increase, prepare the Jupiter receipt, transfer the remaining liquid amount from Jupiter staging, prove it through ICP `query_blocks`, then settle backed IO from reserve to the fixed Jupiter account. Each update performs at most one external monetary or governance effect. No IO leaves reserve before deposit, stake increase and liquid receipt are all proved.

## Direct maturity

For each protected neuron, call `StakeMaturity(40%)` and then `DisburseMaturity(100% of remaining)`. Record drift between the two responses. Two-year actual ICP goes directly to stream liquid and issues no IO. Two-week actual staging balance increase is transferred through the proof-bound two-week receipt and settles the pending entitlement batch.

## Daily entitlements and delayed backing

Observe consecutive 86,400-second Governance reward events around bounded
neuron pagination. Proposal-bearing events use exact current-event canonical
shares. Empty `settled_proposals` uses current exact eligible stake, while a
proposal event with zero eligible shares adds zero. Accumulate daily weights,
record ambiguous skipped spans without credits, and preserve redemption.

Freeze at most one immutable batch for the two-week NNS maturity path. Continue
daily accumulation while waiting for actual ICP. Resume transfers one recipient,
the next resume refreshes that exact SNS neuron, and upgrades preserve recipient
progress. One IO fee is charged per recipient; dust remains in reserve.

## One unwind child

Below target, report UnderTarget and do not stake liquid backing. Above target, split and dissolve one exact excess child. A rising target stops dissolution and merges the child back. A ready child disburses directly to stream liquid, issues no IO and clears the one pending slot.

## Historian and frontend

The frontend selects one canonical subaccount, queries allowance, creates a short exact approval, supplies fee/output maxima and nonce, displays typed progress and warns that unsolicited transfers create no claim. The historian reports source freshness and simplified status but never supplies monetary inputs or completes work.

## Failure expectations

Transport ambiguity preserves the identical Submitted intent and reports Pending. Exact effect proof uses only the canonical named block; there is no global absence proof. Every upgrade returns Paused. Corrupt V1 state fails reopening. A stale callback cannot mutate a newer sequence/variant/phase/fingerprint/epoch.
