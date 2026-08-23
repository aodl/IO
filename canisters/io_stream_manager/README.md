# IO Stream Manager

`io_stream_manager` owns the IO reserve, spendable liquid ICP backing, direct
redemption, purpose-specific claim and daily-stake observations, a bounded
per-neuron registry, one pending entitlement batch, and one serialized
monetary slot.

The scalar claim snapshot brackets its ledger reads with identical NNS control
epoch, operation sequence, and fingerprint observations. It derives:

```text
C = total_io_supply - protocol_reserve_io - nonredeemable_governance_io
B = L + P + U + T
claim_rate = B / C
pooled_target = floor(A_backing * B / C)
reward_target = floor(A_reward * B / C)
```

`L` is spendable liquid backing, `P` active pooled parent principal, `U`
live passive-child principal, and `T` exact backing represented by a persisted
in-transit phase. Permanent capital, unminted maturity, cycles, and operational
balances are excluded. The separate bounded daily observation lists SNS
neurons once. Governance supplies neuron identity/state; each distinct exact IO
Ledger staking Account is read at most once and supplies `A_backing`. A delayed
ancillary SNS `ClaimOrRefresh` cannot hide a successful reward transfer.

Redemption quotes `floor(user_io*B/C)` and separately requires `L` to cover
the gross quote. A liquidity shortfall returns gross, net, and available liquid
before pulling IO, consuming the nonce, or retaining the active slot. A valid
operation preserves exact allowance/account proof, adverse-drift reread,
transfer intent, deduplication, replay, and postcondition verification.

Each successful daily observation updates the sorted registry and one latest
no-effect reconciliation checkpoint. The existing durable one-shot reward timer
wakes the same work. At most one cohort may be committed per daily generation;
there is no target queue or second scheduler. Reward allocation is prospective
and requires `P >= reward_target`.

Jupiter and both maturity roles use one narrow claim-backing receipt. Every
claim credit enters Stream liquid first. A permit freezes exact pre-inflow
economics, the net liquid credit, fingerprint and, where IO is delivered, one
recipient vector. Completion marks ordinary target reconciliation due; any
later liquid-to-parent transfer is determined only from a fresh global target.
This can cost one additional ICP fee, which reduces `B` exactly once under
Policy A, in exchange for eliminating source-specific physical routes.

Production methods cover redeem/resume/proof, claim receipts, reward
observation/backing, lifecycle, caller replay status, and public status. Callers
never provide monetary facts or destinations.

Stable state is a strict prelaunch launch schema with one monetary slot, bounded
registry, latest checkpoint, accumulator, pending batch, and caller replay map.
Install and upgrade reopen Paused; old states are rejected and immutable work
remains resumable.

Useful checks:

```bash
cargo test -p io-stream-manager --lib
cargo run -p xtask -- did_surface
cargo run -p xtask -- validate_stable_storage
cargo check -p io-stream-manager --target wasm32-unknown-unknown
```
