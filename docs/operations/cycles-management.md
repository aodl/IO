# Cycles operations

IO is pre-launch. This policy defines monitoring and response; it does not name
a funding principal, authorize a top-up, or prove that production funding is in
place.

## Scope and ownership

Operations must monitor the Stream Manager, NNS Manager, Historian, and
frontend asset canister individually. SNS framework canisters follow the final
SNS controller/cycles policy and remain an open launch-configuration item.
An explicitly authorized operations service or controller-approved manual
procedure will top up canisters from the final approved cycles source. No
public keeper is a funding authority.

## Reserve and alerts

Before activation, each production canister must hold the greater of (a) 30
days of conservatively projected peak burn and (b) the measured cost of one
full reinstall/upgrade plus 14 days of projected peak burn. The projection is
based on local/hosted load evidence and is reviewed after release changes.
Alert at 50% of that launch reserve or 30 projected days remaining, whichever
comes first; alert critically at 14 projected days. Top-up automation, if
approved, must remain capped, observable, and separate from protocol monetary
authority.

## Public-call bounds

Reward observation returns locally when durable timer state says work is not
due and consumes that due bit before making external reads. Idle NNS `resume`
returns `Idle` without optional reconciliation. Jupiter activation-floor and
completed-block checks are local; each new at-or-above-floor index may still
cause one bounded canonical Ledger/archive lookup. Reward backing can perform
bounded Governance/Root/Ledger/NNS reads when public callers request progress.
There is no durable refresh retry endpoint or availability throttle state.
Principals are not used as rate-limit identities because they are cheap to
create.

Residual risk is explicit: arbitrary callers can spend canister cycles through
fresh Jupiter indexes and backing attempts. These calls cannot fabricate
canonical proof or bypass monetary serialization, but launch operations must
monitor endpoint rates, reject/timeout rates, and cycles burn. If measured burn
is unacceptable, a separately reviewed caller or scheduling design is required;
the launch schema does not pre-commit to a recovery state machine.

If burn materially exceeds load-test projections, operators pause new monetary
preparation where safe, preserve resumable persisted effects, and inspect call
rates, inter-canister reject/timeout rates, timer frequency, Ledger/archive
latency, refresh failures, stable-memory growth, and module/config drift before
raising the funding cap. Funding is not a substitute for resolving abusive or
regressed call behavior.
