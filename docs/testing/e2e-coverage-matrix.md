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
| SNS index returns account history and ordering is handled | Unit index cursor/order tests; real-framework opt-in ledger smoke checks reserve/user account history | Not proved in default CI |
| index lag is handled | Unit and SNS-shaped PocketIC scanner lag tests | Real SNS lag behavior not reproduced |
| archive-required behavior handled or flagged | Unit archive-required checks; SNS-shaped PocketIC archive-required scanner block | Real SNS archive traversal not implemented |
| tiny/dust/rejected transfers do not stall scanners | Unit and SNS-shaped PocketIC dust/rejection scanner tests | Not real SNS covered |

## SNS Governance/Neuron Staking

| Invariant | Current strongest coverage | Gap |
| --- | --- | --- |
| user can stake IO the normal SNS way into an SNS neuron | Real-framework PocketIC direct-governance opt-in tests | Direct governance install only; not yet behind SNS-W-finalized SNS |
| staked IO neuron appears in SNS governance | Real-framework PocketIC direct-governance opt-in tests plus production-shaped DTO unit tests | Not yet through SNS-W-finalized governance |
| minimum neuron stake is enforced | Real-framework PocketIC direct-governance opt-in tests | Not yet through SNS-W-finalized governance |
| dissolve delay set/increased and below-threshold excluded | Unit eligibility/reward-policy tests; real-framework direct-governance dissolve-delay boundary observation | Not yet through SNS-W-finalized governance |
| following/voting/maturity/staked maturity represented | Unit production-shaped governance DTO and participation tests | Real proposal/voting/reward behavior not covered |
| stake increase observed correctly | Unit: `increasing_staked_io_increases_reward_weight_without_double_counting`; real-framework direct-governance top-up test | Not yet through SNS-W-finalized governance |
| dissolve/dissolving/disbursed states handled | Unit eligibility/reward tests; direct-governance dissolve-delay visibility | Real finalized lifecycle not covered |
| participation affects APY/reward only if policy says | Unit reward allocation and governance snapshot tests | Not real SNS |
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
| no APY increase for unstaked, too-short dissolve, dissolving, non-voting when proposals closed | Unit reward-policy and governance eligibility/participation tests | Not real SNS |
| cap, rounding, time windows, participation ratio, epochs | Unit allocation/participation/stake-time tests | No real reward epoch from SNS governance |
| rewards disabled in SNS config | Tooling config guards set SNS native rewards to zero in local templates | Real SNS config observation not automated |
| multiple users, whale vs dust, split/merge lifecycle | Unit/model tests cover weights/dust; split/merge lifecycle is NNS model-oriented | Real SNS neuron split/merge unsupported in automated tests |
| historian displays APY source honestly without protocol truth | Historian freshness/static gates | No browser E2E with real SNS data |

## ICP/NNS Side

| Invariant | Current strongest coverage | Gap |
| --- | --- | --- |
| ICP deposit detected; authorized only; duplicate not processed twice | Unit/model and SNS-shaped PocketIC stream-manager tests | Real ICP ledger/index canisters not in all-real stack |
| tiny ICP deposit rejected terminally without scanner stall | SNS-shaped PocketIC `pocketic_live_tiny_authorized_icp_deposit_does_not_block_later_valid_deposit` | Not real ledger/index |
| ICP index ordering/cursors handled | Unit and SNS-shaped PocketIC | Not real index |
| NNS maturity observations drive backing policy; transient failures retry safely | Unit/model and mock/PocketIC NNS manager tests | Not real NNS governance |
| protected real neuron-owner/neuron never mutation targets | Production wiring/static gates | No mainnet calls by design |

## IO Issuance/Redemption/Accounting

| Invariant | Current strongest coverage | Gap |
| --- | --- | --- |
| ICP deposit causes IO issuance from reserve, not minting | Unit/model; SNS-shaped PocketIC mock ledger; local evidence gate | Not real SNS ledger |
| issued amount, fees, gross/net/fee intent, no negative balances | Unit/model and PocketIC mock canister tests | Not all-real stack |
| redemption return model and net ICP payout | Unit/model and SNS-shaped PocketIC redemption tests | Not real ledger/index |
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
| frontend does not call value-moving canisters | Static prelaunch/historian freshness gates | Covered statically, not browser real SNS |
| frontend displays not live and does not present mock/local SNS values as mainnet truth | Static prelaunch public shell gates | Covered statically |
| historian source freshness/staleness honest | Historian freshness gate and unit tests | No real SNS source adapter |
| historian local SNS evidence clearly local-only | Official local SNS docs/evidence validators | No UI evidence display yet |
| XSS/escaping for IDs/evidence fields | Not covered | Add frontend/historian rendering tests when evidence display exists |

## Summary

Current coverage is strong for IO-owned model logic, production-shaped DTO mapping, mock/SNS-shaped PocketIC retry behavior, stable-state guardrails, and local evidence validation. The opt-in real-framework PocketIC layer now proves real SNS ledger/index behavior and a real-ledger exact-economics value-flow when pinned local artifacts are supplied. It does not yet prove real SNS governance/root behavior in PocketIC, normal SNS neuron staking, real governance rewards/maturity, or a full ICP -> IO -> stake -> APY increase -> redemption E2E flow against all-real canisters.


## Current Real-Framework Stride

`tools/scripts/run-real-framework-e2e` is the opt-in local operator path for pinned real framework artifacts. It fetches/verifies/decompresses configured artifacts, runs the real ledger/index tests, the PocketIC ICP-feature SNS-W bootstrap proof, gzipped NNS proposal publication for all six SNS Wasm slots, SNS-W deploy/swap-open tests, and the real-ledger exact-economics E2E. Governance/root/SNS-W normal staking remains blocked until swap participation/finalization and list-neurons drivers are implemented.
