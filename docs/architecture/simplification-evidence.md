# Simplified execution evidence

Recorded 2026-07-31 on branch `p0-simplified-execution`.

## Deletion and complexity

The comparison base is frozen research commit
`85b6c1c2e879348091226d2494aa2a5831bd473a`.

| Metric | P0 v2 donor | Simplified tree |
|---|---:|---:|
| Combined Rust LOC in both value-moving `src/` trees, `io_core_model`, and `io_ledger_types` | 36,925 | 6,600 |
| Stream-manager production Rust LOC | over 20,000 | 1,506 |
| Production DID methods, stream / NNS | 0 / 0 | 7 / 6 |
| Immediate operation variants, stream / NNS | generic bag | 2 / 3 |
| Launch value-moving stable schemas | migration chains | V1 / V1 |
| Largest production source file | over 13,000 LOC | under 500 LOC |

The value-moving comparison diff is 32,436 deletions and 2,121 insertions.
The combined LOC reduction is 82.1%, exceeding the 50% gate. No replacement
production source file exceeds 1,500 lines. Stream operation records have fewer
than 20 fields and fewer than five optional fields.

The release-target build produced local, uncommitted Wasm only:

| Canister | Simplified release-target Wasm |
|---|---:|
| `io_stream_manager` | 1,572,860 bytes |
| `io_nns_neuron_manager` | 1,065,559 bytes |

Tracked release artifacts were not regenerated or committed.

## Deleted production paths

- ICP and IO index clients from stream-manager production code
- account-history scheduler and cursor state
- automatic source-event discovery
- redemption intake and reserve-return leg
- rejected-redemption refund and quarantine machinery
- generic stream operation field bag and journal
- complete-range absence recovery
- precomputed monetary post-state authority
- prelaunch value-moving migration chains
- debug/mock real-stack execution paths coupled to the old scheduler

Historian and independent real-ledger test code retain observation DTOs where
appropriate.

## Release mock/debug classification

Occurrences outside deleted value-moving paths classify as follows:

| Term | Classification |
|---|---|
| `MockLedgerCanisterClient` | absent from implementation; name remains only in the `simplicity_check` forbidden-term list |
| `mock_account`, `mock_subaccount` | test/debug-only |
| `debug_get_transactions` | test/debug-only mock ledger |
| `debug_disburse_maturity` | test/debug-only mock governance |
| `created_at_time: None`, `fee: None` | fixtures/read DTOs; own production transfer attempts require both |
| `LedgerIndexClient`, `AccountHistoryScanState` | historian/test observation library only; absent from value-moving canisters |

`cargo run -p xtask -- simplicity_check` enforces the value-moving boundary.

## Executed real-source evidence

Using the locally pinned official SNS ledger Wasm and PocketIC, the
`real_sns_icrc2_direct_reserve_pull` test proved:

- `icrc1_supported_standards` contains ICRC-1, ICRC-2, and ICRC-3;
- exact short-lived approval;
- `expected_allowance` protection;
- allowance expiry storage;
- direct `transfer_from` reserve credit;
- approval fee burn;
- transfer-from fee burn.

## Intrinsic implementation blockers retained honestly

This tranche does not claim completion of NNS execution. The clean NNS V1 state,
typed immediate operations, exact 40/60 arithmetic, direct maturity DTOs, fixed
pending slots, target generation/coalescing, narrow DID, and lifecycle are
implemented. Canonical Jupiter block decoding, real governance command
execution, actual maturity receipt observation, receipt delivery, reward
fan-out, and unwind lifecycle remain fail-closed rather than fabricating
completion.

The stream-manager direct redemption path compiles and its canonical equations,
stable reopen, immutable duplicate classification, and real-ledger ICRC-2
boundary are tested. A full PocketIC test installing the simplified stream
manager alongside the real ledger remains follow-up coverage.
