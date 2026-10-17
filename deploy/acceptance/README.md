# IO acceptance predeployment package

Status: **DRAFT**. This directory is an offline, source-backed package. It does
not deploy, query mainnet, hold identities, transfer value, or issue IO.

## Commands

```sh
cargo run -p xtask -- acceptance_preflight draft
cargo run -p xtask -- acceptance_preframework
POCKET_IC_BIN=/home/codexdev/.local/bin/pocket-ic-server \
  cargo run -p xtask -- acceptance_preframework_pocketic
cargo run -p xtask -- acceptance_render acceptance
cargo run -p xtask -- acceptance_render production
cargo run -p xtask -- acceptance_render accounts
cargo run -p xtask -- acceptance_capitalization
cargo run -p xtask -- acceptance_checkpoint A EVIDENCE_DIRECTORY
cargo run -p xtask -- acceptance_checkpoint B EVIDENCE_DIRECTORY
cargo run -p xtask -- acceptance_constructors EVIDENCE_DIRECTORY
cargo run -p xtask -- acceptance_checkpoint C EVIDENCE_DIRECTORY
cargo run -p xtask -- acceptance_sale_real
```

Draft rendering permits explicit `TBD_` values. The monolithic `sealed` mode
fails with instructions to use A/B/C. A requires valid source/artifacts,
identities, generated SNS payloads and baseline installation authority; it does
not require post-sale observations. B requires successful real finalization and
reserve migration before computing C0/B0/Delta K. C requires capitalization,
refresh/anchor evidence and typed install constructors. Local fixtures use
explicit test metadata and never approve deployment parameters.

`acceptance_capitalization` and `acceptance_preframework` remain labelled
arithmetic/component checks over the draft package. Real launch evidence comes
from `acceptance_sale_real` and the explicit checkpoint tests. See
[the runbook](runbook.md) for local executable inputs and the separate
production topology requirements, and [implementation status](implementation-status.md)
for completed validation.

## Frozen topology and source

`acceptance-manifest.json` is the positive allowlist. It contains exactly nine
acceptance roles, the canonical ICP Ledger/Index and NNS Governance system
canisters, and permanent neuron `4_021_054_791_166_218`, controlled by the
acceptance NNS Manager. Known production IO principals and production neuron
`10_292_412_127_977_304_661` are secondary contamination checks.

The reviewed source authority is [sns-source-baseline.json](sns-source-baseline.json).
The parser builder reads it and validators compile it into their approval
boundary. Evidence directories and required-input hashes cannot select another
revision. Its pin is for local rehearsal; final sale-source approval is unresolved.
Governance, Root, Ledger, Index, Swap and Archive artifacts and DIDs must come
from one clean approved revision with the required reward-participation ABI.

Checkpoint A also requires an authorized full-neuron baseline response. An
operator who controls the empty NNS Manager is not thereby authorized to query
its neuron. An existing controller/hotkey or authorized ManageNeuron-followee
principal must provide both typed full-neuron replies. Unknown read authority
blocks the checkpoint; the runbook never adds permissions automatically.

Build acceptance browser assets and Rust Wasm together using
`bash tools/acceptance/build-frontend ABSOLUTE_NEW_OUTPUT_DIRECTORY`, then set
`IO_ACCEPTANCE_FRONTEND_BUILD_DIR` for local tests. The isolated output preserves
tracked frontend assets. Checkpoint A binds executable configuration, manifest,
HTML, bundle bytes and the actual Wasm; PocketIC checks served HTML/JavaScript
and the private manifest's 404. See the runbook for exact browser inputs.

## Genesis distribution and sale price

The total supply remains exactly `100_000_000_000_000_000` e8s. The operator
governance neuron is `3_000_000_000` e8s. The Jupiter governance-only neuron is
`10_000_000_000_000_000` e8s and has the environment-specific Jupiter SNS
Rewards canister as controller. Let the sealed small-sale allocation be `S`
e8s; the reserve-transport stake is derived, never independently entered:

