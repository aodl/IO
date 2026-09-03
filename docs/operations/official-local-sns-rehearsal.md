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
- `deploy/local-sns-rehearsal/scripts/10-bootstrap-official-network.sh`
- `deploy/local-sns-rehearsal/scripts/11-build-local-io-canisters.sh`
- `deploy/local-sns-rehearsal/scripts/12-deploy-local-dapps.sh`
- `deploy/local-sns-rehearsal/scripts/12-provision-local-nns-readiness.sh`
- `deploy/local-sns-rehearsal/scripts/13-propose-and-finalize-sns.sh`
- `deploy/local-sns-rehearsal/scripts/14-discover-sns-canisters.sh`
- `deploy/local-sns-rehearsal/scripts/15-exercise-ledger.sh`
- `deploy/local-sns-rehearsal/scripts/16-exercise-index-and-archives.sh`
- `deploy/local-sns-rehearsal/scripts/17-exercise-governance-and-controllers.sh`
- `deploy/local-sns-rehearsal/scripts/17-observe-one-day-reward.sh`
- `deploy/local-sns-rehearsal/scripts/18-exercise-account-semantic-protocol.sh`
- `deploy/local-sns-rehearsal/scripts/18-package-evidence.sh`
- `deploy/local-sns-rehearsal/scripts/19-cleanup-official-network.sh`

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
flow was reproduced from an isolated clean checkout at
`4320fdf2e613844eabae1927b1a23b98da3a7bc6`: NNS bootstrap, SNS-W candidate
publication, CreateServiceNervousSystem, swap participation/finalization,
canister discovery, Governance treasury funding, ledger/index evidence and
controller handoff all succeeded locally. The current immutable sanitized
package is
`deploy/local-sns-rehearsal/evidence/2026-09-03-270dcf3-anchored-dynamic`.
It is bound to source `270dcf3dc71fc8e7b63c3177b0e3f58fc9246b35`,
immediate artifact child `dc548f555a808f59e6a6c69759cc41fbb7f1f54d`, and
canonical evidence commit `608baa58425389827acf3a5c051d71160cd829f7`.
Earlier packages remain historical as described in
`local-sns-evidence-disposition.md`.

The maintained package includes a renderable local `sns_init` candidate, per-run runtime inputs, evidence capture helpers, no-network validators, and restartable phases 10–19. Those phases verify exact IO release provenance, install Paused dapps, provision canonical staging fee floats and source-shaped local NNS neurons, publish a reviewed Governance/Root bundle through executed local NNS Governance proposals into SNS-W, verify exact compressed hashes, finalize and discover the SNS, submit real treasury and lifecycle proposals, exercise production redemption, capture index/archive/controller evidence, observe one reward event, run the layered account-semantic protocol cases and package evidence fail closed. The prior one-component candidate-Governance/official-Root `unit_variant` incompatibility is historical; same-source candidate Governance/Root compatibility is proved. If the maintained chunk-store CLI route fails before execution, phase 17 submits the exact release Wasm inline through a signed SNS Governance proposal and Root. The inline payload avoids only the unavailable upload store; it does not bypass Governance. Same-release manager upgrades must reopen Paused and resume exact retained operations after authenticated readiness restoration. The current package records one coherent fresh run and restart-safe phase recovery; the thin lifecycle source profile is separate runner coverage and does not retroactively qualify or invalidate any package.

The maintained phase order intentionally activates Stream while SNS Governance
still exposes its canonical dummy genesis reward event: round zero, nonzero end
timestamp, zero span, no proposals, and zero distributed reward. Readiness
freezes that event as a zero-credit baseline; the immediate observation is
structural and may record reconciliation marker zero. Production redemption is
then exercised before the first real distribution. If preparation observes the
legitimate genesis Pool as Busy, phase 15 boundedly continues that same
structural generation through Stream `resume`, `resume_reward_backing`, and NNS
`resume` before retrying; it never suppresses the observation or creates a new
generation merely for redemption. Only after that proof does the reward phase
raise its locally controlled proposer from the SNS fixture's initial delay to
the exact product eligibility duration of 1,296,060 seconds. The rendered local
SNS config sets the Governance maximum dissolve-delay duration to that same
value; retaining a 14-day maximum would silently cap the adjustment below the
product eligibility boundary. Round one records
prospective eligibility without retroactive credit; the phase then submits a
new proposal and requires round two to add exactly one eligible reward credit.
No rehearsal phase fabricates a reward round or delays activation to avoid the
genesis event.

