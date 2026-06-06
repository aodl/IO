# Scheduler Architecture

The value-moving canisters now contain internal scheduler logic for the first mock-driven integration slice.

## io_stream_manager

The stream-manager scheduler is reserved for timer-driven work that:

- scan ICP ledger/index data for Jupiter Faucet deposits
- scan ICP ledger/index data for NNS maturity deposits
- scan IO ledger/index data for user redemption transfers
- classify observed flows
- process authorized streams internally

In debug/test Wasm, `debug_tick` scans configured mock ICP and IO ledger/index histories. It classifies authorized deposits by source and memo, journals each relevant block index, builds `LedgerTransferRequest`s for downstream IO issuance, two-week IO rewards, ICP redemption payouts, and redeemed IO returns, executes those requests through `LedgerTransferClient` mock adapters, and updates journal status from the resulting boundary success or error.

The stream manager stores durable operation journal entries for:

- `JupiterFaucetStream`
- `TwoYearMaturityStream`
- `TwoWeekMaturityStream`
- `Redemption`
- `PrincipalUnwind`
- `UnknownIcpDeposit`

Stream phases are:

- `Observed`
- `Previewed`
- `AwaitingIoIssuance`
- `AwaitingIcpPayout`
- `AwaitingIoReturn`
- `PartiallyDistributed`
- `Completed`
- `FailedRetryable`
- `FailedTerminal`

Two-week maturity journal entries track each recipient neuron/account, the IO amount, transfer status, transfer block, and last error. Redemption entries track the observed IO redemption block, IO amount, ICP payout status/block, IO return status/block, and user account.

The scheduler treats the journal as the retry source of truth. Completed operations are not reprocessed. Retryable operations resume at the first incomplete downstream transfer. Successful downstream transfers are not repeated. Duplicate transfer results are not treated as success unless the duplicate block matches the expected amount, destination account, and memo. Bad fee and insufficient-funds boundary errors are retryable in this milestone because fee policy and funding reconciliation remain future production work. Production-shaped governance canister adapters exist behind the shared governance traits, but the scheduler continues to use the existing mock/debug clients in tests and does not call live NNS or SNS governance.

The stream manager persists scan cursors:

- `last_scanned_icp_index_block`
- `last_scanned_io_index_block`

Scans start after the stored cursor. A cursor advances only after a relevant observed block is safely represented in the journal, represented as a terminal rejected operation, or is known to be already processed. The processed-transaction set remains a duplicate guard for replayed source transaction IDs.

The production DID does not expose `debug_tick`.

## io_nns_neuron_manager

The NNS manager scheduler is reserved for timer-driven work that:

- check and disburse 2-year maturity
- check and disburse 2-week maturity
- rebalance the pooled 2-week neuron
- disburse ready unwind child neurons

In debug/test Wasm, `debug_tick` disburses model maturity, plans two-week pool rebalance work, disburses ready unwind principal, builds `LedgerTransferRequest`s to the stream-manager deposit account with classifier memos, and executes them through a `LedgerTransferClient` mock adapter.

The NNS manager stores durable operation journal entries for:

- `TwoYearMaturityDisbursement`
- `TwoWeekMaturityDisbursement`
- `TwoWeekUnwindPrincipalDisbursement`
- `TwoWeekPoolSplit`
- `TwoWeekPoolMergeBack`
- `TwoWeekPoolRestake`

The implemented disbursement paths use `AwaitingIcpTransfer`, `Completed`, and `FailedRetryable` phases. A maturity or unwind model change is not finalized locally until the boundary ICP transfer to the stream-manager deposit account succeeds. Failed transfers remain retryable, duplicate transfer results require amount/account/memo proof before completion, and successful transfers are not repeated. Two-week pool lifecycle planning is explicit for restake, split, start/stop dissolving, merge-back, and ready principal disbursement.

The NNS manager persists scheduler checkpoints:

- `last_two_year_maturity_check_time`
- `last_two_week_maturity_check_time`
- `last_unwind_check_time`

These checkpoint fields mean the scheduler attempted the corresponding maturity or unwind check at that model time. They do not by themselves mean the check successfully finalized a downstream disbursement. Successful finalization is represented by the corresponding journal operation reaching `Completed` after the ICP transfer and local model update complete.

The production DID does not expose `debug_tick`. Production-shaped NNS command DTOs and a Wasm-gated canister client exist for future reconciliation work, but this scheduler milestone remains unchanged: no live NNS governance calls, no lifecycle semantic changes, and no reward/economics changes are wired into default execution.

## Integration Boundary

Client modules now exist for ICP ledger/index, IO ledger/index, SNS governance, NNS governance, and ledger transfer calls. The shared `io-ledger-types` crate defines the production-shaped ledger/index boundary used by real adapter structs and mock mapping code. The shared `io-governance-types` crate defines NNS/SNS governance boundaries, production-shaped NNS/SNS Candid DTOs, Wasm-gated governance canister client structs, SNS eligibility snapshots, and SNS participation summaries. Downstream transfer paths now use `LedgerTransferClient`; debug/PocketIC scan sources still use mock/local index and transaction-history APIs. Production-shaped ICP/ICRC ledger and index adapters are fixture-tested but not audited or wired into default production execution. Production-shaped NNS/SNS governance adapters are also fixture-tested only and not wired into default production execution. Full archive traversal, audited retry policy, final governance principal wiring, and production lifecycle reconciliation remain future work. Boundary tests cover transfer errors, duplicate transfer proof, fees, account identifiers, account-history cursor gaps, full-ledger-contiguous cursor gaps, archive-required pages, index lag, production-shaped index page mapping, governance Candid fixtures, governance error classification, governance pagination, SNS eligibility, and SNS participation. Production wiring remains future work and should preserve ledger/index/governance/timer-driven flows rather than caller-submitted stream kinds. The journal is production-shaped but not audited.

Operational recovery for stuck journal entries is documented in `docs/security/controller-and-recovery.md` and `docs/operations/emergency-runbook.md`. Recovery must preserve retry/idempotency semantics and must not add production stream-processing or tick APIs.
