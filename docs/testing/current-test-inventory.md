# Current simplified test inventory

- `io-core-model`: checked stateless economics only.
- `io-reward-policy`: 18 unchanged exact allocation tests.
- `io-stream-manager` unit tests: typed intents, canonical Accounts, quote/postconditions and V1 persistence.
- `io-nns-neuron-manager` unit tests: checked split, direct maturity policy and target idempotency.
- `tests/pocketic/io_stream_manager_pocketic.rs`: Paused install and narrow ingress safety.
- `tests/pocketic/io_nns_neuron_manager_pocketic.rs`: Paused install and authority rejection.
- `e2e-real-canisters`: pinned real SNS ledger primitives and installed serialized redemption.
- `tools/scripts/run-io-stream-manager-live-pocketic`: focused live PocketIC driver for stream-manager recovery checks.
- `tools/scripts/provision-pocket-ic`: immutable PocketIC 14.0.0 HTTPS provisioning with executable SHA-256 verification, used by ordinary PR CI before required live tests.
- `cargo run -p xtask -- live_stream_manager_pocketic_gate_check`: static guard for that focused driver.
- `xtask simplicity_check`: source, DID, documentation, schema and complexity guardrails.

Historical scanner/journal coverage does not describe the launch protocol.
