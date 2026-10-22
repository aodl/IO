# Acceptance preflight for later explicit authorization

**Prepared, not executed. No live-network request is authorized by this file.**
The commands below are an operator review plan, not an automatic runbook step.
Do not run them until their exact network, callers, targets and evidence output
directory have been approved. No identity creation/import/export/switch,
impersonation, wallet proxy, funding or settings mutation belongs to preflight.

Inputs are the source identities in [readiness-handoff.md](readiness-handoff.md),
the [frozen manifest](acceptance-manifest.json), reviewed official DIDs and an
absolute, new evidence directory outside both repositories. Record tool versions,
UTC time, network/root-of-trust, exact command, caller, exit status, raw reply,
request bytes and file SHA-256. Preserve failures as evidence. Query replies
alone are not certified management-state evidence.

## 1. Nine-canister inventory and authority

| Role | Exact acceptance canister | Required final sole controller |
| --- | --- | --- |
| SNS Governance | `h7y7s-kyaaa-aaaas-amxrq-cai` | SNS Root |
| SNS Root | `hk7o7-lqaaa-aaaas-amxsa-cai` | SNS Governance |
| SNS Ledger | `hn6il-giaaa-aaaas-amxsq-cai` | SNS Root |
| SNS Index | `he5dx-qaaaa-aaaas-amxta-cai` | SNS Root |
| SNS Swap | `hd4fd-5yaaa-aaaas-amxtq-cai` | SNS Root |
| Stream Manager | `gjthz-7iaaa-aaaas-amxuq-cai` | SNS Root |
| NNS Manager | `gosbn-sqaaa-aaaas-amxua-cai` | SNS Root |
| Historian | `gaqmf-jaaaa-aaaas-amxva-cai` | SNS Root |
| Frontend | `ghrkr-eyaaa-aaaas-amxvq-cai` | SNS Root |

For each literal ID in the table, the later authorized anonymous observations
use these existing CLI forms (substitute `CANISTER_ID` and a new evidence file):

```sh
dfx canister info --network ic --identity anonymous CANISTER_ID
dfx canister status --network ic --identity anonymous CANISTER_ID
```

`info` reads module hash/controllers. Preserve management `status` rejection if
the observer lacks public status access; do not treat `icp`'s public-state
fallback as proof that full cycles/settings were returned. For the installation
authority check, a separately approved **existing genuine installer** may repeat
only `canister status`, using that caller's reviewed name. Do not acquire or
configure credentials to make it work. Stop if status cannot be obtained through
the approved route. Operator identity is expected to be
`zkkkd-i34qc-367ln-e2u7o-ezznu-dkfqh-gtfvz-cviph-6qa4v-evtfs-wqe`;
compare its independently approved principal with the observed controller set.
This file does not authorize accessing that identity.

Obtain subnet placement from certified routing evidence, not ID spelling or
a stale dashboard. The installed agent's source-defined read-only operation is
`Agent::fetch_subnet_by_canister(&canister_id)`, followed by `Subnet::id()` and
`Subnet::contains_canister(&canister_id)`. It reads subnet state, verifies the
certificate/range and avoids the five-minute `get_subnet_by_canister` cache.
Retain the raw certificate/delegation, observed range, subnet ID, verification
result, tool version and evidence hashes through an operator-reviewed collector;
do not invent a subnet if that collector/evidence is unavailable. This review
does not introduce a new collector or dependency. Management status alone and
PocketIC fixture subnet IDs do not prove live routing.

Checkpoint A requires all nine IDs to exist, have empty module hashes, sole
operator control and at least 20T cycles each; all five installed framework
components must share a compatible subnet. An unexpected module/controller,
inaccessible cycles value or incompatible subnet is a stop, not permission to
empty, reinstall, move, top up or change a canister. The canisters are unobserved
until this inspection completes. Retain subnet capabilities, release/interface
compatibility and the reviewed management `install_chunked_code` requirements.
Chunk stores must be on the **target's subnet**. The accepted dapp path uploads
to each empty target itself before handoff, then Governance/Root installs it.
No upload/install test may run on these live targets during read-only preflight.

Prepare one record per role using the existing `baseline-canisters.json` shape:

```json
{
  "governance": {
    "canister_id": "h7y7s-kyaaa-aaaas-amxrq-cai",
    "subnet_id": "UNOBSERVED",
    "controllers": ["UNOBSERVED"],
    "module_hash": "UNOBSERVED",
    "cycles": "UNOBSERVED"
  }
}
```

