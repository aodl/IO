# Sale-free acceptance conversion

Readiness: **INDIVIDUAL-CONTROLLED ACCEPTANCE IMPLEMENTATION VALIDATION PENDING**.

The product decision permits the reviewed human operator to propose through
IOGOV001 and separately vote through the Vote-only IOJUP001 neuron. Authentic
Jupiter DAO governance/mirroring is a production-decentralization requirement,
not an initial acceptance blocker. The independent Jupiter ICP source is still
required and distinct. The updated handoff and read-only preflight preserve
all other live and lifecycle gates. Vote-only gives no ordinary future handover;
retaining and decentralizing the same acceptance SNS requires a separate decision
before live genesis. No elevated permission is added.

The working patch on `feat/acceptance-individual-governance` is based on
`919a12c8a05ddc716ccc4b7a41379c524c2cd2dd` (the preserved six-file readiness
review). Its focused/canonical checks must be recorded separately below;
retained results at the earlier source do not validate these changes.

## Individual-controlled candidate: validation in progress

Branch `feat/acceptance-individual-governance`, based on documentation commit
`919a12c8a05ddc716ccc4b7a41379c524c2cd2dd` (tree
`1aa0a75c7f70c165b4f036bc85c6f3bf4ab60e88`). This candidate is prepared for the authorized local validation-pending checkpoint.
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
| Canonical CI / full Stream and NNS / strict Clippy | Pending execution against the local checkpoint; the explicitly approved icp 0.2.7 was provisioned externally and passed its archive/binary pins and version check. Shared default 1.2.0 is not substituted. The separate reduced-priority focused Clippy attempt was intentionally interrupted (143), not passed. |

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
