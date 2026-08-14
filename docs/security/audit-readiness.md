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
- Jupiter, maturity, and passive unwind: `jupiter-integration-contract.md`, `nns-neuron-manager.md`, and the composition evidence.
- Timers/upgrades: `scheduler.md`, `upgrades.md`, `stable-storage.md`, and `journal-compaction.md`.
- SNS authority/controllers: `sns-root-lifecycle.md`, `docs/security/controller-and-recovery.md`, and `docs/operations/production-wiring.md`.
- Historian non-authority: `historian.md`, `historian-ingestion.md`, and `docs/operations/historian-freshness.md`.

## Evidence classification

| Classification | Evidence |
| --- | --- |
| Proved locally | `docs/testing/e2e-coverage-matrix.md`; exact-source release verification; immutable historical corrected package `deploy/local-sns-rehearsal/evidence/2026-08-12-4320fdf-canonical-economics/` for its recorded release; immutable current package `2026-08-14-4320fdf-canonical-economics` selected for S4 `55b2099a555799c4a032308eb8a39049c7946193` and A4 `09b115f708ec784766327539f9cf4e5e21668d84`; historical mechanics/connectivity packages `2026-08-11-4320fdf` and `2026-08-12-4320fdf-monitoring`, whose redemption economics are superseded under `docs/operations/local-sns-evidence-disposition.md`. |
| Candidate-only | Same-source SNS Governance/Root at IC `4320fdf2e613844eabae1927b1a23b98da3a7bc6`, including reward-share capability and Governance → Root compatibility. |
| Officially available | Pinned official baseline artifacts/tooling in `tests/e2e_real_canisters/wasms.example.toml`; these do not imply official reward-share adoption. |
| External fixture gaps | Real transport-fault injection and non-1.0 maturity modulation, classified in `docs/operations/remaining-work.md`. |
| Not yet audited | Final source/artifacts, launch configuration, dependencies/licenses, controller recovery, and all local evidence require independent review. |
| Mainnet-only | Protected-position audit, production IDs/config, testflight, install/controller handoff, funding, and activation; each requires separate authorization. |

Auditors should run `docs/operations/release-checklist.md`, independently rebuild the recorded source, confirm production Rust is at most 11,100 lines, and verify the final local package is byte-bound to the manifest. Internal validation is not external approval.
