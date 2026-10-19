# Manual IO acceptance deployment runbook

This is a restart-safe operator checklist, not an executable script. Its mainnet
command templates are not run by CI or `xtask`; they require a separately
authorized change window. The local test entry points below use PocketIC. Never source credentials into this repository.

Record observation hashes, proposal/block indexes and checkpoints externally.
Never repeat a value-moving command until canonical state proves it had no effect
or the exact installed protocol deduplication rule permits it.

## Read-only preflight templates

After explicit authorization, record status, subnet, controllers, cycles, module
hash and memory for every frozen canister:

```sh
for canister in \
  h7y7s-kyaaa-aaaas-amxrq-cai \
  hk7o7-lqaaa-aaaas-amxsa-cai \
  hn6il-giaaa-aaaas-amxsq-cai \
  he5dx-qaaaa-aaaas-amxta-cai \
  hd4fd-5yaaa-aaaas-amxtq-cai \
  gjthz-7iaaa-aaaas-amxuq-cai \
  gosbn-sqaaa-aaaas-amxua-cai \
  gaqmf-jaaaa-aaaas-amxva-cai \
  ghrkr-eyaaa-aaaas-amxvq-cai
do
  dfx canister status --network ic "$canister"
done
```

Stop unless all IDs match the positive allowlist and are reserved/empty under
the expected reservation controllers.

Permanent-neuron observation is an explicit predeployment authorization gate.
The method is NNS Governance `get_full_neuron`. Its authorized callers are the
neuron controller, its registered hotkeys, and controllers/hotkeys of a neuron
it follows on the `ManageNeuron` topic. An unrelated operator is rejected.
Controlling the NNS Manager canister does not make an operator that canister's
principal. Permissionless existing-ID `ClaimOrRefresh` grants no read authority.

Before any funding, separately verify that an **existing** operator observation
principal satisfies that permission contract. Prefer an existing authorized
hotkey; this permission also grants voting authority and is not read-only
authority. Do not add a hotkey or change following/controller permissions to
make this checklist pass. The local authorization test uses explicit PocketIC
senders and proves the source contract, not any real acceptance permission.

The following is an inert mainnet template, usable only after separate mainnet
authorization and verification of the selected existing caller:

```sh
dfx --identity VERIFIED_EXISTING_AUTHORIZED_OBSERVER canister call --network ic rrkah-fqaaa-aaaaa-aaaaq-cai \
  get_full_neuron '(4_021_054_791_166_218 : nat64)' --output raw
```

Record neuron ID, controller, staking Account/subaccount, cached stake, ordinary
maturity, staked maturity, dissolve state/delay, auto-stake setting, followees
and pending maturity disbursements. Stop unless the controller is
`gosbn-sqaaa-aaaas-amxua-cai`, delay is exactly 63,115,200 seconds, the neuron is
non-dissolving, auto-stake is disabled, and no maturity disbursement is pending.
Ordinary/staked maturity is neither `B` nor part of the top-up calculation.
Decode the raw response's hexadecimal Candid bytes directly into
`permanent-full-neuron-before.bin`; do not reconstruct the record from JSON or
a public summary. Retain this authentic typed `Result<Neuron, GovernanceError>`
response, and repeat through the same authorized path
for `permanent-full-neuron-after.bin` after refresh. Preserve hotkeys and
followees in both observations. Public `get_neuron_info` proves public stake and
delay fields but does not supply controller, Account, hotkeys, following or
maturity-disbursement evidence; it cannot replace these files.

If no existing authorized observer can provide these replies, stop checkpoint A
before funding or installation. The empty NNS Manager cannot originate the read
at that stage, and its production interface has no full-neuron export. The
smallest alternatives are an already authorized observation principal, or a
separately reviewed sequence using an existing controller-originated read path
if one is available. Neither a new public inspection endpoint nor an automatic
permission change is part of this package. Real acceptance read authority is
unverified.

## Checkpoint sequence

This runbook is **not approved for deployment**. The local rehearsal proved an
initial-registration route on a shared application subnet: install Governance,
upload the four IO Wasms to its chunk store while the operator still controls
it, then complete framework and dapp controller handoff. After sale and
capitalization, operator proposals with Jupiter votes ask Root to install the
final checkpoint C constructors using those chunks. No launch helper is needed.

The deferred-registration route is a separate candidate. Its empty-list sale
fixture proves finalization but not a production installation sequence. A
production topology that places the chunk store and IO targets on different
subnets requires a supported store placement plan; do not silently change the
production dapp list or controller requirements.

