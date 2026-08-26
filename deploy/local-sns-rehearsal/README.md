# IO Official Local SNS Rehearsal

This package is local-only. It provides rehearsal scaffolding and evidence validation for creating a real SNS-created IO ledger/index/governance/root stack in a local rehearsal environment.

It is not production launch configuration, not final tokenomics, not a mainnet SNS proposal, not required CI, and not proof that IO is live.

Do not use `--network ic`. Do not call mainnet. Do not touch NNS Manager
execution canister `oae4c-3iaaa-aaaar-qb5qq-cai` or the two-year protected NNS neuron
`10292412127977304661`.

## Files

- `sns_init.local.template.yaml`: local/dev candidate config template for official SNS tooling.
- `local-vars.example.toml`: local variables template for rendering the SNS init file.
- `runtime.local.example.toml`: ignored per-run SNS plan, neuron and Account inputs; it never defines production values.
- `dfx.local.json`: local custom-canister definitions used by the restartable deployment phase.
- `sns_init.local.yaml`: ignored local render output. Its logo paths resolve relative to this file.
- `canister-ids.local.example.toml`: template for recording local SNS canister IDs and manually observed ledger evidence.
- `commands.local.example.md`: local-only command templates for ledger/index/governance/root evidence.
- `runbook.sh`: single operator entrypoint.
- `scripts/00-check-prereqs.sh`: prerequisite and guardrail check.
- `scripts/01-render-sns-init.sh`: validates local logo files and renders `sns_init.local.yaml` from `local-vars.toml`.
- `scripts/02-record-canister-ids.sh`: creates the ignored local evidence draft.
- `scripts/03-capture-ledger-evidence.sh`: prints local evidence capture commands from recorded IDs.
- `scripts/04-render-local-wiring.sh`: renders ignored local dry-run wiring from validated evidence.
- `scripts/05-validate-evidence.sh`: validates completed local evidence.
- `scripts/10-bootstrap-official-network.sh`: checks pinned `dfinity/ic` `rs/sns/testing` provenance and local SNS tooling prerequisites.
- `scripts/11-build-local-io-canisters.sh`: verifies every exact-source release artifact and hash used by the rehearsal.
- `scripts/12-deploy-local-dapps.sh`: creates the per-run planned local dapp IDs, installs exact release Wasms Paused, and adds NNS Root through the maintained SNS CLI.
- `scripts/12-provision-local-nns-readiness.sh`: claims and shapes the permanent
  local NNS neuron, records a deterministic local pooled-parent memo/followee,
  and leaves the pooled parent absent for lazy creation from Stream liquid
  claim backing.
- `scripts/13-propose-and-finalize-sns.sh`: publishes the reviewed same-source candidate Governance/Root bundle through executed local NNS Governance proposals into SNS-W, verifies the exact compressed hashes, submits CreateServiceNervousSystem, and completes the swap.
- `scripts/14-discover-sns-canisters.sh`: discovers the real SNS canonically and rejects any mismatch with the per-run plan.
- `scripts/15-exercise-ledger.sh`: submits signed treasury proposals, records duplicate/negative ledger behavior, funds liquid ICP, and runs the production ICRC-2 redemption after activation.
- `scripts/16-exercise-index-and-archives.sh`: captures index synchronization, exact Account histories and canonical ledger/Root archive discovery.
- `scripts/17-exercise-governance-and-controllers.sh`: records controller and hash-changing upgrade evidence, registers lifecycle functions, and requires both managers to activate through signed SNS Governance proposals.
- `scripts/17-observe-one-day-reward.sh`: advances only the attached local PocketIC instance by exactly one day and records the proposal-bearing production reward observation and permissionless keeper result.
- `scripts/18-exercise-account-semantic-protocol.sh`: serially executes the
  exact proposal-143660 boundary and controlled current-IO account-semantic,
  carry-forward, liquidity and irreversible-effect recovery cases.
