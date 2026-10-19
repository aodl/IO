# Acceptance implementation and validation

## Official SNS validation — 2026-10-18

Executed NNS proposal 144202 officially published Governance revision
`98c898f8de068a1fc0e97ebc805f9fa7f315d862` with the exact
`latest_reward_event_participation` ABI used by IO. The active reviewed lock and
checkpoint-A authority select the coherent SNS-W version after that
proposal, rather than requiring the historical fcc same-source candidate.
Resolution against downloaded bytes passed for all six official components:
compressed and raw Wasm hashes, embedded source revisions and embedded DIDs all
match the reviewed pins. The clean parser source is independently pinned to
`98c898f8de068a1fc0e97ebc805f9fa7f315d862`. Final source approval remains a
human gate.

The official-source track is `OFFICIAL_SOURCE_VERIFIED_AND_LOCALLY_TESTED`;
the canonical validation track is `CI_GREEN` for the tested implementation
below. The package remains `LOCALLY_REHEARSED_AWAITING_INPUTS`, not approved
for deployment. The dated sections following this update preserve historical
results, including failed aggregates; they are not the current validation status.

### Tested source and inputs

- Branch/HEAD: `feature/self-deployed-sns-acceptance` at
  `d3435371d6aa743b3fc17bb0038cf90e9765dd1e`, with the uncommitted adoption
  implementation. Pre-CI tracked patch SHA-256:
  `2bac20780f04dc429ec9aaffe0ac0a45beffd713664fc1230182d4bbebeb36f1`;
  stable patch ID: `536916ac1f79e08a4e219a79f3396d4c0c5d7515`.
  Status reconciliation after this run changes documentation only.
- Source mode `official`, scope `governance`, no component overrides; reviewed
  baseline `io-official-sns-w-version-after-144202-2026-10-05` is an IO-local
  frozen-tuple label, not a network-issued SNS-W identifier. The resolved
  manifest is
  `/home/codexdev/.cache/io/sns-framework/official-io-official-sns-w-version-after-144202-2026-10-05-df5653d713dc6068/manifest.toml`,
  SHA-256 `c5eeb025b2f4b726e4eae746958413fe3da74717b3641c9acb0aaa51bc57f45c`.
  The previous Governance artifact is an upgrade-test input, not a seventh
  deployed component.
- Clean parser source: `98c898f8de068a1fc0e97ebc805f9fa7f315d862`;
  binary `/home/codexdev/.cache/io/acceptance-parser-98c-auto-frozen-v1/sns-payloads`,
  SHA-256 `472ae7449f13ce776fb2cfd3752c5df75de81858d878b57b9787ccea2ac2bbb5`.
  Its completed build log is `20261018T090215Z-2992436.log` in the directory below.
- Acceptance used current-source optimized IO Wasms and the isolated frontend
  output `/home/codexdev/.cache/io/acceptance-frontend-official-98c-v1`.
  Typed Governance initialization, the existing post-sale parameter proposal,
  and queried effective parameters all prove automatic target advancement
  false; the effective neuron-capacity limit is 1,000. Installed module hashes
  match the reviewed artifacts.

### Executed results

All logs below are local retained files under
`/home/codexdev/.codex-io/sns-framework/commands/`; exact commands, exit statuses
and source checkpoints are in the sibling `PROGRESS.md` and `checkpoints/`.
Each listed invocation exited zero. Requested real cases executed rather than
silently skipping. Profile test names retain historical `candidate` wording
but these runs used the selected official bundle.

| Validation | Actual execution | Command log |
|---|---|---|
| Official `contract` with required `latest_reward_event_participation` | 10 boundary tests, 1 additive Candid test, 1 real contract case (27.33 s) passed | `20261018T160754Z-3349458.log` |
| Official `io` with the same required capability | Boundary/Candid/real contract checks passed; real IO reward-share case passed (192.14 s) | `20261018T160935Z-3351099.log` |
| Official `upgrade` with the same required capability | 1 real reviewed predecessor-to-selected Governance upgrade case passed (222.02 s) | `20261018T161351Z-3354096.log` |
| `acceptance_sale_real` | 1 passed, 0 ignored (198.85 s): genuine sale, successful finalization, Governance Normal, A/B/C, capitalization, Governance/Root installations, frontend binding, entitlement and exact redemption | `20261018T162307Z-3362160.log` |
| `real_launch_timer_only_top_up_and_reward` | 1 passed, 0 ignored (192.54 s): zero keeper calls, generation-1 TopUp and structural finalization, generation 2 and subsequent eligible reward event | `20261018T162647Z-3365522.log` |
| Real A/B/C negative cases | 2 passed, 0 ignored (816.62 s) | `20261018T163053Z-3368834.log` |
| Full NNS recovery target | 29 passed, 0 failed, 0 ignored (1,195.90 s); idle/hard TTL 120/7,200 s unchanged | `20261018T164444Z-3376815.log` |
| Full Stream target | 24 passed, 0 failed, 0 ignored (1,523.37 s); idle/hard TTL 300/3,600 s unchanged | `20261018T170500Z-3393749.log` |

