# Production Backlog

Each entry is issue-sized and separate. Required real-source tests mean tests against real DFINITY canister behavior or official local SNS evidence, not IO-only mocks.

## P0

### Decide IO ledger fee disposition and canonical supply authority
Why: SNS fee burn or collection changes total supply and reserve accounting.
Scope: select fee mode, canonical supply source, fee account treatment, monitoring.
Excluded: implementation of monetary paths.
Dependencies: product/governance decision.
Acceptance: approved ADR, updated invariants, bootstrap gate requirements.
Required real-source tests: official SNS ledger init/fee/supply evidence.
Affected invariants: total supply, redeemable supply, reserve solvency.

### Make redemption fee-aware and canonically reconciled
Why: model credits full IO while ledger returns amount minus IO fee.
Scope: ICP payout fee, IO return fee, model deltas, duplicate proof.
Excluded: reward eligibility/allocation policy.
Dependencies: fee/supply ADR.
Acceptance: user-to-redemption and redemption-to-reserve reconcile exactly.
Required real-source tests: real SNS ledger/index redemption flow.
Affected invariants: reserve balance, total supply, redemption rate.

### Preserve and validate exact ICP payout destinations
Why: mock labels and anonymous principals are not exact user accounts.
Scope: exact Account to ICP destination mapping and validation.
Excluded: new payout product policy.
Dependencies: redemption remediation.
Acceptance: principal/subaccount round-trips without relabeling.
Required real-source tests: real ICRC source account to ICP payout account.
Affected invariants: user funds destination.

### Add immutable created_at_time/memo/fee transfer attempts
Why: retries must not create new semantic attempts after ambiguity.
Scope: all release-reachable monetary transfers.
Excluded: changing reward math.
Dependencies: destination and fee decisions.
Acceptance: every transfer has durable amount, fee, memo, created_at_time and proof state.
Required real-source tests: TooOld, Duplicate, unavailable, archive/index proof.
Affected invariants: idempotency and no double send.

### Eliminate release-reachable mock fallbacks
Why: mock fallback can submit semantically different transfers.
Scope: remove release fallback to mock clients/debug methods.
Excluded: debug-only local harness helpers.
Dependencies: production client paths.
Acceptance: release Wasm cannot call mock/debug ledger/governance methods.
Required real-source tests: release client failure tests.
Affected invariants: production/mock separation.

### Replace Jupiter mock issuance with an exact production ICRC path
Why: Jupiter issuance currently uses mock-shaped reserve transfer.
Scope: exact reserve-to-Jupiter ICRC transfer, fee, attempt, duplicate proof.
Excluded: Jupiter reward amount policy.
Dependencies: fee ADR and durable attempts.
Acceptance: reserve and total supply reconcile after issuance.
Required real-source tests: real SNS ledger reserve-to-Jupiter transfer.
Affected invariants: no minting, reserve solvency.

### Unify runtime production wiring and activation state
Why: validation and execution can read different objects.
Scope: one canonical wiring source and activation state.
Excluded: deploying or enabling active mode.
Dependencies: P0 client path decisions.
Acceptance: `ProductionActive` cannot validate from one object and execute from another.
Required real-source tests: install/upgrade wiring equality tests.
Affected invariants: activation safety.

### Add canonical bootstrap reconciliation and zero-liquid guard
Why: caller-supplied state and 1:1 zero-liquid rate can misprice operations.
Scope: startup reconciliation against ledger/governance sources and zero-liquid fail-closed.
Excluded: monetary policy redesign.
Dependencies: fee/supply ADR.
Acceptance: no monetary execution before canonical reconciliation.
Required real-source tests: real ledger/governance startup snapshots.
Affected invariants: solvency, redemption rate.

## P1

### Add durable cohort history/event association and unblock stale events
Why: stale or same-second events must not block cursors indefinitely.
Scope: durable association/liveness only.
Excluded: reward eligibility/allocation changes.
Dependencies: current exact two-week semantics.
Acceptance: stale events are recorded and cursor liveness is deterministic.
Required real-source tests: delayed/same-second maturity events.
Affected invariants: reward liveness.