```text
3_000_000_000 + 10_000_000_000_000_000
+ (100_000_000_000_000_000 - 3_000_000_000 - 10_000_000_000_000_000 - S)
+ S
= 100_000_000_000_000_000 e8s
```

The fixed memos are `IOGOV001` (`5282519294002868273`), `IOJUP001`
(`5282522618206892081`) and `IORES001` (`5282531345630769201`), all distinct
and outside the candidate sale-neuron memo range `1_000_000..=10_000_000`.
With future-swap allocation zero, `initial_swap_amount == total_swap_amount`,
so the reviewed SNS formula gives all three developer neurons the intended
100% voting-power multiplier.

The sale uses a gross reference price of exactly 1 ICP per IO. The sealed config
sets minimum and maximum total direct participation equal to `S`, with Neurons'
Fund disabled. Final price is nevertheless accepted only after canonical
observation proves actual accepted direct ICP equals `S`; reaching a cap or
seeing an intermediate lifecycle state is not finalization proof.

Sale IO is reconciled as:

```text
distributed sale-neuron stake + burned IO distribution fees + Swap residual = S
```

Accepted ICP is reconciled as:

```text
SNS Governance default-Account ICP proceeds + burned ICP sweep fees
= actual accepted direct ICP
```

The proceeds remain in the SNS ICP treasury. They are neither `B` nor permanent
capital, and there is no treasury-transfer proposal, automatic sweep to IO,
Jupiter notification, or IO issuance against them.

## Reserve migration and canonical C0

The reserve and Jupiter governance-only staking Accounts remain permanently in
Stream's explicit exclusion list. The operator-governance staking Account and
actual sale-token Accounts are not excluded. The Jupiter controller is distinct
from the configured Jupiter inflow/source identity and Stream's Jupiter IO
recipient. After successful sale finalization and the direct
reserve-neuron disbursement, derive rather than predict claim-bearing supply:

```text
C0 = canonical Ledger total supply
   - canonical Stream reserve balance
   - sum(canonical balances of explicit exclusion Accounts)
```

This incorporates actual sale-neuron distribution fees, reserve-disbursement
fee, rounding, and residual balances. The reserve transfer must still go directly
to the canonical Stream IO reserve Account; it never passes through a human
liquid Account.

Moving 100M IO from the already excluded reserve allocation into the separately
excluded Jupiter neuron changes neither `C0` nor `B0`, active backing/reward
stake, Dynamic target, or launch funding. Exclusion follows this exact staking
Account; it is not controller-wide. If the stake is split, disbursed,
or transferred to a nonexcluded Account, the moved IO becomes unbacked claim
supply. The dedicated neuron therefore remains staked and unavailable for
ordinary monetary operations.

The reviewed SNS source creates developer neurons with the configured global
claimer permissions; the manifest label alone does not make this neuron
voting-only. The future Jupiter implementation must call the standard
`RemoveNeuronPermissions` path from its actual canister principal. Derive the
removal set from the final source enum and actual permissions, not a frozen
list. The positive postcondition is exactly one authorized principal (the
acceptance SNS Rewards canister), with exactly `{Vote}`. In particular,
`ManageVotingPermission` must not survive. `Vote` also permits following; the
mirror must not set followees that bypass finalized Jupiter decisions.
That irreversible hardening and an actual canister-originated IO vote
remain external acceptance blockers. The neuron starts with no followees and
must not acquire default following that bypasses finalized Jupiter decisions.

## Operator-funded launch capitalization

The operator holds `30 IO` and the proposal rejection cost is `10 IO`, with a
`1 IO` minimum neuron stake. The operator can propose but cannot carry adoption
while Jupiter abstains. Real-candidate acceptance must observe assessed fees,
their release on adoption, retention on rejection, and subsequent effective
stake checks; arithmetic alone is not that evidence.

The draft sale is `3 IO / 3 ICP`, with two basket events separated by one day.
At the gross 1:1 price its minimum contribution covers
`2 * (1 IO + 0.0001 IO) = 2.0002 IO`. Parser/pre-execution and real finalization
remain required before approving this candidate. Sale-neuron delay affects
reward/active eligibility, not whether its IO is claim-bearing.

