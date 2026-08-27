# Historian architecture

`io_historian` is IO's public read model. It is rebuildable, not canonical protocol truth, and not a value-moving authority. IO protocol is not live and the SNS IO ledger remains not launched on mainnet.

Its production DID is read-only. A typed optional install/upgrade argument is the only configuration path: no-config is inert; explicit SNS Root/controller upgrade installs or replaces the complete observation topology; a normal same-Wasm upgrade preserves it.

The autonomous one-shot refresh observes coherent ledger supply/reserve inputs, Stream and NNS manager status, SNS Root module/controller/archive topology, SNS Governance parameters/reward freshness, and bounded SNS Index Account histories. The index canisters are the normal history abstraction; current balances remain ledger-derived.

The displayed rate is:

```text
redeemable IO = total IO - protocol reserve IO - configured excluded IO
liquid ICP per IO = liquid ICP reserve / redeemable IO
```

All arithmetic is checked. The monetary snapshot commits total supply, reserve,
excluded balances, liquid ICP, denominator, and rate together or retains the
last successful snapshot with an error; it does not combine partial monetary
generations. Other source sections commit independently and carry their own
freshness/success timestamps, so the dashboard does not claim global atomicity.
The missing/stale/error state is never zero, and the historian's rate never
authorizes redemption.

Root-mediated observations distinguish module matching, mismatch, unavailable, and unknown, and retain observed controllers. Stream status supplies reward classification and live/pending credits; the historian does not reconstruct ballots or run another event scanner. NNS manager status is observed without issuing NNS commands.

Stable state retains configuration, last-known observations, timestamps, and errors. Upgrade accepts only the strict launch V1 snapshot, clears the transient refresh marker, marks prior fresh data stale, and re-arms one timer. Obsolete pre-launch state does not decode.

Production reservations remain empty/inert until separately authorized launch work. The protected canister and neuron are not configured sources or deployment targets.