### Implement global ledger/archive traversal where required
Why: account-only scans cannot prove global supply effects.
Scope: ledger/index/archive traversal and completeness state.
Excluded: fee policy selection.
Dependencies: fee/supply ADR.
Acceptance: archive gaps fail closed.
Required real-source tests: archive-involved transfer history.
Affected invariants: canonical proof.

### Bound journals, processed IDs and stable-state growth
Why: unbounded state can threaten upgrades.
Scope: pruning, checkpointing, retention and migration tests.
Excluded: deleting audit-required records without policy.
Dependencies: historian/audit requirements.
Acceptance: bounded worst-case storage with upgrade proof.
Required real-source tests: long-history local evidence.
Affected invariants: upgrade safety.

### Implement production NNS governance mutations and canonical proof
Why: current NNS paths are not production mutation proof.
Scope: NNS governance calls, proof, idempotency.
Excluded: mainnet execution.
Dependencies: durable attempts.
Acceptance: mutations are real-shaped and auditable.
Required real-source tests: real NNS canister local/PocketIC proof.
Affected invariants: NNS lifecycle correctness.

### Implement production SNS governance observations
Why: reward eligibility depends on canonical SNS governance state.
Scope: list neurons/proposals, pagination, participation proof.
Excluded: reward policy changes.
Dependencies: real SNS framework fixtures.
Acceptance: observations are complete or fail closed.
Required real-source tests: finalized SNS governance observations.
Affected invariants: reward eligibility evidence.

### Add internal non-overlapping timers only after P0 remediation
Why: timers automate value movement.
Scope: guarded timer registration and no-overlap execution.
Excluded: enabling before P0 gates.
Dependencies: all P0.
Acceptance: timers cannot run without activation gates.
Required real-source tests: upgrade/resume/no-overlap tests.
Affected invariants: operational safety.

### Complete all-real lifecycle, failure and upgrade tests
Why: mock/SNS-shaped tests are insufficient.
Scope: full real-stack lifecycle, failures, upgrades.
Excluded: mainnet launch.
Dependencies: real Wasm artifact pinning.
Acceptance: no ignored blocker remains for P0/P1 flows.
Required real-source tests: all real framework tests.
Affected invariants: end-to-end production proof.

### Governance/controller/emergency policy
Why: controller and recovery paths are production-critical.
Scope: controllers, recovery, emergency actions, runbooks.
Excluded: executing mainnet changes.
Dependencies: final launch architecture.
Acceptance: approved governance/controller plan.
Required real-source tests: local SNS governance/controller rehearsal.
Affected invariants: authorization and recovery.

### Production historian and monitoring
Why: public status must not invent truth.
Scope: real source ingestion, freshness, alerts, dashboards.
Excluded: monetary execution.
Dependencies: canonical sources.
Acceptance: stale/missing/incomplete states are explicit.
Required real-source tests: historian rebuild from real sources.
Affected invariants: public truthfulness.

### Final SNS tokenomics and launch configuration
Why: local/test config is not final.
Scope: final SNS init, fees, supply, distribution, voting parameters.
Excluded: launching.
Dependencies: ADR and governance decisions.
Acceptance: config reviewed and validated.
Required real-source tests: official local SNS rehearsal.
Affected invariants: tokenomics correctness.

### Hermetic builds and external audit
Why: release artifacts and source must be independently reproducible.
Scope: reproducibility, audit package, security review.
Excluded: feature implementation.
Dependencies: stable codebase.
Acceptance: reproducible build and audit readiness gates pass.
Required real-source tests: artifact verification from source.
Affected invariants: supply-chain integrity.

### Mainnet testflight and launch
Why: final activation requires controlled production evidence.
Scope: testflight, launch checklist, production deployment.
Excluded: this tranche.
Dependencies: all P0/P1 launch gates.
Acceptance: explicit mainnet-approved work order.
Required real-source tests: production/testflight evidence.
Affected invariants: production safety.
