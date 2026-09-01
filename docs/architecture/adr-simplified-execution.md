# ADR: Simplified authenticated execution

- Status: accepted
- Date: 2026-07-31
- Supersedes: pre-execution monetary DIDs, ledger/index intent inference,
  redemption intake and return, automatic complete-absence recovery, and
  prelaunch stable migration compatibility
- Partially superseded by: `adr-anchored-dynamic-backing.md` for redemption
  transport, Dynamic-parent bootstrap/accounting, cohorts, and scheduling

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
`Redemption` or `LiquidReceipt`. The replacement redemption prepares an exact
caller/source/amount/fee/memo quote, proves the caller's ICRC-1 push directly
into reserve, then persists an unconditional at-most-once ICP payout obligation
to the same Account. There is no allowance, `transfer_from`, scanner, intake
Account, IO return leg, or rejected-redemption refund.

The NNS manager owns NNS commands and proof. It serializes immediate work and has
fixed slots for two-year maturity, two-week maturity, and one unwind child. The
stream manager does not re-prove NNS governance internals.

Own ledger calls persist one immutable request before the await. `Ok(block)` and
a matching duplicate from the configured canonical ledger prove the effect.
Retry uses the identical request inside the ledger deduplication window. An
unresolved result outside that window becomes `Stuck` and pauses execution.
Permissionless proof accepts only an exact block matching the persisted intent.
No global absence proof is attempted.

The same ordering applies to Governance effects: exact immutable intent is
durable before submission, and a later dependent irreversible effect is never
submitted while the earlier outcome is ambiguous or lacks its required
canonical postcondition. Definite success is immediately re-observed once and
fixed-size work may continue when proved. `Pending` denotes a real external or
bounded-work boundary, not an arbitrary per-invocation call count. Variable
fan-out remains explicitly bounded.

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
- ICRC-1 transfer fields used for the exact caller push:
  `packages/icrc-ledger-types/src/icrc1/transfer.rs`
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
