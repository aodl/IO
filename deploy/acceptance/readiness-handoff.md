# Sale-free acceptance: governance and operational handoff

Review date: 2026-10-22. **Individual-controlled acceptance authorized as a product policy; revised implementation validation pending.**
This document authorizes no network observation, installation, funding, identity
operation or controller change. The individual-control decision supersedes the
former acceptance dependency on Jupiter DAO governance.

## Source and evidence boundary

| Repository | Reviewed commit | Git tree | Review state |
| --- | --- | --- | --- |
| IO | `76de77b8d997609623f3e84f8dc3a3be4991470b` | `21a9f27be1d9b91a4d4e31354a2600f34e46e4b9` | Clean master; documentation review on `feat/acceptance-readiness-authority-review` |
| Jupiter | `1c86d9b32c774a470489b4e4a5cfe4ddfaae7341` | `ebbe3a3d5fd669910600547431e758a6ff578fd4` | Clean master; read only; no branch or worktree mutation |

Both origin URLs and GitHub master commit/tree identities were checked. The retained prior validation concerns IO executable source at `043cde2d54573371f0388175963ab9fe141a8aec`
(tree `eba7cf4123a59c0562433e4f42a462965af1bb58`); the intervening status update
is documentation only. The individual-governance working patch is based on
`919a12c8a05ddc716ccc4b7a41379c524c2cd2dd` on
`feat/acceptance-individual-governance`; its evidence is recorded separately in
implementation-status.md. These identities do not form a new release pair.

The retained validation directory is
`/home/codexdev/.codex-io/sns-framework/sale-free-resource-aware-validation-20261022T065759Z`.
Its `SHA256SUMS` hash is
`f3fd6d6d4b5adf1a4b4070800d98dcf28a529a0ecd7eb06f8f0b05b41f9a2521`.
This review's source hashes, GitHub results, test commands and logs are outside
the repository at
`/home/codexdev/.codex-io/sns-framework/acceptance-readiness-20261022`.

## A. Acceptance authority and production separation

The accepted initial acceptance policy is one individual exercising decisive SNS
governance through two distinct neurons. The sole authoritative voter field is
`governance_voting_principal`, equal to operator
`zkkkd-i34qc-367ln-e2u7o-ezznu-dkfqh-gtfvz-cviph-6qa4v-evtfs-wqe`.
This is genuine standard Governance/Root operation under centralized authority,
not evidence of decentralized Jupiter decisions.

| Identity / authority | Acceptance requirement |
| --- | --- |
| Human operator | Propose using 30 IO IOGOV001, then separately vote using IOJUP001; live authentication must be independently verified |
| IOJUP001 | 100M IO, 100% multiplier, sole operator principal, exactly Vote, no genesis/checkpoint followees; no monetary entitlement |
| IOGOV001 | 30 IO, full reviewed operator permissions, independent claim-bearing staking Account |
| IORES001 | 899,999,970 IO, existing reserve-migration authority |
| Jupiter ICP source | Separate unresolved principal, distinct from voter and all nine canister roles; confers no voting authority |
| Production SNS Rewards | `alk7f-5aaaa-aaaar-qb4ra-cai`, production overlay only; never substituted for the acceptance voter |
| IO Governance | Frozen `h7y7s-kyaaa-aaaas-amxrq-cai`; live state unobserved |

```text
reviewed human operator (live authentication UNVERIFIED)
  -> MakeProposal via IOGOV001 (30 IO; insufficient alone)
  -> separate RegisterVote via IOJUP001 (100M IO; Vote only)
  -> IO SNS Governance -> SNS Root -> four IO dapps
```

Governance solely controls Root; Root solely controls Governance, Ledger, Index,
Swap and the four IO dapps. The operator has no final controller fallback.
NNS Manager controls the intended permanent and Dynamic NNS positions.
The exact IOJUP001 staking Account is excluded; other Accounts authorized by
that human are not automatically excluded. Standard Vote also permits following
changes, so no-followees is an observed launch invariant, not an immutable
restriction. It does not permit ordinary permission reassignment: retaining and
decentralizing this same acceptance SNS requires a separate handover decision
before live genesis. No elevated permission is added for that purpose.

