# Journal Compaction

This is a plan, not production activation. IO protocol remains not live, no value-moving IO
canister is deployed, production adapters are not active, and SNS IO ledger remains not launched.

## Scope

Covered journals and cursor-like records:

- stream-manager pending operation journals;
- stream-manager completed operations;
- stream-manager retryable failures;
- stream-manager terminal failures;
- stream-manager duplicate proof records;
- stream-manager processed transaction IDs;
- ICP index cursors;
- IO/SNS index cursors;
- NNS lifecycle operation journal;
- tiny/rejected transaction records;
- historian read model records derived from those sources.

## Safety Rules

These records must never be compacted before audit/activation:

- pending retryable operations;
- records needed for duplicate retry/idempotency;
- source block IDs for value-moving deposits/redemptions;
- gross/net/fee payout intent;
- IO return intent;
- failed ICP payout retry state;
- failed IO return retry state;
- governance lifecycle retry state;
- processed transaction IDs without an audited checkpoint/proof strategy;
- protected canister/neuron references.

Safe only after explicit audited policy:

- completed operations after an immutable checkpoint proves duplicate protection;
- terminal failures after operator/audit acknowledgement;
- old processed transaction IDs after a ledger/index lower-bound checkpoint;
- tiny/rejected transaction records after a replay-proof rejection checkpoint.

Historian read model entries are rebuildable and can be bounded by newest deterministic records.
Historian compaction does not authorize compaction of value-moving source journals.

## Current Policy

No automatic value-moving journal compaction is implemented. Current tests and
`validate_stable_storage` expose this as a deliberate bounded-state risk. The value-moving canisters
preserve retry-critical state across migration and upgrade-shaped export/import tests.

Compaction must preserve duplicate retry/idempotency. A retry after compaction must not be able to
issue a second IO transfer, make a second ICP payout, return IO twice, or lose a terminal rejection
that protects against reprocessing.

## Future Checkpoint Design

A future audited compaction design should define:

- a monotonic ledger/index checkpoint per account-history source;
- the exact duplicate-proof key retained after dropping full operation details;
- a retention period for completed and terminal records;
- separate treatment for retryable and terminal failures;
- an operator review/audit marker for compacted terminal failures;
- restore tests from pre-compaction and post-compaction snapshots;
- historian replay behavior after compacted source records.
