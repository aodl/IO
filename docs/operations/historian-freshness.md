# Historian Freshness Operations

The historian freshness model is a no-network validation and display layer. It reports source freshness for a public read model that is rebuildable, not canonical protocol truth, and not a value-moving authority.

Freshness thresholds are conservative code constants in `canisters/io_historian/src/lib.rs`: protocol/reserve/dashboard observations use one-hour thresholds, ICP index health uses a six-hour threshold, canister status and governance freshness use one day, and release artifacts use seven days. Frontend display must consume these source-health DTOs and must not silently infer its own protocol status from missing timestamps.

Source health is evaluated against current historian/canister time. Observation timestamps are source timestamps or source watermarks, not the dashboard's concept of "now". A source can become stale as historian time advances even when no newer observations arrive.

Dashboard source health must make missing/stale/incomplete observations visible. Missing/stale/incomplete fields must not be interpreted as zero protocol value. Observed-only release artifact data means observed artifact manifest state, not audited reproducible release status.

The previous frontend/historian shell is `DevMainnet` only. Those IDs are superseded as production targets, retained only as dev/test canisters, not on the fiduciary subnet, and not production IO protocol canisters. IO protocol is not live. SNS IO ledger remains not launched. IO issuance is not live and IO redemption is not live. Production fiduciary canisters are reserved empty/inert placeholders with no value-moving Wasm installed.

The protected neuron-owner canister and IO NNS neuron are protected/untouched references. Do not use historian freshness work to mutate or deploy to `oae4c-3iaaa-aaaar-qb5qq-cai` or neuron `6345890886899317159`.

Index canisters are the normal account-history abstraction; index canisters remain the default source for account-history observations. Raw ledger/archive traversal is not the default operational path. Archive-required states can be surfaced as source-health warnings for future adapter work.

Run the static freshness gate with:

```bash
cargo run -p xtask -- validate_historian_freshness
```

The command makes no network calls. It checks historian debug methods are absent from production DID, value-moving production DIDs remain constructor-only, prelaunch state does not claim live protocol or launched SNS IO ledger, protected references are not deployment targets, stale/missing/incomplete source states are represented, frontend declarations match the production historian DID, and frontend code does not import `.dfx`, `src/declarations`, or debug declarations.
