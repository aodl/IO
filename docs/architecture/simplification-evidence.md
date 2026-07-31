# Simplified execution evidence

Recorded 2026-07-31 on branch `p0-simplified-execution`.

## Deletion and complexity

The comparison base is frozen research commit
`85b6c1c2e879348091226d2494aa2a5831bd473a`.

| Metric | P0 v2 donor | Simplified tree |
|---|---:|---:|
| Combined production Rust LOC in both value-moving `src/` trees, `io_core_model`, and `io_ledger_types` | 36,925 | 6,025 |
| Stream-manager production Rust LOC | over 20,000 | 3,202 |
| Production DID methods, stream / NNS | 0 / 0 | 7 / 6 |
| Immediate operation variants, stream / NNS | generic bag | 3 / 2 |
| Launch value-moving stable schemas | migration chains | V1 / V1 |
| Largest production source file | over 13,000 LOC | under 1,100 LOC |

The combined LOC reduction remains above the 50% gate. No replacement
production source file exceeds the enforced 1,100-line limit. Typed preparation,
redemption and receipt operations replace the deleted generic field bag.

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

`installed_stream_real_sns_icrc2_redemption` additionally installs the current
stream-manager Wasm with the pinned SNS ledger for IO and PocketIC's official ICP
ledger canister for payout. It proves Paused installation, readiness, approval
and direct pull, a separately resumed official-ledger payout, a separately
committed result, same-Wasm upgrades, exact replay and conflicting-nonce
rejection. It also proves a Jupiter liquid receipt through the official ICP
`query_blocks` interface, exact backed-IO settlement and durable receipt replay
after an upgrade returns the canister to Paused.

## Intrinsic implementation blockers retained honestly

NNS execution remains incomplete. The clean NNS V1 state,
typed immediate operations, exact 40/60 arithmetic, direct maturity DTOs, fixed
pending slots, target generation/coalescing, narrow DID, and lifecycle are
implemented. Stream-side canonical Jupiter block decoding and settlement are
installed and exercised. Real governance command execution, actual maturity
receipt observation, NNS receipt delivery, reward fan-out, and unwind lifecycle
remain fail-closed rather than fabricating completion.

The stream-manager direct redemption path reserves typed preparation before
pricing, commits through exact phases, creates payout intent after IO reaches
reserve, validates V1, reopens Paused and passes the installed canonical-ledger
path. The remaining response-barrier matrix, two-week fan-out and executable NNS
governance continuations remain explicit blockers.
