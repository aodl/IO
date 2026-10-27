# Maintained sale-free lifecycle evidence

**Focused runtime: PASS. Tranche closure: BLOCKED pending canonical CI.**
No deployment, mainnet observation, push or merge is authorized by this record.

The source starts at master `389c9ae754b64e7821cd6a4054ead7152c72c3af`,
tree `8b25289371e1657396b605cbfe0466378cb37975`. The test-only implementation is
`7a77fb7ff64612aa34cbdaec75a6bfd3acdc5d6c`, tree
`3af44ff8740fd4784763ccca7f155ad3a227d068`, on
`feat/acceptance-maintained-sale-free-lifecycle`. Its files match the successful
execution snapshot. Production canister/crate code, Cargo inputs, stable schemas,
official SNS pins, allocations, permissions and timing constants are unchanged.

## Execution and source-bound package

Discovery selected exactly one ignored case. The command below executed one
case, passed, exit 0, in 854.82 seconds:

```sh
cargo test --offline --locked -p e2e-real-canisters \
  acceptance_sale::real_maintained_sale_free_lifecycle \
  -- --ignored --exact --test-threads=1 --nocapture
```

Use the artifact/payload inputs from [the runbook](runbook.md), plus
`IO_ACCEPTANCE_ICP_CLI` pointing to the verified `icp 0.2.7` binary and an
absolute `IO_ACCEPTANCE_EVIDENCE_DIR` outside the repository. Keep
`POCKET_IC_BIN` pinned to PocketIC 14, `POCKET_IC_MUTE_SERVER=1`,
`RUST_TEST_THREADS=1` and low build concurrency. `execution-environment.json`
records the actual absolute inputs; `command.json` records the exact invocation.

Evidence root:
`/home/codexdev/.codex-io/sns-framework/maintained-sale-free-lifecycle-20261026-nN06zo`.
Successful runtime leaf: `runtime/acceptance-genesis-4179498-1793058228248086146`.
Its 176 input/observation files pass the complete inventory and hash checks.
The inner `evidence-manifest.json` SHA-256 is
`9925e8e7669f1a0b7aa0d43822420c644a9fa76a96726c438d9f6fb135cf3168`;
the inner `SHA256SUMS` SHA-256 is
`a9b1b82cbeaff7004dbd6044999e2a76a59a8b1349e09f0dd8a68506cd18198d`.
Source archives, blob/material hashes, staged patch/modes, commit binding,
commands, tool hashes, raw replies and resource logs are retained alongside it.
No local signing material is included.

## Coverage composition

| Requirement | Reused proof | Fresh integrated observations / limitation |
| --- | --- | --- |
| Direct Normal, official tuple, exact IO/IO and 1B supply | Single `Sale` fixture and typed initializer | Fresh owned server/subnets; genesis balances, parameters, deployed/running versions, Root summary and parked Swap records |
| Human proposer and Vote-only voter | Existing exact genesis/authorization checks | Separate decisive votes; proposals 1–4 install all four IO roles; unrelated caller denied; fees released; no observed followees |
| A/B/C and capitalization | Existing checkpoint validators and constructors | 30 IO claims, 30 ICP backing, +20 ICP permanent capital, independent 10 ICP anchor; no issuance; exact reserve-transfer fee |
| Authentic client ingress | Pinned CLI and supported PocketIC loopback gateway | Fresh isolated ephemeral signer transfers 1 ICP on the real local ICP Ledger; response, block 4 and independent balances agree; fee 10,000 e8s |
| SNS-controlled restart | Standard Governance/Root upgrade and existing target-local chunks | Proposals 5/6 upgrade NNS Manager/Historian with the same raw Wasms; physical versions, hashes and sole Root controllers checked |
| Canonical recovery | Existing managers, no keeper/debug monetary hook | NNS returns Ready autonomously; permanent stake, complete Dynamic status and Stream checkpoint preserved |
| Pending/ambiguous phases | Existing exact recovery targets | New integrated upgrades are quiescent; full pending-phase target results still require successful canonical CI on this head |
| Historian | Existing `ObservationConfig` and four public APIs | Preserved configuration, honest stale state, timer-only refresh, seven Fresh sources and nine Matching modules; exact canonical accounting and Ledger/Index tip reconciliation |
| Monetary lifecycle | Existing `redemption_and_reward` helper | Timer-only structural work, real child principal return, current-rate redemption and later eligible reward; 100M neuron excluded |
| Evidence integrity | Raw fixture outputs | Inventory regression rejects missing, corrupt and extra files; final runtime manifest independently reverified |
| Historical SNS-W creation/finalization | Original source-bound packages | Compatibility-only; not executed or substituted for direct genesis |

The signed client is not the fixed operator. Its transfer uses separate local
Accounts and contributes zero protocol funding/backing. `ICP_HOME` is isolated;
the ephemeral directory is removed without reading or logging key/seed contents.
The gateway stops before `prepare_permanent` advances simulated time by days.
Later Governance calls use explicit local PocketIC caller simulation. They prove
standard authorization, not cryptographic possession of the live operator key.

## Authority, restart and economics

The three memo domains derive separate staking subaccounts even though the same
reviewed human principal authorizes both governance neurons:

| Memo | Genesis IO | Staking subaccount hex |
| --- | ---: | --- |
| IOGOV001 | 30 | `39d4b187d2057bdc49dc3078c0a8e97efb4dc09c93ce6d35c699665f35b1e053` |
| IOJUP001 | 100,000,000 | `8d56e3b80021a892f55e161e8725092d866a5f9580badb2d408ebe09dbae5eef` |
| IORES001 | 899,999,970 | `d7a5e01a747275983dbccdf7b596d55c3816844ac7b3731fa8d6c90e9922fc40` |