That structural observation also runs ordinary pooled reconciliation. It may
commit the next structural event marker before reward credit is due; the reward
observer boundedly resolves that exact reconciliation generation before testing
the reward deadline, so an unrelated `Busy` result cannot stand in for the
deadline proof. A remaining same-baseline structural pass is accepted only when
it is exactly zero-credit and leaves the processed-event count unchanged. The
observer requires either that exact zero-credit structural result or `Pending`
while the clock is still before the canonical event-end `+300s` margin; an
unresolved reconciliation generation is not required to manufacture a
`Pending` reply merely to prove the reward boundary. It then recomputes and
advances the exact remaining simulated margin plus one second so the one-shot
timer can wake. The proof accepts either the timer or the permissionless keeper
as the winner. The first post-margin invocation may still finish an exact
zero-credit structural continuation; the observer boundedly settles that same
generation and continues until it proves the one-time reward checkpoint.
Structural work does not consume the reward event or increment reward credit.
If it legitimately occupies the NNS Manager's single immediate slot, the
maintained order does not suppress, cancel, or preempt Pool. The two-year
maturity validator must reject proposal submission while Pool is visible. After bounded
production `resume_reward_backing`, Stream `resume`, NNS `resume`, and any exact
transfer proof settle the canonical target, a fresh proposal is submitted and
must be recorded executed only when exact TwoYear work is actually durable.
Execution repeats the same acceptance checks to cover a validator/execution
race. The reviewed SNS target boundary regards any normal reply as success, so
an unaccepted IO generic action must reject at transport level rather than rely
on a Candid `Err` payload.

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
10. For the next package, run `runbook.sh record-ids` and record root,
    governance, ledger, index, swap, and archive observations. The root
    `deploy/local-sns-rehearsal/canister-ids.local.toml` remains ignored
    run-local input; it does not replace the selected immutable package.
11. Run `runbook.sh capture-evidence`, the maintained lifecycle phases, and
    `runbook.sh exercise-account-semantic-protocol` to observe every mandatory
    Layer A/B/C conclusion.
12. Run `runbook.sh package-evidence` only after every required phase checkpoint
    passes, then run no-network repository validation:

```bash
cargo run -p xtask -- validate_local_sns_rehearsal
cargo run -p xtask -- validate_local_sns_ledger
```

The second command checks selector-bound recorded evidence. It does not call
canisters. It passes for the current package. A future reviewed run must create
a new immutable evidence directory and validate it before selector review; it
must not overwrite or relabel any existing package.

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

Redemption uses an exact prepared ICRC-1 push into the protocol reserve. The
caller sends the prepared amount and memo, then supplies the block for exact
proof; no allowance or spender authority exists. A proved push creates a
durable ICP payout obligation. IO must not assume arbitrary post-launch minting
unless final SNS ledger configuration and governance policy explicitly support
it and a later audited milestone changes this model.

The local rehearsal must prove:

- the protocol reserve account exists on the SNS ledger;
- the reserve balance is funded by the recorded post-finalization SNS-governance treasury transfer;
- the standalone ledger fixture can execute a reserve-to-user transfer;
- the standalone ledger fixture can execute a direct user-to-reserve transfer with the configured fee;
- fee disposition and total-supply deltas are recorded for each transfer.

## What Remains Unproven

The schema-v2 `2026-09-03-270dcf3-anchored-dynamic` package closes the current
local rehearsal item. Layer A proves source-built official SNS launch/wiring;
Layer B proves the exact proposal-143660 NNS boundary; Layer C proves current IO
anchored economics, structural/reward scheduling, prepared-push redemption and
controlled recovery. All earlier packages remain historical and were not
rebound.

Completed local proof does not prove official SNS reward-share release adoption.
Source-built revision `4320fdf2e613844eabae1927b1a23b98da3a7bc6`
contains and locally proves the capability, while the separately reviewed
official lock remains `b904c9dd1bdef8841bd12f03efbc71180a015e25`.
Final SNS configuration/tokenomics/controllers, external audit, protected
mainnet review, mainnet testflight and activation also remain unproved.

IO protocol remains not live. The canonical SNS IO ledger remains not launched on mainnet.

## Completion Checklist

The current release satisfies this completion checklist. A future replacement
package is complete only when official local SNS tooling was run locally; local
SNS root/governance/ledger/index/swap IDs were recorded; local SNS ledger fee
disposition, total-supply deltas, reserve balance, transfer/error/replay and
index observations were captured; Governance/Root/controller state was checked;
all exact NNS and account-semantic phases passed; and both candidate-package and
selected committed-evidence validation pass.

Passing this local evidence gate still does not prove mainnet SNS launch readiness, final tokenomics, final SNS config, mainnet testflight, audit readiness, or production adapter activation.
