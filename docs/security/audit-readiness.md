# External audit package index

This is the handoff index for an independent auditor. It is not a self-audit and does not claim external approval. IO remains prelaunch, Paused/inert where applicable, and not live.

## Recorded release identity

- Source-finalization commit:
  `270dcf3dc71fc8e7b63c3177b0e3f58fc9246b35`.
- Immediate artifact-recording commit:
  `dc548f555a808f59e6a6c69759cc41fbb7f1f54d`.
- Canonical evidence commit:
  `608baa58425389827acf3a5c051d71160cd829f7`.
- Current schema-v2 package:
  `deploy/local-sns-rehearsal/evidence/2026-09-03-270dcf3-anchored-dynamic`.
- Release-manifest SHA-256:
  `6d666a69d3d74a30f45510a23facd2cbc6d0790043dbbea49f96874620428637`.
- Package-manifest SHA-256:
  `023a8f10893f94e871012397d901d0e7e3632690b55269df123fd741f9b30f3c`.
- Package `SHA256SUMS` SHA-256:
  `4ac11006cace6ef74a762fed9d8a4ff55cdfc5c619dc7d2df4156a265d8543c8`.
- Raw/gzip Wasm hashes and exact-source equality: `release-artifacts/manifest.json` and `docs/operations/reproducible-builds.md`.
- Production interfaces: the four production DIDs under `canisters/`.
- Dependency/tooling evidence: `Cargo.lock`, `package-lock.json`, `deny.toml`, and `docs/security/dependency-and-supply-chain.md`.
- Source-open package: `LICENSE` and `docs/security/source-open-package.md`.

Those hashes identify the selected release/evidence authority. The package
records the anchored Dynamic neuron, independent structural scheduler,
replenish-first TwoYear maturity, natural cohorts, and the redemption transport
of that immutable source revision. Semantic-staging redemption in the supplied
source requires a fresh separately authorized evidence package. Older
release pairs and packages remain immutable evidence for their own releases.

## Normative architecture and invariants

- Simplicity and authority: `docs/architecture/simplicity-constitution.md`, `canister-roles.md`, and `api-surface.md`.
- Accepted replacement ADRs: `adr-anchored-dynamic-backing.md` and
  `adr-semantic-redemption-staging.md`. The pooled
  allocation and complexity ADRs are retained as explicitly superseded history.
  Other accepted ADRs include
  `adr-daily-sns-entitlement-events.md`,
  `adr-io-ledger-fee-and-supply-authority.md`, and
  `adr-nns-authority-location.md`. The separate-endowment ADR is superseded.
- Monetary policy: `monetary-policy.md`, `fee-dust-accounting.md`, `reward-allocation.md`, and `sns-eligibility.md`.
- Redemption/idempotency/ambiguity: `stream-manager.md`, `docs/operations/p0-simplified-composition-evidence.md`, and `docs/security/threat-model.md`.
- Effect ordering: immutable intent precedes every potentially irreversible
  effect; definite success is canonically re-observed and may continue;
  ambiguity or an absent required postcondition stops dependent work. Public
  progress exposes real action boundaries rather than internal phases.
- Jupiter activation/replay boundary and residual lookup risk, the permanent
  launch baseline, pre-Ready Dynamic parent, replenish-first maturity, and
  generation-based passive unwind with ready-child priority:
  `jupiter-integration-contract.md`,
  `nns-neuron-manager.md`, and the composition evidence.
- Availability and stable growth: `docs/operations/cycles-management.md`,
  `stable-storage.md`, and the permissionless-endpoint rows in the threat
  model.
- Reward settlement: the Stream Manager README and daily-entitlement ADR
  distinguish exact IO ledger delivery from one bounded, observable,
  best-effort SNS `ClaimOrRefresh` attempt.
- Reward activation: official SNS Governance's round-zero dummy genesis event
  is frozen as a zero-credit baseline. Its exact replay is structural only;
  advancing and credit-bearing events require the ordinary positive-span and
  nonzero-round proofs. Redemption remains available before round one.
- Autonomous execution: Stream/NNS install and upgrade reopen Paused, recover
  known durable work, and retry canonical readiness until safe to enter Ready.
  Protected two-year ordinary maturity uses the existing maturity state machine
  on a weekly volatile timer. Routine lifecycle, continuation, and maturity
  harvesting require no SNS generic function or proposal. Rare exact-block
  proof remains caller-independent and fail-closed over the persisted intent.
- Timers/upgrades: `scheduler.md`, `upgrades.md`, `stable-storage.md`, and `journal-compaction.md`.
- SNS authority/controllers: `sns-root-lifecycle.md`, `docs/security/controller-and-recovery.md`, and `docs/operations/production-wiring.md`.
- Historian non-authority: `historian.md`, `historian-ingestion.md`, and `docs/operations/historian-freshness.md`.

## Evidence classification

| Classification | Evidence |
| --- | --- |
| Proved locally | The selector-bound `2026-09-03-270dcf3-anchored-dynamic` package proves its exact source revision: Layer A source-built official SNS launch/wiring and live-local observations, Layer B exact proposal-143660 NNS mechanics, and Layer C that revision's anchored IO economics and controlled recovery. It is immutable historical evidence, not staging-redemption evidence for this source. `docs/testing/e2e-coverage-matrix.md` identifies the current executable coverage pending a fresh stateful rehearsal. |
| Candidate-only | Same-source SNS Governance/Root at IC `4320fdf2e613844eabae1927b1a23b98da3a7bc6`, including reward-share capability and Governance → Root compatibility. The separately reviewed official lock remains `b904c9dd1bdef8841bd12f03efbc71180a015e25`; local source proof does not establish official adoption. |
| Officially available | Pinned official baseline artifacts/tooling in `tests/e2e_real_canisters/wasms.example.toml`; these do not imply official reward-share adoption. |
| External fixture gaps | Real transport-fault injection and non-1.0 maturity modulation, classified in `docs/operations/remaining-work.md`. |
| Not yet audited | Final source/artifacts, launch configuration, dependencies/licenses, controller recovery, and all local evidence require independent review. |
| Mainnet-only | Protected-position audit, production IDs/config, testflight, install/controller handoff, funding, and activation; each requires separate authorization. |

## Current simplicity diagnostics

The selected source measures:

| Component | Hardened pre-refactor | Current | Change |
| --- | ---: | ---: | ---: |
| Stream Manager | 5,416 | 5,215 | -201 |
| NNS Manager | 6,002 | 7,073 | +1,071 |
| Combined governed production | 14,187 | 15,196 | +1,009 |

`docs/architecture/adr-pooled-claim-backing-complexity-exception.md` and
`cargo run -p xtask -- simplicity_check` are current authority. The command
prints raw LOC as diagnostic review information and continues to enforce
semantic architecture boundaries. The former 5,520 / 6,125 / 14,485 ceilings
and older 11,100-line recalibration ADR remain historical review records, not
launch correctness criteria.

Auditors should run `docs/operations/release-checklist.md`, independently
rebuild the recorded source, run the executable simplicity check, verify all
required workflows belong to the exact reviewed head, and verify the selected
local package is byte-bound to the release manifest. Internal validation is not
external approval.