IO ordinary proposals retain an 86,400-second initial period and 43,200-second
WFQ parameter. Critical actions have source-defined minimums (432,000 and
216,000 seconds respectively). Read each canonical proposal's actual deadline;
WFQ is not guaranteed additional time. Human acceptance has no cross-DAO
finality dependency and no guarantee against human or network delay.

The reviewed Jupiter source `1c86d9b32c774a470489b4e4a5cfe4ddfaae7341`
still supplies no approved canonical jUP decision source or IO mirror.
Its owner-index Root/OpenChat discovery is not voting authority. Authentic
source decisions, exact action/hash/instance/replay binding, decentralized
upgrade authority, durable vote reconciliation and a feasible deadline contract
remain **production-decentralization requirements**. No Jupiter code, Candid,
stable field or deployment is changed. A post-submission one-day DAO decision
plus observation/vote latency cannot be presumed to fit an unextended one-day
IO proposal. No production authority or timetable is approved here.

## B–C. Individual-controlled implementation and local proof

The acceptance adapter, existing renderers/checkpoints and real SNS fixture use
the same human principal for both permission records. Actual canister roles and
the independent ICP source remain collision-checked. Production retains its
separate pinned SNS Rewards voter. Canonical memo domains derive different
staking subaccounts even with identical permission principals.

Fresh tests must establish proposer-only non-adoption, the separate decisive
standard RegisterVote, rejection-cost charging/release/retention, Root installs
with physical module/controller observations, exact Vote-only permissions,
unrelated-caller rejection and denial of all non-voting authority. Full A/B/C
must retain 30 IO claims, 30 ICP backing, +20 ICP permanent capital, independent
10 ICP anchor, timer-only progression and exact 100M monetary exclusion.
Retained mock-canister votes prove only their recorded source's authorization;
they are not the new individual-controlled test result. See implementation-status.md
for executed results and source identities.

## D. Lifecycle coverage and disposition for review

| Requirement | Existing proof at reviewed source | Remaining gap / proposed disposition |
| --- | --- | --- |
| Six official components, direct Normal genesis, exact IO/IO, parked Swap, controller graph | Retained typed A/B/C launch and component/module observations | Live installation observations remain required |
| 1B distribution, reserve disbursement, C0=30 IO, B0=30 ICP, permanent +20 ICP, independent 10 ICP anchor | A/B/C and negative cases; exact Ledger/NNS records | Repeat with real identities and separately authorized funding |
| Root installs all four current IO Wasms via target-local chunks | Four physical module hashes and proposal IDs in `root-install-proposals.json` | Revised human-vote execution requires current-source validation |
| Timer-only TopUp, 14-day return, semantic redemption and later eligible official reward shares; Jupiter excluded | Completed A/B/C runtime evidence, zero keeper calls | Executed schedules, not a universal time bound |
| Default blessed framework upgrade without SNS-W deployment registration | Reviewed 143747 → 144202 Governance upgrade, actual Success journal, Root/module/version observations | Future successor compatibility is not implied; custom SNS-specific paths have separate registry restrictions |
| Fresh current-source raw/gzip bytes across independent source/Cargo-home paths | Four-artifact reproduction; 632 source files bound to 043cde2 | No historical manifest or installed evidence rebound |
| Recovery, interfaces, stable state, served frontend, full NNS/Stream and strict Clippy | Canonical CI and existing targeted upgrade/recovery suites | Not a single fresh maintained-topology restart/recovery package under the accepted human authority |
| Source-built SNS CLI, SNS-W creation/discovery and sale finalization | Historical maintained `lifecycle` phases 10–17, old source-bound packages | These exercise a sale-created fixture, not the accepted launch. Accepted disposition: creation/finalization are compatibility-only |
| Maintained host tooling, actual client ingress, restart-safe owned topology, end-to-end evidence packaging and historian convergence | Historical profile; source-aware low-level proofs cover substantial behavior | Fresh direct-genesis package and exact restart/observation gaps need explicit gate disposition; no automatic waiver |
| Production Jupiter voting and source-decision recovery | None | Production-decentralization gate; not an initial acceptance requirement |

