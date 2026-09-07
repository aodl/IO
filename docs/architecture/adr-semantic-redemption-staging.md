# ADR: Semantic redemption staging

- Status: Accepted
- Scope: Stream Manager redemption transport, accounting, scheduling, API, and read-model coherence
- Supersedes: prepared direct-to-reserve push redemption in `adr-anchored-dynamic-backing.md` and `adr-simplified-execution.md`

## Decision

IO uses one fixed, domain-separated redemption staging Account owned by the
Stream Manager. An ordinary ICRC-1 transfer into this Account is explicit
redemption intent encoded by Account topology. The Account is not the formal
protocol reserve, so staged IO remains claim-bearing until its ICP payout is
canonically successful.

The single Stream scheduler polls the IO index for this one Account about once
per minute. A permissionless no-argument `process_redemptions()` call prompts
the same worker, subject to a durable global ten-second expensive-work
cooldown. Each invocation reads at most one page of 32 transactions, keeps at
most 64 pending block IDs, and activates at most one redemption. The in-flight
worker guard, durable post-await comparisons, and the existing monetary slot
serialize timer, generic resume, proof, and manual work.

SNS account history is newest-first with an exclusive upper `start` cursor.
The scanner retains its committed head watermark while a captured interval is
incomplete and resumes from the oldest returned transaction. Newer arrivals
wait for the next head capture; queue pressure cannot skip the active interval.
Ordering is chronological within each bounded page, not globally oldest-first
across a multi-page catch-up.

Index data is discovery only. Before value moves, Stream exact-proves the
candidate through the canonical IO ledger current/archive boundary and derives
the amount and payout Account from that block. Callers cannot supply a block,
amount, destination, quote, fee, memo, nonce, or other monetary fact.

After proof, Stream reads a fresh coherent `B/C` snapshot and canonical fees. It
freezes no quote and creates no payout operation until spendable liquid ICP
covers the entire gross. Normal illiquidity leaves the block queued and the IO
claim-bearing.

Canonical ICP payout success retires exactly the staged amount economically.
During the narrow paid-but-unswept phase, canonical `C` subtracts that amount
once. Stream then persists and exact-recovers a staging-to-reserve transfer for
the staged amount minus the canonical IO fee. The transfer debits staging by
the full amount; the fee burn plus reserve credit changes physical claim supply
by that same full amount. Sweep success removes the temporary exclusion, so
neither `B` nor `C` jumps at the accounting handoff.

Sweep creation is conditional on the exact current paid operation after the
fee await. Its timestamp and transfer identity are persisted once. Callback
success may monotonically resolve a compatible later retry of that immutable
intent; a stale rejection cannot replace accepted success or newer progress.
An exact public proof can atomically complete a matching Stuck sweep without a
second ledger transfer.

Submitted payout and sweep effects are observation barriers. Until exact proof
resolves an ambiguous effect, Stream and Historian report the canonical
economic snapshot unavailable instead of guessing which physical balance has
changed.

## Why this is simpler

Direct user transfers into the formal reserve reduced `C` before Stream could
observe them. That forced a pre-transfer quote and brought preparation state,
caller nonces, request fingerprints, expiry, a special memo, fee/slippage
bounds, per-caller pending/replay state, a caller-supplied block settlement API,
and an exceptional payout-debt phase.

The staging Account removes that cause. The transfer is explicit intent without
claim retirement, so the protocol can discover it later and quote the actual
economic state when it is ready to pay. There is no quote reservation, pull
allowance, refund subsystem, dust aggregation, or generic liability ledger.

## Narrow scanner exception

The launch rule remains: no generic monetary scanner. The only monetary
discovery scanner observes one fixed semantic redemption Account through
account-filtered history, is strictly bounded and globally rate-limited, and
cannot authorize a payout. Every candidate requires canonical ledger proof.
Index delay is therefore an availability failure: the staged IO stays in `C`
and unrelated structural/reward work continues.

This preserves one semantic Account, one discovery cursor and ordered bounded
candidate set, one active monetary operation, and one scheduler.

## Unsupported deposits

Transfers below `minimum_redemption_io_e8s`, transfers that cannot yield
positive net ICP at the enforced claim floor, and unsupported ledger operations
are advanced past for scanning. They create no payout or refund and do not
block later valid transfers. Any received IO remains in staging and therefore
remains claim-bearing. Clients must send at least the configured minimum.
