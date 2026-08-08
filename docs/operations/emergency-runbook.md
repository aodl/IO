# Emergency runbook

This runbook contains safe investigation and containment for the simplified protocol. It does not authorize deployment, controller changes, funding, or any mainnet operation.

## First response

Pause through the reviewed governance command, preserve the exact active operation and avoid introducing a second monetary path. A pause blocks new work but does not erase typed work. Every same-Wasm or forward-fix upgrade reopens Paused.

Inspect `get_status`, the caller-visible progress, the exact transfer intent and the canonical ledger block named by the operation. Historian output is observation only and cannot classify or complete monetary work.

## Redemption phases

- `Preparing`: the exact normalized request owns the one operation slot. No monetary effect exists. A failed canonical read may clear only this matching preparation.
- `IoPullSubmitted`: the immutable IO `transfer_from` intent is Submitted. A rejection or timeout is `Pending`/ambiguous while the intent remains inside its deduplication window.
- `IoInReserve`: the exact IO pull succeeded. IO is in reserve and no ICP payout intent existed before this point.
- `PayoutSubmitted`: the immutable ICP payout was created at its first submission. Retry only the identical intent within its deduplication window.
- `PayoutSucceeded`: the canonical payout block is persisted and conservative postconditions still require confirmation.
- `CompletionPrepared`: the exact replay result is durable. Resume applies the caller record idempotently.
- `CallerResultApplied`: the caller nonce/result is durable. Resume may clear only the matching active operation.
- `Stuck`: automated retry is not safe. Keep Paused and prove the exact named block through the ledger's canonical current/archive interface or ship a reviewed forward fix.

Never mark completion by assertion, change a payout destination, recreate an intent with a new timestamp, infer a user account from text, or attempt a global proof that a transfer is absent.

## Exact transfer proof

For an IO pull, use the pinned SNS ledger current/archive transaction and require the exact `transfer_from` spender, source, reserve destination, amount, fee, memo and timestamp. For an ICP payout or receipt, use official `query_blocks` and exactly the archive callback returned for the requested block. Require the exact account identifiers, amount, fee, ICRC-1 memo/timestamp and no spender.

No proof of absence exists. If the exact effect cannot be proved, retain Paused and prepare a governance-reviewed upgrade.

## Liquid receipts and rewards

Only Jupiter and two-week maturity receipts exist. The active receipt binds one sequence, kind-specific source, source-operation ID, amount, destination and memo. The sequence advances only after settlement completes. Exact completed replay uses `LastCompletedReceipt`; a conflicting replay is rejected.

Jupiter settlement transfers backed IO from reserve only after the exact liquid ICP receipt is proved. Two-week settlement must preserve the pending entitlement batch and recipient index across upgrades, transfer one recipient per resume, record one best-effort refresh attempt on the following resume, and retain forfeiture and rounding dust in reserve. The exact transfer is recipient completion; refresh rejection or transport failure must not hold the monetary slot.

## NNS operations

The NNS manager owns governance proof. Jupiter and two-week sending staging accounts each have their own bounded pre-funded fee float. Two-year maturity and ready unwind principal go directly to the stream liquid account and issue no IO. Never add a general fee ledger, staging account for direct flows, ledger scanner, or stream-side governance proof.

Until every governance continuation is executable, readiness returns `ImplementationIncomplete` and lifecycle remains Paused.

## Upgrade or stable-state failure

Stop release activity. Reproduce with the narrowest stable/upgrade test, then run `cargo run -p xtask -- validate_stable_storage`, `cargo run -p xtask -- did_surface`, and the affected PocketIC path. Preserve active operations and pending slots. Use a reviewed same-schema forward fix; do not add a prelaunch migration chain.

## Historian divergence

Treat stale, missing or incomplete historian data as an observation incident. Correct historian ingestion or its read model. Do not mutate value-moving state to match a dashboard and do not add historian-driven completion.

## Release containment

On a DID, artifact, validation, or real-source failure, stop the release. Do not weaken the gate, edit hashes by hand, publish generated artifacts from this work, or use unverified Wasm. Escalate through the governance/security review path with the exact failing command and evidence.