`tools/scripts/test-sns-framework --profile lifecycle` delegates to the
SNS-W creation/finalization runbook. Running that sale exercise would not reproduce the accepted direct genesis. No fresh historical
sale topology was executed in this review. The mandatory fresh owned-topology
maintained-lifecycle gate remains **BLOCKED / NOT RUN** until its unique
direct-genesis requirements execute. Do not mark it complete, delete it, or create a second active
launch path. The proposed replacement evidence is the exact direct-genesis
A/B/C package under the accepted human authority plus a source-bound maintained
restart/upgrade/observation package for the unique gaps above.

### Liquidity before unlock

The checked-in constants, not historical daily-cadence notes, govern this review:
`SNS_USER_DISSOLVE_DELAY_SECONDS=1,296,060`, NNS child delay `1,209,600`, and
`STRUCTURAL_SYNC_INTERVAL_SECONDS=43,200`. Stream schedules structural work
independently of its daily reward observation's 300-second margin. Both managers
retry active recovery on 60-second deadlines; those are wake opportunities,
not service-time guarantees. NNS records the actual observed child's dissolve
timestamp, prioritizes ready-child return, and serializes immediate operations.

For an exit just after observation, a sufficient timing condition is
`detection + start_service + 1,209,600 + return_service < 1,296,060`.
With detection no greater than 43,200 seconds, **start and return service plus
all scheduling, contention, reflection and recovery delays must total less than
43,260 seconds**. The executable healthy model budgets 600 seconds for service,
leaving 42,660 seconds. Its assertion calculates child readiness; actual
principal return still needs successful Governance/Ledger calls and canonical
proof, so the model's allowance is not an independently established return SLA.

Exact NNS dissolve duration is protocol-defined once StartDissolving is actually
observed. No source-defined finite upper bound covers subnet outages, timer
dispatch, repeated Busy/Pending, pool contention, ambiguous Split/StartDissolving/
Disburse outcomes, index lag, upgrade/restart or exact-block recovery. A call
timeout and ready-child priority preserve safe recovery, not a maximum wall
clock latency. Thus there is no universal liquidity-before-unlock guarantee.
Conditional on timely successful service and enough fee capacity, the autonomous
schedule supplies liquidity before unlock. Otherwise backing remains accounted
in `B=L+P+U+T`; an accepted redemption may wait for liquid backing, retaining its
exact durable payout obligation. Eventual payout itself depends on eventual
availability and successful proof, not merely elapsed time.

Product review must explicitly accept that conditional availability contract or
request a separate finite-bound design review. No dissolve delay, monetary
behavior or scheduler was changed. Source: `docs/architecture/scheduler.md`,
`reward_timer.rs`, `recovery_timer.rs`, `unwind_flow.rs` and the test-only
`healthy_operation_budget_seconds` in `tests/economics/src/lib.rs`.

## E. Live preflight and independent audit

[Read-only preflight](read-only-preflight.md) contains inert commands, exact
identities, evidence templates and staged approval boundaries. No live request
was executed. The acceptance voter is the reviewed operator; its live authentication and the
independent inflow principal remain unverified; permanent-neuron visibility/authority and all nine live canister states
are unverified. Controlling another canister does not authorize an operator to
call `get_full_neuron` as that canister.

The official tuple is `io-official-sns-w-version-after-144202-2026-10-05`.
The lock SHA-256 is
`df5653d713dc60684bcfc7952ef0a7e5112c87025501f530b79c1c40cea722d4`;
the resolved manifest SHA-256 is
`c5eeb025b2f4b726e4eae746958413fe3da74717b3641c9acb0aaa51bc57f45c`.
Component publication IDs, source revisions, raw/gzip hashes and embedded DID
hashes are in the [unchanged official lock](../../tests/e2e_real_canisters/wasms.example.toml).
Governance's installed gzip hash is
`24ef2004240d2a34bcdaa616134d27bc18d11c1e96b378a46d564feedcc1a8c3`;
its embedded DID hash is
`0d3a00e821c8959991cfb90f4366d101fce340e78f3fb76b0de4ed28c1c08c38`.
The five running component hashes must match actual installed gzip modules.
When Root observes zero running Archives, the sixth deployed-version field is
the reviewed blessed Archive template, not a claim that an Archive is installed.
Automatic target advancement is false. No unpublished successor is assumed.