- `scripts/18-package-evidence.sh`: packages a new immutable layered evidence
  directory only after every required official and controlled checkpoint,
  self-verifies its checksum inventory, validates the candidate package, and
  updates the canonical selector only after candidate validation succeeds.
- `scripts/19-cleanup-official-network.sh`: scoped cleanup reminder for local-only processes.

`canister-ids.local.toml` is the operator-filled local evidence file and should not be treated as production config.

`generated/local-production-wiring.toml` is local helper output only:

- Human-readable local evidence-derived wiring.
- Not accepted by production wiring validators.
- Do not use as install args.

All operator scripts require:

```bash
IO_LOCAL_SNS_REHEARSAL_ACK=local-only
```

They reject mainnet-like arguments, protected IO asset IDs, and `--network ic`/`-n ic` use. The scripts are optional/manual and not required CI.

## Official Local Flow

Follow the current official ICP/DFINITY SNS testing documentation as the source of truth for local NNS plus SNS-W setup. The historical standalone `dfinity/sns-testing` repository is deprecated; if the official docs reference successor tooling or a new repository/location, use that current official location.

The package provides a renderable local `sns_init` candidate, local evidence capture helpers, no-network validators, and an operator runbook. The maintained source flow has proved local SNS-W deployment, swap finalization, reserve funding, ledger/index behavior and controller handoff. Phases 12–17 encode the launch, monetary, index/archive, controller, upgrade-attempt and authenticated lifecycle paths as restartable commands. Their existence is not evidence that a run passed: a completed package must not be emitted until the same-source candidate run supplies every canonical observation.

Manual sequence:

1. Run `IO_LOCAL_SNS_REHEARSAL_ACK=local-only deploy/local-sns-rehearsal/runbook.sh check`.
2. Copy `local-vars.example.toml` and `runtime.local.example.toml` to their ignored `.local.toml` forms. Fill only fresh per-run local values; never reuse prior ephemeral IDs.
3. Run `runbook.sh render-sns-init` to produce ignored `sns_init.local.yaml`.
4. Run `runbook.sh bootstrap-official-network` against an isolated pinned clean `dfinity/ic` checkout and loopback endpoint.
5. Write ignored `install-args.local/io_stream_manager.did` and `install-args.local/io_nns_neuron_manager.did` for the reviewed local Accounts. The thin lifecycle adapter reads the uniquely owned `sns-testing-init` `topology.json`, rewrites only its isolated input copies with that topology's NNS/SNS allocation IDs, and derives the canonical Governance-owned treasury distribution subaccount for nonce `0`. Phase 12 re-derives and renders that one exact excluded Account plus the reviewed bundle's compressed/source Governance hash before installation. Manual runs use the same helper and fail if the excluded assignment is missing or duplicated. Run `runbook.sh build-local-io-canisters` to verify the exact release provenance and hashes.
6. Set `IO_LOCAL_SNS_BUNDLE_DIR` to the reviewed same-source Governance/Root resolver output, then run the restartable `deploy-local-dapps`, `propose-and-finalize-sns`, and `discover-sns-canisters` phases.
7. Run `exercise-ledger`, `exercise-index-and-archives`, `exercise-governance-and-controllers`, and `observe-one-day-reward` after their canonical prerequisites exist. The governance phase submits the current exact raw historian Wasm inline through the signed SNS Governance proposal and Root execution path. The inline payload avoids only the unavailable chunk-store upload path and does not bypass Governance. The strict pre-launch schema is exercised through a same-release upgrade: before and after module hashes must equal the release-manifest raw hash, and the post-upgrade public status must confirm that the typed observation configuration was applied.
8. Run `runbook.sh record-ids` and record the canonically discovered IDs in ignored `canister-ids.local.toml`.
9. Run `runbook.sh capture-evidence` and the command templates in `commands.local.example.md`.
10. Observe the treasury-transfer fee burn and capture the canonical activation baseline after the real SNS-governance reserve-funding proposal.
11. Verify fee disposition, total supply deltas, reserve balance, bad-fee, insufficient-funds, duplicate, and account-history behavior.
12. Verify SNS governance/root/swap availability and dapp controller state.
13. Test an SNS-governance-controlled dapp upgrade proposal and lifecycle proposal without direct management-canister substitution.
14. Run `runbook.sh validate` and `cargo run -p xtask -- validate_local_sns_ledger`.
15. Run `runbook.sh exercise-account-semantic-protocol` with the exact pinned
    PocketIC, proposal-143660 Governance artifact, real SNS bundle and current
    release Wasms.