Use three checkpoints: A seals pre-SNS inputs and baseline observations; B
observes finalized sale/reserve migration and produces C0/B0/Delta K; C binds
actual capitalization, refreshed permanent stake, anchor funding and final IO
constructors. The local validator exposes these boundaries as `acceptance_checkpoint A|B|C`.
The older draft/sealed package check is not a substitute for these boundaries.
Do not guess future observations at checkpoint A. Local fixture success does not
approve production identities, metadata, sale parameters or installation authority.

For permanent refresh, the inspected NNS `manage_neuron` dispatch handles
`ClaimOrRefresh` by existing neuron ID before controller-only operations; the
refresh reads canonical ledger stake without changing controller. The real existing-ID test proved a non-controller refresh succeeds while
valid configuration changes remain unauthorized. Controlling NNS Manager does not impersonate its principal. Record IO
supply before and after permanent top-up as well as Stream funding: neither
operation may issue IO.

Every row is mandatory. “Recover” means reobserve canonical state and continue
from the first unproved postcondition; it never means blindly repeat a mutation.

| # | Prerequisite and intended action | Verification, stop and recovery |
|---:|---|---|
| 1 | Review source and pre-SNS inputs (checkpoint A). | Require one final approved coherent SNS framework bundle, approved sale and valid metadata, exact identities, baseline observations and proven install authority. Do not require post-sale C0 or final topped-up principal here. Use the explicit A boundary; the monolithic sealed package check does not prove it. |
| 2 | Run the nine status observations above. | Require frozen empty IDs, allowlisted reservation controllers, sufficient cycles and recorded subnets. Stop on an installed module or unknown controller. |
| 3 | Obtain the authorized full-neuron response before funding. | Require the existing caller permission above, canonical typed evidence, exact staking Account and baseline cached stake; retain maturity observations separately. Stop on denied/unknown read authority, configuration contradiction or pending maturity disbursement. |
| 4 | Review the final source approval and resolve it through `tools/scripts/test-sns-framework`. | The existing official TOML lock is the reviewed six-component authority; `sns-source-baseline.json` hash-binds that lock and separately pins the clean parser source. These pins are local rehearsal evidence, not final approval. Require each component's reviewed proposal, source revision, compressed/raw Wasm and embedded DID. Caller-provided matching hashes cannot authorize another version. |
| 5 | Render SNS inputs only; leave final IO constructors for checkpoint C. | Require 1B IO: operator 30 IO, Jupiter 100M IO, sale S, reserve 899,999,970-S IO; reject cost 10 IO, minimum stake 1 IO. Require zero IO treasury/future Swap, NF disabled, valid baskets and real parser validation. |
| 6 | Install the five SNS canisters using the reviewed preallocated-canister procedure; preload IO chunks into Governance before relinquishing operator control. | Require exact module hashes, source-defined controller transitions and `automatically_advance_target_version = false` in the typed Governance init payload. The frozen self-deployed SNS must not follow later SNS-W targets before the sale completes. Stop if the final framework cannot safely use the frozen IDs. |
| 7 | Query Ledger genesis balances before opening the sale. | Require total supply 1B and exact operator/Jupiter/reserve/Swap distribution. The Jupiter staking Account owner must be IO SNS Governance and its subaccount must derive from the acceptance SNS Rewards controller plus memo `IOJUP001`. Treasury and future Swap must be zero. Stop on unexplained balances. |
| 8 | Open the sale and accept real participant contribution(s). | The operator may be the only participant, but record every buyer and actual accepted amount. Do not infer success from reaching the cap and do not assume finalization is immediate or atomic. |
| 9 | Observe finalization to completion. | Require canonical Swap `Committed` terminal state, successful sweep/claim results, Governance `Normal`, completed dapp handoff and actual accepted direct ICP exactly `S`. Neurons' Fund must be zero. Stop on transitional/partial finalization. |
| 10 | Reconcile sale outputs. | Require distributed sale-neuron stake + burned IO fees + Swap residual = `S`; SNS Governance default ICP balance + burned sweep fees = accepted ICP. Record actual sale neuron Accounts. Sale proceeds remain in that SNS ICP treasury Account and are not `B` or permanent capital. |
| 11 | Query all three developer neurons. | Require fixed distinct memos, exact 30 IO operator stake, exact 100M IO Jupiter stake controlled by the acceptance SNS Rewards canister, derived reserve stake, operator/Jupiter voting eligibility and 100% developer multipliers. Require the reserve neuron under the human controller to be canonically Dissolved with Disburse permission. The Jupiter neuron must be non-dissolving with no followees. |
| 12 | Actual acceptance SNS Rewards canister calls standard RemoveNeuronPermissions on its dedicated neuron. | Derive all non-Vote permissions from the pinned enum and observed permission records, including ManageVotingPermission. Require exactly one principal (that canister) and exactly {Vote}; all other principals have no authority. Vote also permits following: no followees may bypass authenticated Jupiter decisions. Stop until the Jupiter implementation proves this irreversible action and postcondition. |
| 13 | Disburse the reserve neuron directly to `{ owner = gjthz-7iaaa-aaaas-amxuq-cai; subaccount = 0x781a5fc2d878692d1fa555359c190f96d61f85b392cf11b80f4666169d789027 }`. | Require exact destination bytes before signing. Stop if Governance is not Normal or the neuron is not Dissolved. Recover by inspecting the neuron and Ledger blocks; never route through a human Account. |
| 14 | Record canonical post-migration Ledger supply, Stream reserve and both explicit exclusion balances in `launch-observation.draft.json`; run `acceptance_capitalization`. | Require `C0 = supply - reserve - reserve-neuron balance - Jupiter-neuron balance`, including the 30 IO neuron and all nonexcluded sale tokens. Require the Jupiter balance to remain exactly 100M IO and prove that moving it from excluded reserve to excluded stake did not change `C0`. Stop on unresolved fees, residuals or arithmetic. |
| 15 | Compare the generated targets: `B0=C0`, exact `Delta K=2*C0/3`, transfer `ceil(2*C0/3)` e8s. | Record any `1/3` or `2/3` e8 upward rounding. Do not use sale proceeds or the Jupiter 40/60 ratio as a market price. The 100M Jupiter allocation adds zero to both targets. |
| 16 | Top up the already verified permanent-neuron staking Account by exactly the generated `Delta K` transfer amount. | Recheck the baseline immediately before transfer. Stop if it differs from checkpoint 3. Preserve the transfer block and inspect it before any retry. |
| 17 | Submit canonical NNS `ClaimOrRefresh` for neuron `4_021_054_791_166_218`, then re-read it. | Require final cached stake to reflect the verified baseline plus the canonical credited top-up. Record controller/configuration again. Bind `audited_permanent_principal_e8s` only to this final observed cached principal. Never count ordinary/staked maturity. |
| 18 | Observe pre-funding `B`, then transfer exactly `B0-B` ICP e8s to Stream liquid AccountIdentifier `104871600196c2231e927e1db940290ad0fed08c2c6e593a9b73094a28c346cd`. | Require final canonical claim backing exactly `C0`; stop if pre-funding `B>C0`. Record the block before any retry. Query IO total supply before/after and require no IO issuance. |
| 19 | Operator contributes 10 ICP to Dynamic AccountIdentifier `23f97e98045b2376759598bdd5b49e538f1c19cfd96280437f7c72a2cb1b8362`. | Verify controller derivation and memo 0. Checkpoint C's `dynamic_anchor_seed_e8s` is the initial canonical balance before fees: reject below 10 ICP, accept 10 ICP or excess. Anchor entitlement stays 10 ICP; excess is excluded surplus and adds no B, C, issuance or permanent credit. Later reconciliation can consume anchor fees; do not reapply the initial threshold to that later balance. |
| 20 | Adopt the existing standard Governance-parameter proposal, then have Root install NNS Manager with the final observed permanent principal. | In the single `ManageNervousSystemParameters` proposal require both `max_number_of_neurons = 1000` and `automatically_advance_target_version = false`; query the effective parameters and installed SNS module hashes before IO installation. Then require the current-source NNS Manager hash and exact SNS/Stream/Jupiter/NNS identities, fees, block floor and Accounts. |
| 21 | Observe autonomous Dynamic bootstrap. | Require ClaimOrRefresh, NNS Manager controller, exact 14-day delay/following, canonical refreshes and recorded parent. Do not manually create it. |
| 22 | Observe NNS Manager readiness. | Require autonomous `Paused -> Ready`, controller/baseline checks and timers without lifecycle proposals. Monetary runtime readiness has no Jupiter-mirror heartbeat dependency. |
| 23 | Use an adopted Governance proposal to have Root install Stream with its checkpoint C constructor. | Require exact Ledger/Index/Root/Governance/NNS identities, module hash, fees and exactly the reserve and Jupiter-neuron explicit exclusions. |
| 24 | Observe Stream readiness and economic snapshot. | Require autonomous `Paused -> Ready`, `C=C0`, `B=B0`, canonical rate 1 ICP/IO, no sale-proceeds backing entry, and no active/reward/Dynamic weight from the 100M Jupiter stake. |
| 25 | Exercise a redemption. | Derive gross ICP from the canonical admission-time B/C ratio; the initial 1:1 reference rate does not erase later IO fee burns. Net payout subtracts the applicable ICP fee. Preserve current fee and exact-proof behavior. |
| 26 | Use adopted Governance proposals to have Root install Historian and the verified acceptance Frontend; verify the reviewed controller graph. | Build the browser before Rust as described below. Require exact installed Wasm hashes, served HTML/JS bytes and acceptance configuration. Any fallback is separately sealed. |
| 27 | Operator submits through its 30 IO neuron under SubmitProposal authority. | Observe the 10 IO assessed reject cost, clearing on adoption or retention on rejection, and subsequent effective stake. Prove operator Yes alone cannot carry adoption against idle Jupiter voting power. Governance readiness is separate from monetary runtime readiness. |
| 28 | Exercise the future Jupiter mirror from the actual acceptance SNS Rewards canister. | Require allowlisted IO Governance observation, unambiguous proposal mapping, finalized authenticated Jupiter decision, correct IO proposal/neuron vote, upgrade-safe retry state, and execution before the action-specific IO deadline. Human impersonation and operator following are invalid. Compare action-type deadlines across both SNSs; the daily owner-index scan is not presumed sufficient. |
| 29 | Exercise the current `opt nat64`/`opt nat` reward-share path with Jupiter abstaining, voting Yes and voting No. | Require the reviewed official Governance to count Jupiter's SNS voting power while Stream gives its exact excluded Account zero IO credits and denominator weight; ordinary-neuron economic credits must be identical across those Jupiter choices. |
| 30 | Exercise isolated Jupiter/source flow. | Require its distinct inflow identity and independent 60/40 issuance/capitalization economics, without treating that ratio as sale price or redemption rate. Do not repoint Jupiter's `reward_sns_root_canister_id`. |
| 31 | Observe daily reward, weekly TwoYear and frozen 14-day maturity work. | Require autonomous scheduling and no recurring DAO lifecycle/maturity proposal. |
| 32 | Freeze evidence. | Record source/artifact hashes, sale/finalization observations, accounts, funding blocks, final permanent principal, configs, controllers, canister-originated Jupiter vote evidence and all stop checks. |

