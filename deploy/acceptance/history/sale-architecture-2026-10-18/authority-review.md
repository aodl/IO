# Self-deployed launch authority review

Evidence level: the real self-deployed sale with an empty initial dapp list,
direct reserve migration, and NNS existing-ID refresh passed. Initial-dapp registration,
post-sale IO installation, autonomous readiness and exact redemption passed
in the complete local launch.
Pinned IC source: `fcc281f7200a8d3192ec46c225b436b2096bbf58`, clean.

| Target | Current authority | Actual caller required | Method | Required postcondition |
|---|---|---|---|---|
| Empty SNS Governance, Root, Ledger, Index, Swap | Local operator at creation | Local operator | management `install_code`, or `clear_chunk_store` / `upload_chunk` / `install_chunked_code` for large payloads | Exact same-source bytes and builder-produced payload installed |
| SNS framework controllers | Local operator | Local operator | management `update_settings` | Root controls Governance/Ledger/Index; Governance controls Root; reviewed Swap controller policy |
| ICP Ledger buyer deposit | Participant's Account owner | Participant | `transfer` | Exact contribution in Swap's participant subaccount |
| ICP Ledger sweep | Swap owns buyer subaccounts | Swap | `transfer` | Accepted ICP minus sweep fees retained in Governance treasury |
| NNS Governance Fund settlement | NNS/SNS-W relationship would be required for Fund participation | No external caller for this launch | No call | Fund-disabled branch returns success with zero participation |
| SNS Ledger distribution | Swap owns the sale distribution Account | Swap | Ledger transfer | Exact participant staking credits, fees and residual conservation |
| SNS Governance claims | Configured Swap | Swap | `claim_swap_neurons` | All recipes successfully claimed, no invalid/global failures |
| SNS Governance mode | Configured Swap | Swap | `set_mode` | Normal reached through finalization, never fixture assignment |
| SNS Root controller finalization | Configured Swap or Governance; all-dapps form is Swap-only | Swap | `set_dapp_controllers` | All registered dapps Root-only; empty candidate registration list does no management calls |
| Reserve transport neuron | Genesis controller with Disburse permission | Genesis controller | `manage_neuron(Disburse)` | Direct transfer to complete Stream reserve Account; fee/residual observed |
| Governance capacity | Proposer and voting neurons | Operator submits; Jupiter caller votes | `manage_neuron(MakeProposal(ManageNervousSystemParameters))` | Canonical maximum 1,000 observed before IO readiness |
| SNS Governance chunk store, after framework install and before controller handoff | Operator | Operator | management `upload_chunk` | Exact IO raw-Wasm chunks retained on the existing Governance canister |
| Governance-owned chunks after handoff | Governance itself may inspect its chunk store; Root is Governance's controller | Governance validates; Root installs | `stored_chunks`; `install_chunked_code` | Shared application-subnet route; no additional helper canister |
| Empty IO canisters registered in Root's init payload | Root | Root after operator proposal and Jupiter vote | `UpgradeSnsControlledCanister` with `mode = Install` | Observed: checkpoint C arguments and exact raw Wasm installed |
| Empty IO canisters under deferred-registration candidate | Operator | Operator | management `install_code`, or the same chunked-install methods | Checkpoint C constructors installed before operator relinquishes authority |
| IO controller handoff under candidate | Operator | Operator | management `update_settings` | Root is controller before registration |
| Deferred-registration candidate: SNS Root dapp registration (not exercised) | Governance-only endpoint | Governance after adopted proposal | `register_dapp_canisters` | IO dapps registered and Root-only controller state observed |
| Local permanent baseline creation | Operator controls a dissolved NNS parent | Operator | `manage_neuron(DisburseToNeuron)` | Observed: child created with NNS Manager controller and exact two-year delay |
| Existing permanent NNS neuron | NNS neuron controller retains monetary/configuration authority | Any caller can request balance refresh by existing ID | `manage_neuron(ClaimOrRefresh)` | Executed on PocketIC 14 bootstrap NNS: cached stake 2 -> 3 ICP; controller, dissolve policy, followees, hot keys and auto-stake unchanged; stranger Configure rejected with NotAuthorized (3) |

## Source locations

- `rs/sns/swap/src/swap.rs`: `finalize_inner` sequences ICP sweep, Fund
  settlement, recipes, SNS sweep, claims, mode change and Root controller change.
  `settle_neurons_fund_participation` returns without its NNS call when Fund
  participation is false.
- `rs/sns/governance/src/governance.rs`: `claim_swap_neurons` and `set_mode`
  check the configured Swap principal, not SNS-W membership.
