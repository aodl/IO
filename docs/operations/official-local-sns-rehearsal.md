# Official Local SNS Rehearsal

This runbook describes how to prove IO assumptions against a real SNS-created ledger stack in a local environment. It is optional/manual, local-only, and outside required CI because the maintained local flow depends on heavyweight source-built tooling.

For a lighter local real-framework path that does not use official SNS launch tooling, use `tests/e2e_real_canisters` with pinned local SNS ledger/index Wasms. That path installs the real framework Wasms directly in PocketIC and records evidence with `deploy/local-sns-rehearsal/real-canister-e2e-evidence.example.toml`; it is not a substitute for an official SNS launch rehearsal because it does not prove SNS-W, swap, root/governance launch wiring, or final SNS tokenomics.

It must not use `--network ic`, must not call mainnet, must not touch NNS
Manager execution canister `oae4c-3iaaa-aaaar-qb5qq-cai`, and must not touch
the two-year protected NNS neuron `10292412127977304661`.

## Package

- `deploy/local-sns-rehearsal/README.md`
- `deploy/local-sns-rehearsal/sns_init.local.template.yaml`
- `deploy/local-sns-rehearsal/local-vars.example.toml`
- `deploy/local-sns-rehearsal/canister-ids.local.example.toml`
- `deploy/local-sns-rehearsal/commands.local.example.md`
- `deploy/local-sns-rehearsal/runbook.sh`
- `deploy/local-sns-rehearsal/scripts/00-check-prereqs.sh`
- `deploy/local-sns-rehearsal/scripts/01-render-sns-init.sh`
- `deploy/local-sns-rehearsal/scripts/02-record-canister-ids.sh`
- `deploy/local-sns-rehearsal/scripts/03-capture-ledger-evidence.sh`
- `deploy/local-sns-rehearsal/scripts/04-render-local-wiring.sh`
- `deploy/local-sns-rehearsal/scripts/05-validate-evidence.sh`

The rendered local `sns_init.local.yaml` is not final tokenomics and is not a mainnet SNS proposal. It exists only to create a real local SNS ledger/index/governance/root stack for integration testing.

IO_TEST remains a non-canonical staging ledger label and must not be confused with the real SNS-created local IO ledger created by this rehearsal.

## Current SNS Tooling

Follow the current official ICP/DFINITY SNS testing documentation as the source of truth. The historical standalone `dfinity/sns-testing` repository is deprecated; if the official docs reference successor tooling or a new repository/location, use that current official location.

Local SNS rehearsal uses Bazel, `. scripts/env.sh`, `sns-testing-init`, `sns-testing`, the source-built `sns` CLI, and Quill where governance proposals need it. Required repository workflows must not depend on the dfx SNS extension.

The user-local Bazel launcher is Bazelisk `v1.29.0`, downloaded from the
published GitHub release artifact `bazelisk-linux-amd64`. Its published and
observed SHA-256 is
`5a408715e932c0250d28bd84555f12edbf70117de42f9181691c736eacc4a992`.
It is installed as `/home/codexdev/.local/bin/bazelisk` with a local `bazel`
symlink. No system package or elevated privilege is used. The maintained source
flow was reproduced from the clean sibling checkout at
`4320fdf2e613844eabae1927b1a23b98da3a7bc6`: NNS bootstrap, SNS-W candidate
publication, CreateServiceNervousSystem, swap participation/finalization,
canister discovery, Governance treasury funding, ledger/index evidence and
controller handoff all succeeded locally. The immutable completed sanitized
historical package is `deploy/local-sns-rehearsal/evidence/2026-08-11-4320fdf/`. Its mechanics observations remain valid; its redemption-rate/excluded-Account evidence is superseded as described in `local-sns-evidence-disposition.md`.

The committed package includes a renderable local `sns_init` candidate, per-run runtime inputs, evidence capture helpers, no-network validators, and restartable phases 12–17. Those phases verify exact IO release provenance, install Paused dapps, provision canonical staging fee floats and two source-shaped local NNS neurons, publish a reviewed Governance/Root bundle through executed local NNS Governance proposals into SNS-W, verify its exact compressed hashes, finalize and discover the SNS, submit real treasury and lifecycle proposals, exercise production redemption, and capture index/archive/controller evidence. The prior one-component candidate-Governance/official-Root `unit_variant` incompatibility is historical; same-source candidate Governance/Root compatibility is proved. If the maintained chunk-store CLI route fails before execution, phase 17 submits the exact release Wasm inline through a signed SNS Governance proposal and Root. The inline payload avoids only the unavailable upload store; it does not bypass Governance. The strict pre-launch Historian schema is exercised through a same-release upgrade with typed configuration: both module observations must equal the exact release-manifest raw hash, and the post-upgrade public status must report the configuration as active. The phases fail closed and persist checkpoints. The completed historical package records one clean run; the thin lifecycle source profile is separate runner coverage and does not retroactively qualify or invalidate that package.

## Manual Flow