## Local execution boundaries

The local-only executable entry point is `cargo run -p xtask -- acceptance_sale_real`.
It requires the pinned parser adapter (`IO_ACCEPTANCE_SNS_PAYLOAD_TOOL`), the
resolved reviewed official bundle (`IO_REAL_SNS_WASM_DIR` and
`IO_REAL_SNS_WASM_MANIFEST`), current-source IO Wasms
(`IO_ACCEPTANCE_IO_WASM_DIR`), the isolated acceptance browser/Wasm output
(`IO_ACCEPTANCE_FRONTEND_BUILD_DIR`), and `POCKET_IC_BIN`. The driver generates
a fresh local evidence directory and invokes:

```sh
cargo run -p xtask -- acceptance_checkpoint A LOCAL_EVIDENCE_DIRECTORY
cargo run -p xtask -- acceptance_checkpoint B LOCAL_EVIDENCE_DIRECTORY
cargo run -p xtask -- acceptance_constructors LOCAL_EVIDENCE_DIRECTORY
cargo run -p xtask -- acceptance_checkpoint C LOCAL_EVIDENCE_DIRECTORY
```

A checks empty-canister authority observations, all declared SNS/IO artifact
bytes, checked IO interfaces, and parser-generated SNS payloads. IO artifacts
are available before A; final monetary constructors are generated only after B
and actual capitalization observations.
B checks successful raw finalization replies and sale/distribution conservation.
C checks capitalization, public permanent-neuron creation/refresh evidence,
actual artifact bytes and production-type constructors. Informational balances
are not fabricated to satisfy an earlier boundary. These implementations are
subject to the execution results recorded in `implementation-status.md`.

