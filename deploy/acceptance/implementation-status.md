# Sale-free acceptance conversion

Readiness: **BASE IMPLEMENTATION VALIDATED; MAINTAINED-LIFECYCLE TRANCHE
PENDING CANONICAL CI**. The integrated base retains its source-bound validation;
the added test tranche and live acceptance gates remain open.

## Maintained lifecycle tranche — runtime passed, canonical validation blocked

The new test-only commit is `7a77fb7ff64612aa34cbdaec75a6bfd3acdc5d6c`,
tree `3af44ff8740fd4784763ccca7f155ad3a227d068`, based on merged master
`389c9ae754b64e7821cd6a4054ead7152c72c3af`. Its new ignored maintained
sale-free case executed once successfully (854.82 seconds, exit 0), including
real signed local Ledger ingress, standard SNS-controlled NNS/Historian
upgrades, automatic recovery, full A/B/C monetary progression and final
Historian convergence. The [source-bound record](maintained-lifecycle.md)
contains the coverage matrix, commands, raw evidence paths and hashes.

Affected host tests passed 107 + 4 cases; 82 opt-in cases were ignored by that
host command, not counted as executions. Affected strict Clippy and static
checks passed. The separate new ignored case really ran. The original four
current-source raw/gzip artifacts still match independent reproduction; no
production code, schema, allocation, permission or official SNS pin changed.

Canonical CI on clean `7a77fb7` passed recorded-source A/B reproduction,
security, frontend and static/unit phases, then lost its owned Stream server
during severe host memory pressure and an OOM event. Stream recorded 8 passes
and 16 transport failures. The failed aggregate was deliberately stopped
(exit -15; monitor shell 241), all 75 pre-existing assets restored, and the
worktree verified clean. Full NNS 29/29 and final workspace Clippy did not
execute. No functional canister defect is established; no source correction
was made. **This tranche is validation-pending, not integration-ready**, until
a resource-scheduled canonical rerun exits zero with Stream 24/24, NNS 29/29
and final strict Clippy. Earlier validated implementation results below retain
their own source identities; they do not certify this added test tranche.

The runtime package is at
`/home/codexdev/.codex-io/sns-framework/maintained-sale-free-lifecycle-20261026-nN06zo`.
No mainnet preflight, push, merge, funding, live identity operation or deployment
was performed. Historical SNS-W sale lifecycle remains compatibility-only.

The product decision permits the reviewed human operator to propose through
IOGOV001 and separately vote through the Vote-only IOJUP001 neuron. Authentic
Jupiter DAO governance/mirroring is a production-decentralization requirement,
not an initial acceptance blocker. The independent Jupiter ICP source is still
required and distinct. The updated handoff and read-only preflight preserve
all other live and lifecycle gates. Vote-only gives no ordinary future handover;
retaining and decentralizing the same acceptance SNS requires a separate decision
before live genesis. No elevated permission is added.

The validated implementation on `feat/acceptance-individual-governance` is
`e6126f344499dc2abf4f248b99fb273fa5cf9a41`, Git tree
`6888ca2d8bce033b01a3240c0d3c35c98603f176`, based on
`919a12c8a05ddc716ccc4b7a41379c524c2cd2dd` (the preserved six-file readiness
review). The results below validate this implementation; the earlier sale-free
baseline remains separate historical evidence.

## Individual-controlled candidate: validated implementation

Branch `feat/acceptance-individual-governance`, based on documentation commit
`919a12c8a05ddc716ccc4b7a41379c524c2cd2dd` (tree
`1aa0a75c7f70c165b4f036bc85c6f3bf4ab60e88`). Canonical validation ran against
the exact clean committed implementation identified above. This status update
changes documentation only and is outside that validated Git tree.
The frozen executable checkpoint and full file hashes are retained in
`/home/codexdev/.codex-io/sns-framework/individual-acceptance-governance-20261022T213640Z`.
Its initial combined patch SHA-256 is
`bd2644f6eadc38a60d7ca4555e15ba6a7ffb0945ce6f1d409f69fe9ae4ab73f9`;
subsequent documentation-only result updates are outside that snapshot.
Production canister/crate source, stable schemas, Cargo.lock and the official
SNS artifact lock are unchanged.