Each staking Account's owner is its SNS Governance canister. The 100M Account
remains excluded from claims, backing and rewards; the human's other Accounts
are not excluded by ownership. Reserve migration burns only 10,000 IO e8s.
Initial structural TopUp records 3,000,000,000 e8s of claim backing and
3,999,990,000 e8s physical Dynamic principal, with the real 10,000 ICP-e8s fee
charged to the independent anchor.

NNS Manager proposal 5 advances physical canister version 86→90; Historian
proposal 6 advances 81→85. PocketIC 14's pinned IC source
`4d09932de49c9cc271e04f06d6c327b4df3cc82d` increments versions at Stop request,
Stop completion, Upgrade and Start. The test waits for that complete sequence
and restored status, rather than mistaking Root's asynchronous acknowledgement
for installation. Before/after raw hashes are identical and controllers remain
solely Root. Adopted proposals release their 10 IO charge. Standard Governance
controls Root; Root controls Governance, Ledger, Index, Swap and all four dapps.

Historian retains generation 6 and reports all seven sources Stale immediately
after restart; its ordinary timer converges to generation 7. Final generation
30 has seven Fresh sources, nine Matching modules and an up-to-date Index tip.
A Fresh cached snapshot can predate a new reward checkpoint: the bounded observer
retains those intermediate snapshots and waits for canonical catch-up. It never
rewrites observations or substitutes successful defaults. The constructor
configures no history Accounts, so no complete transaction-history claim is made.

Final claims are 2,949,980,000 e8s; backing is 2,949,999,667 e8s, comprising
149,981,000 liquid and 2,800,018,667 pooled e8s, with zero live child/transit
backing. The claim-rate fraction is exactly those backing/claim values.
Dynamic physical principal is 3,799,978,667 e8s: pooled claim principal plus
999,960,000 anchor e8s and zero excluded surplus. The 0.5 IO redemption credits
49,990,333 ICP e8s after its real 10,000-e8s payout fee; both IO transfer fees
are 10,000 e8s. Reward proposal 7 advances processed events 16→17 and adds
10^18 to both eligible and policy credit. Operator reward shares are
2,800,000,000; IOJUP001's 10^16 shares generate no monetary entitlement.

The official tuple remains the reviewed version after proposal 144202, without
overrides; the [lock and hash table](readiness-handoff.md#e-live-preflight-and-independent-audit)
retain publication, source, compressed/raw and DID identities. Five SNS modules
are installed from their official gzip bytes. Zero running Archives use the
blessed Archive-template normalization. Automatic target advancement is false.
The parked Swap has zero genesis allocation, remains source-frozen Adopted and
has unreachable timestamps; no sale/participation/finalization is used. The
earlier dust-tolerant regression and 143747→144202 framework-upgrade proof remain
bound to their original sources; no successor blessing is invented here.

All four raw Wasms and canonical gzip hashes match the retained independent
current-source reproduction. Wasm build inputs are unchanged; source comparisons
and fresh byte hashes are in `reproduction-input-binding.json` and
`current-artifact-byte-comparisons.json`. No historical release pair is rebound.

## Canonical failure and remaining gates

Affected host tests passed 107 library and 4 binary cases; 82 opt-in cases were
ignored in that command. The new ignored lifecycle case ran separately.
Affected-package strict all-targets/all-features Clippy, rustfmt, whitespace,
acceptance preflight and real-canister harness checks passed.

Canonical `cargo run --offline --locked -p xtask -- test_ci` ran on clean
`7a77fb7`. Recorded-release build A/B comparisons, security (Deny/Audit), static,
interface, stable-state, frontend and unit phases passed. The first Stream
target recorded 8/24 passes before the owned server vanished; the remaining
16 failures are HTTP IncompleteMessage/ConnectionRefused, not monetary assertions.
The host OOM counter rose 10→11; full-memory-pressure averaged 45.33% over five
minutes. A requested 20-second monitoring wait took 1,367 seconds. The server
was lost before its hard TTL; kernel log access was denied, so OOM victim
identity versus idle expiry during host starvation cannot be independently
distinguished. No production defect is established.

The failed aggregate was deliberately stopped before further heavy groups:
canonical exit -15 (SIGTERM), monitor shell exit 241. The two-case NNS target
started but was interrupted; **full NNS recovery 29/29 and final workspace
Clippy did not execute**. All 75 pre-existing asset files were restored
byte/mode identically; foreign jobs were not signalled. `ci-first-attempt-diagnosis.json`
and `canonical-ci.log` retain the original failure. Require a complete fresh
canonical exit zero, Stream 24/24, NNS 29/29 and final strict Clippy before
closing this tranche. Schedule the retry with sufficient measured headroom
after the overlapping heavy aggregate; independent editing sessions do not
require a separate runner or a general exclusivity policy.

Four development attempts failed in new harness assertions before the successful
run: loopback hostname, management history/routing, version increment expectation,
and cached freshness versus catch-up. Their source/logs remain in `attempt-01`
through `attempt-04`; compile-only errors are recorded separately and count as
zero runtime executions. No source correction followed the canonical resource
failure.

Live gates remain operator authentication; independent Jupiter ICP source and
activation; nine-canister authority/subnet/module/cycles evidence; authentic
permanent-neuron/Account observations; approved conditional liquidity wording;
proposer-fee/key-loss sign-off; disposable-versus-in-place-handover decision;
independent audit; and separately approved funding/install/controller operations.
The 43,260-second worst-phase service budget is conditional, not a universal
liquidity bound. Three retained 10 IO rejection charges can exhaust the 30 IO
proposer; stop after the first retained charge. Vote-only supplies no ordinary
authority handover. Jupiter's future decentralized mirror remains a production
requirement, not initial acceptance authority. No mainnet preflight was executed.
