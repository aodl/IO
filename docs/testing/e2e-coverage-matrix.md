# E2E Coverage Matrix

This matrix separates proof strength. A mock test uses IO-owned mock canisters or host fixtures. An SNS-shaped PocketIC test runs IO canisters plus mock SNS-shaped ledger/index/governance/root canisters. A real-framework PocketIC test runs real SNS framework Wasms in PocketIC. An official local SNS rehearsal uses local DFINITY SNS tooling and is complete only when `deploy/local-sns-rehearsal/canister-ids.local.toml` exists and `cargo run -p xtask -- validate_local_sns_ledger` passes.

No current all-real-canister PocketIC E2E test exists for the full IO protocol. The real-framework PocketIC ledger/index, direct-governance, SNS-W publication, SNS-W deployment, and swap-open layers are executable only when pinned Wasms are supplied locally. No current test proves the canonical mainnet SNS IO ledger exists. IO protocol remains not live.

Legend: Unit, Mock/PocketIC, SNS-shaped PocketIC, Real-framework PocketIC, Official local SNS rehearsal, Not covered, Blocked by tooling.

## SNS Ledger/Index

| Invariant | Current strongest coverage | Gap |
| --- | --- | --- |
| real SNS ledger exists and exposes ICRC methods | Real-framework PocketIC opt-in: `real_sns_ledger_index_smoke`, if pinned artifacts are supplied | Not covered in default CI; blocked when local artifacts are absent |
| real SNS index canister exists and exposes account-history methods | Real-framework PocketIC opt-in: `real_sns_ledger_index_smoke`, if pinned artifacts are supplied | Not covered in default CI; blocked when local artifacts are absent |
| IO ledger transfer fee read/assumed correctly | Unit: `icrc_error_mapping_preserves_duplicate_and_bad_fee`, `fee_and_dust_values_are_explicit_at_boundary`; real-framework opt-in ledger smoke reads `icrc1_fee` | Not proved in default CI |
| reserve account exists and balance is sufficient | Real-framework opt-in ledger smoke initializes and verifies reserve balance; unit reserve-shape tests | Not proved in default CI |
| reserve-to-user transfer works | Real-framework opt-in ledger smoke; SNS-shaped PocketIC stream-manager tests | Not proved in default CI |
| user-to-reserve transfer works | SNS-shaped PocketIC redemption tests; official evidence gate if present | Not real-framework PocketIC covered |
| BadFee and InsufficientFunds map correctly | Unit ledger mapping; real-framework opt-in ledger smoke observes real ledger errors | Not proved in default CI |
| Duplicate transfer maps to idempotent success and duplicate proof is recoverable | Unit duplicate proof; real-framework opt-in ledger smoke observes duplicate block; same-Wasm upgrade test rechecks duplicate behavior | Duplicate matching is proved at ledger/index level, not yet through stream-manager real clients |
| total supply remains constant for reserve-transfer issuance/redemption | Unit local SNS model; real-framework opt-in ledger smoke checks constant supply across reserve transfer | Not checked for redemption in real-framework layer yet |
| SNS index returns account history and ordering is handled | Unit index cursor/order tests; real-framework opt-in ledger smoke checks reserve/user account history; finalized IO real-stack active-stake test configures the finalized SNS index as the stream-manager IO scanner and proves a no-traffic scan leaves state consistent | Not proved in default CI; value-moving SNS index redemption/reward scans through stream-manager are not yet real-framework covered |
| index lag is handled | Unit and SNS-shaped PocketIC scanner lag tests | Real SNS lag behavior not reproduced |
| archive-required behavior handled or flagged | Unit archive-required checks; SNS-shaped PocketIC archive-required scanner block | Real SNS archive traversal not implemented |
| tiny/dust/rejected transfers do not stall scanners | Unit and SNS-shaped PocketIC dust/rejection scanner tests | Not real SNS covered |

## SNS Governance/Neuron Staking