| Revised candidate check | Executed result |
| --- | --- |
| Acceptance host components | 25 passed, zero failed/ignored |
| `parsed_acceptance_production_economic_equivalence` | One exact ignored test executed and passed, exit 0; 12 invalid configurations rejected, including self-consistent human authority in production |
| `real_acceptance_operator_exact_vote_only_permissions` | One exact ignored test executed and passed, exit 0; 1,303.57 seconds; direct operator voting, unrelated-caller denial, non-voting and permission-reassignment denial |
| Required security scan | Exit 0; existing Deny/Audit policy passed, no dependency or exception changes |
| Formatting / whitespace | `cargo fmt --all -- --check` and `git diff --check` passed |
| `real_operator_30_io_proposer_fees` | One exact ignored test executed and passed, exit 0; adoption releases cost, three rejections retain 30 IO of fees, fourth proposal lacks effective stake |
| `real_launch_checkpoints_a_b_c` | One exact ignored test executed and passed, exit 0; about 1,760 seconds wall time; dust-tolerant capitalization, all four official Governance/Root installations, timer-only TopUp/return/redemption and later eligible reward exclusion |
| `real_checkpoint_a_b_negative_cases` | One exact ignored test executed and passed, exit 0; about 1,344 seconds wall time |
| `real_checkpoint_c_negative_cases` | One exact ignored test executed and passed, exit 0; about 738 seconds wall time |
| `real_parked_swap_official_framework_upgrade_without_sale` | One exact ignored test executed and passed, exit 0; about 150 seconds wall time; official 143747-to-144202 transition, physical hashes/deployed version, successful journal and Vote-only permissions |
| Fresh four-artifact reproduction | Exit 0; all four raw Wasms and four canonical gzip outputs byte-identical across independent source/Cargo-home paths; all 634 snapshot materials verified |
| Canonical CI | `cargo run --offline --locked -p xtask -- test_ci`: exit 0 on 2026-10-24, 10,066.96 seconds, exact committed source above; all required groups passed |
| Full Stream / NNS recovery | Stream 24/24 in each of three canonical invocations (2,500.15 / 2,251.90 / 1,519.55 seconds); NNS recovery 29/29 (1,189.52 seconds); zero failed, ignored or filtered in these targets |
| Final strict Clippy | Executed and passed: `cargo clippy --workspace --all-targets --all-features -- -D warnings`; the separate earlier focused attempt interrupted with 143 is not counted as a pass |
| Canonical security / interface / stable / provenance / frontend / Wasm | All passed; recorded-release reproduction remains separate from the fresh four-current-source-Wasm proof |

Canonical run: `2026-10-24T08:24:45Z` through `2026-10-24T11:12:32Z`.
The verified `icp 0.2.7` binary was first in invocation-local PATH; its SHA-256
is `99aaef26bd765ce197c1de525ddb437ad1d3e933e5d3ca2d720ed189c23b7667`.
PocketIC 14 used independent server/port paths, `RUST_TEST_THREADS=1`,
`CARGO_BUILD_JOBS=1`, `POCKET_IC_MUTE_SERVER=1` and offline Cargo/npm inputs.
The ordinary real-canister suite passed 106 tests with 81 expected opt-in tests
ignored. The seven exact selections above supply their separate execution
evidence; ignored cases are not counted as executed. No long focused case was
repeated for the unchanged executable inputs.

Evidence directory:
`/home/codexdev/.codex-io/sns-framework/individual-governance-final-ci-20261023T063809Z`.
`successful-canonical-ci.log` SHA-256:
`fd22d400efa9155a6dbe9f94f549a08cb11339172104fda07e3a2e6834dad419`.
`successful-canonical-ci-result.json` SHA-256:
`e9312e6820cdcfe1a6932296120ab993b695c13992e0f19199464f0af1c87efd`.
`successful-ci-source.json` SHA-256:
`5af8de7bd00d936b10eba87b6da7f4d287c0e360a13945668698cded6c429283`.
`successful-ci-summary.json` records target counts and restoration of all 75
preserved assets with their original hashes/modes. No kernel OOM-counter rise
occurred during this run. Confirmed IO-owned servers retained after completed
targets were stopped only after their test parents exited. Foreign processes
were not signalled. The original failed aggregate and resource observations are
retained in `canonical-ci-first-attempt.log` and `retained-ci-diagnosis-20261024.md`:
its server-loss/connection-refused failures under host pressure did not establish
a source defect. The aggregate itself finished exit 1, and its final Clippy passed.