Requested token branding remains `IO`/`IO`. The inspected SNS validator requires
a token name of at least four characters and symbol of at least three. Supply
`FINAL_TOKEN_NAME` and `FINAL_TOKEN_SYMBOL`, or separately review a framework
change; the renderer does not silently rename the product. Local valid test
metadata is not deployable branding.

At the reviewed SNS source, normal actions use the configured initial voting
period and wait-for-quiet increase. IO renders `1 day` and `12 hours`, giving
an initial one-day deadline and at most one additional day of wait-for-quiet.
Critical actions instead impose minima of `5 days` and `2.5 days`, permitting
up to five additional days. Determine each actual action's criticality from its
topic/configuration, and read the proposal's canonical deadline. Jupiter's
canonical voting parameters have not been supplied; cross-SNS deadline
feasibility therefore remains unresolved. Discovery, finalized Jupiter decision,
and the IO vote must fit before the IO deadline without relying on an extension.
A daily owner-index scan cannot establish that bound for a one-day IO action.

After `C0` and pre-funding backing `Bpre` are observed, the operator separately
funds:

```text
B0 target = C0 ICP
Stream transfer = C0 - Bpre ICP
Delta K exact = 2 * C0 / 3 ICP
Delta K transfer = ceil(2 * C0 / 3) whole e8s
Dynamic anchor = 10 ICP independently
```

Rounding `Delta K` upward avoids undercapitalization. The generated plan reports
whether this adds `0`, `1/3`, or `2/3` of one ICP e8 above the exact rational
target. These ICP transfers create no IO.

The permanent top-up is never added blindly. First record the exact cached
principal, ordinary maturity, staked maturity, dissolve configuration and pending
disbursements. Transfer only the calculated `Delta K` to the verified permanent
staking Account, run canonical `ClaimOrRefresh`, and bind
`audited_permanent_principal_e8s` only to the newly observed cached principal.
Existing principal, new `Delta K`, ordinary maturity, staked maturity, the 10 ICP
Dynamic anchor, and SNS ICP treasury proceeds remain distinct quantities.

## Rates and fees

- Gross sale reference price: 1 ICP/IO.
- Launch claim rate after operator funding: `B0/C0 = 1 ICP/IO`.
- Redemption gross at that checkpoint: 1 ICP per redeemed IO.
- Redemption net: gross less the existing ICP payout fee; the caller's IO ledger
  transfer fee remains the existing separate ingress cost.

The Jupiter 60/40 capitalization rule determines operator-funded `B0` and
`Delta K`; it does not define a market price or redemption rate.

For Dynamic reconciliation, the 10 ICP anchor supplies the transfer-fee capacity.
The full `B0` claim credit is retained even though the transfer amount is
`B0 - canonical ICP fee`.

## Artifact inventory

- `acceptance-manifest.json`: frozen allowlist.
- `canonical-sns-economics.json`: shared tokenomics and 1:1 sale rule.
- `acceptance-identity-overlay.json`, `production-identity-overlay.json`:
  permitted environment differences.
- `acceptance-sns-init.draft.yaml`, `production-sns-init.draft.yaml`: rendered
  parameterized standard-sale drafts.
- `required-inputs.draft.json`: predeployment sealing inputs.
- `sale-contract.json`: configured and observed sale invariants.
- `launch-observation.draft.json`: post-finalization and capitalization evidence.
- `genesis-neurons.draft.json`: fixed neurons plus derived reserve formula.
- `acceptance-accounts.draft.json`: all economic Accounts, including the SNS ICP
  treasury Account.
- `stream-exclusions.draft.json`: exact reserve and Jupiter-neuron exclusions.
- `funding.draft.json`: `B0`, `Delta K`, anchor and fee classifications.
- `io-init-configs.draft.json`: IO constructor drafts.
- `controller-topology.draft.json`: controller stages.
- `blockers.json`: concise unresolved inputs.
- `runbook.md`: restart-safe human-only procedure.

The preframework profile uses an explicit numerical fixture to prove IO-side
arithmetic. It is not evidence that a real sale finalized.
