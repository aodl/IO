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
| Jupiter vote | Sole acceptance Jupiter SNS Rewards principal has Vote; operator cannot vote or escalate; no following bypass |
| IO installation | Operator submits with 30 IO, Jupiter authentic caller adopts, Governance instructs Root using target-local chunk stores |
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
