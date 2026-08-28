# External audit package index

This is the handoff index for an independent auditor. It is not a self-audit and does not claim external approval. IO remains prelaunch, Paused/inert where applicable, and not live.

## Recorded release identity and source-only successor

- Source-finalization commit:
  `9462a1a0df602f06fa845bd31f9fcd0adf80067a`.
- Immediate artifact-recording commit:
  `46713f8499cf9a63a6cd4879b1fff9c1f9ef0be5`.
- Canonical evidence commit:
  `1ed1399130358ed5788cae99f3d65d82cbbc70a9`.
- Current schema-v2 package:
  `deploy/local-sns-rehearsal/evidence/2026-08-26-9462a1a-account-semantic`.
- Release-manifest SHA-256:
  `011113d83510e66976f5d3eabefc57ef30ba44fc49a8973fad29be043b374431`.
- Package-manifest SHA-256:
  `254e8866394d62ba9fbe2b0290709d2c9dbaeb8ed84c8b3446b9a1912af0eb78`.
- Package `SHA256SUMS` SHA-256:
  `a1915cef319b569955c5d75ad88c4a3715af8483573d2cf9b3a2af108d6bea29`.
- Raw/gzip Wasm hashes and exact-source equality: `release-artifacts/manifest.json` and `docs/operations/reproducible-builds.md`.
- Production interfaces: the four production DIDs under `canisters/`.
- Dependency/tooling evidence: `Cargo.lock`, `package-lock.json`, `deny.toml`, and `docs/security/dependency-and-supply-chain.md`.
- Source-open package: `LICENSE` and `docs/security/source-open-package.md`.

Those hashes remain the last immutable recorded release/evidence identity. The
pooled-parent identity hardening and execution/API simplification on this branch
postdate it. They deliberately do not regenerate or rebind artifacts or the
canonical package; a new exact source-finalization, artifact-only child, and
fresh rehearsal/evidence tail are required after this source is accepted.

## Normative architecture and invariants

- Simplicity and authority: `docs/architecture/simplicity-constitution.md`, `canister-roles.md`, and `api-surface.md`.
- Accepted ADRs: `adr-pooled-claim-backing-allocation.md`,
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
  launch baseline, lazy pooled parent, maturity, and bounded passive unwind:
  `jupiter-integration-contract.md`,
  `nns-neuron-manager.md`, and the composition evidence.
- Availability and stable growth: `docs/operations/cycles-management.md`,
  `stable-storage.md`, and the permissionless-endpoint rows in the threat
  model.
- Reward settlement: the Stream Manager README and daily-entitlement ADR
  distinguish exact IO ledger delivery from one bounded, observable,
  best-effort SNS `ClaimOrRefresh` attempt.
- Timers/upgrades: `scheduler.md`, `upgrades.md`, `stable-storage.md`, and `journal-compaction.md`.
- SNS authority/controllers: `sns-root-lifecycle.md`, `docs/security/controller-and-recovery.md`, and `docs/operations/production-wiring.md`.
- Historian non-authority: `historian.md`, `historian-ingestion.md`, and `docs/operations/historian-freshness.md`.

## Evidence classification

| Classification | Evidence |
| --- | --- |
| Proved locally | The current selector-bound `2026-08-26-9462a1a-account-semantic` package first: Layer A source-built official SNS launch/wiring and live-local observations, Layer B exact proposal-143660 NNS mechanics, and Layer C current IO account semantics and controlled recovery. `docs/testing/e2e-coverage-matrix.md`, exact-source release verification, and the package's manifest/checksum inventory provide the cross-checks. Earlier 2026-08-11, 2026-08-12, 2026-08-14 and intermediate `2026-08-26-716d51e-account-semantic` packages are secondary immutable historical evidence for their own releases only. |
| Candidate-only | Same-source SNS Governance/Root at IC `4320fdf2e613844eabae1927b1a23b98da3a7bc6`, including reward-share capability and Governance → Root compatibility. The separately reviewed official lock remains `b904c9dd1bdef8841bd12f03efbc71180a015e25`; local source proof does not establish official adoption. |
| Officially available | Pinned official baseline artifacts/tooling in `tests/e2e_real_canisters/wasms.example.toml`; these do not imply official reward-share adoption. |
| External fixture gaps | Real transport-fault injection and non-1.0 maturity modulation, classified in `docs/operations/remaining-work.md`. |
| Not yet audited | Final source/artifacts, launch configuration, dependencies/licenses, controller recovery, and all local evidence require independent review. |
| Mainnet-only | Protected-position audit, production IDs/config, testflight, install/controller handoff, funding, and activation; each requires separate authorization. |

## Current simplicity diagnostics

The current source-only successor measures:

| Component | Hardened pre-refactor | Current | Change |
| --- | ---: | ---: | ---: |
| Stream Manager | 5,416 | 5,390 | -26 |
| NNS Manager | 6,002 | 6,020 | +18 |
| Combined governed production | 14,187 | 14,177 | -10 |

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
