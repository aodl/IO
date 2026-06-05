# Scheduler Architecture

The value-moving canisters now contain internal scheduler logic for the first mock-driven integration slice.

## io_stream_manager

The stream-manager scheduler is reserved for timer-driven work that:

- scan ICP ledger/index data for Jupiter Faucet deposits
- scan ICP ledger/index data for NNS maturity deposits
- scan IO ledger/index data for user redemption transfers
- classify observed flows
- process authorized streams internally

In debug/test Wasm, `debug_tick` scans configured mock ICP and IO ledger/index histories. It classifies authorized deposits by source and memo, journals each relevant block index, issues IO from the mock protocol reserve account, scans IO redemption transfers, pays ICP through the mock ICP ledger, and returns redeemed IO to the mock protocol reserve account.

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

The scheduler treats the journal as the retry source of truth. Completed operations are not reprocessed. Retryable operations resume at the first incomplete downstream transfer. Successful downstream transfers are not repeated.

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

In debug/test Wasm, `debug_tick` disburses model maturity, plans two-week pool rebalance work, disburses ready unwind principal, and sends mock ICP ledger transfers to the stream-manager deposit account with classifier memos.

The NNS manager stores durable operation journal entries for:

- `TwoYearMaturityDisbursement`
- `TwoWeekMaturityDisbursement`
- `TwoWeekUnwindPrincipalDisbursement`
- `TwoWeekPoolSplit`
- `TwoWeekPoolMergeBack`
- `TwoWeekPoolRestake`

The implemented mock-ledger disbursement paths use `AwaitingIcpTransfer`, `Completed`, and `FailedRetryable` phases. A maturity or unwind model change is not finalized locally until the ICP transfer to the stream-manager deposit account succeeds. Failed transfers remain retryable, and successful transfers are not repeated.

The NNS manager persists scheduler checkpoints:

- `last_two_year_maturity_check_time`
- `last_two_week_maturity_check_time`
- `last_unwind_check_time`

These checkpoint fields mean the scheduler attempted the corresponding maturity or unwind check at that model time. They do not by themselves mean the check successfully finalized a downstream disbursement. Successful finalization is represented by the corresponding journal operation reaching `Completed` after the ICP transfer and local model update complete.

The production DID does not expose `debug_tick`.

## Integration Boundary

Client modules now exist for ICP ledger/index, IO ledger/index, SNS governance, NNS governance, and ICP ledger transfer calls. They currently target mock canisters in debug/test integration. Production wiring remains future work and should preserve ledger/index/timer-driven flows rather than caller-submitted stream kinds. The journal is production-shaped but not audited.

Operational recovery for stuck journal entries is documented in `docs/security/controller-and-recovery.md` and `docs/operations/emergency-runbook.md`. Recovery must preserve retry/idempotency semantics and must not add production stream-processing or tick APIs.
