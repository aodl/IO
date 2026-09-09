# ADR: Semantic redemption staging

- Status: Accepted
- Scope: Stream Manager redemption transport, accounting, scheduling, API, and read-model coherence
- Supersedes: prepared direct-to-reserve push redemption in `adr-anchored-dynamic-backing.md` and `adr-simplified-execution.md`

## Decision

IO uses one fixed, domain-separated redemption staging Account owned by the
Stream Manager. An ordinary ICRC-1 transfer into this Account is redemption
intent encoded by Account topology. Staging is not the formal reserve, so valid
staged IO remains claim-bearing until settlement.

A dedicated coarse timer polls the configured SNS index for this Account about
once per minute. The permissionless, no-argument `process_redemptions()` method
only coalesces a near-term timer wake; it performs no index, ledger, fee, quote,
proof, or transfer call in the caller's invocation. One Stream monetary slot,
immutable outgoing intents, and post-await exact-operation comparisons serialize
redemption with reward, structural, receipt, and recovery work. One monetary
slot is normative; one physical timer object is not. Manual near-term wakes have
one heap-only global ten-second admission cooldown, independent of caller. The
cooldown may reset on upgrade; normal coarse polling is independent of it.

SNS account history is newest-first with an exclusive upper `start` cursor.
The scanner retains a committed watermark while traversing a captured head
interval. Its stable state is a committed head, captured head, exclusive resume
cursor, optional bounded error, and a unique service queue of at most 64 block
IDs. A page and its cursor are committed in one state write. Queue pressure
cannot acknowledge a partially represented page. New arrivals above a captured
head wait for the next head pass. Candidate discovery is deterministic and
bounded, but settlement order is not FIFO.

A normal coarse worker scans before servicing: it may read at most one index
page while queue capacity remains, even when service candidates already exist,
and then it may service at most one candidate. The requested page size is
limited to the number of free queue slots, so every hint from an acknowledged
page can be represented atomically. Near-term wakes continue an already
captured interval, or perform head discovery when the queue is empty, without
turning actionable backlog draining into repeated head polling. A scanner error
does not prevent service of a candidate that was already represented and still
requires canonical ledger proof.

The index may suppress obviously irrelevant candidates as an availability
optimization. It still cannot authorize any monetary effect. A faulty index can
delay discovery, which is already an accepted availability dependency, but
cannot fabricate a payout. Scanner coverage advances over every returned
transaction ID, while only an ordinary transfer hint whose destination is the
fixed staging Account and whose amount meets the configured minimum enters the
bounded proof queue. The canonical IO ledger, including its archive path, then
independently proves the operation type, source Account, destination, amount,
and all required monetary facts. Any contradiction discards the candidate and
moves no money. A caller cannot supply a block, payout Account, amount, quote,
fee, memo, nonce, or other monetary fact.

Before a payout operation exists, Stream reads a fresh coherent `B/C` snapshot
and canonical IO/ICP fees, checks them against reviewed configuration, computes
`gross = floor(X * B / C)` and `net = gross - ICP fee`, and requires `L >= gross`.
Illiquidity leaves the block queued and claim-bearing. Because no quote,
reservation, payout entitlement, or debt exists before activation, an illiquid
service head is moved behind the other discovered candidates. This prevents one
oversized or temporarily illiquid redemption from blocking smaller payable
redemptions without introducing reservations, liabilities, or per-user
scheduling state.

Stream retains at most 64 canonically discoverable pending redemption blocks. A
completely occupied queue backpressures newer discovery until an existing
candidate completes or is discarded. No transaction is silently skipped;
staged IO remains claim-bearing. Avoiding this bounded availability limit would
require another persistent overflow or deferred-redemption structure and is not
justified for launch.

The coarse head poll remains about once per minute. A successful page with no
candidate schedules an internal near-term wake while its captured interval is
incomplete. When one discovered candidate completes or is discarded, an
internal near-term wake drains another candidate or continues that interval.
Index failure and an illiquid valid candidate return to coarse retry. This does
not batch pages or monetary effects: each worker still handles at most one index
page and one payout/sweep operation, and public wake hints remain subject to the
global cooldown.

Settlement has two typed stages:

```text
Payout { immutable ICP transfer attempt }
→ Sweep { canonical payout block, immutable IO transfer attempt }
→ complete
```

Each irreversible intent is persisted as Submitted before its ledger await and
all retries reuse that exact identity. Ambiguous effects require exact canonical
proof. A definitive no-effect response to the first payout submission may clear
the active operation and defer the staging block behind other discovered
candidates for a fresh quote at coarse cadence. A
later no-effect response after an earlier ambiguous dispatch cannot prove
absence. Sweep ambiguity remains conservative because ICP has already left.

The IO sweep fee is frozen with the activation snapshot. Payout success creates
and persists the exact staging-to-reserve sweep without another fee-read await.
An unexpected governed fee change can make the sweep Stuck for reviewed exact
recovery. Stream and Historian do not manufacture a coherent monetary snapshot
during an active two-ledger redemption settlement. Claim supply and claim rate
are unavailable until the exact sweep completes, after which ordinary physical
supply/reserve accounting is coherent again.

The browser is a deliberately small initiator. It validates amount, minimum,
fee, selected Account balance, destination, and consent, then makes one ordinary
ICRC-1 transfer with no special memo. It does not retain a receipt database or
automatically retry an ambiguous transport result. A confirmed staging transfer
triggers one best-effort wake hint. The automatic scanner is the durable recovery
path for any transfer that committed.

After a same-schema upgrade, an active immutable redemption remains Paused.
SNS Governance uses `resume()` or exact governed proof to complete the
payout/sweep while Paused; only after the active operation clears may ordinary
readiness preflight enter Ready and install reward and redemption timers.

## Safety complexity retained

- Claim-bearing staging prevents an unobserved transfer from retiring claims.
- Durable scanner coverage prevents a valid staged transfer from being skipped
  across singleton pages, bursts, queue pressure, new arrivals, and upgrades.
- Canonical ledger proof prevents index or caller data from authorizing money.
- Fresh economics and whole-gross liquidity admission prevent overpayment and
  unsecured payout debt.
- One monetary slot prevents incompatible Stream monetary effects from running
  concurrently.
- Immutable Submitted payout and sweep intents, exact retries, exact proof, and
  post-await comparisons prevent duplicate irreversible effects and stale
  callback regression.

## What IO deliberately does not guarantee

IO does not guarantee browser-level exactly-once intent across reload, automatic
retransmission of an ambiguous wallet transfer, or browser completion recovery
after local history loss. An ambiguous call says that it may have succeeded and
directs the user to wallet or ledger history before authorizing another transfer.
An explicitly authorized second valid transfer is another redemption, not a
protocol solvency failure.

IO does not guarantee that `process_redemptions()` settles work synchronously,
an exact one-minute poll, or one-second effect recovery. Coarse retry may add a
minute of latency. It does not guarantee FIFO settlement,
continuously available claim rate during a two-ledger settlement, proactive
detection of fee drift after activation, or successful replay of a proof after
the matching active operation has completed. These omissions affect latency or
observability, not payout authority, scanner coverage, or exact-effect safety.

## Unsupported deposits

Transfers below `minimum_redemption_io_e8s`, transfers that cannot yield positive
net ICP, and unsupported ledger operations create no payout or refund and do not
block later valid transfers. Their IO remains in staging and claim-bearing.
Clients must send at least the configured minimum.
