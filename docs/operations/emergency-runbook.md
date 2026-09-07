# Emergency runbook

This runbook contains safe investigation and containment for the simplified protocol. It does not authorize deployment, controller changes, funding, or any mainnet operation.

## First response

Pause through the reviewed governance command, preserve the exact active operation and avoid introducing a second monetary path. A pause blocks new work but does not erase typed work. Every same-Wasm or forward-fix upgrade reopens Paused.

Inspect `get_status`, the caller-visible progress, the exact transfer intent and the canonical ledger block named by the operation. Historian output is observation only and cannot classify or complete monetary work.

Caller-visible redemption progress is deliberately coarse: `Idle`, `Pending`,
`RateLimited`, `Completed(result)`, or `Stuck(text)`. `get_status.operation_phase` supplies the
diagnostic internal phase names below; those names are not a public workflow
contract.

## Redemption phases

- Before activation, an ordered candidate block remains in the bounded staging
  queue. Its IO is claim-bearing. Index lag, fee drift, or insufficient liquid
  ICP creates no payout intent or debt.
- `PayoutPrepared`: the canonical staging block, fresh coherent `B/C`, matching
  fees, and whole-gross liquidity have fixed one immutable payout to the exact
  source Account.
- `PayoutSubmitted`: the immutable ICP payout was created at its first submission. Retry only the identical intent within its deduplication window.
- `PayoutSucceeded`: the canonical payout block is persisted; exactly the staged
  amount is economically retired and temporarily excluded from `C`.
- `SweepPrepared` / `SweepSubmitted`: the immutable staging-to-reserve transfer
  debits the full staged amount by sending amount-minus-fee and paying the
  canonical IO fee. Retry or prove only this exact intent.
- `Stuck`: automated retry is not safe. Keep Paused and prove the exact named block through the ledger's canonical current/archive interface or ship a reviewed forward fix.

After sweep proof, the physical supply burn plus reserve credit replaces the
temporary economic exclusion without changing post-payout `C`. The latest
bounded result, active-operation clear, and candidate removal commit exactly
once; ledger blocks remain the durable external history.

Never mark completion by assertion, change a payout destination, recreate an intent with a new timestamp, infer a user account from text, or attempt a global proof that a transfer is absent.

## Exact transfer proof

For redemption intake, use the pinned SNS ledger current/archive transaction
and require an ordinary transfer into the fixed staging Account, a valid source
Account, positive supported amount, and non-consumption. Index history alone is
never authority. For the ICP payout, reserve sweep, or receipt, exact-match the
persisted intent through the canonical current/archive boundary, including
accounts, amount, fee, memo, timestamp, and spender constraints.

No proof of absence exists. If the exact effect cannot be proved, retain Paused and prepare a governance-reviewed upgrade.

## Liquid receipts and rewards

Only Jupiter and two-week maturity receipts exist. The active receipt binds one
sequence, recipient policy, paired amount, destination, memo, and frozen
pre-inflow economics. The sequence advances only after settlement completes.
Exact completed replay uses `LastCompletedReceipt`; a conflicting replay is
rejected.

Jupiter settlement transfers backed IO from reserve only after the exact liquid ICP receipt is proved. Two-week settlement must preserve the pending entitlement batch and recipient index across upgrades, transfer one recipient per resume, record one best-effort refresh attempt on the following resume, and retain forfeiture and rounding dust in reserve. The exact transfer is recipient completion; refresh rejection or transport failure must not hold the monetary slot.

For all potentially irreversible effects, immutable intent is durable before
submission. Definite success is immediately re-observed once and fixed-size
work may continue when proved. Ambiguity or an absent required postcondition
stops dependent effects. One-recipient settlement remains a deliberate
per-flow bounded-work limit.

## NNS operations

The NNS manager owns governance proof. Jupiter and two-week sending staging accounts each have their own bounded pre-funded fee float. Two-year maturity and ready unwind principal go directly to the stream liquid account and issue no IO. Never add a general fee ledger, generic monetary scanner, or stream-side governance proof. The one Stream redemption scanner is restricted to its fixed semantic Account and requires canonical proof before value movement.

New NNS work requires Ready after the zero-maturity baseline and exact target
reconciliation. Post-upgrade remains Paused, while already immutable unwind,
maturity, outgoing-transfer and receipt work resumes through its typed evidence.
The semantic maturity staging Account is controlled-value authority; callers do
not supply upstream Mint evidence.

## Upgrade or stable-state failure

Stop release activity. Reproduce with the narrowest stable/upgrade test, then run `cargo run -p xtask -- validate_stable_storage`, `cargo run -p xtask -- did_surface`, and the affected PocketIC path. Preserve active operations and pending slots. Use a reviewed same-schema forward fix; do not add a prelaunch migration chain.

## Historian divergence

Treat stale, missing or incomplete historian data as an observation incident. Correct historian ingestion or its read model. Do not mutate value-moving state to match a dashboard and do not add historian-driven completion.

## Release containment

On a DID, artifact, validation, or real-source failure, stop the release. Do not weaken the gate, edit hashes by hand, publish generated artifacts from this work, or use unverified Wasm. Escalate through the governance/security review path with the exact failing command and evidence.