The profiles prove exact current-event timestamps and `opt nat` shares, positive
participation with zero native SNS reward issuance, zero/stale/absent-share
handling, IO normalization and Jupiter economic exclusion, and source-defined
Governance upgrade compatibility. Historical-manifest compatibility remains
static validation, not runtime execution of arbitrary older locks.

Real acceptance evidence is
`target/acceptance-sale-3363171-1792340636447101422`; timer-only evidence is
`target/acceptance-sale-3366281-1792340882222348909/timer-only.json`.
The timer-only run retained backing `3299980000` e8s and separately charged
`10000` e8s to the excluded anchor. The acceptance redemption paid exactly
`49990303` ICP e8s for `50000000` IO e8s. The first official acceptance attempt
failed because the parameter-observation DTO omitted the automatic-advance
field; retaining that field fixed the summary comparison. The fresh complete
pass above, not that failed attempt, is the acceptance result.

### Canonical CI, security and reproducibility

The unchanged canonical `cargo run --offline --locked -p xtask -- test_ci`
ran serially with PocketIC 14, `POCKET_IC_MUTE_SERVER=1`,
`RUST_TEST_THREADS=1`, `CARGO_BUILD_JOBS=2`, `CARGO_NET_OFFLINE=true`, and the
verified acceptance inputs above. It exited **0** from 17:31:02 to 19:38:00 UTC
on 18 October 2026: **2 h 6 m 58 s**. Log:
`20261018T173102Z-3475880.log`, SHA-256
`de6f9dd3556e381948e1e8e71b9b04205a4653fd7de1eb34e0f2c41b7daafff9`.
The matching resource log is in the sibling `resources/` directory.

All required groups passed: formatting, all-target compilation, acceptance and
source/provenance gates, interfaces, stable storage, simplicity, frontend,
unit tests, required PocketIC/SNS/Root groups, local integration, E2E, workspace
tests, value-moving Wasm compilation and strict all-target/all-feature Clippy.
Inside the aggregate, the full NNS recovery target passed 29/29 in 1,194.55 s;
the full Stream target passed 24/24 in each of its three invocations
(1,525.38, 1,519.29 and 1,519.58 s). No required group was skipped. Workspace
tests retain 77 opt-in ignored registrations; the requested official runtime
cases were separately executed above.

The required security gate passed: cargo-deny, cargo-audit and duplicate-tree
inspection completed successfully with `quinn-proto 0.11.18` and
`rustls 0.23.46`. Four existing allowed unmaintained warnings remain; no new
advisory exception was introduced. Serial runs used 8,325,529,600 bytes physical
RAM, two CPUs, no swap and no finite applicable cgroup memory limit. No new
OOM event or transport failure occurred; the historical OOM-kill counter stayed
at 15. This does not attribute the older failures to OOM.

Historical reproducibility passed: checked-in artifacts for recorded source
`270dcf3dc71fc8e7b63c3177b0e3f58fc9246b35` equalled two isolated builds with
different source and Cargo-cache paths. This does **not** establish a completed
release or release reproducibility for the adoption source. Fresh current-source
release artifacts and source-bound rehearsal evidence remain required. Historical
packages were not rewritten; original browser, ignored generated and debug
outputs were restored byte-for-byte, and release artifacts were unchanged.

The separate official `lifecycle` profile is **BLOCKED / NOT RUN**: it requires
a fresh owned topology and its authorized local workflow. No historical topology
substitutes for that run. Final source/artifact and sale/metadata approval,
canonical acceptance identities/controllers/supported subnet observations,
authorized permanent full-neuron observation, actual Jupiter proposal-mirror
and Vote-only integration, and independent audit/mainnet authorization remain
external prerequisites. No mainnet operation or deployment is claimed.

## Historical fcc rehearsal — 2026-09-14

The complete local launch passed through `acceptance_sale_real`: real sale,
A/B/C, reserve migration, capitalization, permanent refresh, initially registered
Root installation, autonomous readiness, Dynamic reconciliation, daily
entitlement, and exact real-ledger redemption. All eight final focused commands
passed. Canonical `test_ci` also passed. No deployment is approved or performed.

## Source and preservation

- IO branch: `replace-prepared-push-staging-account`.
- IO HEAD: `6e2c7c6a53a99bd78770053f42e3bbbe0db00a68`.
- Preserved staged binary patch SHA-256:
  `e96bb938a829bb2cefdded85ee457790d3e76276255c031ca9902304b5ac93e3`.
- IC source: clean `fcc281f7200a8d3192ec46c225b436b2096bbf58`, branch
  `agent/expose-sns-reward-event-shares`; tracked-diff SHA-256 is the empty-patch
  hash `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`.
- Source mode `local`, scope `same-source`, all six SNS component overrides;
  official baseline `io-official-sns-baseline-2026-08-01`.
