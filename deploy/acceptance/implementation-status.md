# Acceptance implementation and validation — 2026-09-14

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
