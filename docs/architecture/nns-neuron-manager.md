# NNS Neuron Manager

The manager controls one audited permanent neuron and a lazily created pooled
parent. The pooled parent uses a fixed memo, exact 1,209,600-second
non-dissolving delay, auto-stake off, and a fixed configured following policy.
Production memo and followee values remain unresolved non-runnable template
inputs. No external capital seeds the parent.

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

Permanent maturity stakes 40% first and disburses all remaining ordinary
maturity. Its actual Mint is entirely new claim backing and issues no IO.
Pooled-parent maturity disburses 100%, then splits the actual Mint into 40%
permanent gross and 60% claim gross. The Stream jointly freezes the physical
claim route and backed recipient settlement before effects.

Jupiter and pooled-maturity claim credits remain in NNS staging and outside
claim backing until Stream persists the matching receipt permit. From that
point through any rejected, no-effect, `BadFee`, or canonically funded
`InsufficientFunds` transfer, the exact credit remains in transit. Ambiguous
submitted transfers make claim observation pending. Permanent-maturity Mint is
unpaired yield and enters transit immediately; it never releases IO.

Every persisted non-monetary Governance command is recovered by observing
canonical state before another command. Exact postcondition advances the same
immutable operation; a safely earlier state reissues the idempotent command,
keeps the persisted phase and returns Pending; contradictory identity or
monotonicity is Stuck. This applies to Jupiter and pooled-parent refresh,
remaining delay increase, fixed following policy, child StartDissolving, and
zero-principal delay/merge cleanup. These retries never repeat an ICP transfer.

Exact pool-reconciliation replay is resolved locally before lifecycle, asset,
voting-power, or policy work. Completed, passive, and active replay therefore
makes zero Governance calls even while Paused or policy refresh is rejected.

Each committed unwind and passive cohort retains its exact ICP fee basis.
Claim observation compares it with the canonical current fee and rejects new
monetary quotes on drift while continuing to report physical child principal.
No fee-debt scalar is introduced.

One update submits at most one external effect. Install and upgrade reopen
Paused; immutable submitted/proved operations remain resumable.