- Bundle preparation passed, followed by the capability-bearing `contract`
  profile using `--source bundle` against the exact resolved artifact set.
- Governance DID SHA-256:
  `0d3a00e821c8959991cfb90f4366d101fce340e78f3fb76b0de4ed28c1c08c38`.
- Resolved manifest:
  `/home/codexdev/.cache/io/sns-framework/local-fcc281f7200a-clean-same-source-io-official-sns-baseline-2026-08-01-1c7b278cca608e13/manifest.toml`.

| SNS component | Raw Wasm SHA-256 (direct-install module hash) | Compressed `.wasm.gz` artifact SHA-256 |
|---|---|---|
| governance | `3a916fe6465cfb7817bb5fceebb8ec78b4be92fe28483db8b4209f52ab597b11` | `a5a456a75e5665ff35458142d235787da2ae763ae5c87c3f53488f89fd828b17` |
| root | `f38d96291417811b7c473c29be3d5d79db50d545fc748a452ae8cd07ce092116` | `9ab55367045abbcc228a5ccfd8d4160248cb4db98ac806303dc227a7ed6d92d4` |
| ledger | `4422ed028bcd59dd5c4f190f0850ee9ee2d39992bceaa1cbb0bc6317ee5ad5a9` | `d6856114a3808d6c85f3b5780b7155623308c237686fc8c8c62438f4e9532ce6` |
| index | `450b73d1938f496ccac196ad62fecc8042e108480bcc3ba53d3fa4bb8b0eccec` | `ee1c3d2f4bf7cc97b002429022ccf95040803987e9c5eed60dbb99774f276b69` |
| swap | `eac96f8c651af325cad714d19dc7349e4bfc2881556b24636e7155271e5e04b6` | `31e67f3d9299e24f619dc19d2ab8ffd8d8410cadc397b13ee6184368beb25b93` |
| archive | `e97f64059a1ff4f24c8032d405a10ca3c0f4bfe3efcce15a0932d185271bf294` | `c6af8668191ee042591dd8db64f5fe9943c80781aeb2144da0eb9b1b33b96d59` |

The complete launch installed these optimized current-source IO raw Wasms,
also recorded with their bytes and sidecars in checkpoint A:

| IO component | Raw Wasm / installed module SHA-256 |
|---|---|
| Stream | `3597cf1e187cb9cf271fc7fd8d30776e9fe4d2d6139410f9bede5a508daee60a` |
| NNS Manager | `2a31315b6a59d96d116a547cd692d0d6ed797686d383ccc558bca27c09842236` |
| Historian | `7334fd89f2bcb3c91fc52785f300b628cc107ece119b10f7600e61681f8dfcd5` |
| Frontend | `8624eb2539beb3ff0a884a662536027244c0d1396c1027cad7f2c7cf77dafe03` |

NNS in the new sale/refresh tests is PocketIC 14's real bootstrap NNS. Its build
metadata reports release/optimization 3/compiler 1.93.1 without an IC commit;
it is not claimed to be built from the SNS source revision. No SNS-W is needed
for the self-deployed sale. No privileged NNS/Governance/Root caller is
impersonated in the acceptance launch.

## Implemented behavior

- Generated canonical YAML is consumed by the pinned friendly parser,
  pre-execution validator and five-canister payload builder. Parsed economics
  match across acceptance/production labels; local metadata remains explicit.
  The builder's 200,000-neuron default is reduced to 1,000 through an actual
  adopted post-sale parameter proposal, not a fixture genesis patch.
- A checks source/artifact bytes, complete Accounts, parser payloads, baseline
  identities and install authority. B checks raw finalization results and
  distribution conservation before deriving C0/B0/Delta K. C checks actual
  funding, refreshed permanent stake, anchor, authority and production-typed
  constructor bytes against the checked-in interfaces.
- Offline frozen-acceptance contexts require real metadata, approved inputs,
  exact allowlisted roles and existing full-neuron baseline/refresh evidence.
  They cannot inherit local metadata. No real mainnet observations are claimed.
- Existing Governance chunk storage plus Root's Install proposal mode installs
  all four IO dapps after capitalization, with initial dapp registration retained.
  The shared application subnet is material to the proved chunk-store route.
- Real proposer fees and exact canister-originated `{Vote}` permissions are
  tested separately from the retained reward/history/replay/recovery scenarios.
- Stream's completed TopUp now finalizes the same structural checkpoint using
  canonical post-transfer observations before clearing the operation. This
  repairs the observed consumed-generation replay loop without adding state,
  timers, queues or public methods.
- Frontend's existing Candid `version` query is exported by its Wasm. Host-linked
  canister Candid exports are gated to avoid duplicate host linker symbols.

## Observed economics

The first complete green package is
`target/acceptance-sale-301932-1789383755805662037` (117.86 seconds).
The final focused rerun passed in 116.68 seconds and wrote
`target/acceptance-sale-312855-1789384809337519657`.
All amounts below are e8s unless noted.

