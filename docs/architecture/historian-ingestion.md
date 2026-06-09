# Historian Ingestion Architecture

`io_historian` is a bounded public read model for production observations. It is rebuildable from source observations and is not canonical protocol truth. It is not a value-moving authority and must not become a control plane for issuance, redemption, reserves, neuron management, or SNS launch state.

Production ingestion is observation/freshness only. The current architecture defines source-shaped DTOs and traits for release artifacts, canister status/module hashes, ICP index health, future IO/SNS index health, NNS governance freshness, SNS governance freshness, protocol snapshots, reserve snapshots, and frontend/dashboard freshness. These interfaces are deterministic and testable; this milestone does not activate live production adapters, call mainnet, deploy anything, or move value.

The historian source model uses `HistorianIngestionSource`, `HistorianObservation`, `IngestionBatch`, `IngestionSourceKind`, `ObservationFreshness`, `SourceHealth`, `IngestionCursor`, `IngestionWatermark`, and `StalenessPolicy`. Freshness states explicitly represent fresh, stale, missing, incomplete, observed-only, prelaunch/not-applicable, error/retryable, and unknown observations. Missing/stale/incomplete states are visible, and missing/stale/incomplete fields must not be interpreted as zero protocol value.

Release artifact observations represent the observed artifact manifest, wasm/gzip hashes, byte sizes, git commit, module hash observations, match/mismatch/unavailable states, and freshness. They do not claim reproducible-build audit status.

Canister status observations distinguish Phase 1 public shell canisters from future value-moving canisters. Phase 1 frontend and historian can be shown as deployed public shell canisters. Future `io_stream_manager`, `io_nns_neuron_manager`, SNS root/governance/ledger/index, and future IO/SNS index observations remain not deployed/not allocated until a later milestone observes and configures them. The protected canister/neuron are protected/untouched references, not IO deployment targets.

Index health uses index canisters as the normal account-history abstraction. Raw ledger/archive traversal is not the default path. Index health observations can represent latest/head height, oldest/backfill cursor, account-history cursor state, lag, stale observations, incomplete scans, and archive-required states surfaced by ledger/index DTOs.

Governance freshness covers NNS and SNS-shaped observations without implying launch. IO protocol is not live. SNS IO ledger remains not launched. SNS governance missing because SNS has not launched is prelaunch/not-applicable, not an error.
