# IO NNS Neuron Manager

`io_nns_neuron_manager` is the narrow proof-oriented owner of the permanent NNS
neuron, lazy pooled claim-backing parent, Jupiter staging, maturity staging, one
immediate NNS command, and at most 32 passive unwind cohorts.

The permanent neuron retains its audited principal and exact post-Mission-70
two-year maximum-delay configuration. Its maturity path stakes 40% of observed ordinary maturity,
disburses 100% of the remainder, and treats the actual delayed Mint as entirely
new claim backing. It does not issue IO or split that Mint 40/60 again.

The pooled parent is created only from existing Stream liquid claim backing
when the canonical target reaches the NNS minimum. It uses one fixed memo,
exact 1,209,600-second delay, auto-stake off, and one fixed following policy.
Production memo/followee values remain unresolved TODOs in deliberately
non-runnable install arguments. Parent creation proves the staking transfer,
claimed neuron ID, cached stake, delay, following, dissolve state, and
auto-stake state before recording the parent.

Stream owns under-target source transfers. NNS freezes a typed permit bound to
the reconciliation generation, expected parent principal, destination, credit,
fee, operation sequence, memo, time, and canonical fingerprint. It proves the
exact Ledger block and cached-principal increase before completion.

Over-target work separates Split and StartDissolving submission/proof. Only a
canonically dissolving child enters the sorted bounded passive collection.
Postcommit cancellation never stops or merges the child. The earliest ready
cohort returns principal to Stream liquid, then proves zero maturity or merges
zero-principal maturity into the parent before retiring. Pending member reward
re-entry never retains the child slot.

Pooled maturity disburses 100% ordinary maturity. The actual Mint is split into
40% permanent gross and 60% claim gross. Stream freezes the finite joint route,
fees, target, reward coverage, and recipient settlement before any effect. NNS
then proves each permanent, liquid, or direct-parent effect one at a time.

Production methods cover Jupiter notify, maturity start/prepare/resume/proof,
pooled reconciliation/resume/proof, claim-backing observation, lifecycle, and
status. Callers cannot choose a neuron, destination, amount, memo, followee, or
vote. The existing daily reconciliation path refreshes voting power; there is
no extra scheduler.

Stable state is a strict prelaunch launch schema. Install and upgrade reopen
Paused, old development states are rejected, and exact submitted/proved work
remains resumable. One invocation performs at most one external effect.

Useful checks:

```bash
cargo test -p io-nns-neuron-manager --lib
cargo run -p xtask -- validate_nns_boundary_pin
cargo run -p xtask -- validate_stable_storage
cargo check -p io-nns-neuron-manager --target wasm32-unknown-unknown
```

No mainnet inspection, deployment, controller action, neuron action, or funding
is authorized by this component.
