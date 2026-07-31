# ADR: Simplified authenticated execution

- Status: accepted
- Date: 2026-07-31
- Supersedes: constructor-only monetary DIDs, ledger/index intent inference,
  redemption intake and return, automatic complete-absence recovery, and
  prelaunch stable migration compatibility

## Context

The P0 research implementations explored index-driven intent discovery, generic
liability and operation records, complete-range absence proofs, and migration of
prelaunch experimental state. Those mechanisms made the monetary state machine
larger than the launch protocol they protected.

IO is not live. Its reserved production canisters are inert and contain no
value-bearing protocol state. The launch implementation can therefore begin from
one explicit V1 schema.

## Decision

Value-moving canisters accept narrow authenticated commands. A command binds the
caller, exact accounts, sequence, amount, fees, memo, timestamp, and canonical
destination. Callers never choose arbitrary monetary destinations and never mark
work complete.

The stream manager serializes all monetary work in one typed active operation:
`Redemption` or `LiquidReceipt`. Redemption uses ICRC-2 to pull IO from
`Account { owner = caller, subaccount = from_subaccount }` directly into the
reserve, then pays ICP to the same account. There is no intake account, IO return
leg, redemption scanner, or rejected-redemption refund.

The NNS manager owns NNS commands and proof. It serializes immediate work and has
fixed slots for two-year maturity, two-week maturity, and one unwind child. The
stream manager does not re-prove NNS governance internals.

Own ledger calls persist one immutable request before the await. `Ok(block)` and
a matching duplicate from the configured canonical ledger prove the effect.
Retry uses the identical request inside the ledger deduplication window. An
unresolved result outside that window becomes `Stuck` and pauses execution.
Permissionless proof accepts only an exact block matching the persisted intent.
No global absence proof is attempted.

Exact current/archive retrieval is limited to external Jupiter deposits,
NNS-generated receipts, and proof of a stuck active transfer. Index scanning and
global reconciliation belong to the historian and cannot authorize value
movement.

Standard SNS fee burn remains mandatory. Fee changes require pause, no active
operation, governance-approved configuration update, current-fee verification,
and unpause.

Production activation remains unavailable in this tranche.

## Pinned official interfaces

Official source is pinned to `dfinity/ic` commit
`d26582c6eb03dd047250cac23ce52fc68680d662` (fetched 2026-07-31).
DTOs and method names come from these paths:

- SNS/ICRC ledger service and standards:
  `rs/ledger_suite/icrc1/ledger/ledger.did` and
  `rs/ledger_suite/icrc1/ledger/src/main.rs`
- ICRC accounts and transfers:
  `packages/icrc-ledger-types/src/icrc1/account.rs`,
  `packages/icrc-ledger-types/src/icrc1/transfer.rs`
- ICRC-2 approve, allowance, and transfer-from:
  `packages/icrc-ledger-types/src/icrc2/approve.rs`,
  `packages/icrc-ledger-types/src/icrc2/allowance.rs`,
  `packages/icrc-ledger-types/src/icrc2/transfer_from.rs`
- ICRC-3 current/archive block DTOs:
  `packages/icrc-ledger-types/src/icrc3/blocks.rs`,
  `packages/icrc-ledger-types/src/icrc3/archive.rs`,
  `rs/ledger_suite/icrc1/archive/archive.did`
- ICP transfer and archive blocks:
  `rs/ledger_suite/icp/ledger.did`,
  `rs/ledger_suite/icp/ledger_archive.did`
- NNS `StakeMaturity`, `DisburseMaturity`, existing-neuron
  `ClaimOrRefresh`, `Split`, configure `StartDissolving` /
  `StopDissolving`, `Merge`, and `Disburse`:
  `rs/nns/governance/canister/governance.did` and the corresponding
  implementations under `rs/nns/governance/src/governance/`
- SNS governance neuron refresh after reward transfer:
  `ClaimOrRefresh` through `manage_neuron` in
  `rs/sns/governance/canister/governance.did`

At this commit, `icrc1_supported_standards` is implemented with `ICRC-1`,
`ICRC-2`, and `ICRC-3` (plus later standards) in
`rs/ledger_suite/icrc1/ledger/src/main.rs`. The required launch standards are
therefore available in the pinned official local SNS ledger.

The pinned NNS DTO confirms `StakeMaturity { percentage_to_stake : opt nat32 }`
returns both remaining `maturity_e8s` and `staked_maturity_e8s`.
`DisburseMaturity` takes `percentage_to_disburse : nat32` plus an optional
destination account and returns optional `amount_disbursed_e8s`.

## Consequences

Throughput is serialized and callers can receive `Busy`. Rare ambiguity can
pause until exact proof or an SNS-governed upgrade. Unsupported transfers are not
processed or refunded. These are deliberate launch constraints.

Every replacement deletes the replaced production call path. Research history is
preserved in the frozen branches and `/home/codexdev/io-pre-simplification.bundle`;
it is not runtime migration code.