| SNS component | Publication | Version gzip SHA-256 | Raw SHA-256 |
| --- | --- | --- | --- |
| governance | 144202 | `24ef2004240d2a34bcdaa616134d27bc18d11c1e96b378a46d564feedcc1a8c3` | `e2fd07bffa7d71241c3b47e2c8398ef78bfc26b95427a3711a9e579ad5f9f190` |
| root | 143739 | `3762dd6f1348830a1d6b19252ca21f518c9f9d3a2b79b1dd99d75c0de6d46ef4` | `356960e58691a7438cb07c73806cf8d0948247508f0efc281cb2ccb7a382f836` |
| ledger | 140965 | `354dd6ecfdc72b5409805b31dea22c9db11df6e14095a5a68924eb63535e6d8a` | `a273d741019b4324b22e2e64cfb0239de148ae9693e8801102a04118dfd383c0` |
| index | 140964 | `dab6808d0dfc06e5e88336d0c3d3e45e5448c6e36c2a781f3e9e09bd450f528c` | `0768d43e0ca776c80515eb917839cbee0adca38ef69fb28280407c28fe4ea979` |
| swap | 144201 | `f3c9cd2a503f3136c91f1d44c5b4bac2d5f15a7d347731aaff03e5fa80df8c48` | `3d17f42df29a764fa3d813923646f85aac9f61320d97b2b07c1045ffb342d638` |
| archive | 140966 | `f7f48f402a128cbd18dc7734c7f149f2814c8181766f39897a66d514d47ccf11` | `ff0ba15ca5f807ee4dc77efefc58cb5e3dafee493f67b278b4e5f0b0323dfb3e` |

| Current-source IO component | Raw SHA-256 | Canonical gzip SHA-256 |
| --- | --- | --- |
| io_stream_manager | `37e708eac77f5b1c242d0b0c2d35ebb159d65af61b2cd49fa1d9ece3b41e8a68` | `c16c698317c691e119aa88b3e35fdbbebd85f39af39f44dddb5d9b4604a515e4` |
| io_nns_neuron_manager | `ad3c3f5f4a99be1b4e14b339e8618614330702b1fdf3afe8600100edb963c8b5` | `3c6495a77b7c31b50ccb706256df4fbe8cf5bf597d6707b7ab08ce8e75080e27` |
| io_historian | `cadc0a1c8ff0895bf0ac46f8c8d0c0beb7935acaf0c65568cb9f58e6f57d4d86` | `ea144ed627263bfc7aacc6717120b580761870abcaecb7c13f25ae2448ee7223` |
| io_frontend | `b1e64b3c37db290db2d73d5b53dd926a6bd510ce755e43acf772b4d474697fdd` | `c902ea51adfb532c51b63bbcc7c259e19ecf8ec2046885e22e46f4fecf81520c` |

Current-source IO raw/gzip hashes, independent build inputs, focused runtime
logs, full CI results and the exact 244 feasibility checksums are indexed by
[implementation-status.md](implementation-status.md). These are retained proofs,
not new executions of those long-running tests in this review.

This review reran three focused existing component tests, serially and offline:

```sh
cargo test --offline --locked -p io-proposed-economics-tests anchored_dynamic_backing::candidate_cadences_fit_and_twelve_hours_is_the_slowest_credible_choice -- --exact --nocapture
cargo test --offline --locked -p io-proposed-economics-tests anchored_dynamic_backing::healthy_worst_case_liquidates_before_fifteen_days_and_one_minute -- --exact --nocapture
cargo test --offline --locked -p xtask acceptance::observations::tests::existing_permanent_component_rejects_identity_and_policy_mutations -- --exact --test-threads=1 --nocapture
```