The complete local test exercises initial dapp registration, with IO raw-Wasm
chunks uploaded to the existing Governance canister before controller handoff.
After C, standard Governance proposals instruct Root to install the final
constructors in Install mode. This route requires the observed shared
application subnet. Its execution status is reported separately from source
inspection. The deferred-registration candidate remains available to component
cases and does not alter production requirements. Real Jupiter mirror
integration remains external.

### Acceptance browser artifact construction

Use `bash tools/acceptance/build-frontend ABSOLUTE_NEW_OUTPUT_DIRECTORY` with
installed locked npm/Cargo inputs. It copies public assets to the new output,
then invokes the existing `npm run build:frontend` followed by the offline locked
release Rust/Wasm build. It explicitly sets all six browser inputs:

```text
IO_FRONTEND_NETWORK=ic
CANISTER_ID_IO_HISTORIAN=gaqmf-jaaaa-aaaas-amxva-cai
CANISTER_ID_IO_STREAM_MANAGER=gjthz-7iaaa-aaaas-amxuq-cai
CANISTER_ID_IO_LEDGER=hn6il-giaaa-aaaas-amxsq-cai
CANISTER_ID_FRONTEND=ghrkr-eyaaa-aaaas-amxvq-cai
IO_FRONTEND_DEMO=false
```

