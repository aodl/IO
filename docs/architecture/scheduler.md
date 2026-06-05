# Scheduler Architecture

The value-moving canisters now contain internal scheduler skeletons, but they do not make real external calls yet.

## io_stream_manager

The stream-manager scheduler is reserved for timer-driven work that will eventually:

- scan ICP ledger/index data for Jupiter Faucet deposits
- scan ICP ledger/index data for NNS maturity deposits
- scan IO ledger/index data for user redemption transfers
- classify observed flows
- process authorized streams internally

The current `scheduler_tick_once()` is a deterministic no-op that returns planned future steps and leaves value-moving state unchanged.

## io_nns_neuron_manager

The NNS manager scheduler is reserved for timer-driven work that will eventually:

- check and disburse 2-year maturity
- check and disburse 2-week maturity
- rebalance the pooled 2-week neuron
- disburse ready unwind child neurons

The current `scheduler_tick_once()` is a deterministic no-op that returns planned future steps and leaves model state unchanged.

## Integration Boundary

Real ICP ledger, IO ledger, NNS governance, SNS governance, and external canister calls remain future work. Those integrations should be added behind deterministic scheduler/client boundaries with targeted tests before any production wiring.
