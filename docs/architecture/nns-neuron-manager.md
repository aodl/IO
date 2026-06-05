# NNS Neuron Manager

`io_nns_neuron_manager` manages IO's NNS-side operational model. It does not issue IO and it does not calculate SNS-staker rewards.

## 2-Year Neuron

The 2-year NNS neuron represents permanent productive capital. Its maturity can increase liquid ICP after disbursement, but the 2-year principal is not liquid NAV and never issues IO.

## 2-Week Pool

The pooled 2-week NNS neuron backs the active IO SNS staking strategy. The model has explicit lifecycle plans for:

- `TwoWeekPoolRestake`
- `TwoWeekPoolSplit`
- `TwoWeekPoolStartDissolving`
- `TwoWeekPoolStopDissolving`
- `TwoWeekPoolMergeBack`
- `TwoWeekUnwindPrincipalDisbursement`

Target increases plan restake. Target decreases plan split and unwind. A cancel before readiness plans stop-dissolving and merge-back. A ready child plans principal disbursement into the liquid reserve path. If governance succeeds but a downstream ledger transfer fails, the journal retries the downstream transfer rather than repeating the governance mutation.

## Boundary Status

The canister has a production-shaped NNS governance trait and a mock adapter. The mock adapter calls debug methods only inside `clients::nns_governance`. Real NNS governance calls are future work.