Neither `.dfx` discovery nor inherited values override these assignments.
Set `IO_ACCEPTANCE_FRONTEND_BUILD_DIR` to that output for local acceptance and
`cargo test -p io-frontend --test io_frontend_pocketic -- --nocapture`.
Checkpoint A requires `io-artifacts.json`'s frontend `browser_assets` to name
its `public/` directory. It validates manifest/config, the executable JSON
configuration, content-addressed bundle bytes, HTML reference, and those exact
embedded bytes in the hash-bound install Wasm. The PocketIC asset test queries
HTML and its JS path and compares response bytes, while the private manifest
must return 404. Retain the isolated output with the evidence. This construction
does not modify tracked `public/` or approve a final production browser build.

The complementary `acceptance_sale::real_launch_timer_only_top_up_and_reward`
test uses the real sale and Governance/Root installations, then only simulated
time, ticks and status queries through initial TopUp completion, finalized
structural reconciliation and a later eligible reward event. The original
`acceptance_sale_real` path retains explicit keepers for deterministic recovery
and redemption. Keep their evidence distinct; neither asserts a one-minute SLA.

## Explicitly prohibited launch paths

- No SNS treasury-transfer proposal for IO capitalization.
- No automatic treasury sweep to IO.
- No Jupiter notification for sale proceeds.
- No IO issuance against sale proceeds, `B0`, or `Delta K`.
- No blind permanent-neuron transfer based on an unverified cached-stake baseline.

## Restart and recovery rules

- ICP/IO transfers and neuron disbursements are irreversible. Recover through
  canonical Ledger/neuron observation and exact proof, never compensation from a
  human liquid Account.
- A partially finalized sale remains a sale-finalization recovery problem. Do
  not calculate `C0` until terminal success and Governance Normal are observed.
- If readiness remains Paused, correct the external invariant or install a
  reviewed Wasm; autonomous retries must not require a lifecycle proposal.
- If a transfer result is ambiguous, use existing deduplication while valid and
  then exact canonical block proof. Do not scan or guess.

### Frozen acceptance evidence (offline only)

`context.json` must explicitly set `local_fixture=false` and `root_install=true`
for deployment evidence. A binds all nine roles to the checked-in acceptance
allowlist, the operator/Jupiter identities to the reviewed required inputs and
overlay, and every declared raw Wasm/interface hash to verified bytes. It
requires approved sale inputs and real metadata; the local renderer refuses
this context. Generate deployment YAML with `acceptance_render acceptance`,
then feed that exact YAML and execution context through the pinned payload tool.

An existing permanent neuron uses `permanent-full-neuron-before.bin` (the typed
NNS `get_full_neuron` Result) at A. Required baseline fields are identity,
controller, exact Account, stake, non-dissolving two-year delay, auto-stake false,
and an observed empty pending-disbursement list. `follow_policy` is an array of
`{ "topic": <i32>, "followees": [<nat64>, ...] }` entries matching that baseline.
Ordinary/staked maturity balances remain informational and do not fund B0 or
Delta K. A does not require a refreshed stake or final audited principal.

C additionally consumes `permanent-full-neuron-after.bin`, the successful
existing-ID refresh response, canonical ledger balances, and the actual ICP
ledger-tip response `activation-ledger-tip.bin`. It verifies unchanged identity
and configuration, the exact fee-independent top-up credit, and the final
activation floor/audited principal declared for installation. These are offline
validation inputs, not authorization to obtain or mutate mainnet state.
