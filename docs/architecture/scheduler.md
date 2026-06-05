# Scheduler Architecture

The value-moving canisters now contain internal scheduler logic for the first mock-driven integration slice.

## io_stream_manager

The stream-manager scheduler is reserved for timer-driven work that:

- scan ICP ledger/index data for Jupiter Faucet deposits
- scan ICP ledger/index data for NNS maturity deposits
- scan IO ledger/index data for user redemption transfers
- classify observed flows
- process authorized streams internally

In debug/test Wasm, `debug_tick` scans configured mock ICP and IO ledger/index histories. It classifies authorized deposits by source and memo, processes each block index once, issues IO from the mock protocol reserve account, scans IO redemption transfers, pays ICP through the mock ICP ledger, and returns redeemed IO to the mock protocol reserve account.

The production DID does not expose `debug_tick`.

## io_nns_neuron_manager

The NNS manager scheduler is reserved for timer-driven work that:

- check and disburse 2-year maturity
- check and disburse 2-week maturity
- rebalance the pooled 2-week neuron
- disburse ready unwind child neurons

In debug/test Wasm, `debug_tick` disburses model maturity, plans two-week pool rebalance work, disburses ready unwind principal, and sends mock ICP ledger transfers to the stream-manager deposit account with classifier memos.

The production DID does not expose `debug_tick`.

## Integration Boundary

Client modules now exist for ICP ledger/index, IO ledger/index, SNS governance, NNS governance, and ICP ledger transfer calls. They currently target mock canisters in debug/test integration. Production wiring remains future work and should preserve ledger/index/timer-driven flows rather than caller-submitted stream kinds.
