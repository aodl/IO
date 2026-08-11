# Historian freshness operations

The historian is a public read model that is rebuildable, not canonical protocol truth, and not a value-moving authority. IO protocol is not live and the SNS IO ledger remains not launched on mainnet.

Every configured source reports `Fresh`, `Stale`, `Missing`, or `ErrorRetryable`; an unconfigured prelaunch historian reports `PrelaunchNotConfigured`. The missing/stale/error observations remain visible and must not be interpreted as zero. A failed retry updates the attempt/error fields without erasing the last-known successful timestamp or values.
Stored error text is capped, as are Root inventories/controllers, operation
labels, Governance metadata and index transaction kinds; a canonical response
outside those bounds becomes a retryable source error rather than unbounded
stable state.

Freshness is based on current historian/canister time and the timestamp of the coherent refresh generation. An upgrade deliberately changes previously fresh source state to stale, clears the transient in-progress marker, and re-arms one timer. When no newer observations arrive, freshness is never silently extended.

The frontend consumes the historian's source-health DTO directly. It does not infer health from a balance, substitute an empty list for a canonical zero, or import debug/value-moving canister declarations.

The index canisters remain the normal Account-history abstraction. Current reserve balances come from the ledgers. SNS Root supplies controller/module/archive topology. The protected canister and neuron are not configured historian sources.

Run the no-network guardrail with:

```bash
cargo run -p xtask -- validate_historian_freshness
```

It checks the typed upgrade configuration, read-only production DID, no public ingestion/configuration method, bounded timer/adapters, explicit freshness/error states, frontend declaration parity, prelaunch inert wiring, and protected-reference exclusions.
