# Direct-genesis authority and maintenance

The active path uses the approved coordinated official tuple bound by
`sns-source-baseline.json`, not six assumed same-source binaries. Exact
installation bytes are independently verified even though source approval is
established. No SNS-W deployment record or completed sale is fabricated.

| Operation | Authorized principal / evidence |
|---|---|
| Empty-canister initialization and chunk upload | Observed sole operator; typed five-component payloads; four IO target-local chunk stores |
| Framework handoff | Local operator before acceptance; Root owns Governance/Ledger/Index/Swap, Governance owns Root |
| IO initial registration | Four frozen empty dapps appear in typed Root init; Root sole controller before C |
| Reserve disbursement | Operator controls `IORES001`; standard Governance Disburse pays canonical Stream reserve and burns the real fee |
| Permanent top-up | Operator transfers derived increment to the exact observed NNS staking Account; existing-ID ClaimOrRefresh needs no control right |
| Acceptance vote | Reviewed human operator separately votes with IOJUP001; exactly Vote, no genesis/checkpoint followees, unrelated callers unauthorized |
| IO installation | Operator submits with 30 IO, then separately votes with the 100M governance-only neuron, Governance instructs Root using target-local chunk stores |
| Default framework upgrade | Standard UpgradeSnsToNextVersion; actual six-component deployed version, Root statuses, SNS-W default path and blessed bytes |

Pinned source review at Swap/Governance `98c898f8de068a1fc0e97ebc805f9fa7f315d862`:

- `rs/sns/governance/src/governance.rs`: deployed_version_or_err requires a
  populated version for validation. get_or_reset_deployed_version uses observed
  modules. Standard upgrade logic normalizes an empty Archive set to its blessed
  template and records actual success in the upgrade journal.
- `rs/sns/governance/src/sns_upgrade.rs`: get_running_version asks Root for
  framework statuses; get_upgrade_params discovers the default next tuple and
  fetches the corresponding blessed Wasm from SNS-W.
- SNS-W `get_next_sns_version`, `list_upgrade_steps`, `get_wasm` do not require a
  deployed-instance record for the default path. Inserting an SNS-specific custom
  path has a separate registry restriction. Local mirroring of reviewed published
  predecessor/successor bytes is not a new official blessing.
- `rs/sns/swap/src/swap.rs`: fully specified Init enters Adopted; opening compares
  the clock in seconds against its stored start. `u64::MAX` seconds exceeds the
  u64 nanosecond clock's maximum seconds (`18,446,744,073`). Ordinary elapsed time
  cannot reach it. Auto-finalization is false. Timer tasks continue; this is
  constructor compatibility, not a named inactive mode or a positive-allocation
  sale validated by Params::validate.
- Root's summary includes the actual installed Swap status. `get_state` elides
  buyers/recipes; their dedicated endpoints and derived-state counts are required.
  Governance native treasury valuation uses a nonzero Swap price: zero price
  restricts those proposals/metrics and must not be replaced by invented proceeds.

The frozen Swap limits (one participant, min/max direct and per-participant ICP
100,000,000 e8s, basket count two / one-day interval) and proposal sentinel zero
are required compatibility fields. They do not authorize a subscription or NNS
creation proposal. Zero IO allocation makes an offered sale invalid under normal
sale Params validation. Later unsolicited ICP does not create accepted
participation and does not block acceptance. Record observed default-Account
balances as quarantined, non-backing funds, never permanent capital or protocol
funding. This observation does not inventory every Swap-owned subaccount. Later
IO transfers remain claim-bearing in total supply minus Stream reserve and the
two explicit staking exclusions; actual fee burns must reconcile. Swap is never
an additional claim exclusion. The exact genesis allocations and checkpoint
conservation checks remain mandatory.
Pinned `Swap::error_refund_icp` accepts only Aborted or Committed, so
unsolicited ICP (including unrefreshed buyer-subaccount deposits) can remain
stranded while Adopted. There is no automatic recovery promise; review such
funds separately without opening or pretending to finalize a sale.
Every future Swap successor needs source review and invariant regression before
upgrade. Disabling automatic target advancement prevents silent acceptance of
new target versions; no unpublished successor is assumed.

