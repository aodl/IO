# External audit package index

This is the handoff index for an independent auditor. It is not a self-audit and does not claim external approval. IO remains prelaunch, Paused/inert where applicable, and not live.

## Release identity

- Final source and artifact-recording commits: `release-artifacts/manifest.json` and the final tranche report.
- Raw/gzip Wasm hashes and exact-source equality: `release-artifacts/manifest.json` and `docs/operations/reproducible-builds.md`.
- Production interfaces: the four production DIDs under `canisters/`.
- Dependency/tooling evidence: `Cargo.lock`, `package-lock.json`, `deny.toml`, and `docs/security/dependency-and-supply-chain.md`.
- Source-open package: `LICENSE` and `docs/security/source-open-package.md`.

## Normative architecture and invariants

- Simplicity and authority: `docs/architecture/simplicity-constitution.md`, `canister-roles.md`, and `api-surface.md`.
- Accepted ADRs: `adr-simplified-execution.md`, `adr-daily-sns-entitlement-events.md`, `adr-io-ledger-fee-and-supply-authority.md`, `adr-nns-authority-location.md`, and `adr-protected-reward-backing-nns-neuron.md`.
- Monetary policy: `monetary-policy.md`, `fee-dust-accounting.md`, `reward-allocation.md`, and `sns-eligibility.md`.
- Redemption/idempotency/ambiguity: `stream-manager.md`, `docs/operations/p0-simplified-composition-evidence.md`, and `docs/security/threat-model.md`.
- Jupiter activation/replay/throttling, both protected-neuron launch baselines,
  maturity, and passive unwind: `jupiter-integration-contract.md`,
  `nns-neuron-manager.md`, and the composition evidence.
- Availability and stable growth: `docs/operations/cycles-management.md`,
  `stable-storage.md`, and the permissionless-endpoint rows in the threat
  model.
- Reward settlement: the Stream Manager README and daily-entitlement ADR
  distinguish exact IO ledger delivery from the bounded, observable,
  retryable SNS `ClaimOrRefresh` result.
- Timers/upgrades: `scheduler.md`, `upgrades.md`, `stable-storage.md`, and `journal-compaction.md`.
- SNS authority/controllers: `sns-root-lifecycle.md`, `docs/security/controller-and-recovery.md`, and `docs/operations/production-wiring.md`.
- Historian non-authority: `historian.md`, `historian-ingestion.md`, and `docs/operations/historian-freshness.md`.

## Evidence classification

| Classification | Evidence |
| --- | --- |
| Proved locally | `docs/testing/e2e-coverage-matrix.md`; exact-source release verification for each recorded lineage; immutable 2026-08-12 and 2026-08-14 canonical packages as evidence for their own releases; and historical mechanics/connectivity packages 2026-08-11 and 2026-08-12-monitoring, whose redemption economics are superseded under `docs/operations/local-sns-evidence-disposition.md`. The selector-bound master-descended package remains truthful for its recorded pair, but a later hardening release needs fresh current evidence rather than rebinding it. |
| Candidate-only | Same-source SNS Governance/Root at IC `4320fdf2e613844eabae1927b1a23b98da3a7bc6`, including reward-share capability and Governance → Root compatibility. |
| Officially available | Pinned official baseline artifacts/tooling in `tests/e2e_real_canisters/wasms.example.toml`; these do not imply official reward-share adoption. |
| External fixture gaps | Real transport-fault injection and non-1.0 maturity modulation, classified in `docs/operations/remaining-work.md`. |
| Not yet audited | Final source/artifacts, launch configuration, dependencies/licenses, controller recovery, and all local evidence require independent review. |
| Mainnet-only | Protected-position audit, production IDs/config, testflight, install/controller handoff, funding, and activation; each requires separate authorization. |

Auditors should run `docs/operations/release-checklist.md`, independently rebuild the recorded source, confirm production Rust is at most 11,100 lines, verify all required workflows belong to the exact reviewed head, and verify the final local package is byte-bound to the manifest. Internal validation is not external approval.