| Observation | Value |
|---|---:|
| Genesis IO supply | 100000000000000000 |
| Post-migration IO supply | 99999999999970000 |
| Stream IO reserve | 89999996699990000 |
| Excluded Jupiter stake | 10000000000000000 |
| Excluded reserve-neuron remainder | 0 |
| Operator stake | 3000000000 |
| Participant basket stakes, combined | 299980000 |
| Swap IO residual | 0 |
| IO distribution fees / reserve disbursement fee | 20000 / 10000 |
| Retained SNS ICP treasury | 299990000 |
| C0 = B0 | 3299980000 |
| Permanent top-up (ceiling of 2 C0 / 3) | 2199986667 |
| Initial Dynamic anchor | 1000000000 |
| Initial active backing / reward-active stake | 3000000000 / 0 |
| Dynamic claim principal after reconciliation | 3000000000 |
| Anchor fee consumed by initial TopUp | 10000 |
| Redeemed IO | 50000000 |
| Actual net ICP payout | 49990303 |

The daily event recorded positive entitlement only for the operator; the
100M-IO Jupiter neuron and short sale baskets received none. This is daily
entitlement evidence, not a claim that a full two-week reward payout ran in
this acceptance test. The 1:1 launch reference does not erase later IO fee burns:
redemption used actual C=3299960000 and B=3299980000, producing gross 50000303
and net 49990303 ICP e8s. Sale treasury proceeds remained separate.

## Commands and evidence

Exact commands, start/end times, exit statuses, resource heartbeats and tracked
patch checkpoints are recorded in
`/home/codexdev/.codex-io/sns-framework/PROGRESS.md` and its `commands/` directory.
Final focused commands run serially through
`/home/codexdev/.cache/io/acceptance-final-focused.py`; each command is separately
recorded by the monitored-command wrapper.

The real tests use these explicit inputs:

```text
POCKET_IC_BIN=/home/codexdev/.local/bin/pocket-ic-server
POCKET_IC_MUTE_SERVER=1
IO_REAL_SNS_WASM_DIR=/home/codexdev/.cache/io/sns-framework/local-fcc281f7200a-clean-same-source-io-official-sns-baseline-2026-08-01-1c7b278cca608e13/wasms
IO_REAL_SNS_WASM_MANIFEST=/home/codexdev/.cache/io/sns-framework/local-fcc281f7200a-clean-same-source-io-official-sns-baseline-2026-08-01-1c7b278cca608e13/manifest.toml
IO_ACCEPTANCE_SNS_PAYLOAD_TOOL=/home/codexdev/.cache/io/acceptance-parser-20260914-c/sns-payloads
IO_ACCEPTANCE_IO_WASM_DIR=/home/codexdev/src/IO/target/wasm32-unknown-unknown/release
```

Core build/check commands:

```sh
tools/scripts/test-sns-framework --source local --scope same-source --profile contract --prepare-only
cargo build --offline --locked -p io-stream-manager -p io-nns-neuron-manager -p io-historian -p io-frontend --release --target wasm32-unknown-unknown
cargo build --offline --locked -p mock-nns-candidate-actor --target wasm32-unknown-unknown
cargo run --offline --locked -p xtask -- acceptance_sale_real
cargo test --offline --locked -p xtask acceptance:: -- --nocapture
cargo test --offline --locked -p io-stream-manager --lib
cargo clippy --offline --locked -p xtask -p e2e-real-canisters -p io-stream-manager -p io-frontend --all-targets -- -D warnings
cargo fmt --all -- --check
```

All seven focused real tests passed as separate processes with the environment
above and this command for each full test name:

```sh
cargo test --offline --locked -p e2e-real-canisters <FULL_TEST_NAME> -- --ignored --exact --test-threads=1 --nocapture
```

- `acceptance_sale::self_deployed_sale_finalizes_and_migrates_reserve`
- `acceptance_sale::real_checkpoint_a_b_negative_cases`
- `acceptance_sale::real_checkpoint_c_negative_cases`
- `acceptance_sale::real_operator_30_io_proposer_fees`
- `acceptance_sale::real_jupiter_canister_exact_vote_only_permissions`
- `acceptance_sale::parsed_acceptance_production_economic_equivalence`
- `acceptance_sale::real_nns_existing_id_noncontroller_refresh`

The current-source `tests::candidate_reward_shares_drive_io_rewards` integration
also passed serially in 166.72 seconds against the pinned Governance candidate.
It was executed directly from the test binary built by the successful canonical
gate, so this final validation required no additional build or network fetch.

Native SNS CLI and parser adapter builds passed. The CLI build's exact command
is in `commands/20260914T072330Z-47390.log`; adapter success is in
`commands/20260914T084920Z-118611.log`. The parser's `build-inputs.json` records
its native Bazel configuration separately from the six-Wasm build configuration.

