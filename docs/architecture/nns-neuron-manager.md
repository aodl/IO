# NNS Neuron Manager

The manager controls the protected permanent neuron and one launch-bootstrapped
**Dynamic 14-day IO neuron**. Before Ready it derives the deterministic memo-0
staking Account, proves that it does not collide with the permanent-neuron
Account, observes at least the 10 ICP protocol seed, claims or refreshes the
neuron, and proves its exact identity and policy. The Dynamic neuron is
non-dissolving, has an exact `1_209_600`-second delay, has auto-stake maturity
disabled, and follows the protected permanent IO neuron on the reviewed topics.
The permanent neuron is recorded and operationally expected to follow the
alpha-vote neuron, subject only to a separately authorized mainnet inspection.

The first 10 ICP is excluded anchor capital. Any positive staking-Account
residual is excluded unattributed surplus, so hostile dust neither blocks
bootstrap nor enters claim backing. Canonical accounting maintains:

```text
physical Dynamic principal
  = claim-bearing Dynamic principal
  + excluded anchor available
  + excluded unattributed surplus
```

An unexplained positive residual is excluded surplus; a negative residual is a
fail-closed invariant violation. `anchor_available_e8s` never exceeds 10 ICP.
Qualifying claim fees consume it exactly once, while exact permanent-capital
fees increment `permanent_fee_shortfall_e8s`. Neither scalar is donor
provenance, and neither excluded category enters `B = L + P + U + T`.

Under target, Stream owns the liquid-to-staking transfer and NNS freezes a
permit bound to the reconciliation generation, operation sequence, exact
claim-bearing parent principal, destination, fee, and canonical fingerprint.
The transfer fee consumes anchor capacity so the fee amount is reclassified as
claim-bearing value without inventing physical ICP. Over target, NNS persists
one sticky Split operation. The Split fee and the committed future child
Disburse fee consume anchor capacity once when the exact child is identified.
Anchor exhaustion stops before the next fee-bearing irreversible command.

There is no product cohort cap and no `CapacityPending` result. Each structural
generation can create at most one aggregate child. Before another Split, the
manager services the oldest ready child; an active, ambiguous, or proof-pending
return blocks later child creation. With the selected 12-hour structural
cadence and exact 14-day child lifetime, the healthy reachable live population
is `ceil(1_209_600 / 43_200) + 1 = 29`, a sizing result rather than a limit.

Cancellation before Split commitment can net away an exit. After commitment,
the child completes its exact lifecycle. Principal returns to Stream liquid,
zero-principal maturity is proved zero or merged with exact conservation, and
the child record retires. One ephemeral recovery timer reconstructs the
earliest active retry or child `ready_at_seconds` after restart/upgrade and
invokes the same state-aware recovery logic; it never blindly repeats an
ambiguous effect or authorizes unrelated work while Paused.

Maturity uses two fixed, domain-separated semantic Accounts. After canonical
`DisburseMaturity(100%)` finalization, the complete positive role balance is
captured once; late value remains for the next capture and the other role
Account cannot satisfy the operation. Jupiter and TwoWeek keep checked gross
40/60 semantics. Claim-leg fees consume anchor, permanent-leg fees create
permanent shortfall, and their exact replays cannot classify a fee twice.

TwoYear maturity issues no IO. It restores the anchor deficit first, restores
permanent shortfall second, charges those reimbursement-transfer fees directly
to fresh maturity without creating recursive debt, and applies ordinary gross
40/60 only to the valid remainder. A remainder too small for the next transfer
stays in the semantic Account for a later capture.

Every persisted non-monetary Governance command is recovered by observing
canonical state before a dependent command. Exact postcondition advances the
same immutable operation; a safely earlier state may reissue the idempotent
command under its recovery policy. Definite success triggers one immediate
canonical reread and continues in the same invocation when proved. Ambiguity or
a postcondition not yet visible retains the submitted phase and returns
`Pending`; a definitive rejection is propagated rather than disguised as
`Pending`, and contradictory identity or monotonicity is `Stuck`. This applies
to Jupiter and pooled-parent refresh,
remaining delay increase, fixed following policy, child StartDissolving, and
zero-principal delay/merge cleanup. These retries never repeat an ICP transfer.

Exact pool-reconciliation replay is resolved locally before lifecycle, asset,
or policy work. Completed, passive, and active replay cannot initiate, repeat,
or duplicate its monetary or Governance effect and remains available while
Paused. A harmless canonical query or independent best-effort voting-power
maintenance call is outside that replay correctness contract.

Each committed unwind and passive cohort retains its exact ICP fee basis.
Claim observation compares it with the canonical current fee and rejects new
monetary quotes on drift while continuing to report physical child principal.
The only fee-debt aggregate is exact permanent-capital shortfall; claim fees
consume anchor and reimbursement-operation fees remain fresh maturity cost.

Before every potentially irreversible effect, the exact immutable intent is
persisted. A later dependent effect is never submitted while an earlier effect
is ambiguous or lacks its canonical postcondition. Once that proof exists, a
fixed-size flow may continue to another step in the same update. Install and
upgrade reopen Paused; immutable submitted/proved operations remain resumable.

The stop boundary is effect-based, not a global call count. Ambiguous outgoing Ledger transfers, Split,
child Disburse, and parent cached-stake reflection remain exact because a retry
could duplicate value or lose control. Provenance of fungible ICP already held
in a semantic Account is not a protocol proof requirement.
