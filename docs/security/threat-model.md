# Threat Model

This threat model tracks the current mock-driven IO implementation and the guardrails added before real ledger/NNS/SNS clients.

| Threat | Current Mitigation | Tests / Guardrails | Remaining Gaps | Future Hardening |
| --- | --- | --- | --- | --- |
| Unauthorized stream classification | Production flow is intended to be ledger/index observed, not caller asserted. Debug stream methods are absent from production DIDs. | `cargo run -p xtask -- did_surface`; scheduler tests for source/memo classification. | Real index clients are not implemented. | Verify source accounts/subaccounts and memos against real ledgers/indexes. |
| Unbacked IO issuance | Monetary model issues IO only from backed flows and excludes protocol reserve/governance supply from redeemable supply. | Unit/e2e tests for Jupiter Faucet, 2-year, and 2-week flows. | Real IO ledger mint/transfer client is not audited. | Audit real ledger transfer semantics and supply reconciliation. |
| Redemption double-pay | Redemption journal separates ICP payout and IO return phases. Completed phases are not repeated. Downstream transfer paths use `LedgerTransferClient`; duplicate transfer errors are modelled as idempotency signals only after amount/account/memo proof. | Retry and upgrade-before-retry tests; scheduler boundary duplicate-proof tests; `io-ledger-types` duplicate proof tests. | Real ledger/index finality, production scan adapters, and archive traversal are not integrated. | Idempotency keys tied to real ledger blocks and transfer memos. |
| Redemption IO-return replay | IO return status and block are persisted before completion. | Journal replay tests. | Real IO ledger duplicate detection remains future work. | Use ledger block ids and subaccounts as durable replay guards. |
| Stuck pending journal operations | Pending operations persist phase, retry count, last error, and downstream block data. | Stable-state and PocketIC upgrade/retry tests. | No production operator read API by policy. | Historian/governance-observed operational reporting. |
| Cursor rewind/replay | Scheduler cursors advance only after observed work is represented safely; processed transaction ids remain duplicate guards. | Cursor and duplicate replay tests. | Archive gap handling is not production-wired. | Real index archive pagination and monotonic cursor proofs. |
| Index lag/archive gaps | Current tests use mock indexes and ledger histories. Debug/PocketIC scan sources still use mock `debug_get_transactions`. Boundary types model lag, archive-required states, missing blocks, duplicate block indexes, and cursor gaps. | Mock index tests exercise scans and cursors; scheduler boundary cursor tests. | Real ICP/IO index clients and archive fetches are missing. | Explicit lag detection, archive range validation, and alerting. |
| Mock/test assumptions leak into production | Debug APIs live in debug DIDs and are checked out of production DIDs. | DID guardrail and exact Wasm method string scan. | Rust runtime strings can cause false positives if scan is too broad. | Metadata/export inspection once final Wasm metadata policy is defined. |
| Broad production API exposed | Value-moving production DIDs must remain install-args-only. | `did_surface` rejects broad/debug methods and checks service shape. | Manual review still required for hidden behavior changes. | CI required DID checks and external audit review. |
| Artifact substitution | Raw/gz artifacts have SHA sidecars and a manifest. | `verify_artifacts` checks sidecars, manifest, sizes, and stale files. | Builds are not fully hermetic. | Reproducible container/Nix builds and multi-builder comparison. |
| Dependency compromise | Cargo.lock is respected and security tooling is wired. | `security_scan_required` runs cargo-deny, cargo-audit, and duplicate tree reporting. | Policy starts permissive for duplicates/licenses. | Tighten deny policy after reviewing the dependency graph. |
| Controller compromise | Controller model and recovery expectations are documented. | Release checklist requires no mainnet calls and artifact hash checks. | Final SNS handoff process is unresolved. | SNS-root controlled upgrades and emergency governance procedures. |
| Malicious/buggy upgrade | Stable snapshots and journals are tested across upgrades. | PocketIC upgrade-before-retry tests; artifact verification. | Stable layout is not audited for mainnet scale. | Audit storage migrations and run proposal payload verification. |
| Frontend misinformation | Frontend is not a source of protocol truth. | Docs separate frontend from value-moving canisters. | Certified assets/read-model plan is missing. | Certified historian/frontend and source-of-truth links. |
| Historian divergence | Historian is a read-model canister, not a value-moving authority. | Placeholder historian has a narrow public DID. | Real reconstruction logic is not implemented. | Source-ledger reconciliation and divergence alerts. |

## Production API Policy

For `io_stream_manager` and `io_nns_neuron_manager`, the production DID shape remains:

```did
service : (InitArgs) -> {}
```

Debug/test APIs may exist only in debug DIDs and test builds.