`UNOBSERVED` is deliberately invalid, including for module hash: replace it with
JSON null only after observing an actually empty canister. Add all other roles
using checkpoint names `root`, `ledger`, `index`, `swap`, `stream`, `nns_manager`,
`historian`, `frontend`. Keep raw status/routing evidence alongside that summary;
the summary is not proof of its own assertions. Record caller and current vs
eventual controller vectors separately; never write desired controllers into
the observed field.

The positive allowlist must also reject collisions with known production IO
principals `thset-pqaaa-aaaar-qb7wa-cai`, `oae4c-3iaaa-aaaar-qb5qq-cai`,
`tjqj3-uaaaa-aaaar-qb7xa-cai`, `torpp-zyaaa-aaaar-qb7xq-cai`, Jupiter inflow
`acjuz-liaaa-aaaar-qb4qq-cai`, production SNS Rewards
`alk7f-5aaaa-aaaar-qb4ra-cai`, and protected production neuron
`10_292_412_127_977_304_661`. A denylist alone is not the authority boundary.

## 2. Authentic public permanent-neuron evidence

Target NNS Governance `rrkah-fqaaa-aaaaa-aaaaq-cai`, exact neuron
`4_021_054_791_166_218`, expected controller
`gosbn-sqaaa-aaaas-amxua-cai`. The observer must be unrelated, with neither
controller nor hotkey privilege. Anonymous is suitable for the planned public
observation; record `2vxsx-fae` as observer. Use the authentic embedded NNS
Governance DID as `NNS_DID`, not the SNS Governance DID or a guessed response
type. The pinned NNS Governance raw Wasm has SHA-256
`573af1cde5bf55a5e4dbf2d47f8dd340f7a73a107eebbc645fe1202b97f61e85`.
After verifying that file against the unchanged official lock, extract its DID
offline with `ic-wasm "$REVIEWED_NNS_WASM" metadata candid:service` into a new
file. The six SNS DIDs at the bundle root do not include this NNS interface.

Prepare `permanent-list-neurons-before-request.did` with this exact argument:

```candid
(record {
  neuron_ids = vec { 4_021_054_791_166_218 : nat64 };
  include_neurons_readable_by_caller = false;
  include_empty_neurons_readable_by_caller = opt false;
  include_public_neurons_in_full_neurons = opt true;
  page_number = null;
  page_size = null;
  neuron_subaccounts = null;
})
```

After explicit observation approval, collect the authentic wire responses:

```sh
didc encode --defs "$NNS_DID" --method list_neurons \
  < "$EVIDENCE/permanent-list-neurons-before-request.did" \
  > "$EVIDENCE/permanent-list-neurons-before-request.hex"
dfx canister call --network ic --identity anonymous --query \
  --candid "$NNS_DID" --output raw \
  rrkah-fqaaa-aaaaa-aaaaq-cai list_neurons \
  --argument-file "$EVIDENCE/permanent-list-neurons-before-request.did" \
  > "$EVIDENCE/permanent-list-neurons-before.hex"
dfx canister call --network ic --identity anonymous --query \
  --candid "$NNS_DID" --output raw \
  rrkah-fqaaa-aaaaa-aaaaq-cai get_full_neuron \
  '(4_021_054_791_166_218 : nat64)' \
  > "$EVIDENCE/permanent-get-full-neuron-before-unauthorized.hex"
```

Convert each saved hex file to the corresponding `.bin` by decoding hexadecimal
bytes, without rebuilding a Candid value. The existing decoder consumes these
filenames and the authentic `NnsProductionListNeuronsResponse`; never wrap it in
a fabricated `get_full_neuron` response. Require exactly one matching full
record, Public visibility (`Some(2)`), expected ID/controller, correct 32-byte
staking subaccount, positive cached principal with zero neuron fees, exact
63,115,200-second non-dissolving delay, auto-stake false, no spawn and an explicit
empty pending-maturity-disbursement list. Record ordinary/staked maturity,
hotkeys, followees and refresh metadata. Verify the observed subaccount against
the reviewed baseline and the ICP Ledger staking Account; never derive it from
neuron ID alone. Preserve every existing economic check and the expected policy
review, then require the same observer's actual `get_full_neuron` error type 3.
Summary `neuron_infos` is insufficient.

Evidence context uses `local_fixture=false`,
`permanent_observation_route="public-list-neurons"`,
`permanent_observer="2vxsx-fae"` and the frozen NNS Manager identity. Unresolved
reviewed-input fields remain blockers. Record principal before top-up separately
from maturity; reward accrual must not become alleged operator capital.

