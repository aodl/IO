# Sale-free acceptance: governance and operational handoff

Review date: 2026-10-22. **Development validated; acceptance blocked.**
This review authorizes no network observation, installation, funding, identity
operation or controller change. It introduces no voting implementation.

## Source and evidence boundary

| Repository | Reviewed commit | Git tree | Review state |
| --- | --- | --- | --- |
| IO | `76de77b8d997609623f3e84f8dc3a3be4991470b` | `21a9f27be1d9b91a4d4e31354a2600f34e46e4b9` | Clean master; documentation review on `feat/acceptance-readiness-authority-review` |
| Jupiter | `1c86d9b32c774a470489b4e4a5cfe4ddfaae7341` | `ebbe3a3d5fd669910600547431e758a6ff578fd4` | Clean master; read only; no branch or worktree mutation |

Both origin URLs and GitHub master commit/tree identities were checked. IO's
executable source is unchanged from `043cde2d54573371f0388175963ab9fe141a8aec`
(tree `eba7cf4123a59c0562433e4f42a462965af1bb58`); the intervening status update
is documentation only. These identities describe reviewed inputs, not a new
release/artifact-recording pair.

The retained validation directory is
`/home/codexdev/.codex-io/sns-framework/sale-free-resource-aware-validation-20261022T065759Z`.
Its `SHA256SUMS` hash is
`f3fd6d6d4b5adf1a4b4070800d98dcf28a529a0ecd7eb06f8f0b05b41f9a2521`.
This review's source hashes, GitHub results, test commands and logs are outside
the repository at
`/home/codexdev/.codex-io/sns-framework/acceptance-readiness-20261022`.

## A. Authority gate: no approved Jupiter decision source