Each executed one test, passed, exit 0; logs are `focused-1.log` through
`focused-3.log` in this review's external evidence directory. The public-neuron
request in the preflight was encoded offline against the pinned NNS Wasm's
extracted authentic DID: 88 bytes, SHA-256
`2907d124aeccf0bc87d3b673780057eaa8875686b4e0c9b7ad6ccc6a78a84695`.
This tests request construction, not live visibility. All 377 retained final
validation checksums and the 33 + 211 original feasibility checksums were
reverified successfully. No build of new canister Wasms, new PocketIC integration
or local aggregate rerun was needed for documentation-only work stopped at the
authority gate.

GitHub master was verified as the reviewed `76de77b8` source. All three hosted
workflows on that exact head completed successfully on 2026-10-22:

| Workflow | Actual result |
| --- | --- |
| [Canonical test](https://github.com/aodl/IO/actions/runs/39821182602), job 119972563649 | Exit 0; Stream 24/24 in three aggregate invocations (673.70, 677.43, 682.06 seconds), NNS recovery 29/29 (528.72 seconds), zero ignored/filtered in those targets; Wasm and strict Clippy passed |
| [Security](https://github.com/aodl/IO/actions/runs/39821182605), job 119972564606 | Success; Deny/Audit passed with four allowed maintenance warnings |
| [Recorded-release reproducibility](https://github.com/aodl/IO/actions/runs/39821182592), job 119972565319 | Success; historical recorded source `270dcf3`, not a replacement for the fresh acceptance-source build |

Full logs and final job/step conclusions are retained in this review's external
evidence directory. Hosted real-canister defaults passed 106 and explicitly
ignored 81 opt-in tests; the three separately executed retained acceptance cases
provide their focused proof. Optional absent-artifact observations are not
represented as runtime passes. The fresh four-Wasm proof remains separately
source-bound to `043cde2d`, whose executable files are unchanged in reviewed
master. These hosted results do not validate an unimplemented Jupiter mirror.

Audit priorities: individual acceptance authority, exact Account separation and
Vote-only limits; future production Jupiter authority, action/hash/environment
binding, finality, voting deadlines and ambiguous-vote recovery; live neuron authority and staking Account; controller
handoff and target-subnet chunk stores; exact A/B/C conservation and fee burns;
paired reward exclusion; semantic redemption safety under delayed liquidity;
source/artifact provenance, DID and stable compatibility; and the lifecycle
gate disposition. Internal tests and this review are not an independent audit.

Parked Swap's reviewed `Adopted` constructor is frozen with zero allocation,
MAX-second opening/due timestamps, disabled auto-finalization, compatibility-only
nonzero limits and manual proposal sentinel zero. Buyers, recipes and accepted
ICP must stay zero. Unsolicited ICP is quarantined, never backing or funding,
and may be irrecoverable through normal Adopted interfaces. Later IO remains
claim-bearing under canonical exclusions/fee burns. Default-Account observations
do not inventory all Swap subaccounts. Native treasury zero-price limitations
and future Swap semantic changes require review; no fake finalization or proceeds
are permitted. Detailed source restrictions remain in [authority-review.md](authority-review.md).

## F. Change control and next gate

The preserved six-file documentation review was committed as
`919a12c8a05ddc716ccc4b7a41379c524c2cd2dd` before the product decision.
The new unstaged acceptance implementation reconciles that review with the
approved individual-control policy. It changes no Jupiter code, production IO
monetary code, official SNS pins, Candid, stable schema, timer or runtime state.
General concurrency-policy files remain unchanged. No staging, commit, push,
merge or live operation is authorized for this working patch.

Next gate: complete revised focused and canonical validation, then review the
source-bound evidence and authorize any commit separately. Live operator
authentication, topology, permanent-neuron evidence, fresh maintained direct-genesis
lifecycle closure, conditional availability wording and independent audit remain
required. IO is **not acceptance ready** solely from local authorization tests.
Production Jupiter integration remains a later separate governance workstream.
