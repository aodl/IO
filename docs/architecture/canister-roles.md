# IO Canister Roles

## io_nns_neuron_manager

NNS-only canister. It manages:

- IO's 2-year NNS neuron `6345890886899317159`.
- A pooled 2-week NNS staking position for active IO SNS stakers.
- Temporary split/dissolving children needed to unwind the pooled 2-week position.

It transfers ICP maturity/principal to `io_stream_manager` using source metadata that lets the stream manager classify the flow.

## io_stream_manager

Main economic canister. It owns the protocol accounting model:

- Jupiter Faucet ICP -> 40% to 2-year stake, 60% liquid, backed IO to Jupiter Faucet.
- 2-year maturity ICP -> 40% restaked, 60% liquid, no IO issued.
- 2-week maturity ICP -> 40% restaked to 2-week pool, 60% liquid, backed IO to eligible IO SNS neurons.

## io_historian

Placeholder read model for stream and reward history.

## frontend

Placeholder UI canister.
