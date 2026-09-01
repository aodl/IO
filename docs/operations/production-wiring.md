# Anchored dynamic-backing production wiring

Production wiring is a non-runnable plan. Stream explicitly configures the IO
and ICP ledgers, NNS Manager, Jupiter receipt source, Jupiter claim-bearing IO
recipient, IO reserve, liquid ICP Account, SNS Governance/Root, exact fees and
retry windows. A distinct bounded list contains genuinely nonredeemable
governance staking Accounts; it must not contain the Jupiter recipient.
The corresponding value-moving source is `io_stream_manager`.

NNS explicitly configures the ICP ledger, Governance, permanent neuron,
maturity and Jupiter staging Accounts, Stream liquid destination, one fixed
Dynamic-neuron memo, one fixed followee, exact 10 ICP anchor target, fees, and
retry windows. The Dynamic neuron is deliberately preseeded protocol capital;
there is no claim-funded lazy bootstrap, caller-selected destination, or
separate pooled fee float.
The corresponding NNS control source is `io_nns_neuron_manager`.

The resolved production policy is:

- Dynamic-neuron memo `0`, used only as the fixed deterministic NNS staking
  nonce and not as application metadata;
- Dynamic-neuron followee `10_292_412_127_977_304_661`, exactly equal to the
  configured protected two-year neuron ID;
- permanent-neuron controller/NNS Manager
  `oae4c-3iaaa-aaaar-qb5qq-cai`;
- the permanent neuron is recorded and operationally expected to follow
  alpha-vote neuron `2_947_465_672_511_369`; this remains subject to separately
  authorized mainnet verification, and deployment does not change it.

Before Ready, bootstrap compares the deterministic Dynamic staking Account
with the permanent neuron's canonical Account and rejects equality. It requires
a canonical balance of at least 10 ICP, claims or refreshes the neuron, and
proves exact 14-day non-dissolving delay, auto-stake off, fixed following, and
identity. Exactly 10 ICP initializes excluded anchor; any positive residual is
excluded unattributed surplus. Neither category enters claim backing or issues
IO, and excess cannot block bootstrap or prove a later exact transfer.

Authenticated policy observation independently attempts `RefreshVotingPower`
for the permanent and Dynamic neurons. These remain best-effort governance
maintenance: timestamp age and refresh failure do not gate monetary work, one
failed attempt does not suppress the other, and no followees are changed. The
12-hour Stream structural scheduler and the NNS recovery/ready-child timer are
separate one-shot scheduling mechanisms with no stable timer timestamp.

## Role identity record

- Stream Manager: `thset-pqaaa-aaaar-qb7wa-cai`
- NNS Manager: `oae4c-3iaaa-aaaar-qb5qq-cai`
- Historian: `tjqj3-uaaaa-aaaar-qb7xa-cai`
- Frontend: `torpp-zyaaa-aaaar-qb7xq-cai`

Their non-authoritative source roles remain `io_historian` and `io_frontend`.

Two-year protected neuron `10292412127977304661` remains a protected reference
and is never a mutation target outside the installed NNS Manager. The Dynamic
neuron ID is discovered and recorded only through canonical pre-Ready
bootstrap; configuration does not invent it. Physical principal is partitioned
as claim-bearing principal plus excluded anchor plus excluded surplus.

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
