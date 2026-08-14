# Historian ingestion architecture

`io_historian` is a bounded public read model that is rebuildable, not canonical protocol truth, and not a value-moving authority. IO protocol is not live and the SNS IO ledger remains not launched on mainnet. The historian never authorizes issuance, redemption, reserve movement, neuron commands, lifecycle activation, or launch state.

Production adapters are activated only by a typed controller/SNS Root upgrade argument. There is no public configure or ingest method. An absent configuration performs no network observation. A same-Wasm upgrade preserves configuration unless an explicit complete replacement is supplied.

Each one-shot timer runs at most one generation. Ledger balances needed for the redemption display are fetched as one coherent batch. Any missing reply, overflow, zero denominator, or `total < reserve + excluded` leaves the last-known snapshot unchanged and records an explicit retryable error; missing/stale/error is never zero. The historian's rate is explanatory and never an input to `redeem`.

Canonical source adapters are deliberately direct:

- SNS and ICP ledgers for current supply/balances;
- Stream and NNS manager narrow `get_status` methods, including the manager's exact latest target/status and passive-unwind principal;
- public NNS Governance build metadata and two bounded configured neuron-info queries; ordinary maturity remains unavailable rather than reconstructed or obtained through controller impersonation;
- SNS Root summaries for installed module hashes, controllers, SNS topology, dapps, and archives;
- SNS Governance parameters and latest reward event;
- SNS Index status and bounded recent Account histories.

The index canisters are observation inputs only and never authorize monetary action.

Index canisters are the normal account-history abstraction. Ledgers remain canonical for current balances. Archives are discovered, but raw archive traversal is not a default path and no event or monetary scanner exists.

Expected release identities are typed `(role, canister, raw Wasm SHA-256)` entries. Observations distinguish matching, mismatch, unavailable, and unknown. Inability to observe is not a mismatch. Reward-share availability is an optional exact capability-bearing Governance hash that must equal the expected Governance hash. The reviewed local candidate supplies it; an official module without the reviewed field leaves it absent and is reported capability unavailable. Matching the candidate hash does not claim an official capability-bearing release exists.

Frozen-cohort totals, proposal participation ratios, generic ingestion traits, generic cursors, and scanner-era histories were removed from the current model. A narrow legacy decoder exists only for stable upgrade compatibility.