Remaining external requirements are recorded in `blockers.json`. Real branding,
identities, sale parameters and baseline observations remain unapproved/unset.
The actual Jupiter canister's permanent hardening and mirrored voting remain an
external integration milestone. All installs, funding, neuron operations and controller changes were confined
to local PocketIC. No mainnet query or operation, staging, commit, push, reset,
clean, stash or rebase was performed.

## Corrective hardening — 2026-10-16

The starting IO checkout was clean `temp` at
`12438287435857991cdec8485d8df9c7bf7e7e90`, with parent
`6e2c7c6a53a99bd78770053f42e3bbbe0db00a68`, equal to `origin/temp` (0 ahead,
0 behind). There were no staged, unstaged or untracked files and no initial
patch identity. The earlier observations above remain historical evidence.

Checkpoint C accepts an initial Dynamic seed of at least 10 ICP; runtime
bootstrap still records zero claim principal and exactly 10 ICP of anchor
capacity. Physical excess is excluded surplus. The initial seed check is not
applied to later fee-depleted anchor balances. Component cases cover
999999999, 1000000000, 1000000001 and 6000000000 e8s. The real C mutation test
also checks that accepted excess leaves the exact constructor bytes unchanged.

The existing frontend pipeline has an isolated public-directory input. The
acceptance wrapper explicitly supplies all six browser inputs, builds the
browser before Rust, and preserves tracked assets. A shared offline check binds
executable JSON configuration, the private manifest, content-addressed JS,
HTML and embedded Wasm bytes. Local installed-asset checks compare HTTP bodies;
the Governance/Root launch also checks the exact installed module hash and
private-manifest 404. No schema or canister method is added or changed.

`sns-source-baseline.json` is the single reviewed source-revision authority.
Parser construction reads it; validators and the test driver compile it in.
The official parser adapter explicitly disables automatic SNS target-version
advancement in the typed Governance init payload. The same false value travels
through the already-required post-sale parameter proposal with the neuron-cap
change, and checkpoint C verifies the queried effective value. This is launch
configuration only; it adds no IO runtime state, timer or method.
Untrusted evidence cannot authorize another revision by repeating its hash.
The reviewed friendly-parser hash and all six mandatory DID hashes/capability
checks are enforced. Every cached DID also matches its Git blob at the reviewed
pin. The sibling IC checkout has unrelated work at another revision; it is
untouched and rejected by parser construction. The verified cached rehearsal
bundle/parser are reused; no new candidate source is fetched or selected.
Final deployment-source approval remains unresolved.

The complementary timer-only launch test retains the real sale and ordinary
Governance/Root installation route. After installation/readiness, its window
uses only PocketIC time advancement, ticks and status queries through TopUp,
structural finalization and later reward entitlement. The keeper-assisted
launch/redemption and recovery paths remain intact.

The timer-only test reproduced a Ready Stream stuck with a prepared TopUp and
an NNS Pool awaiting its transfer after two simulated hours. The existing reward
callback retried observation, which returned Busy while that operation occupied
the monetary slot. The narrow runtime correction resumes that exact durable
TopUp in the same callback before observing another checkpoint, using the
existing 60-second retry. No timer, cadence, stable field, worker or monetary
rule is added. A separate earlier run lost its local PocketIC server during a
long host pause in offline C validation, before reaching the timer window; its
connection-refused failure is not timer evidence. The owned test server uses a
3600-second idle TTL and 7200-second hard TTL, independently of simulated time.

The full-neuron authorization extension passed against PocketIC 14's bootstrap
NNS (compiler 1.93.1, release optimization 3, no IC commit in build metadata).
The local operator controls a canister that controls the neuron: that operator
cannot read the full neuron. The neuron controller and a separately configured
local hotkey can read the complete typed record, and reads preserve that record.
Existing-ID refresh remains permissionless and grants no read permission.
Inspected NNS source also authorizes controllers/hotkeys of ManageNeuron
followees. Public neuron summaries omit required private evidence. No production
inspection endpoint or permission change is introduced. An existing authorized
observer must supply both authentic typed replies before the manual preflight
can proceed; real acceptance read permission is unknown.

Validation results are recorded in the monitored command logs. Focused component
tests passed (23 acceptance tests), browser unit tests passed (41), and the
installed frontend HTTP test passed. The offline required security scan failed
on nine advisories in the unchanged reference lockfile: `quinn-proto 0.11.15`
requires at least `0.11.18`, and `rustls 0.23.40` requires at least `0.23.46` for
the reported advisories. Dependency remediation is outside these five fixes;
no ignore rule, lockfile change or claimed green security gate is included.

