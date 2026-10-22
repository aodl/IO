# Sale-free acceptance conversion

Readiness: **VALIDATION-PENDING FOR LIVE ACCEPTANCE; DEVELOPMENT CHECKS GREEN**.

The 2026-10-22 [readiness handoff](readiness-handoff.md) reviews integrated
master `76de77b8d997609623f3e84f8dc3a3be4991470b` and Jupiter
`1c86d9b32c774a470489b4e4a5cfe4ddfaae7341`. No canonical Jupiter decision
source or post-submission voting-deadline contract is established by those
sources. Mirror implementation stops at that gate. The handoff also records
the maintained-lifecycle disposition proposed for review and the source-defined
12-hour structural cadence's conditional liquidity limit. The
[live preflight](read-only-preflight.md) is prepared but unexecuted; these
documents do not close any live acceptance blocker.

Validated on 2026-10-22: `feat/sale-free-acceptance-genesis` commit
`043cde2d54573371f0388175963ab9fe141a8aec`, Git tree
`eba7cf4123a59c0562433e4f42a462965af1bb58`, with TLS parent
`5da0ef9c76edcd44c3c5dd6d4565f1ad0d5f57da`. The source was clean throughout
validation. This status update is a documentation-only follow-up to that tree.

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

Live acceptance still requires real Jupiter voting/mirroring integration, live
permanent-neuron visibility/authority, acceptance topology, a fresh owned-topology
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

External prerequisites remain actual Jupiter canister-originated proposal voting
and deadline/retry integration, live permanent-neuron visibility or genuine
existing read authority, frozen nine-canister observations, final A/B/C funding
and installation evidence, hosted release closure, and a separate fresh owned
maintained lifecycle run. That lifecycle remains BLOCKED / NOT RUN. The fixture
proves no mainnet visibility; no unpublished official successor is assumed.
