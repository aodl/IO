# IO

IO is an Internet Computer/Rust protocol workspace. IO is not live: production canisters remain reserved/inert, production activation is unavailable, and this repository performs no mainnet deployment.

The launch architecture follows [the simplicity constitution](docs/architecture/simplicity-constitution.md): explicit authenticated commands, canonical balances, typed serialized operations, exact proofs for rare ambiguity, and historian-only global observation.

## Monetary roles

- `io_stream_manager`: direct ICRC-2 redemption, IO and liquid ICP reserves, proof-bound receipts, daily entitlement accumulation and one pending backed batch.
- `io_nns_neuron_manager`: protected NNS neurons, Jupiter 40/60 execution, direct maturity and one unwind child. Production authority is intended to remain at existing neuron controller `oae4c-3iaaa-aaaar-qb5qq-cai`; no mainnet action is authorized here.
- `io_historian`: ledger/index/archive histories, monitoring and public read models; never monetary authority.
- `frontend`: advisory approval/redemption and historian views.

Install always starts Paused. Governance readiness validates self-bound configuration, canonical fees and ledger standards. Each update invocation performs at most one external monetary or governance effect. Unsupported direct transfers create no claim and are not automatically refunded.

## Development

Use the repository Rust toolchain and locked dependencies. Principal gates are:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo run -p xtask -- simplicity_check
cargo run -p xtask -- test_all
cargo run -p xtask -- verify_release
```

PocketIC uses `/home/codexdev/.local/bin/pocket-ic-server` when available. Required workflows never contact mainnet and never use `dfx sns`.

Reserved mapping record: `io_stream_manager` `thset-pqaaa-aaaar-qb7wa-cai`; `io_nns_neuron_manager` `tatch-ciaaa-aaaar-qb7wq-cai` (unused); `io_historian` `tjqj3-uaaaa-aaaar-qb7xa-cai`; `frontend` `torpp-zyaaa-aaaar-qb7xq-cai`.