The corrected timer-only case passed in 362.73 seconds against optimized
current-source Stream Wasm
`8f21bda82ea6f05654074d1ce6d0db53da91040786e787d12283e1437ed7d280`.
Its local evidence is
`target/acceptance-sale-1691477-1792172276395051405/timer-only.json`:
generation 1 finalized at target/observed pooled backing `3000000000`, total
claim backing stayed `3299980000`, and the canonical Dynamic staking balance
was `3999990000` after one `10000` fee against the excluded anchor. With zero
keeper calls, generation 2 and reward round 8 were then observed; the processed
reward-event count increased from 0 to 1 and eligible policy credit from 0 to
`1000000000000000000` (internal credit units, not issued IO).

The exact installed frontend served `/` and
`/generated/app.88098eefae09.js` successfully, with bytes matching its isolated
build output and a 404 for the private manifest. Its configuration is network
`ic`, demo `false`, and the four acceptance IDs in the runbook. SHA-256 values:

- JS: `88098eefae0902f97ca7b4f40f6eca399106318a8877d8774f17ec3360c01c5f`.
- HTML: `349a1973e391f8cd8f7387731cec3795b8a8822cdd24936b768b3d56bc450259`.
- Installed frontend Wasm: `a0b4155ea37171baead8d104f464288f7ef6b8e51668fca4a37e33b76e8ca444`.

The isolated output is
`/home/codexdev/.cache/io/acceptance-hardening-WkhZEp0k/frontend-verified`.
A separate build with inherited wrong IDs, local network and demo enabled
produced byte-identical output because the wrapper supplies every explicit
acceptance value. Tracked frontend assets were untouched by both builds. This
evidence does not approve the final production browser build or replace the
historical release artifacts above.

The keeper-assisted `acceptance_sale_real` rerun passed in 282.89 seconds,
including genuine sale finalization, A/B/C, the parameter proposal and four
Governance/Root installations, entitlement and the exact 49990303 e8s redemption
payout. The focused C mutation suite passed in 757.96 seconds. The expanded A/B
negative suite passed in 840.32 seconds, rejecting altered Wasm, required DID,
parser and source evidence. Its first attempt was deliberately interrupted
before the default PocketIC server's hard TTL; that attempt is not validation.
All Sale fixtures use the same owned server with sufficient host lifetime.

The existing same-source bundle `contract` profile passed with required
`latest_reward_event_participation`, including the real Governance case
(67.06 seconds), ten reward-boundary tests and the additive DTO test. Source
mode was `bundle`, scope `same-source`, reviewed IC commit
`fcc281f7200a8d3192ec46c225b436b2096bbf58`, recorded clean tree/no tracked diff,
official baseline `io-official-sns-baseline-2026-08-01`, and all six SNS
components overridden together. The resolved manifest was
`/home/codexdev/.cache/io/sns-framework/local-fcc281f7200a-clean-same-source-io-official-sns-baseline-2026-08-01-1c7b278cca608e13/manifest.toml`.
The six raw/compressed hashes and Governance DID hash are the unchanged values
in the historical source-built artifact table above, independently reverified
by this profile and the checkpoint tests. This remains local rehearsal
provenance, not final deployment-source approval.

Broader validation exposed two directly related frontend harness assumptions.
The existing source guard expected the default no-argument cleanup call; it
checks the selected `generatedDir` and isolated-public input instead, and its
exact retest passed. The certified-asset gate generated a new browser bundle
but ran the package's live asset test against an older debug Wasm before its
Rust rebuild. The strict served-byte assertion correctly rejected that mismatch.
The existing gate steps are reordered to build embedded assets before both
test invocations; no assertion, test or builder is removed. The complete
reordered gate passed (explicit live test 55.58 seconds, twelve asset tests
52.79 seconds, package live test 54.60 seconds). Interrupted aggregate attempts
are not counted as completed validation. Production Wasms are unchanged by
these test/tooling corrections.

Byte-for-byte restoration of the starting browser outputs exposed the same
ordering requirement at the root CI boundary: `test_unit` also executes the
frontend PocketIC target, but preceded `frontend_required`. Its strict HTML
comparison rejected cached `app.bdb872e86a0f.js` Wasm against restored
`app.fefd0a864fc3.js` assets (42.87 seconds). Root CI runs the existing frontend
gate first, with an ordering assertion in the existing source guard. No test
is removed or relaxed. The exact guard and formatting checks passed; the
canonical aggregate is rerun after this final tooling correction. Production
IO Wasms and the acceptance-specific artifact evidence above are unchanged.

### Final corrective validation — 2026-10-17

The starting checkout was `temp` at
`12438287435857991cdec8485d8df9c7bf7e7e90`, upstream `origin/temp`, ahead/behind
0/0, with no staged, unstaged or untracked files and zero diff statistics.
It matched the reviewed reference exactly; its parent was
`6e2c7c6a53a99bd78770053f42e3bbbe0db00a68`. The reference commit's stable patch
ID is `a0ec5bf6847bdfeb37be647cd5113025663c16f2`; there was no initial worktree
patch. Pre-existing ignored frontend and debug outputs were preserved too.