16. Run `runbook.sh package-evidence` to create a new sanitized immutable
    package. Packaging refuses missing checkpoints and never edits an earlier
    dated package.

After advancing PocketIC beyond signed-ingress time, use the repository observer instead of weakening ingress validation:

```bash
IO_POCKET_IC_SERVER_URL=http://127.0.0.1:8888 \
IO_POCKET_IC_INSTANCE_ID=0 \
IO_LOCAL_SNS_GOVERNANCE_ID=<local-governance> \
IO_LOCAL_STREAM_MANAGER_ID=<local-stream> \
cargo run -p e2e-real-canisters --bin observe_existing_reward
```

The observer attaches to the existing local instance, performs canonical anonymous queries, and optionally calls the permissionless production `resume_reward_work` when `IO_LOCAL_REWARD_RESUME=1`; it has no debug or state-mutation bypass.

## Repository Validators

These validators do not call canisters and do not require `dfx`:

```bash
cargo run -p xtask -- validate_local_sns_rehearsal
cargo run -p xtask -- validate_local_sns_ledger
cargo run -p xtask -- validate_local_sns_scripts
```

`validate_local_sns_rehearsal` checks the package structure and local-only guardrails.

`validate_local_sns_ledger` reports `corrected pooled-claim-backing rehearsal evidence missing` until a fresh authorized rehearsal produces the ignored root evidence file. The incomplete example uses the pooled architecture and cannot validate as completed evidence. Old `production-redemption-v1`, seeded-principal, 252,460,800-second reward-backing evidence remains immutable only under dated `evidence/` history and is not active readiness authority.

`validate_local_sns_scripts` copies the operator scripts to a temp directory, writes fixture local variables and completed local evidence, runs the no-network executable paths, and checks positive and negative guardrails. It does not call canisters and does not require the dfx SNS extension.

`validate_local_sns_committed_evidence` validates historical immutable packages and requires `evidence/current-canonical.toml` to select exactly one completed current canonical-economics package for launch readiness. The selector has a closed versioned schema and binds the selected leaf directory, release source, artifact-recording commit, release manifest, package manifest and package checksum inventory. It rejects missing or extra selectors, unexpected fields and entries, non-leaf paths, traversal, and symlinks. The historical completed shape contains
`manifest.toml`, toolchain/SNS/ledger/governance/controller/archive evidence,
the local init, sanitized commands, and `SHA256SUMS`. A completed monitoring
package additionally contains `release-evidence.toml` and the direct PocketIC
`historian-dashboard.log`. The validator requires every recorded tool version
to have a matching exact SHA-256 and byte-binds each monitoring package to the
release manifest stored at its own artifact-recording commit. Only the explicitly
selected package is also required to match the checked-in release manifest and
its current source/artifact lineage.
The current canonical shape additionally preserves exact Stream/NNS/historian Candid inputs, the typed Account map, checked redemption economics and the treasury Account history. It independently calculates excluded total, redeemable supply, gross and net, verifies ledger balance identities and requires the historian rate to agree. It also requires canonical historian supply/reserve/liquid/rate,
module/controller, lifecycle, Governance, index and archive observations to be
fresh and complete. No completed package may contain blocker/placeholder text.

The account-semantic shape adds a fixed Account map, source-built official SNS
binary hashes, exact NNS boundary identity, phase inventory, structured
scenario results, controlled PocketIC log and explicit proof-layer map. It
requires distinct fixed TwoWeek and TwoYear maturity Accounts, no maturity
Mint-provenance surface, Jupiter/TwoWeek paired settlement, TwoYear
no-issuance treatment, `B = L + P + U + T`, 100/20/50/70 carry-forward,
cross-Account isolation, global-rate redemption and exact replay/recovery.
Official SNS observations, exact NNS Governance mechanics and controlled IO
orchestration remain separate claims in the package rather than being
presented as one environment.

