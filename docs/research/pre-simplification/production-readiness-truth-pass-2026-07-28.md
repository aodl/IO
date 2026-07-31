# Production Readiness Truth Pass: 2026-07-28

## Baseline

- Expected master: `d26b898f35fd876f38111a302a5bb88200e9ba1a`
- Implementation/source commit ancestor: `e1f1e1e69c19fe08161706c4fc6345e7e63bf88c`
- Proof/release commit ancestor: `0d17a02ddfa8afa5c21f6f886f23fe14377ee0cb`
- Working branch: `production-readiness-truth-pass`
- No mainnet command, deployment, timer registration, `ProductionActive` enablement, production DID expansion, release artifact edit, or frontend generated asset edit is part of this tranche.

## Evidence Reviewed

- IO code: `crates/io_core_model/src/lib.rs`, `crates/io_production_wiring/src/lib.rs`, `canisters/io_stream_manager/src/lib.rs`, `canisters/io_stream_manager/src/logic.rs`, `canisters/io_stream_manager/src/scheduler/mod.rs`, `canisters/io_stream_manager/src/clients/icp_ledger.rs`, `canisters/io_nns_neuron_manager/src/lib.rs`, `canisters/io_nns_neuron_manager/src/scheduler/mod.rs`, `tests/e2e_real_canisters/src/io_protocol_real_stack.rs`.
- IO docs and deployment fixtures under `docs/operations`, `docs/testing`, `docs/architecture`, `docs/security`, `deploy/local-sns-rehearsal`, and `deploy/production-wiring`.
- DFINITY official source pinned to `dfinity/ic@2d7f90fb23672cc3b81c216a33d04c75672dd308`: `rs/sns/init/src/lib.rs`, `rs/ledger_suite/icrc1/ledger/src/lib.rs`, `rs/ledger_suite/common/ledger_core/src/balances.rs`, `rs/sns/testing/README.md`.

## Reproduction Matrix