- `rs/sns/root/src/lib.rs`: `set_dapp_controllers` checks configured
  Governance/Swap callers and management authority for each registered dapp.
- `rs/sns/root/canister/canister.rs`: `register_dapp_canisters` asserts the
  Governance caller before entering its implementation.
- `rs/nns/governance/src/governance.rs`: `manage_neuron_internal` handles
  ClaimOrRefresh before ordinary command dispatch. The ID/subaccount branch
  calls `refresh_neuron_by_id_or_subaccount` without a caller argument;
  `refresh_neuron` reads the ledger balance and updates cached stake/age.
  It does not replace controller, followees, or dissolve configuration.

## Candidate versus production

The complete deferred-registration installation route is **unproven and unapproved**. Its local configuration
has an empty initial dapp list and retains operator installation authority until
monetary constructor observations exist. Production keeps its initial dapp list
and its reviewed launch requirements. Successful finalization with an empty list
does not prove the production controller/install sequence.

The friendly parser cannot configure the neuron-capacity bound. Its payload
builder defaults to 200,000, while Stream accepts at most 1,000. A standard
post-sale parameter proposal is a candidate supported operation; successful
execution and installed observation passed before checkpoint C. The
adapter must not silently patch this parameter in a test-only genesis payload.

No framework impossibility has been established by this source review. Real
execution must prove all successful sweep/claim results and conservation, and
must test authority failures with otherwise valid requests.

PocketIC 14's maintained `install_canister` helper uses the target itself as the
chunk store for large Wasms. Every chunk and installation call carries the
operator principal; this route creates no helper canister. Installed module
hashes are compared with the raw Wasm hashes, while gzip transport artifact
hashes remain separately recorded in checkpoint A.

## Executed independent authority case

```sh
POCKET_IC_BIN=/home/codexdev/.local/bin/pocket-ic-server POCKET_IC_MUTE_SERVER=1 cargo test --offline --locked -p e2e-real-canisters acceptance_sale::real_nns_existing_id_noncontroller_refresh -- --ignored --exact --test-threads=1 --nocapture
```

Passed: one real test in 46.10 seconds. The bootstrap NNS build metadata reports
release profile, optimization 3, compiler 1.93.1; it does not report an IC commit.
This confirms the inspected operation's authority behavior on PocketIC 14's real
bootstrap NNS, not a claim that its NNS Wasm came from the SNS candidate commit.
The test uses ordinary ledger funding/claim/refresh calls and no privileged NNS
caller impersonation. Configuration probes cover increasing dissolve delay,
adding a hot key and enabling auto-stake maturity.

## Executed initial-registration installation route

The pinned action supports `mode = Install` and a chunked Wasm payload.
`rs/sns/governance/src/proposal.rs` validates the mode and chunks;
`upgrade_non_root_canister` passes the selected mode and constructor arguments
to Root. Management `stored_chunks` permits a canister to inspect its own store
(`rs/execution_environment/src/canister_manager.rs`). Thus Governance can
validate chunks in its own store while Root, its controller, installs them.
The operator uploads these chunks after installing Governance and before
relinquishing its controller authority. IO dapps are present in Root's initial
payload and handed to Root before sale finalization.

This route executed in `target/acceptance-sale-232084-1789382112934963709`: all
four Install proposals completed and Root reported the exact module hashes and
sole-controller state. Checkpoint C mutations and autonomous readiness also
completed. The later complete run
`target/acceptance-sale-301932-1789383755805662037` also passed redemption after
repairing Stream's completed-TopUp checkpoint transition. Its shared application-subnet topology matters: a
production layout that separates Governance and IO targets across subnets needs
its own supported chunk-store plan. Deferred registration remains a separate
candidate; neither route changes production requirements silently.

## Executed self-deployed sale

`acceptance_sale::self_deployed_sale_finalizes_and_migrates_reserve` passed in
66.56 seconds using the same-source bundle identified in implementation-status.md.
Checkpoint A passed before SNS installation without B/C observations. The real
Swap accepted 3 ICP, committed via its periodic task, swept one contribution,
created two recipes, swept both SNS allocations, and claimed both neurons, all
with zero failures. Fund-disabled settlement returned zero participation and
zero Fund neurons. Governance mode and Root finalization calls returned success.
No SNS-W membership or privileged caller impersonation was used.

Checkpoint B passed after direct reserve disbursement: C0 = B0 = 32.9998,
Delta K = 21.99986667 ICP, and retained SNS ICP treasury = 2.9999 ICP.
Evidence: `target/acceptance-sale-215868-1789380950878243929`. The empty initial
dapp list proves no IO install/controller handoff. The separate complete launch
above proves installation with initial registration.
