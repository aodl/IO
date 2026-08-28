# IO NNS Neuron Manager

`io_nns_neuron_manager` is the narrow effect-recovery owner of the permanent
NNS neuron, lazy pooled claim-backing parent, Jupiter staging, two fixed semantic
maturity staging Accounts, one immediate NNS command, and at most 32 passive
unwind cohorts.

The permanent neuron retains its audited principal and exact post-Mission-70
two-year maximum-delay configuration. Its maturity path disburses 100% into the
fixed two-year staging Account, freezes the complete post-finalization balance,
and sends 40% gross to permanent capital and 60% gross to Stream liquid. It
issues no IO. Production identifies it as neuron
`10_292_412_127_977_304_661`, controlled by this manager. It is recorded and
operationally expected to follow alpha-vote neuron `2_947_465_672_511_369`.
This remains subject to separately authorized mainnet verification; this
component never changes the permanent neuron's followees.

The pooled parent is created only from existing Stream liquid claim backing
when the canonical target reaches the NNS minimum. It uses one fixed memo,
exact 1,209,600-second delay, auto-stake off, and one fixed following policy.
Production uses memo `0` solely as the fixed deterministic NNS staking nonce;
it carries no application metadata. The parent follows the protected two-year
neuron, never alpha-vote directly, and configuration validation requires the
followee ID to equal `two_year_neuron_id`. Before any bootstrap permit exists,
readiness and runtime derive the candidate Account and prove it differs from
the permanent neuron's canonical staking Account. Unsolicited ICP at a
non-colliding candidate Account is harmless surplus: parent creation proves the
exact Stream transfer and records the canonical actual principal as
`OverTarget` when necessary. It then proves the claimed neuron ID, cached
stake, delay, following, dissolve state, and auto-stake state before recording
the parent.

Stream owns under-target source transfers. NNS freezes a typed permit bound to
the reconciliation generation, expected parent principal, destination, credit,
fee, operation sequence, memo, time, and canonical fingerprint. It proves the
exact Ledger block and a monotone cached-principal increase before completion.
The expected IO credit must be fully reflected; unsolicited excess is recorded
as actual favourable backing and reported `OverTarget`, never attributed to
IO's transfer.

Over-target work separates Split and StartDissolving submission/proof. The
split fee is recognized when physical child principal is proved, and the
unavoidable future disbursement fee is recognized once at sticky commitment.
Only a
canonically dissolving child enters the sorted bounded passive collection.
Postcommit cancellation never stops or merges the child. The earliest ready
cohort returns principal to Stream liquid, then proves zero maturity or merges
zero-principal maturity into the parent before retiring. Pending member reward
re-entry never retains the child slot.

Two-week maturity disburses 100% ordinary maturity into its distinct fixed
staging Account. Its complete unprocessed balance, including value left after
an earlier capture and any donation received before the next capture, uses the
same checked 40/60 paired-inflow algebra as Jupiter. Stream
freezes backed IO for the frozen entitlement generation before the claim leg
can become redeemable. Neither maturity path accepts or stores a Mint block.

Production methods cover Jupiter notify, maturity start/prepare/resume,
pooled reconciliation/resume/proof, claim-backing observation, lifecycle, and
status. Callers cannot choose a neuron, destination, amount, memo, followee, or
vote. The daily pool-policy observation makes independent best-effort
`RefreshVotingPower` attempts for the permanent neuron and, when it exists, the
pooled parent. Either attempt may fail without blocking the other or any
monetary path. Neither changes followees, and there is no extra scheduler.

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