| Invariant | Current strongest coverage | Gap |
| --- | --- | --- |
| user can stake IO the normal SNS way into an SNS neuron | Real-framework PocketIC direct-governance opt-in tests; finalized SNS-W lifecycle staking test | Stream-manager reward snapshot consumption not yet real-stack |
| staked IO neuron appears in SNS governance | Real-framework PocketIC direct-governance opt-in tests plus production-shaped DTO unit tests; finalized SNS-W lifecycle `list_neurons` tests | Stream-manager reward snapshot consumption not yet real-stack |
| minimum neuron stake is enforced | Real-framework PocketIC direct-governance opt-in tests; finalized SNS-W lifecycle minimum-stake rejection | Stream-manager reward snapshot consumption not yet real-stack |
| dissolve delay set/increased and below-threshold excluded | Unit eligibility/reward-policy tests; real-framework direct-governance dissolve-delay boundary observation; finalized SNS-W lifecycle below/two-week/start-dissolving/stop-dissolving checks | Stream-manager-consumed reward snapshots not yet real SNS proof |
| following/voting/maturity/staked maturity represented | Unit production-shaped governance DTO and participation tests; finalized SNS-W lifecycle yes/no voting, following vote propagation, and non-voter weighting tests | Stream-manager reward-snapshot consumption, maturity, and staked maturity are not yet real SNS proof |
| stake increase observed correctly | Unit: `increasing_staked_io_increases_reward_weight_without_double_counting`; real-framework direct-governance top-up test; finalized SNS-W lifecycle top-up test | Stream-manager reward snapshot consumption not yet real-stack |
| dissolve/dissolving/disbursed states handled | Unit eligibility/reward tests; direct-governance dissolve-delay visibility; finalized SNS-W start/stop dissolving tests | Stream-manager-consumed reward snapshots not yet real SNS proof |
| participation affects APY/reward only if policy says | Unit reward allocation and governance snapshot tests; finalized SNS-W lifecycle no-closed-proposals and non-voter weighting tests | Stream-manager reward snapshot consumption not yet real-stack |
| multiple neurons aggregation and duplicate prevention | Unit governance snapshot duplicate/page tests | Real pagination not covered |
| hotkey/controller authorization respected | DTO permission mapping tests | Real auth calls not covered |
| malformed SNS neuron IDs rejected, not coerced | Unit conversion/governance snapshot tests | Covered at boundary, not canister-run |
| governance pagination and transient errors | Unit pagination/error classification | Real transient failures not covered |

## IO APY/Reward Policy

The codebase implements this as two-week maturity backed IO reward allocation from SNS governance snapshots, not as a separate APY calculator.

| Invariant | Current strongest coverage | Gap |
| --- | --- | --- |
| baseline without SNS staking, liquid IO only, stale/missing governance | Unit reward-policy zero/no eligible tests; snapshot error tests | No historian/end-to-end APY display proof from real governance |
| APY increase when eligible staked IO neuron exists | Unit `eligible_sns_staking_increases_io_reward_entitlement`; SNS-shaped PocketIC reward allocation | Not real SNS staking |
| no APY increase for unstaked, too-short dissolve, dissolving, non-voting when proposals closed | Unit reward-policy and governance eligibility/participation tests plus finalized SNS dissolve-state and non-voter lower-weight proof | Unstaked and stream-manager-consumed reward snapshots are not yet real SNS proof |
| cap, rounding, time windows, participation ratio, epochs | Unit allocation/participation/stake-time tests | No real reward epoch from SNS governance |
| rewards disabled in SNS config | Tooling config guards set SNS native rewards to zero in local templates | Real SNS config observation not automated |
| multiple users, whale vs dust, split/merge lifecycle | Unit/model tests cover weights/dust; split/merge lifecycle is NNS model-oriented | Real SNS neuron split/merge unsupported in automated tests |
| historian displays APY source honestly without protocol truth | Historian freshness/static gates | No browser E2E with real SNS data |

## ICP/NNS Side

| Invariant | Current strongest coverage | Gap |
| --- | --- | --- |
| ICP deposit detected; authorized only; duplicate not processed twice | Unit/model, SNS-shaped PocketIC stream-manager tests, and `io_stream_manager_real_jupiter_deposit_scanned_from_real_icp_index` against local real NNS ledger/index | Not run in default CI; negative real ICP scanner cases remain narrower than the mock/SNS-shaped suite |
| tiny ICP deposit rejected terminally without scanner stall | SNS-shaped PocketIC `pocketic_live_tiny_authorized_icp_deposit_does_not_block_later_valid_deposit` | Not real ledger/index |
| ICP index ordering/cursors handled | Unit and SNS-shaped PocketIC; real finalized-stack Jupiter deposit test waits for the local real ICP index and proves one account-history scan is processed once | Broader real index lag/archive/ordering cases remain mock/SNS-shaped |
| NNS maturity observations drive backing policy; transient failures retry safely | Unit/model and mock/PocketIC NNS manager tests | Not real NNS governance |
| protected real neuron-owner/neuron never mutation targets | Production wiring/static gates | No mainnet calls by design |

## IO Issuance/Redemption/Accounting

| Invariant | Current strongest coverage | Gap |
| --- | --- | --- |
| ICP deposit causes IO issuance from reserve, not minting | Unit/model; SNS-shaped PocketIC mock ledger; local evidence gate; finalized-stack real Jupiter deposit funds stream-manager reserve from finalized SNS ledger tokens and issues exactly 60 IO for 100 ICP through the real SNS ledger reserve | Not run in default CI; real reward value-moving paths and broader redemption variants remain separate |
| issued amount, fees, gross/net/fee intent, no negative balances | Unit/model and PocketIC mock canister tests | Not all-real stack |
| redemption return model and net ICP payout | Unit/model and SNS-shaped PocketIC redemption tests; finalized-stack real redemption test scans finalized SNS IO index history, pays ICP on the local NNS ledger, and returns IO minus SNS ledger fee to the finalized SNS reserve account | Current-rate/redemption-after-yield variants remain separate |
| duplicate redemption retry does not double-pay | SNS-shaped PocketIC retry tests | Not real ledgers |
| mid-flight upgrade preserves retry intent | SNS-shaped PocketIC upgrade-before-retry tests; stable fixtures | Not real SNS framework |
| ledger/index lag does not corrupt state; failed transfers retry safely; terminal failures auditable | Unit and SNS-shaped PocketIC | Not real SNS |
| reserve accounting and active staked IO accounting consistent; no supply mismatch | Unit/model; official evidence flag | Not all-real |
| no historian fake values | Historian freshness/static gates | No real data source |