The final implementation patch SHA-256 (tracked code/build/test changes and
reviewed source baseline, excluding explanatory docs and generated index) is
`7df7dca6929ff4be029ee1a7575f5b0b99f90d8a29655ba030d78ac58015c40f`.
The two new files are independently bound by SHA-256:

- `tools/acceptance/build-frontend`:
  `6aed51d00a7a5e5ac5ae975d150e038ad977118a70302c1a20cc978dd34d42b8`.
- `tools/acceptance/frontend_assets.rs`:
  `dc8b2ba8016a1c3119e57c57c4ae538e8c8ab38d619bd33609d28309bb34188f`.

The final canonical aggregate completed with exit 1, not a green validation.
Its log is
`/home/codexdev/.codex-io/sns-framework/commands/20261016T214304Z-1950550.log`;
the matching resource log and exact commands/patch checkpoints are retained
under `/home/codexdev/.codex-io/sns-framework/`. Cargo/npm were offline, with
two build jobs and serial muted PocketIC 14. Offline audit wrappers outside
the repository disable advisory fetching, not any security checks.

Passed in that final aggregate: formatting, workspace all-targets check,
acceptance/static/interface/stable/simplicity gates, both recorded-source
reproductions, frontend-before-unit ordering and HTTP byte checks, 161 xtask
tests, 65 Stream unit tests, 46 NNS unit tests, the first complete 24-case
Stream PocketIC target (2456.92 seconds), two NNS Manager cases (64.28 seconds),
two Historian cases (36.31 seconds), SNS topology (55.82 seconds), both Root
groups (117.47 and 116.43 seconds), and local CLI/API checks. Workspace frontend
and Historian live tests also passed (69.84 and 115.14 seconds). The concluding
Wasm check and strict all-targets/all-features clippy passed (56.52 seconds for
clippy).

Failures are retained, not converted into passes:

- Required security scan: the nine unchanged-lockfile advisories listed above.
- Repeated Stream target: 9 passed, 15 transport failures, 3189.19 seconds.
  `first_payout_definitive_no_effect_defers_candidate_at_coarse_cadence` timed
  out; subsequent cases received connection refused at `127.0.0.1:41273`.
  The same target, including that case, passed in full earlier in this run.
- Workspace NNS recovery: 1 passed, 28 transport failures, 1825.86 seconds.
  The dust case timed out; subsequent cases received connection refused at
  `127.0.0.1:45401`, near the target's existing 1800-second host server limit.
  The earlier completed aggregate had passed that dust case and eighteen other
  recovery cases against unchanged production code before its own later server
  expiry. This is not a claim that the full workspace command passed.
- A fresh exact payout-deferral retry lost its server with HTTP
  `IncompleteMessage` at `127.0.0.1:41051`, before a monetary assertion
  (506.17 seconds). Its log is `20261017T003130Z-2140620.log` in the command
  directory above.

The final isolated Dynamic dust/fee regression passed against current code in
153.16 seconds:
`cargo test --offline --locked -p io-nns-neuron-manager --test io_nns_governance_recovery_pocketic account_policy_zero_memo_rejects_permanent_collision_and_accepts_candidate_dust -- --exact --test-threads=1 --nocapture`.
It proves zero initial claim credit, fixed 10 ICP anchor, excluded unsolicited
surplus, fee-only anchor consumption and exact permit replay. Its monitored
log is `20261017T004235Z-2144582.log`. This successful narrow rerun does not
turn the failed aggregate into a pass.

Resource heartbeats show sustained roughly 127–170 MiB available RAM and no
swap before these losses; an OOM termination is not proven. No monetary
assertion failed. No timer, monetary rule, assertion or dependency policy was
weakened in response. Reliable aggregate execution and separately reviewed
dependency remediation remain required; this source is not called CI-green
or commit-ready.

Focused acceptance commands used the verified bundle/parser/current optimized
IO Wasms and isolated frontend directory specified above, with
`POCKET_IC_BIN=/home/codexdev/.local/bin/pocket-ic-server`,
`POCKET_IC_MUTE_SERVER=1`, `RUST_TEST_THREADS=1`, `CARGO_NET_OFFLINE=true` and
`NPM_CONFIG_OFFLINE=true`. Exact command bodies were:

```sh
cargo test --offline --locked -p xtask acceptance:: -- --nocapture
npm run test:frontend-unit
cargo test --offline --locked -p io-frontend --test io_frontend_pocketic -- --nocapture
cargo test --offline --locked -p e2e-real-canisters acceptance_sale::real_nns_existing_id_noncontroller_refresh -- --ignored --exact --test-threads=1 --nocapture
cargo test --offline --locked -p e2e-real-canisters acceptance_sale::real_launch_timer_only_top_up_and_reward -- --ignored --exact --test-threads=1 --nocapture
cargo run --offline --locked -p xtask -- acceptance_sale_real
cargo test --offline --locked -p e2e-real-canisters acceptance_sale::real_checkpoint_c_negative_cases -- --ignored --exact --test-threads=1 --nocapture
cargo test --offline --locked -p e2e-real-canisters acceptance_sale::real_checkpoint_a_b_negative_cases -- --ignored --exact --test-threads=1 --nocapture
cargo run --offline --locked -p xtask -- test_ci
```