The existing `reproduction-committed-source-binding.json` binds all executable
inputs of the four-current-source-Wasm reproduction to this commit. Its 634
source materials and raw/gzip comparisons remain valid; final proposer-risk
documentation was the only difference from the reproduction snapshot. This does
not rebind historical release manifests or complete the separate live release gate.

No draft PR or hosted workflow run exists for this feature head as of the
2026-10-24 integration review. The PR description is prepared externally;
opening it awaits separate repository-write authorization. Local canonical
success is recorded independently of any later hosted check conclusion.

The permissions case completed authentic genesis and reserve observations:
30 IO operator, 100M IO excluded, reserve fee burn 10,000 e8s. Its checkpoint B
computed C0=30 IO, B0=30 ICP, permanent increment=20 ICP, independent anchor=10 ICP.
This case does not fund those contributions or prove the full lifecycle.
The separate full A/B/C case passed as recorded above. Runtime replies are copied beneath
`runtime-evidence/acceptance-genesis-1558866-1792706856694939907`; per-case logs,
exact commands, kernel exit evidence and resource samples are retained beside it.
Both real selections use `--ignored --exact --test-threads=1 --nocapture`.

The two-CPU shared host recorded sustained load averages 15–21 and available
memory around 1.5–3 GiB. The case passed despite slower checkpoint validation.
Serial cases use independent owned servers and port files; no foreign process
was signalled and no exclusive-host prerequisite or general agent rule was added.
The dispatcher was briefly held between tests, then resumed using the measured
successful execution and available memory; its wall timing includes that hold,
while the test runtime above does not.

The source-backed proposer rejection review in authority-review.md identifies a
live acceptance risk requiring product sign-off: after full reserve migration the
frozen plan has no funded non-privileged proposer top-up source. Three rejected
or expired proposals can stall the Root-controlled installation sequence. Submit
one proposal at a time and stop after the first retained 10 IO charge; no genesis,
permission, reject-cost or monetary change is included.
Loss of the sole operator's signing authority can also prevent proposals and the
decisive vote, with no alternate voter or ordinary Vote-only handover in this
configuration. Both exhaustion/deadlock and key-loss consequences require explicit
predeployment review; development CI does not approve those acceptance risks.

## Retained validated sale-free baseline

Validated on 2026-10-22: `feat/sale-free-acceptance-genesis` commit
`043cde2d54573371f0388175963ab9fe141a8aec`, Git tree
`eba7cf4123a59c0562433e4f42a462965af1bb58`, with TLS parent
`5da0ef9c76edcd44c3c5dd6d4565f1ad0d5f57da`. The source was clean throughout
validation. This retained validation predates the individual-governance working patch.

