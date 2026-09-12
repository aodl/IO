# IO NNS Neuron Manager

`io_nns_neuron_manager` is the narrow effect-recovery owner of the permanent
NNS neuron, the launch-bootstrapped Dynamic 14-day IO neuron, Jupiter staging,
two fixed semantic maturity staging Accounts, one immediate NNS command, and
generation-aggregated passive unwind cohorts.

The permanent neuron retains its audited principal and exact post-Mission-70
two-year maximum-delay configuration. Its maturity path disburses 100% into the
fixed two-year staging Account, freezes the complete post-finalization balance,
and sends 40% gross to permanent capital and 60% gross to Stream liquid. It
issues no IO. Production identifies it as neuron
`10_292_412_127_977_304_661`, controlled by this manager. It is recorded and
operationally expected to follow alpha-vote neuron `2_947_465_672_511_369`.
This remains subject to separately authorized mainnet verification; this
component never changes the permanent neuron's followees.
Readiness verifies that this manager is the controller and that no maturity
disbursement is already in progress. Pre-existing ordinary maturity and staked
maturity are permitted observations: neither enters claim backing or changes
the audited principal, and existing staked maturity is left untouched.
`auto_stake_maturity` remains disabled.

The Dynamic parent must exist before Ready. Production memo `0` is solely its
fixed deterministic NNS staking nonce and carries no application metadata.
Bootstrap derives that staking Account, proves it differs from the permanent
neuron Account, observes at least the 10-ICP protocol seed, claims or refreshes
the neuron, and proves its exact 1,209,600-second delay, non-dissolving state,
fixed protected-neuron followee, and auto-stake policy. The first 10 ICP is the
excluded anchor; unsolicited excess is excluded surplus. Neither seed nor
surplus issues IO, enters claim backing, enlarges anchor entitlement, or blocks
bootstrap.

Stream owns under-target source transfers. NNS freezes a typed permit bound to
the reconciliation generation, expected parent principal, destination, credit,
fee, operation sequence, memo, time, and canonical fingerprint. It proves the
exact Ledger block and a monotone cached-principal increase before completion.
The expected claim credit must be fully reflected. Physical parent principal is
partitioned into claim-bearing principal, anchor available, and excluded
surplus; unexplained positive residual defaults to surplus and never becomes a
claim by subtraction.

Over-target work separates Split and StartDissolving submission/proof. The
split fee is recognized when physical child principal is proved, and the
unavoidable future disbursement fee is recognized once at sticky commitment.
Only a
canonically dissolving child enters the sorted passive collection.
Postcommit cancellation never stops or merges the child. The earliest ready
cohort returns principal to Stream liquid, then proves zero maturity or merges
zero-principal maturity into the parent before retiring. Pending member reward
re-entry never retains the child slot. A one-shot recovery timer is derived
from the active recovery boundary and earliest `ready_at_seconds`; ready-child
return takes priority over creating another child. There is no 32-cohort
product limit or `CapacityPending` result.

Two-week maturity disburses 100% ordinary maturity into its distinct fixed
staging Account. Its complete unprocessed balance, including value left after
an earlier capture and any donation received before the next capture, uses the
same checked 40/60 paired-inflow algebra as Jupiter. Stream
freezes backed IO for the frozen entitlement generation before the claim leg
can become redeemable. Neither maturity path accepts or stores a Mint block.
Two-year maturity first uses its complete semantic Account capture to restore
the Dynamic anchor deficit, paying that restoration-transfer fee from the same
fresh capture without creating recursive debt. Only the valid remainder
receives the ordinary gross 40/60 allocation. Each fresh leg contributes its
post-fee net credit, and no TwoYear path issues IO.

The anchor protects only fees caused by relocating existing claim backing:
Stream-to-Dynamic top-up and the sticky Split plus future child-disbursement
commitment. Jupiter, TwoWeek maturity, and ordinary TwoYear delivery are fresh
value; their fees reduce their fresh net credits and do not consume anchor.

Production methods cover Jupiter notify, Stream-bound two-week maturity
preparation, pooled reconciliation, deterministic resume/proof, claim-backing
observation, and status. Callers cannot choose a neuron, destination, amount,
memo, followee, or vote. The daily pool-policy observation makes independent best-effort
`RefreshVotingPower` attempts for the permanent neuron and Dynamic parent.
Either attempt may fail without blocking the other or any
monetary path. Neither changes followees, and there is no extra scheduler.

`observe_claim_assets` and `observe_pool_policy` remain Stream-authorized
protocol boundaries. The separate permissionless
`observe_dynamic_backing_status` update performs the same canonical parent
observation but returns only the redacted physical/economic partition and
verified delay/follow/auto-stake policy needed for operational status and
release evidence. It creates no permit, performs no monetary effect, and adds
no stable state.

Install and upgrade schedule automatic readiness while internally Paused.
Known durable work is recovered first; readiness then revalidates the permanent
neuron, Dynamic parent, fees, and accounting before entering Ready. An invalid
or transient observation stays fail-closed and is retried without a governance
resume action. Once Ready, a separate volatile interval creates one weekly
opportunity, exactly 604800 seconds later, to start the existing TwoYear
maturity state machine when ordinary maturity meets its minimum threshold.
Active work and a pending TwoYear record block duplication; passive TwoWeek
maturity and non-ready unwind children do not reserve the immediate slot. There
is no governance queue, persistent cadence state, or second monetary slot.

Stable state is a strict prelaunch launch schema. Install and upgrade reopen
Paused, old development states are rejected, and exact submitted/proved work
remains resumable. Every potentially irreversible effect has persisted exact
intent before submission. A later dependent effect waits on ambiguity or a
missing canonical postcondition, but definitively successful and proved fixed
steps may continue in the same invocation.

Useful checks:

```bash
cargo test -p io-nns-neuron-manager --lib
cargo run -p xtask -- validate_nns_boundary_pin
cargo run -p xtask -- validate_stable_storage
cargo check -p io-nns-neuron-manager --target wasm32-unknown-unknown
```

No mainnet inspection, deployment, controller action, neuron action, or funding
is authorized by this component.