Browser construction used
`bash tools/acceptance/build-frontend /home/codexdev/.cache/io/acceptance-hardening-WkhZEp0k/frontend-verified`,
then the existing offline locked release build for Stream, NNS Manager and
Historian. The separate same-source `contract` command and its full bundle
path are recorded above. The aggregate ran the requested `cargo fmt --all --
--check`, `cargo check --workspace --all-targets`, `cargo test --workspace`,
`cargo clippy --workspace --all-targets --all-features -- -D warnings`, DID,
stable-storage and simplicity gates. No mainnet query, permission inspection,
credential access, deployment or new SNS source/dependency download occurred.

| Correction | Final classification | Focused proof |
|---|---|---|
| A: initial anchor dust | FIXED_AND_TESTED | Below/exact/excess threshold cases; real C unchanged constructors; fee consumption distinct from seed |
| B: browser artifact | FIXED_AND_TESTED | Isolated explicit configuration; executable/manifest/hash/Wasm binding; installed HTML/JS equality and private-manifest 404 |
| C: SNS provenance | FIXED_AND_TESTED | Shared compiled approval boundary; source/parser/all-six Wasm/DID mutations; real same-source capability contract |
| D: autonomous TopUp | FIXED_AND_TESTED | Reproduced Busy stall; existing callback resumes same durable operation; timer-only checkpoint and reward progression passed |
| E: full-neuron read | SOURCE_PROVEN_EXTERNAL_PRECONDITION | Local controller/hotkey success, unrelated operator denied; real authorized observer remains unverified |

Net implementation line changes are +17 monetary runtime, +14 frontend build
support, and +719 acceptance/test tooling and source-binding data (including
181 lines in the two new host-only files). No new production runtime module,
stable field/schema, public method, worker, queue or timer is introduced.
The source pin literals are consolidated; one shared browser validator serves
both checkpoints and the existing test infrastructure. These additions prove
the five requested properties rather than add another deployment path.

The corrective package is `LOCALLY_REHEARSED_AWAITING_INPUTS`, not approved for
deployment. Remaining external inputs are human sale/metadata decisions,
actual Jupiter mirror and Vote-only integration, final SNS source/artifact
approval, canonical acceptance identity/subnet observations, and an existing
authorized permanent-neuron observer. No unfinished IO implementation was
identified beyond these corrections; the repository-wide security and host
validation blockers above remain explicit.

Final input hash rechecks matched the tested files. The resolved bundle manifest
SHA-256 is `1c7b278cca608e134357d3e9d43d32bb4d900f2ea27e51a16e1650f15c162192`;
the parser binary is
`cbecc3f7ffd09bac9274f4a6ad16fdb85a23fadbdbb29d7bf9ff3842fe75c797`.
Its adapter/build-input binding and reviewed friendly-parser hash remain
required. All six raw/compressed Wasm hashes match the source-built table
above. The required DIDs also matched their Git blobs at the rehearsal pin:

| DID | SHA-256 |
|---|---|
| Governance | `0d3a00e821c8959991cfb90f4366d101fce340e78f3fb76b0de4ed28c1c08c38` |
| Root | `9867bfe913aa17d42d891c22187273e643eedb3ec8e4b3af4b894e74945d5806` |
| Ledger | `6ce029d5dc6a430bd9cdc8adcabfb32f7e497dcafaa360974d3d1770b225df52` |
| Index | `c029dc066f5b8a9312fb79d1ae193c59228a3818b649df25773ac3e1fadbeab6` |
| Swap | `2f0c524b3f4dd84a63ef0cbe8df37042ad662f7befc9612afe9b9e15ce900f56` |
| Archive | `e0a2ec22f38e8fd450b2aff24d68aefddbf5108bb43c23ee7e854bb956e9c0ee` |

After validation, original tracked `public/index.html`, the entire pre-existing
ignored generated directory (bundle and manifest together), release artifacts
and pre-existing debug artifacts were restored and compared byte-for-byte with
the starting backups. Changed validation outputs were moved recoverably to
`/home/codexdev/.cache/io/acceptance-hardening-WkhZEp0k/post-aggregate-final`.
No unrelated work was discarded. No run-owned live PocketIC server remains.

Post-restoration checks passed: all 23 acceptance component tests, formatting,
`did_surface`, `validate_stable_storage`, `simplicity_check`, draft
`acceptance_preflight`, build-wrapper shell syntax and `git diff --check`.
The implementation/new-file hashes above are unchanged. Subsequent edits are
explanatory documentation only; the failed full aggregate is not relabelled
or rerun solely for those edits. Branch/HEAD and upstream 0/0 are unchanged;
nothing is staged, committed or pushed. Mainnet deployment remains unauthorized.