1. Prepare a clean local SNS testing environment using the current official ICP/DFINITY SNS testing documentation.
2. Run `IO_LOCAL_SNS_REHEARSAL_ACK=local-only deploy/local-sns-rehearsal/runbook.sh check`.
3. Copy `deploy/local-sns-rehearsal/local-vars.example.toml` to ignored `local-vars.toml` and fill only local principals.
4. Run `runbook.sh render-sns-init` to write ignored `sns_init.local.yaml`.
5. Deploy IO app canisters locally.
6. Add local NNS root as co-controller where the official SNS launch tooling requires it.
7. Validate `deploy/local-sns-rehearsal/sns_init.local.yaml` with local SNS tooling.
8. Submit the local SNS proposal through the local SNS testing flow.
9. Let SNS-W deploy local SNS canisters.
10. Run `runbook.sh record-ids` during a new rehearsal and record root, governance, ledger, index, swap, and archive observations. The root `deploy/local-sns-rehearsal/canister-ids.local.toml` remains ignored run-local evidence; corrected pooled-claim-backing evidence is missing until a fresh authorized package is reviewed.
11. Run `runbook.sh capture-evidence` and the command templates to observe ledger/index/governance/root behavior.
12. Run no-network repository validation:

```bash
cargo run -p xtask -- validate_local_sns_rehearsal
cargo run -p xtask -- validate_local_sns_ledger
```

The second command checks only the recorded local evidence file. It does not call canisters.

`validate_local_sns_ledger` reports the corrected pooled-claim-backing evidence
as missing until a fresh authorized run fills the ignored root file. A later
reviewed package must create a new immutable evidence directory; it must not
overwrite the 2026-08-11 package or relabel it as evidence for later source
commits.

## Ledger Assumptions to Prove Manually

Run local canister calls against the local SNS ledger/index principals recorded in `canister-ids.local.toml`:

- `icrc1_fee` returns the fee configured in `sns_init.local.yaml`.
- `icrc1_total_supply` matches the local total supply configuration.
- `icrc1_balance_of` for the protocol reserve account is non-zero and sufficient for rehearsal issuance.
- `icrc1_transfer` supports reserve-to-user transfers using IO's account/subaccount encoding.
- `icrc1_transfer` returns `BadFee` for an intentionally wrong fee.
- `icrc1_transfer` returns `InsufficientFunds` for an unfunded source subaccount.
- Repeating a transfer with the same created-at time/memo produces duplicate behavior that IO can prove against the duplicate block.
- The SNS index `get_account_transactions` endpoint returns the expected reserve/user account history in a stable order for historian observation evidence; it is not monetary command authority.
- Index lag or archive-required behavior is either observed and recorded or explicitly marked as future work in the local evidence file.
- SNS governance exposes nervous-system parameters.
- SNS root is available and can report controlled dapp canisters or support the corresponding official local query.
- A governance-controlled dapp upgrade proposal is tested if supported by the local tooling.

## Issuance Model

IO issuance is resolved conservatively as a transfer from a protocol reserve account/subaccount funded after SNS finalization and before activation by an executed SNS-governance treasury-transfer proposal.

Redemption uses an authenticated ICRC-2 pull directly into the protocol reserve. IO must not assume arbitrary post-launch minting unless final SNS ledger configuration and governance policy explicitly support it and a later audited milestone changes this model.

The local rehearsal must prove:

- the protocol reserve account exists on the SNS ledger;
- the reserve balance is funded by the recorded post-finalization SNS-governance treasury transfer;
- the standalone ledger fixture can execute a reserve-to-user transfer;
- the standalone ledger fixture can execute a direct user-to-reserve transfer with the configured fee;
- fee disposition and total-supply deltas are recorded for each transfer.

## What Remains Unproven

The immutable `2026-08-12-4320fdf-canonical-economics` package proves same-source candidate Governance/Root compatibility for its recorded historical release pair,
authentic inline SNS-controlled hash-changing historian upgrade,
Governance-authorized stream and NNS-manager activation using the source-shaped
local NNS fixture, production ICRC-2 redemption, canonical ledger/index
histories, and one exact proposal-bearing daily reward event. The separate
`2026-08-12-4320fdf-monitoring` package preserves historical mechanics and historian connectivity. The corrected historical package uses the derived Governance treasury distribution Account in both Stream and historian configuration and passes the independent checked-arithmetic evidence validator.

The immutable 2026-08-12 and 2026-08-14 packages remain bound to their recorded
releases and were not rebound. This includes the authority, final-readme, and
final-validator packages produced on the diverged historical `misc` lineage.
The explicit `current-canonical.toml` selector must name a newly generated
package and bind its release manifest, package manifest, checksum inventory,
source-finalization commit, and immediate artifact-recording child to the exact
master-descended release. It names
`2026-08-14-master-descended-4320fdf-canonical-economics`, whose fresh isolated
run closes the current local rehearsal item without changing any historical
package.

Completed local proof does not prove official SNS reward-share release adoption,
final SNS configuration/tokenomics/controllers, external audit, or mainnet
testflight and activation.

IO protocol remains not live. The canonical SNS IO ledger remains not launched on mainnet.

## Completion Checklist

The rehearsal is complete only when official local SNS tooling was run locally; local SNS root/governance/ledger/index/swap IDs were recorded; local SNS ledger fee disposition, total-supply deltas, and reserve balance were observed; reserve-to-user and direct user-to-reserve transfers were observed separately; bad fee, insufficient funds, duplicate behavior, duplicate block proof, and index account history were observed; SNS governance/root/swap availability and dapp controller state were checked; and `cargo run -p xtask -- validate_local_sns_ledger` passes against the filled evidence file.

Passing this local evidence gate still does not prove mainnet SNS launch readiness, final tokenomics, final SNS config, mainnet testflight, audit readiness, or production adapter activation.
