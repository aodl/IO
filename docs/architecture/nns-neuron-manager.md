# NNS Neuron Manager

The manager controls one audited permanent neuron and a lazily created pooled
parent. The pooled parent uses a fixed memo, exact 1,209,600-second
non-dissolving delay, auto-stake off, and a fixed configured following policy.
Production memo `0` is only the fixed nonce for deterministic NNS staking-
subaccount derivation, not application metadata. The pooled parent follows the
permanent IO two-year neuron `10_292_412_127_977_304_661`, and validation
requires `pooled_parent_followee_id == two_year_neuron_id`. The permanent
neuron is recorded and operationally expected to follow alpha-vote neuron
`2_947_465_672_511_369`. This remains subject to separately authorized mainnet
verification; IO does not mutate that following. No external capital seeds the
parent.

Before readiness or runtime bootstrap can create a monetary permit, IO derives
the memo-bound candidate staking Account and compares it with the canonical
staking Account observed for the permanent neuron. Equality is a configuration
failure. A positive balance at a distinct candidate Account is accepted as
unattributed surplus: IO proves only the exact Stream transfer, requires the
claimed neuron to use that Account and to cover the exact credit, records the
canonical actual principal, and lets ordinary reconciliation unwind any
`OverTarget` amount.

Under target, Stream owns the liquid-to-staking transfer and NNS freezes a
permit bound to the reconciliation generation, operation sequence, parent
principal, destination, fee, and canonical fingerprint. NNS proves the exact
ledger block and cached-principal increase. Over target, NNS performs one
split/start command; canonical `StartDissolving` proof moves the child into a
bounded collection of at most 32 passive cohorts.

Cancellation before split commitment can net away an exit. After commitment,
the child completes its own lifecycle. Earliest-ready promotion is
deterministic. Principal returns to Stream liquid, zero-principal maturity is
proved zero or merged to the parent with exact conservation, and the child
record retires independently of generation-free member reward re-entry.

Maturity uses two fixed, domain-separated subaccounts owned by this canister.
After `DisburseMaturity(100%)` passes the canonical finalization boundary, the
manager freezes the applicable Account's complete positive balance once. A
completed delivery debits exactly that frozen capture, so any late arrival
left in the Account is unprocessed semantic ICP for the next operation. The
other maturity Account can never satisfy the operation. No Mint block, source
operation, or originating-neuron attribution is stored after custody.

Jupiter and two-week maturity use the same checked 40% permanent / 60% claim
gross split. Their claim credit remains quarantined until Stream freezes the
matching backed-IO obligation at the pre-inflow rate. The only recipient-policy
difference is the configured Jupiter Account versus the frozen two-week
entitlement generation. Two-year maturity uses the same physical split but
issues no IO and transfers its claim credit directly to Stream liquid as
ordinary yield.

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
No fee-debt scalar is introduced.

Before every potentially irreversible effect, the exact immutable intent is
persisted. A later dependent effect is never submitted while an earlier effect
is ambiguous or lacks its canonical postcondition. Once that proof exists, a
fixed-size flow may continue to another step in the same update. Install and
upgrade reopen Paused; immutable submitted/proved operations remain resumable.

The stop boundary is effect-based, not a global call count. Ambiguous outgoing Ledger transfers, Split,
child Disburse, and parent cached-stake reflection remain exact because a retry
could duplicate value or lose control. Provenance of fungible ICP already held
in a semantic Account is not a protocol proof requirement.