The reviewed Jupiter repository supplies no canonical jUP Governance principal,
deployment evidence, decision interface or approved alternative decentralized
decision process for IO voting. Its [SNS Rewards README](https://github.com/aodl/JUPITER_FAUCET_SUITE/blob/1c86d9b32c774a470489b4e4a5cfe4ddfaae7341/canisters/sns-rewards/README.md)
describes jUP Root as a future replacement for development owner indexing.
This is a source-backed missing prerequisite, not proof that no such deployment
exists anywhere. No live deployment was queried.

| Authority or identity | Evidence / required relationship | Status |
| --- | --- | --- |
| Deciders of Jupiter's IO vote | Approved decentralized Jupiter governance and its decision rules | Unresolved product/governance prerequisite |
| Binding source decision | Canonical source principal, proposal ID, typed action and irreversible decision state | No approved source or record format supplied |
| Production SNS Rewards | `alk7f-5aaaa-aaaar-qb4ra-cai` | Documented live owner-index service; not the acceptance voter |
| Acceptance SNS Rewards | Independent reviewed canister identity, actual module and controller evidence | Unresolved; never substitute production |
| Jupiter ICP inflow source | Separate authenticated monetary source principal | Unresolved; grants no voting authority |
| Owner-index Root | OpenChat `3e3x2-xyaaa-aaaaq-aaalq-cai`, resolving Governance/Ledger for owner attribution | Not an IO vote authority; must not be repurposed |
| IO Governance | `h7y7s-kyaaa-aaaas-amxrq-cai` | Frozen acceptance target; live state unobserved |
| IO Jupiter neuron | `IOJUP001`, 100M IO, 100% multiplier, sole acceptance SNS Rewards principal, exactly `{Vote}` | Genesis and authorization locally proved; real mirror absent |
| IO operator neuron | 30 IO; may propose; cannot vote for Jupiter or grant itself Jupiter permissions | Locally proved using a caller mock |
| Upgrader of Jupiter SNS Rewards | Existing actual controller graph; future intended jUP DAO handoff | Deployment docs allow bootstrap control before DAO handoff; live authority unverified |

Intended authority chain, with unresolved links shown explicitly:

```text
approved Jupiter decentralized decision source [UNRESOLVED]
  -> canonical finalized, action-bound decision [UNRESOLVED]
  -> acceptance Jupiter SNS Rewards [ID / implementation / control UNRESOLVED]
  -> RegisterVote as that canister on IOJUP001
  -> IO SNS Governance -> SNS Root -> four IO dapps
```

IO's locally proved final controller graph is separate from neuron permissions:
Governance solely controls Root; Root solely controls Governance, Ledger, Index,
Swap, Stream, NNS Manager, Historian and Frontend. NNS Manager is the intended
controller of the permanent and Dynamic NNS positions. The operator retains no
fallback controller
in the accepted final graph. Jupiter SNS Rewards is external to the four IO
dapps. An individual who can replace Jupiter SNS Rewards could replace its vote
logic: its authentic decentralized upgrade authority must therefore be part of
the governance contract, not merely a Wasm hash check.

### Minimum proposal-binding review contract

This is a requirement for a future approved implementation, not a new schema
or an implemented authorization policy. Bind the canonical source governance
principal, decision/proposal ID and final result to the acceptance context,
exact IO Governance principal, IO proposal ID, neuron, action variant and all
execution parameters. For installation bind target, install mode, raw module
hash, constructor bytes/hash and chunked-Wasm descriptor; for generic actions
bind function ID, registered target/method and exact payload hash. Include a
single-use authorization identity and expiry before the actual target deadline.
A title, summary, owner snapshot or caller-supplied Boolean is insufficient.

Pinned Governance `get_proposal` summarizes blobs larger than 64 bytes with
their length and SHA-256; `list_proposals` clears some payloads. A future mirror
must verify the source-defined canonical representation, including the
constructor and chunked module hash, rather than hash a redacted response as if
it were the original payload. See upstream [types.rs](https://github.com/dfinity/ic/blob/98c898f8de068a1fc0e97ebc805f9fa7f315d862/rs/sns/governance/src/types.rs#L1783)
and [Governance get_proposal](https://github.com/dfinity/ic/blob/98c898f8de068a1fc0e97ebc805f9fa7f315d862/rs/sns/governance/src/governance.rs#L1778).
Exact successful or already-cast vote evidence must be reconciled through valid
Governance observations; a caller-filtered ballot list is not authentication.
Missing, rejected, stale, ambiguous or unfinalized decisions never imply Yes.

### Deadline feasibility

Pinned official Governance is `98c898f8de068a1fc0e97ebc805f9fa7f315d862`.
[topics.rs](https://github.com/dfinity/ic/blob/98c898f8de068a1fc0e97ebc805f9fa7f315d862/rs/sns/governance/src/topics.rs#L63)
classifies actions; `Action::voting_duration_parameters` applies critical
minimums. The accepted typed initializer sets no custom criticality override.
Each proposal freezes its own initial period and wait-for-quiet
parameters at creation; later parameter changes are not a substitute for
reading the actual proposal.

No live proposal was queried and no new cross-DAO decision record was obtained.
The table derives windows from the accepted initializer and pinned implementation;
it is not a measured mirror round trip. A future integration proof must retain
each canonical `get_proposal` creation time, action, topic, frozen periods,
current WFQ deadline and final decision alongside the source decision record.

| Acceptance action class | Initial period | WFQ parameter | Deadline without extension |
| --- | ---: | ---: | --- |
| Four initial IO installations (`UpgradeSnsControlledCanister`, Install mode), dapp registration/settings | 86,400 s | 43,200 s | creation + 86,400 s |
| Framework upgrade / advance target | 86,400 s | 43,200 s | creation + 86,400 s |
| Motion and normal-topic custom execution | 86,400 s | 43,200 s | creation + 86,400 s |
| Governance/ledger parameters, metadata, treasury/mint; generic-function registration/removal, topic changes, deregistration and other critical operations | 432,000 s minimum | 216,000 s minimum | creation + 432,000 s with accepted parameters |

Use `wait_for_quiet_state.current_deadline_timestamp_seconds`, falling back to
creation + the proposal's own initial period only when that state is absent.
WFQ extends only on qualifying tally changes, up to initial + twice its
parameter (nominal maxima 172,800 / 864,000 seconds); it provides no guaranteed
extra time. `ProposalData::accepts_vote` uses strictly before the deadline;
`register_vote` rejects strictly after it. Do not design around this exact-second
boundary or an already-decided proposal's reward-only voting window.
See [proposal.rs](https://github.com/dfinity/ic/blob/98c898f8de068a1fc0e97ebc805f9fa7f315d862/rs/sns/governance/src/proposal.rs#L2134)
and [register_vote](https://github.com/dfinity/ic/blob/98c898f8de068a1fc0e97ebc805f9fa7f315d862/rs/sns/governance/src/governance.rs#L3870).

For creation time `t0`, discovery delay `d`, source decision time `j`, observation
and verification `v`, submission/recovery `s`, the required condition is
`d + j + v + s < target_deadline - t0`, with a reviewed safety margin. Earliest
discovery could be immediate in a future implementation; no discovery mechanism
exists in SNS Rewards. Latest and worst-case completion are not bounded by the
reviewed Jupiter source. Its daily owner scan calls only `list_neurons`, uses
100-neuron pages, 60-second start/busy retries and 300-second page-failure retries;
none observes proposals or records final decisions. A bounded individual call
timeout does not bound total retries, outage or upgrade interruption.

If the future decision source uses this standard SNS implementation, its
minimum ordinary period is one day (default four days), and early irreversible
majority decisions are possible but not guaranteed. Even the minimum unextended
one-day finality path, started after IO discovery, plus positive observation
and vote latency cannot reliably fit IO's one-day installation window. Waiting
for source WFQ can take longer. Critical IO windows may offer more nominal room,
but source parameters and availability assumptions remain unknown. There is
therefore **no demonstrated post-submission mirror deadline contract** for the
required initial installations.

Product approval must first select the authentic Jupiter authority, its
upgrade/control chain, exact finality rule and timing contract. Choices for
separate review include waiting for that governance system, binding a genuine
prior decision to an exact future IO action, or reviewing IO voting periods.
None is selected or implemented here. An unrelated DAO, automatic approval,
operator discretion and mock votes are not alternatives to that authority.

## B–C. Implementation and real integration

**Blocked at the authority/timing gate; zero new voting tests executed.**
No Jupiter file, Candid method, stable field, migration or timer was changed.
`sns-rewards/src/state.rs` preserves its `VersionedStableState::V1` metadata
(memory 0) and owner maps A/B (memories 1/2). The two production queries remain
`get_relay_reward_context` and `resolve_default_icp_accounts`. Root changes
invalidate both maps; unchanged-Root upgrades restore snapshots/cursors and
restart scanning. Existing four ignored PocketIC owner-index tests use mocked
Root/Governance/Ledger responses; they are not canonical governance evidence.
No new stable compatibility claim or new Jupiter validation result is made.

The retained IO A/B/C proof uses `mock_nns_candidate_actor` to call Governance
as a canister. It proves Vote authorization, operator non-adoption, Root's four
installs, and unchanged economics. It does not prove real Jupiter decisions,
proposal mapping, deadline service, replay recovery or live deployment.
Positive/negative/upgrade voting scenarios in the requested next-stage contract
remain mandatory after authority/timing approval; implementing a mock decision
source would not close them.

## D. Lifecycle coverage and disposition for review

| Requirement | Existing proof at reviewed source | Remaining gap / proposed disposition |
| --- | --- | --- |
| Six official components, direct Normal genesis, exact IO/IO, parked Swap, controller graph | Retained typed A/B/C launch and component/module observations | Live installation observations remain required |
| 1B distribution, reserve disbursement, C0=30 IO, B0=30 ICP, permanent +20 ICP, independent 10 ICP anchor | A/B/C and negative cases; exact Ledger/NNS records | Repeat with real identities and separately authorized funding |
| Root installs all four current IO Wasms via target-local chunks | Four physical module hashes and proposal IDs in `root-install-proposals.json` | Real Jupiter authorization absent |
| Timer-only TopUp, 14-day return, semantic redemption and later eligible official reward shares; Jupiter excluded | Completed A/B/C runtime evidence, zero keeper calls | Executed schedules, not a universal time bound |
| Default blessed framework upgrade without SNS-W deployment registration | Reviewed 143747 → 144202 Governance upgrade, actual Success journal, Root/module/version observations | Future successor compatibility is not implied; custom SNS-specific paths have separate registry restrictions |
| Fresh current-source raw/gzip bytes across independent source/Cargo-home paths | Four-artifact reproduction; 632 source files bound to 043cde2 | No historical manifest or installed evidence rebound |
| Recovery, interfaces, stable state, served frontend, full NNS/Stream and strict Clippy | Canonical CI and existing targeted upgrade/recovery suites | Not a single fresh maintained-topology restart/recovery package with authentic Jupiter |
| Source-built SNS CLI, SNS-W creation/discovery and sale finalization | Historical maintained `lifecycle` phases 10–17, old source-bound packages | These exercise a sale-created fixture, not the accepted launch. Recommend reclassifying creation/finalization as compatibility-only, subject to explicit review |
| Maintained host tooling, actual client ingress, restart-safe owned topology, end-to-end evidence packaging and historian convergence | Historical profile; source-aware low-level proofs cover substantial behavior | Fresh direct-genesis package and exact restart/observation gaps need explicit gate disposition; no automatic waiver |
| Real Jupiter voting and source-decision recovery | None | Required new integration after authority/timing resolution |

`tools/scripts/test-sns-framework --profile lifecycle` delegates to the
SNS-W creation/finalization runbook. Running that sale exercise would not close
the missing Jupiter authority or reproduce direct genesis. No fresh historical
sale topology was executed in this review. The mandatory fresh owned-topology
maintained-lifecycle gate remains **BLOCKED / NOT RUN, pending reviewed
disposition**. Do not mark it complete, delete it, or create a second active
launch path. The proposed replacement evidence is the exact direct-genesis
A/B/C package plus real Jupiter integration and a source-bound maintained
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
was executed. Acceptance SNS Rewards and the independent inflow principal remain
unset; permanent-neuron visibility/authority and all nine live canister states
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

Audit priorities: genuine Jupiter authority and upgrade control; action/hash/
environment binding and irreversible decision finality; voting deadline and
ambiguous-vote recovery; live neuron authority and staking Account; controller
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

This stage changes documentation only. No Jupiter changes, production monetary
changes, official artifact-pin changes, Candid changes, stable migrations or new
runtime state are authorized or introduced. No staging, commit, push or merge
belongs to this stage. General concurrency-policy files remain unchanged.

Next implementation gate: approve a genuine Jupiter governance decision source,
its deployment/control chain, environment-specific identities, canonical
proposal binding and deadline contract. Then implement/test the smallest voting
integration in an independent Jupiter worktree, including pending-vote upgrade
and negative cases. Until that gate and the listed live/lifecycle/audit gates
close, IO is **not acceptance ready**.
