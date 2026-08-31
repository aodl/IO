# Official SNS Testing

IO runs SNS-shaped mock/PocketIC tests, pinned real-canister profiles, and an optional maintained source-built local SNS-W rehearsal.

We do not currently run the official SNS launch locally in required CI.

Official SNS testing is optional and heavier. The current official ICP/DFINITY SNS testing documentation is the source of truth. The historical standalone `dfinity/sns-testing` repository is deprecated; if the official docs reference successor tooling or a new repository/location, use that current official location.

The maintained official local SNS flow uses the source-built `sns` CLI; this is optional/manual, local-only for the local rehearsal layer, and not part of required IO workflows. The selected schema-v2 package `2026-08-31-b6a26f2-anchored-dynamic` records completed candidate Governance/Root launch compatibility, Stream and NNS-manager activation, prepared-push redemption and exact reward/structural observations for source `b6a26f223b4c37f021ea398f3003b4d149683ee9` and artifact child `af7e0791384808b1f8304e7048b86ade9a328306`. SNS testflight remains a separately authorized mainnet rehearsal.

IO's canonical IO ledger should be the SNS ledger; any IO_TEST ledger is non-canonical.

NNS Manager execution canister `oae4c-3iaaa-aaaar-qb5qq-cai` and the
two-year protected NNS neuron `10292412127977304661` are not touched by these tests.

## Layer 1: IO Mock/PocketIC SNS-Shaped Harness

This layer uses repo-owned mocks for SNS governance, SNS root, ledger, and index canisters. It tests IO-specific accounting, typed-operation retry, governance-read mapping, root/controller upgrade intent, stable-state behavior, and the reviewed simplified production DIDs.

It does not run official SNS launch, SNS-W, decentralization swap, or mainnet testflight.

## Layer 2: PocketIC NNS/SNS/Application Subnet Topology

This layer uses PocketIC topology support to create NNS, SNS, and application subnets, then installs IO dapp canisters and mocks on appropriate subnets where practical.

It is useful for canister placement, principal ranges, constructor wiring, and controller behavior. It is still not official launch unless real SNS canisters are installed.

## Layer 3: Official Local SNS Launch Rehearsal

This optional layer follows the current official ICP/DFINITY SNS testing documentation and must use the source-built `sns` CLI to rehearse official local launch mechanics. It can validate whether a candidate `sns_init.yaml` can move through the local SNS launch process after a local operator completes the run.

This layer is not required CI, not part of `verify_release`, not run by `test_ci`, and not a substitute for security review or tokenomics decisions.

The local package lives in `deploy/local-sns-rehearsal/`. It renders a local SNS init file from ignored inputs and implements restartable phases 12–18 for exact release dapp installation, NNS Root preparation, same-source Governance/Root publication, SNS-W creation and finalization, canonical discovery, treasury funding, ledger/index/archive evidence, controller and upgrade checks, lifecycle registration/activation, production redemption, exact NNS boundary tests and account-semantic IO orchestration. The upgrade phase can fall back from the maintained chunk-store CLI to an inline exact-Wasm SNS Governance proposal; Root still performs the upgrade and no direct management-canister substitution is used. Every phase remains guarded, loopback-only and evidence-producing; the automation itself is not proof of successful execution.

Canonical packages classify conclusions into three layers. Source-built official
SNS tooling proves the local SNS topology and governance/controller/ledger
observations. The exact proposal-143660 PocketIC suite proves the active NNS
Governance mechanics. Controlled current-IO PocketIC fixtures prove paired
issuance, TwoYear no issuance, full semantic-Account capture, donation
carry-forward and exact recovery. The official SNS topology is not presented
as proof that it installed proposal 143660, and controlled failure injection is
not presented as a live-local observation.

The repository validator `cargo run -p xtask -- validate_local_sns_rehearsal` is no-network and may run in normal checks. `cargo run -p xtask -- validate_local_sns_ledger` validates the selector-bound current package and is green for the current release. A future rehearsal still uses an ignored run-local inventory and may become selectable only after a new immutable package validates; it does not overwrite or rebind the current or historical packages.

Source-built revision `4320fdf2e613844eabae1927b1a23b98da3a7bc6`
locally proved `latest_reward_event_participation`. The separately reviewed
official artifact lock remains Governance revision
`b904c9dd1bdef8841bd12f03efbc71180a015e25`; local source proof is not official
release adoption. Launch still requires a reviewed official release containing
the capability and refreshed official artifact and DID pins.

The issuance model under this layer is protocol reserve transfer. The
standalone rehearsal proves reserve-to-user and exact user-to-reserve ICRC-1
push mechanics. The launch Stream Manager prepares the quote and proves the
supplied push block before creating the ICP payout obligation; it has no
allowance or spender authority. IO does not assume arbitrary post-launch
minting or constant supply unless the final ledger fee mode proves it.

## Layer 4: Mainnet SNS Testflight

This future manual layer uses a mock SNS on mainnet to test day-to-day governance operations before real launch. It can test upgrade proposal operations, root control, controller handoff, frontend/historian configuration, and proposal tooling.

It does not perform the real SNS launch, does not run a real swap, and must not be confused with the final NNS launch proposal.

## Local References

- `tools/sns/README.md`
- `tools/sns/sns_init.io.template.yaml`
- `tools/sns/sns_init.io.local.yaml`
- `tools/sns-testing/README.md`
- `deploy/local-sns-rehearsal/README.md`
- `deploy/local-sns-rehearsal/sns_init.local.template.yaml`
- `deploy/local-sns-rehearsal/runbook.sh`
- `deploy/local-sns-rehearsal/commands.local.example.md`
- `tools/sns/testflight/README.md`
- `tools/sns/launch-readiness.toml`

Official reference points used for this package are the pinned Internet Computer `rs/sns/testing` and `rs/sns/cli` sources for SNS testing, local SNS rehearsal tooling, testflight, and PocketIC NNS/SNS subnet integration.