## Upgrade And Stable State

| Invariant | Current strongest coverage | Gap |
| --- | --- | --- |
| upgrade before/after deposit and IO transfer journal phases | Stable fixtures; SNS-shaped PocketIC retry upgrades | Not all boundaries covered with real canisters |
| redemption upgrade before/after ICP payout and IO return | SNS-shaped PocketIC retry upgrade tests | Not real ledger/index |
| upgrade after SNS neuron stake observed before APY update | Not covered | Needs real or production-shaped governance snapshot journal integration |
| stable migration preserves retry/idempotency; future schema rejects; corrupt state fails closed | Stable-storage fixtures and xtask gate | Not real canister state from SNS framework |

## Frontend/Historian

This domain is the historian/frontend honesty layer.

| Invariant | Current strongest coverage | Gap |
| --- | --- | --- |
| frontend does not call value-moving canisters | Static prelaunch/historian freshness gates plus `frontend_does_not_import_stream_manager_declarations` and `frontend_does_not_import_nns_neuron_manager_declarations` source checks | Covered statically, not browser real SNS |
| frontend displays not live and does not present mock/local SNS values as mainnet truth | Static prelaunch public shell gates plus `frontend_real_status_displays_not_live` and `frontend_real_status_shows_local_evidence_as_local_only` source checks | Covered statically |
| historian source freshness/staleness honest | Historian freshness gate and unit tests | No real SNS source adapter |
| historian local SNS evidence clearly local-only | Official local SNS docs/evidence validators | No UI evidence display yet |
| XSS/escaping for IDs/evidence fields | `frontend_escapes_canister_ids_evidence_fields` source check verifies frontend evidence rendering uses text APIs and not HTML insertion | Static only; add browser rendering tests when evidence display exists |

## Summary

Current coverage is strong for IO-owned model logic, production-shaped DTO mapping, mock/SNS-shaped PocketIC retry behavior, stable-state guardrails, and local evidence validation. The opt-in real-framework PocketIC layer now proves real SNS ledger/index behavior, SNS-W publication/deploy/swap participation/finalization, real SNS governance list-neurons, SNS neuron staking, voting/following/non-voter weighting and proposal rejection fee observation after finalization, finalized root dapp listing/control/upgrade and non-dapp exclusion, IO canister installation against finalized SNS IDs, stream-manager finalized SNS active-stake refresh, no-traffic local ICP/SNS index scan safety through stream-manager, real local ICP index scanning for a Jupiter Faucet deposit, exact 60 IO reserve issuance for a 100 ICP deposit through the finalized SNS ledger, one finalized-SNS IO redemption scan with local ICP payout and SNS IO reserve return, and a real-ledger exact-economics value-flow when pinned local artifacts are supplied. It does not yet prove stream-manager participation-weighted reward allocation from real SNS neuron IDs, real reward ledger/index scanner processing through stream-manager, current-rate redemption variants, real governance maturity, or a full ICP -> IO -> stake -> APY increase -> redemption E2E flow through IO value-moving canisters against all-real sources.


## Current Real-Framework Stride

`tools/scripts/run-real-framework-e2e` is the opt-in local operator path for pinned real framework artifacts. It fetches/verifies/decompresses configured artifacts, runs the real ledger/index tests, the PocketIC ICP-feature SNS-W bootstrap proof, gzipped NNS proposal publication for all six SNS Wasm slots, SNS-W deploy/swap-open/participation/commit/finalization/finalized-governance/finalized staking/finalized no-closed-proposals weighting/finalized yes/no voting/finalized following/finalized non-voter weighting/finalized-root tests, builds local debug IO Wasms, installs IO canisters against the finalized local SNS with local ICP index and finalized SNS index scanner principals configured, verifies stream-manager finalized-governance active-stake refresh plus no-traffic local ICP/SNS index scan safety, proves real local ICP index deposit scanning and finalized SNS reserve issuance for a 100 ICP Jupiter Faucet deposit, proves one finalized-SNS IO redemption scan with local ICP payout and SNS IO reserve return, and runs the real-ledger exact-economics E2E. Participation-weighted stream-manager reward allocation plus real reward scanner processing remain separate implementation work.
