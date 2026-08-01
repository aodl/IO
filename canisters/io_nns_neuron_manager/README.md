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
- `start_maturity`
- `resume_maturity`
- `prove_maturity_mint`
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
is proved from one exact ICP Mint block and becomes liquid backing. Two-year
Mint proof completes directly against the stream liquid account and issues no
IO. Two-week Mint proof persists a typed delivery phase for the proof-bound
stream receipt. Governance DTOs are local and pinned to `dfinity/ic` commit
`021bf342f66296d5605b355a61b2430406a83783`; the matching Governance and ICP
ledger Wasms and exact source behavior are recorded in
[`nns-boundary-pin.md`](../../docs/testing/nns-boundary-pin.md). The canister
does not depend on a generic governance-types crate.

## Direct two-week unwind

An over-target canonical two-week stake creates one immediate typed unwind
operation. It splits exactly the excess, starts the one child dissolving,
merges it back if a newer target rises before readiness, or disburses the ready
child directly to the stream liquid Account. Completion requires the exact ICP
Transfer block returned by NNS Governance (or an explicitly supplied block for
an ambiguous callback); no staging Account, stream receipt, IO issuance, queue,
or second child exists.

## Stable state

Launch state is one `StableCell<NnsStateV1>` with one typed immediate operation
whose unwind variant owns its one child, plus fixed passive slots for two-year
and two-week maturity. Only V1 is supported; no prelaunch migration chain is
compiled.
