# IO Suite

Initial Rust workspace for the IO protocol suite, following the Jupiter Faucet suite style without copying unnecessary canister boundaries.

## Canisters

- `io_nns_neuron_manager` — NNS-only operational canister. It controls/manages the existing 2-year IO NNS neuron and a pooled 2-week NNS neuron strategy. It does **not** issue IO or calculate IO rewards.
- `io_stream_manager` — main monetary-policy canister. It scans/classifies ICP streams, applies the 40/60 split, issues backed IO when the stream type requires it, handles redemptions, and computes IO SNS-staker entitlement.
- `io_historian` — placeholder read-model canister.
- `frontend` — placeholder frontend canister.

Known live NNS neuron configuration:

```text
IO 2-year NNS neuron id: 6345890886899317159
Current controlling canister principal: oae4c-3iaaa-aaaar-qb5qq-cai
```

## Tests

Run the whole first-pass suite:

```bash
cargo run -p xtask -- test_all
```

Useful subsets:

```bash
cargo run -p xtask -- test_unit
cargo run -p xtask -- test_pocketic_integration
cargo run -p xtask -- test_local_integration
cargo run -p xtask -- stream_manager_unit
cargo run -p xtask -- nns_neuron_manager_unit
```

The current PocketIC and local-integration tests are deterministic Rust harnesses that model the intended flows. They are deliberately shaped so they can later be replaced with real PocketIC and `icp-cli` deployments while preserving the same `xtask` entry points.

## Build

This repo intentionally uses `icp.yaml`/`icp-cli` style configuration, not `dfx.json`, matching the Jupiter Faucet suite convention.

## Expanded test suite

Run the full suite with:

```bash
cargo run -p xtask -- test_all
```

This now runs unit, PocketIC-shaped integration, local CLI-shaped integration, and e2e model tests. The most important added coverage is:

- 2-year maturity strengthens backing and issues no IO.
- 2-week maturity issues backed IO to eligible IO SNS neurons.
- Fast-forwarded maturity accrual in `io_nns_neuron_manager`.
- Two-week unwind splits and cancel-dissolve merge-back behaviour.
- Duplicate transaction/idempotency checks.
- Unknown-source rejection.
- Reward weighting by stake-time and closed-proposal participation.
- End-to-end stream flow across the NNS manager and stream manager models.

The PocketIC tests are currently deterministic model harnesses. They are deliberately named and structured as PocketIC tests so the next implementation phase can replace the in-memory fast-forward helpers with real PocketIC time advancement and canister calls while preserving the same command surface.