Both package forms reject unexpected or uncovered files, duplicate checksum entries, path traversal, symlinks, non-regular files, secret/private-key markers, and mainnet endpoint or network arguments.

Package lineage and disposition are recorded in
[`docs/operations/local-sns-evidence-disposition.md`](../../docs/operations/local-sns-evidence-disposition.md).
A historical package is immutable evidence of what happened in its own run and
remains bound to that run's recorded source/artifact pair. It must never be
edited or rebound when the release changes. Having the complete canonical proof
shape and being selected for current-release comparison are separate facts;
`evidence/current-canonical.toml` explicitly selects one immutable package and
binds its release/package/checksum manifests. The selector itself is the source
of truth for the current identity. Candidate and local fixture evidence is not
mainnet evidence. Restart-safe external logs remain supporting diagnostics
rather than the committed-evidence contract.

## Issuance Model Under Test

IO issuance is modelled as reserve transfer, not arbitrary minting:

- reserve-to-user transfer for issuance;
- user-to-redemption transfer for the incoming redemption IO;
- redemption-to-reserve transfer for the protocol IO return;
- observed fee disposition and total-supply deltas for each transfer.

Under standard 10,000 e8s fee-burn evidence with no hidden top-up, the rehearsal amounts are `100_000_000`, `99_990_000`, and `99_980_000`. The preceding SNS-governance reserve-funding transfer is a separate detailed record. For genesis supply `S`, reserve funding fee `f₀`, and later observed transfer fees `fᵢ`, final supply is `S - f₀ - sum(fᵢ)`.

The protocol reserve account/subaccount is funded after finalization and before activation by an SNS-governance treasury-transfer proposal. For desired reserve `R`, remaining treasury `T`, and transfer fee `f`, genesis treasury must contain at least `R + T + f`. Evidence must prove treasury decrease `R + f`, reserve increase `R`, and supply decrease `f`. The first reserve-to-user supply and reserve pre-balances must equal the reserve-funding post-balances.

The reserve owner is the local `io_stream_manager` canister and its exact configured non-default subaccount distinguishes the reserve. The redemption Account may have that same owner but must use a distinct exact subaccount. Canister-role IDs remain mutually distinct; Accounts are validated separately from role uniqueness.

Proof records use closed `ProofSource` values (`SnsLedgerBlock`, `SnsIndexAccountHistory`, `SnsLedgerArchive`) and closed `ProofMethod` values (`Icrc3GetBlocks`, `IcrcIndexGetAccountTransactions`, `ArchiveGetBlocks`). Source principals must match the recorded ledger/index/archive role. Archive proofs must be within a ledger/root-discovered archive range. Duplicate replay evidence is recorded separately and must point to the exact original successful block; each ordinary successful transfer does not need its own duplicate replay.

## Done Criteria

The local SNS rehearsal is complete only when:

- official local SNS tooling was run locally;
- local SNS root/governance/ledger/index/swap IDs were recorded;
- local SNS ledger fee disposition, total-supply deltas, and reserve balance were observed;
- reserve-to-user, user-to-redemption, and redemption-to-reserve transfers were observed separately;
- bad fee, insufficient funds, and duplicate behavior were observed;
- duplicate block was verified;
- index account history was observed;
- SNS governance/root/swap availability was observed;
- dapp controller state was checked;
- `cargo run -p xtask -- validate_local_sns_ledger` passes against the filled local evidence file.
- the account-semantic driver has passed all Layer B/C cases against the exact
  recorded release;
- the new package validates before and after its exact selector binding;
- no maturity Mint block or provenance endpoint was used.

This still does not prove mainnet SNS launch readiness, final tokenomics, final SNS config, mainnet testflight, audit readiness, production adapter activation, or that IO is live.
