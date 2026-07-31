# io_nns_neuron_manager

The launch NNS canister exclusively owns proof and commands for the protected
two-year neuron, pooled two-week position, Jupiter staging, two-week maturity
staging, direct maturity policy, and one pending unwind child. Each sending
staging Account has its own bounded fee float; there is no general fee Account.

IO is not live. Production canisters remain inert.

## Production API

The production DID contains only:

- `notify_jupiter_deposit`
- `set_two_week_target`
- `resume`
- `prove_active_transfer`
- `set_paused`
- `get_status`

Jupiter notification is permissionless and carries one exact ICP block. The
configured Jupiter Faucet default Account, the NNS-manager default destination,
and the canonical transfer are the authority; no Jupiter callback exists.
Processed Jupiter blocks have narrow permanent replay protection. Target
updates remain authenticated, strictly generated, coalesced to the latest
desired target, and do not form a queue. See the pinned
[Jupiter integration contract](../../docs/architecture/jupiter-integration-contract.md).

## Direct maturity policy

For canonical ordinary maturity `M`, the manager calls
`StakeMaturity(40%)`, validates returned remaining and staked maturity, then
calls `DisburseMaturity(100% of remaining)`. The actual modulated ICP received
is liquid backing. Two-year receipts go directly to the stream liquid account
and issue no IO. Two-week receipts stage for a proof-bound stream receipt.

## Stable state

Launch state is one `StableCell<NnsStateV1>` with one typed immediate operation
and fixed optional slots for two-year maturity, two-week maturity, and one
unwind. Only V1 is supported; no prelaunch migration chain is compiled.