If Public full evidence is unavailable, stop and report the existing genuine
controller/hotkey observation route that is actually available. An existing
authorized `get_full_neuron` response may be used only through the already
supported authorized route with independently verified caller authority.
Operating a controller of NNS Manager does not let that person impersonate NNS
Manager in a Governance query. No hotkey addition, visibility change, neuron
mutation or new inspection endpoint is authorized.

## 3. Jupiter identities and trust chain

Leave `acceptance_jupiter_sns_rewards_canister_id` and the independent
`acceptance_jupiter_source_principal` unresolved until supplied and approved.
For each, verify identity, environment separation, observed module/controllers,
actual upgrade authority and consistency across the overlay, topology, Accounts
and constructors. Check the separate ICP source's authorization and activation
block floor without treating it as a voting principal.

Before sealing the voter, require the canonical Jupiter decision governance's
exact principal, reviewed source/module, Root/controller graph, actual proposal
parameters, finality rule and canonical decision records. Check who can upgrade
SNS Rewards and hence choose its future votes. Owner-context discovery through
OpenChat is not that chain. The acceptance voter must remain the sole `{Vote}`
principal of `IOJUP001`, with no ordinary/topic/default followees. No production
principal, mock or unresolved authority may satisfy this check.

## 4. Later A/B/C evidence and manual approval boundaries

These are planned stages, **not executable authorization**. Keep their evidence
requests distinct from the observation-only approval above.

| Boundary | Evidence required before approval | Later action requiring separate approval |
| --- | --- | --- |
| A: construction | Reviewed source/bytes, all nine status/routing/cycles records, authentic permanent baseline, real Jupiter authority/deadline contract and identities, exact Accounts, typed five-component initializers and six-component version | Installation of the approved framework; no canister creation or funding is implied |
| Framework / handoff | Actual six-component hashes (Archive template when none running), Normal mode, IO/IO, 1B allocation, exact permissions; target-local upload plan; live status observations | Chunk uploads and sole-controller handoff under the approved plan |
| B: reserve | Genuine Governance/Root and neuron observations; canonical zero genesis allocations; parked Swap constructor/Adopted/empty buyers/recipes/accepted participation; recorded quarantined default balances | Reserve-neuron disbursement to the canonical Stream reserve Account |
| Capitalization | Post-reserve total supply, reserve, two explicit staking exclusions, actual fee burn and derived C0; permanent cached/physical principal baseline; approved independent source Accounts and fee budget | Exact transfers for B0=C0, permanent increment ceil(2*C0/3) and independent 10 ICP anchor; no IO issuance |
| C: refresh / final construction | Exact transfer blocks and Account deltas; existing-neuron identity/configuration reconciliation; independent anchor; no-issuance; final Rust constructors and artifact hashes | Existing-ID ClaimOrRefresh through the genuine authority/standard supported route; each actual IO installation proposal and authenticated Jupiter decision/vote |
| Installed readiness | Physical modules and controller graph, NNS then Stream autonomous Ready, frontend served bytes; distinct observed balances | Separately approved acceptance operations; no automatic activation/deployment sign-off |

Canonical undisturbed no-sale observations yield C0=30 IO, B0=30 ICP, permanent
increment 20 ICP and a separate 10 ICP anchor. Those are derived examples, not
editable funding constants. Fees and cycles are additional. Later IO transfers
to Swap remain claim-bearing under the same canonical supply/reserve/exclusion
formula; unsolicited ICP is observable quarantined non-backing money and may
be irrecoverable. Never count it as permanent capital, IO backing, fee funding
or participation. Do not claim default-Account checks inventory all subaccounts.

After observations exist, the existing offline validators are:

```sh
cargo run --offline --locked -p xtask -- acceptance_checkpoint A "$EVIDENCE"
cargo run --offline --locked -p xtask -- acceptance_checkpoint B "$EVIDENCE"
cargo run --offline --locked -p xtask -- acceptance_checkpoint C "$EVIDENCE"
```

They do not collect live evidence or authorize the intervening mutations. Use
actual request/reply files with the documented existing names and context, not
handwritten success records. Retain before/after full-neuron records with the
same route and caller, unchanged configuration and exact existing-ID refresh;
preserve no-issuance, final constructors, Root summary and final module checks.
After installation, public Root summary uses `update_canister_list=false`; its
replicated observation endpoint requires separate approval even though it does
not authorize a controller or monetary mutation. Do not call it before Root is
installed or confuse it with a query.

Independent audit, reviewed lifecycle-gate disposition, genuine Jupiter
integration and the conditional liquidity contract remain separate gates. A
successful read-only inspection alone cannot make acceptance ready.
