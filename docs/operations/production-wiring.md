# Pooled claim-backing production wiring

Production wiring is a non-runnable plan. Stream explicitly configures the IO
and ICP ledgers, NNS Manager, Jupiter receipt source, Jupiter claim-bearing IO
recipient, IO reserve, liquid ICP Account, SNS Governance/Root, exact fees and
retry windows. A distinct bounded list contains genuinely nonredeemable
governance staking Accounts; it must not contain the Jupiter recipient.
The corresponding value-moving source is `io_stream_manager`.

NNS explicitly configures the ICP ledger, Governance, permanent neuron,
maturity and Jupiter staging Accounts, Stream liquid destination, one fixed
pooled-parent memo, one fixed followee, exact parent minimum, fees, and retry
windows. There is no separately funded pooled parent, separate pooled fee
float, or caller-selected destination.
The corresponding NNS control source is `io_nns_neuron_manager`.

The resolved production policy is:

- pooled-parent memo `0`, used only as the fixed deterministic NNS staking
  nonce and not as application metadata;
- pooled-parent followee `10_292_412_127_977_304_661`, exactly equal to the
  configured protected two-year neuron ID;
- permanent-neuron controller/NNS Manager
  `oae4c-3iaaa-aaaar-qb5qq-cai`;
- permanent-neuron audited following remains alpha-vote neuron
  `2_947_465_672_511_369`; deployment does not change it.

Readiness and bootstrap compare the candidate pooled-parent staking Account
with the permanent neuron's observed canonical Account and reject equality
before a transfer permit exists. Unsolicited ICP at a distinct candidate
Account is recorded only as canonical pooled principal and any surplus is
handled by ordinary `OverTarget` reconciliation; it creates no issuance or
entitlement.

The daily authenticated pool-policy observation independently attempts
`RefreshVotingPower` for the permanent neuron and the pooled parent when it
exists. These are best-effort governance-maintenance calls: timestamp age and
refresh failure do not gate monetary work, one failed attempt does not suppress
the other, no followees are changed, and no additional timer or stable
scheduler exists.

## Role identity record

- Stream Manager: `thset-pqaaa-aaaar-qb7wa-cai`
- NNS Manager: `oae4c-3iaaa-aaaar-qb5qq-cai`
- Historian: `tjqj3-uaaaa-aaaar-qb7xa-cai`
- Frontend: `torpp-zyaaa-aaaar-qb7xq-cai`

Their non-authoritative source roles remain `io_historian` and `io_frontend`.

Two-year protected neuron `10292412127977304661` remains a protected reference
and is never a mutation target outside the installed NNS Manager. The pooled
parent remains lazy; memo and followee are fixed as above, but no production
pooled-parent neuron ID is invented before canonical bootstrap.

All production files remain dry-run validation inputs. IO issuance/redemption
is inactive, and this document authorizes no inspection, installation,
controller change, funding, or mainnet operation.

## Production Wiring Checklist

This is dry-run/config validation only: No production execution is active, the
IO protocol remains not live, the SNS IO ledger is not launched,
no value-moving Wasm installed, no production activation has happened, and no IO
issuance/redemption is enabled. Production activation is a later audited
milestone. The reserved IDs and `ReservedNotLive` entries are planned wiring
placeholders only; every target is reserved, empty/inert, and not live. The
IO_TEST ledger is non-canonical. Validation follows the `icp-cli` convention,
and required workflows do not use `dfx`.