The retained `real_parked_swap_official_framework_upgrade_without_sale` regression
uses the reviewed Governance 143747-to-144202 transition. It must prove default
path discovery, blessed retrieval, proposal adoption, resulting module hashes,
deployed version and an actual Success journal outcome. The active launch uses
144202 directly. A motion or dapp upgrade is not framework-upgrade proof.

Completed feasibility evidence remains external under
`/home/codexdev/.codex-io/sns-framework/maintainable-no-sale-20261020` and
`/home/codexdev/.codex-io/sns-framework/no-sale-probes-20261020`.
Conversion execution results are reported separately in implementation-status.md.

Acceptance is intentionally individual-controlled. Production Jupiter authority
and cross-DAO deadlines remain separate requirements. Vote-only prevents ordinary
permission reassignment and provides no future handover. Standard Vote includes
following changes; exact empty following is observed at genesis/checkpoints.
The operator's ability to authenticate with the configured principal is an
unverified live prerequisite; local PocketIC caller simulation proves authorization
semantics, not possession of signing credentials.

## Proposal rejection budget and launch liveness

Pinned Governance source `98c898f8de068a1fc0e97ebc805f9fa7f315d862` defines
`make_proposal` in `rs/sns/governance/src/governance.rs`: SubmitProposal is
required, effective stake must cover the 10 IO rejection cost, and that cost is
charged before recording the proposal. `Neuron::stake_e8s` in `neuron.rs`
subtracts accumulated neuron fees from cached stake. `process_proposal` returns
the charge on adoption before executing the action. An adopted action that
subsequently fails execution therefore differs from a rejected or expired
proposal: rejection retains the charge; execution failure alone does not.
Validation errors returned before proposal creation incur no proposal charge.
The real `real_operator_30_io_proposer_fees` case proves adoption releases the
charge, one expiry and two No decisions retain 30 IO, and the next submission
fails for insufficient effective stake.

Standard `refresh_neuron` reads the existing staking Account's Ledger balance;
`claim_or_refresh_neuron` permits refreshing by neuron ID without an additional
controller privilege. Thus a holder can transfer existing IO to the exact
IOGOV001 staking Account, pay the real Ledger fee, then refresh its cached stake.
Refreshing alone neither cancels retained rejection fees nor creates IO. Native
SNS rewards are zero. ICP funding does not purchase IO in this sale-free launch.

There is **no funded, non-privileged top-up source in the frozen installation
plan**. Before checkpoint B, reserve IO is operator-disbursable but redirecting
it to the proposer would change the prescribed full reserve migration and
30 IO claim baseline. After B, all reserve IO belongs to Stream's exact reserve
Account; the operator cannot transfer that canister-owned balance. The 100M
Account is Governance-owned and its neuron grants only Vote, not Disburse or
SubmitProposal. The reserve-transport neuron is empty, treasury/Swap allocations
are zero, and no other liquid claim-bearing IO is provided before installation.
The 30 IO neuron's eventual disbursement burns retained management fees; it does
not recover the consumed proposal budget. No privileged recovery, new issuance,
reserve diversion or assumed outside IO donor is part of this launch contract.

After controller handoff Root is sole controller of the four IO targets, and
Root's `change_canister` authenticates Governance. The operator cannot bypass a
stalled proposal system by installing directly. With three final rejections and
no already-submitted proposal capable of restoring progress, this topology can
become unable to submit the remaining installations or any recovery proposal.
This is an unrecoverable governance stall **within the frozen plan**, not proof
that an authorized redesign could never recover it. It is a live acceptance
risk requiring explicit product sign-off before handoff/genesis, even with the
same person holding both voting authorities.

Submit only one installation proposal at a time. Verify the exact action,
target, constructor and artifact hashes offline, confirm both configured neuron
permissions and the operator's ability to authenticate, then cast the separate
IOJUP001 Yes promptly against the observed proposal ID and actual deadline.
Do not depend on wait-for-quiet. Observe adoption, restored proposal-effective
stake and the target's eventual physical module/controller state before the next
installation. If execution fails after adoption, stop to diagnose it rather than
confusing that failure with a retained rejection charge. On the **first retained
10 IO rejection/expiry charge**, stop further installation submissions and
funding; preserve the proposal and neuron records and require a reviewed
continuation decision. Never spend the remaining two charges through blind
retries. Any proposed replenishment must first identify real available IO,
Ledger authority, fees and canonical accounting; this review approves none.
