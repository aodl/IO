# io_nns_neuron_manager

The launch NNS canister exclusively owns proof and commands for the protected
two-year neuron, the two-week-staker reward-backing NNS neuron, Jupiter staging,
reward-backing maturity staging, direct maturity policy, and one pending unwind
child. The reward-backing parent is non-dissolving at the approved 252,288,000-
second (eight-year) NNS delay; it is not a 14-day NNS position. Each sending
staging Account has its own bounded fee float; there is no general fee Account.

IO is not live. Production canisters remain inert.

## Production API

The production DID contains only:

- `notify_jupiter_deposit`
- `reconcile_two_week_backing_readiness`
- `prepare_two_week_maturity`
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
Processed Jupiter blocks have narrow permanent replay protection. The
authenticated reconciliation persists the exact desired target and reports
whether its liquid 60% maturity leg can start immediately. Target generations
are independent of entitlement-batch generations. Only Ready permits the
stream manager to freeze and immediately submit one batch generation and target
to `prepare_two_week_maturity`. There is no target queue. See the pinned
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

First readiness proves the exact configured parent, seeded stake, zero
prelaunch ordinary and staked maturity, disabled auto-stake, exact
non-dissolving approved delay, and absence of pending maturity or child
ambiguity. The proof survives upgrade; post-upgrade remains Paused. Later
retained staked maturity is expected, but auto-stake or dissolve-state drift
blocks new preparation. Immutable active and passive delivery work can still
resume while new preparation remains blocked.

## Direct two-week unwind

An over-target canonical parent creates one immediate typed unwind operation.
It splits exactly the excess and canonically starts the one child dissolving.
The child then becomes passive so the eight-year wait cannot block maturity on
the reduced parent. A newer target may promote that exact child for merge-back;
a ready child may be promoted for direct disbursement. Completion requires the
exact ICP Transfer block returned by NNS Governance (or an explicitly supplied
block for an ambiguous callback); no staging Account, stream receipt, IO
issuance, queue, ladder, or second child exists.

## Stable state

Launch state is one `StableCell<NnsStateV1>` with one typed immediate operation,
one optional passive unwind child, and fixed passive slots for two-year and
reward-backing maturity. Only V1 is supported; no prelaunch migration chain is
compiled.