| Committed-candidate validation | Actual result |
| --- | --- |
| `acceptance_sale::real_launch_checkpoints_a_b_c` | One executed, passed; exit 0; 530.97 seconds |
| `acceptance_sale::real_checkpoint_a_b_negative_cases` | One executed, passed; exit 0; 857.46 seconds |
| `acceptance_sale::real_checkpoint_c_negative_cases` | One executed, passed; exit 0; 530.45 seconds |
| Fresh independent current-source reproduction | Four raw Wasms and four canonical gzip outputs byte-identical; exit 0; 632 snapshot files matched HEAD Git blobs |
| [Hosted canonical CI](https://github.com/aodl/IO/actions/runs/39732151304) | Exit 0; full Stream 24/24 in each of three aggregate invocations; NNS recovery 29/29; zero ignored/filtered in those targets |
| [Hosted security](https://github.com/aodl/IO/actions/runs/39732151296) | Passed Cargo Deny and Audit; four allowed maintenance warnings |
| [Hosted recorded-release reproduction](https://github.com/aodl/IO/actions/runs/39732151301) | Passed for historical recorded source; separate from the fresh current-source proof |

The focused commands used `cargo test --offline --locked -p e2e-real-canisters`
with each exact name above and `-- --ignored --exact --test-threads=1 --nocapture`.
They ran serially on pinned PocketIC 14 with the reviewed official SNS bundle,
typed initializer and current-source IO/frontend Wasms. Independent Jupiter jobs
overlapped with separate servers/state/output paths; sampled available RAM stayed
at least 2,416 MiB during the focused commands, with no observed OOM-counter rise.
Hosted CI separately passed frontend, security, interface, stable-state, provenance,
Wasm and strict all-targets/all-features Clippy gates. Its real-canister default
suite passed 106 tests and explicitly ignored 81 opt-in cases; the three local
exact invocations above provide their runtime evidence.

The dusted launch retained 1 ICP in Swap's default Account, Adopted state and zero
accepted participation. C0/B0 remained 30 IO/30 ICP, the permanent increment
20 ICP and the independent Dynamic anchor 10 ICP. Quarantined ICP contributed
zero backing/funding. Timer-only TopUp, principal return, the exact 49,990,333
e8s ICP redemption credit and the later operator entitlement passed with zero
keeper calls and Jupiter monetarily excluded.

Commands, logs, resource samples, hashes and retained typed replies:
`/home/codexdev/.codex-io/sns-framework/sale-free-resource-aware-validation-20261022T065759Z`.
Fresh build inputs/results and all four raw/gzip artifacts:
`/home/codexdev/.cache/io/sale-free-committed-reproduction-043cde2d5457-20261021T212003Z`.
The build executed on 2026-10-22; its directory suffix identifies the prepared
driver. Historical release artifacts/evidence were not rebound or modified.

Live acceptance still requires operator authentication and the separate ICP
source, permanent-neuron visibility/authority, acceptance topology, a fresh owned-topology
maintained lifecycle, the applicable finite liveness proof and independent audit.
No merge, live deployment, funding or identity operation is approved by these results.

Parked Swap default-Account ICP deposits are observable quarantined funds and
do not block acceptance or contribute to backing, permanent capital or funding.
Later IO transfers remain claims under the existing supply/reserve/exclusion
formula, with actual fee burns. The exact zero genesis allocation, Adopted state,
frozen constructor and zero accepted participation/buyers/recipes remain required.
Observing default Accounts does not inventory all Swap-owned subaccounts.

No production canister source, monetary state, schema, launch economics or official
SNS pins changed during validation. No staging, commits or pushes were performed.

The active path is typed direct Normal genesis with the installed parked standard
Swap, exact three-neuron allocation, observed C0-derived A/B/C capitalization,
and Governance/Root installation of the four IO roles. No active sale fallback,
post-launch permission removal, or monolithic sealing path remains.

## Historical conversion baseline

The following records describe the earlier pre-dusting conversion, before the
two development commits. They retain their original source/evidence authority;
queued or uncommitted descriptions here do not describe the validated tree above.

| Validation | Actual result |
| --- | --- |
| Host tool package | 164 passed, zero failed/ignored; includes all 23 acceptance components |
| Typed initializer equivalence | One ignored test explicitly executed and passed; ten malformed configurations rejected |
| Optimized current-source IO build | All four roles built successfully |
| Independent reproduction | All four raw and canonical `gzip -n -c` artifacts match across source and Cargo-home paths |
| Focused strict Clippy | Passed for xtask and real-canister test packages; canonical final gates remain required |
| Formatting and diff whitespace | Passed |
| Feasibility evidence preservation | 33 + 211 checksums verified; original public-neuron probe and SNS helper byte-preserved |
| Historical sale records | Three files archived byte-for-byte; no active authority |
| Conversion PocketIC probes | Eleven focused exact runtime tests passed; parser component also rerun successfully |
| Full NNS recovery target | 29 passed, zero failed/ignored; exit 0 in 2287.68 seconds |
| Full Stream / canonical CI | Not run yet; waiting for an uncontended runner |
| Required security scan | Passed after the two approved host TLS patches; first failure retained |

The approved Cargo.lock patches are `webpki-roots 1.0.7 -> 1.0.10` and
`rustls-webpki 0.103.15 -> 0.103.16` in PocketIC's host TLS dependency graph.
Neither has a normal canister dependency edge. Both crate downloads completed;
the repeated required security scan passed. No advisory exception was added.

The complete A/B/C launch passed in 305.03 seconds with all four optimized IO
Wasms, autonomous readiness, Dynamic principal return, exact redemption and a
later eligible proposal event. The 0.5 IO redemption credited 49,990,333 ICP e8s
after the 10,000 e8s payout fee. The proposal event credited exactly one daily
operator entitlement using actual Governance shares; Jupiter remained excluded.
The reviewed 143747-to-144202 Governance upgrade passed with an actual successful
journal outcome, physical module hashes and no SNS-W deployed-instance record.
The public typed list-neuron observation and same-caller get_full_neuron denial
also passed. No mainnet visibility is inferred.

Runtime diagnosis corrected only test-harness behavior: numeric cycles JSON,
bounded tick draining before day-sized clock advances, and waiting for backed
re-entry before submitting the reward motion. Read-only failure diagnostics and
the failed/interrupted attempts are retained; interruption is not counted as a
pass. No monetary-runtime correction was needed.

The focused launch probes completed without a foreign test workload. A new
foreign aggregate began during the full NNS target; that target still completed
successfully and its resource overlap is retained. Stream, independent
post-patch reproduction and canonical CI remain queued rather than starting
alongside that aggregate. The post-patch normal Wasm dependency graph is already
byte-identical, and primary rebuilds match all four raw/gzip artifacts.

The independent build's first Frontend copy phase failed because the external
Cargo target path differed from the existing helper's expected repository target
path. An alias corrected the external workflow; the repeated phase and all eight
byte comparisons passed. No repository helper change was needed.

| Current-source IO artifact | Raw SHA-256 |
| --- | --- |
| io_stream_manager | `37e708eac77f5b1c242d0b0c2d35ebb159d65af61b2cd49fa1d9ece3b41e8a68` |
| io_nns_neuron_manager | `ad3c3f5f4a99be1b4e14b339e8618614330702b1fdf3afe8600100edb963c8b5` |
| io_historian | `cadc0a1c8ff0895bf0ac46f8c8d0c0beb7935acaf0c65568cb9f58e6f57d4d86` |
| io_frontend | `b1e64b3c37db290db2d73d5b53dd926a6bd510ce755e43acf772b4d474697fdd` |

Artifacts are retained outside the historical release at
`/home/codexdev/.cache/io/acceptance-sale-free-io-20261020`.
Independent source inputs and reproduction results are at
`/home/codexdev/.cache/io/sale-free-reproduction-20261020`.
These are uncommitted-source build evidence, not a newly committed release pair.

Full commands, first failures, source/patch checkpoints, artifact hashes and
resource evidence are retained under
`/home/codexdev/.codex-io/sns-framework/sale-free-acceptance-conversion-20261020`
and the external monitored-command ledger's commands/resources/checkpoints.
Historical sale records are in history/sale-architecture-2026-10-18/ and retain
only their original dated source authority.

External acceptance prerequisites remain human operator authentication, independent
Jupiter ICP source identity, live permanent-neuron visibility or genuine
existing read authority, frozen nine-canister observations, final A/B/C funding
and installation evidence, hosted release closure, and a separate fresh owned
maintained lifecycle run. That lifecycle remains BLOCKED / NOT RUN. The fixture
proves no mainnet visibility; no unpublished official successor is assumed.

The fresh reproduction completed on 2026-10-23 at
`/home/codexdev/.cache/io/individual-acceptance-reproduction-20261023`.
`source-inputs.json` truthfully binds an uncommitted snapshot to base HEAD, patch
hashes, 634 material hashes, independent source/Cargo-home paths and release
profile. `results.json` records eight successful byte comparisons and separate
raw/gzip hashes. These are the same four IO hashes in the retained table below;
no historical release manifest is rebound. All 75 preserved user frontend,
ignored generated and artifact files remain byte/mode identical before CI.