| ID | Classification | Evidence | Affected invariant | Inert severity | Remediation boundary | Tests | ADR needed |
| --- | --- | --- | --- | --- | --- | --- | --- |
| A | Confirmed | DFINITY SNS init builds ledger args without fee collector; ledger burns fees when collector is absent. IO keeps `total_io_supply_e8s` locally in `crates/io_core_model/src/lib.rs:46` and computes redeemable supply from it at `101`. | Canonical supply cannot be local-only when unrelated transfers/staking burn fees. | P0 design blocker, not exploitable while inert. | Decide fee disposition and canonical supply authority. | Real SNS ledger fee/supply/account-history tests. | Yes |
| B | Confirmed | Model returns full IO to reserve at `crates/io_core_model/src/lib.rs:431`; scheduler sends IO return amount after fee at `canisters/io_stream_manager/src/scheduler/mod.rs:4567` and `4678`. | Model reserve and ledger reserve can diverge. | P0 design blocker, not exploitable while inert. | Redemption fee-aware reconciliation. | Real redemption user-to-redemption and redemption-to-reserve fee tests. | Yes |
| C | Confirmed | Model has net fields at `crates/io_core_model/src/lib.rs:219`, but scheduler constructs redemption with gross as net at `canisters/io_stream_manager/src/lib.rs:1741` and sends `effective_net_user_icp_payout_e8s` at `scheduler/mod.rs:4630`. | Gross-minus-fee payout policy is not enforced at execution. | P0 design blocker, not exploitable while inert. | ICP payout fee-aware attempt plan. | Real ICP payout fee and source-balance tests. | Product policy confirmation |
| D | Confirmed | Source account becomes mock label at `scheduler/mod.rs:5253`; payout destination becomes `mock_account` with `Principal::anonymous()` in `clients/icp_ledger.rs:73` and `scheduler/mod.rs:4635`. | Exact redemption destination does not round-trip. | P0 loss-of-funds risk if active. | Preserve exact ICRC/ICP account mapping. | Exact principal/subaccount payout tests. | No |
| E | Partially confirmed | Reward/rejected-refund paths have durable attempts; Jupiter issuance, normal redemption payout/IO return, and NNS maturity transfer requests still use `created_at_time: None` in release-reachable request construction. | Idempotency under ambiguous results is incomplete. | P0 double-send risk if active. | Immutable transfer attempt records for every release path. | TooOld/Duplicate/unknown-result tests with index proof. | No |
| F | Confirmed | Release-reachable ICP payout fallback on `CanisterCallFailed` calls mock transfer at `scheduler/mod.rs:2241`; mock accounts and debug transaction APIs are in boundary clients. | Production path can submit semantically different transfer. | P0 if active. | Remove release mock fallback and debug proof dependencies. | Release Wasm/client-shape tests. | No |
| G | Confirmed | Jupiter issuance uses mock transfer request at `scheduler/mod.rs:4210` with no real fee, no durable created_at_time, and no canonical duplicate proof. | Reserve issuance is not production ICRC-shaped. | P0 if active. | Exact production ICRC issuance path. | Real SNS ledger reserve-to-Jupiter tests. | Fee ADR dependency |
| H | Confirmed | `ProductionWiringConfig` validates one object, while canister init args retain top-level principal fields; `ProductionActive` is rejected at `crates/io_production_wiring/src/lib.rs:180`. | Future validation/execution split-brain possible. | P0 activation blocker. | Single runtime wiring source. | Install-arg equality and activation tests. | No |
| I | Partially confirmed | Same-second/stale cohort rejection tests exist; stale events can block cursor advancement if association/liveness is not durable. | Source cursor liveness. | P1 while inert. | Durable event/cohort association only. | Deterministic same-second/stale/delayed event tests. | No |
| J | Confirmed | `ProtocolState::new` accepts caller-supplied supply/reserve values at `crates/io_core_model/src/lib.rs:85`; no canonical pre-execution reconciliation gate exists. | Bootstrap can start from false monetary state. | P0 activation blocker. | Bootstrap reconciliation gate. | Real-source startup reconciliation tests. | Fee ADR dependency |
| K | Confirmed | `redemption_rate` returns 1:1 when supply is zero or liquid ICP is zero at `crates/io_core_model/src/lib.rs:111`. | Positive IO with zero liquid ICP can be mispriced. | P0 activation blocker. | Zero-liquid guard in monetary remediation. | Positive-supply/zero-liquid issuance/redemption tests. | No |
| L | Confirmed | Saturating arithmetic remains in model/support code; journals, processed transaction sets and whole-state `stable_save` scale without production pruning/checkpoint rules. | Fail-closed arithmetic and bounded state. | P1/P2 while inert. | Checked arithmetic and storage bounds. | Property/unit/storage-growth tests. | No |
| M | Confirmed safe/inert | `ProductionActive` rejected in wiring validation; production DIDs remain constructor-only under `did_surface`; no init/post-upgrade monetary timers observed. | No live protocol. | Safety-preserving. | Keep gates until P0 complete. | `did_surface`, release Wasm guardrails. | No |

## Corrected Readiness Judgement

IO is not live and not production-ready. Reserved production canisters remain inert. Existing evidence proves important mock, SNS-shaped PocketIC, finalized-SNS, and static guardrail behavior, but it does not prove production monetary correctness.

## Backlog

P0: decide IO ledger fee disposition and supply authority; make redemption fee-aware; preserve exact payout destinations; add immutable transfer attempts; eliminate release mock fallbacks; replace Jupiter mock issuance; unify wiring; add bootstrap reconciliation and zero-liquid guard.

P1: durable cohort/event association; global ledger/archive traversal; bounded storage; production governance mutations/observations; internal timers after P0; all-real lifecycle/failure/upgrade tests; governance/controller/emergency policy; historian/monitoring; final tokenomics; hermetic builds/audit; mainnet testflight/launch.

P2: documentation polish, operator ergonomics, and observability refinements after P0/P1 behavior is real.

## Unresolved Decisions

- Fee burn versus fee collector versus zero/other fee configuration.
- Canonical total supply source and archive traversal requirements.
- Treatment of any fee collector account in redeemable supply.
- Product confirmation of gross-minus-fee ICP payout semantics.

## Gates Before Timers

- P0 monetary remediation complete.
- Exact durable transfer attempts and duplicate proof on every release path.
- Canonical bootstrap reconciliation complete.
- Release-reachable mocks removed.
- Real-source failure and upgrade tests pass.

## Gates Before Production Adapters

- Fee/supply ADR decided.
- Production wiring is single-source and activation state is unified.
- Production DIDs remain constructor-only until audited activation.
- Local evidence remains local-only and no local result is treated as production evidence.

## No Live Protocol Statement

No protocol deployment, mainnet call, timer registration, `ProductionActive` enablement, or monetary code change occurred in this tranche. IO remains not live.
