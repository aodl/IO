# IO Historian

## Role in IO

`io_historian` is IO's bounded public read model. It observes canonical
services, retains last-known values and health, and presents a dashboard-shaped
snapshot. It is rebuildable and is not canonical protocol truth.
It is not a value-moving authority. It cannot authorize issuance, redemption,
reserve movement, NNS commands, SNS lifecycle, or launch state.

IO protocol is not live; the SNS IO ledger remains not launched. Production
reservations are inert, and missing/stale/error observations are never
treated as zero.

## Dependencies and observation authority

One non-overlapping, one-shot timer generation observes:

- the SNS Ledger's total supply, protocol-reserve balance, and configured
  excluded IO balances;
- the ICP Ledger's liquid-reserve balance;
- Stream Manager and NNS Manager `get_status`;
- public NNS Governance build metadata and bounded public neuron-info queries
  for the distinct configured reward-backing parent and protected IO NNS neuron
  IDs;
- SNS Root topology, controllers, module hashes, and discovered archives;
- SNS Governance parameters and latest reward event; and
- SNS Index status and bounded recent histories for configured Accounts.

Ledgers remain canonical for current balances. The index canisters are the
normal account-history abstraction; archives are discovered and represented
without an unbounded traversal or monetary scanner. Root is canonical for SNS
topology, controllers, and module observations. Public NNS neuron information
supplies stake, staked maturity, dissolve delay, and state, but not ordinary
maturity; the Historian does not invent that value or impersonate a
controller.

Only a completely successful set of monetary reads commits a coherent checked
redeemable-supply/redemption-rate snapshot. Root observations distinguish
`Matching`, `Mismatch`, `Unavailable`, and `Unknown`.

## Configuration and bounded collections

Production has no public configuration or ingestion update method.
`ObservationConfig` is accepted only as an install/upgrade argument, so
configuration authority belongs to controller/SNS Root upgrade authority:

- `null` on first install leaves all sources `PrelaunchNotConfigured` and makes
  no observation calls;
- `opt config` validates and activates observation;
- `null` on a later same-Wasm upgrade preserves existing configuration; and
- `opt replacement` validates the complete replacement and clears observations
  belonging to the former topology.

Configuration enforces these collection/timing limits:

| Limit | Value |
| --- | ---: |
| Excluded Accounts | 16 |
| History Accounts | 8 |
| Expected modules | 12 |
| Recent transactions requested per history | 16 |
| Refresh interval | 60..=86,400 seconds |

It also rejects anonymous or duplicate source principals, duplicate Accounts or
names, malformed expected SHA-256 values, missing required module identities,
and invalid reserve/exclusion relationships. The optional reward-share
capability hash must equal the expected Governance module hash; generic module
freshness therefore cannot fabricate capability availability.

## Production API

The checked-in [production Candid](io_historian.did) is read-only:

- `version`
- `get_public_status`
- `get_dashboard_state`
- `get_protocol_snapshot`
- `get_redemption_rate`

There are no production list, configuration, ingestion, refresh, or debug
methods. A debug build retains only a permissionless local refresh trigger;
completed monitoring evidence must use autonomous canonical refresh.

## Lifecycle, stable state, and freshness

Configuration, last-known successful observations, timestamps, per-source
errors, and the bounded read model survive upgrade. The transient
refresh-in-progress flag does not survive, so interruption cannot wedge future
refresh. Upgrade presents former `Fresh` records as `Stale` until the re-armed
timer succeeds.

For public queries, a source stored as `Fresh` is presented as `Stale` when its
last successful observation is older than two times the configured refresh
interval. The comparison uses nanosecond timestamps and a strict “older than”
boundary. A retryable failure changes health to `ErrorRetryable` and records the
attempt/error while retaining the last successful values and success timestamp.

One refresh attempt has a single generation number, but unrelated source
sections commit independently when their own call succeeds. The dashboard does
not claim that last-known values from different sources form one globally
atomic generation: callers must inspect each section's freshness and success
timestamp. The monetary protocol snapshot is stricter—total supply, reserve,
excluded balances, liquid ICP, denominator, and rate are computed and committed
together from one successful set of ledger reads, so it never combines partial
monetary generations. The global last-success timestamp advances only when all
sources are `Fresh` after the attempt.

The historical v1/v2 stable record is decoded through a narrow compatibility
shape. Historical scanner/cohort records do not re-enter the current public
model.

## Failure and consistency semantics

Only one refresh generation runs at a time. Individual source failures stay
scoped and visible; last-known observations are not zeroed. Monetary snapshots
are atomic across their required ledger reads, while other source sections can
retain independently successful data. Bounded reads, capped errors, and one-
shot timer rearming prevent overlapping or unbounded background work.

Historian output is evidence for operators and clients, not permission to
complete a monetary operation. Exact transfer/governance proofs remain with the
value-moving managers.

## Commands and verification

```bash
cargo test -p io-historian
cargo run -p xtask -- historian_tests
POCKET_IC_BIN=/home/codexdev/.local/bin/pocket-ic-server \
  cargo run -p xtask -- historian_required
cargo run -p xtask -- validate_historian_freshness
cargo run -p xtask -- did_surface
```

Run PocketIC targets serially. See the repository [xtask guide](../../tools/xtask/README.md)
for aggregate gates.

## Non-goals and protected production state

The Historian is not a transfer scanner, ledger replacement, controller,
arithmetic oracle for value-moving canisters, or source of launch readiness.
It performs no caller impersonation and exposes no production ingestion method.
Protected NNS Manager execution canister `oae4c-3iaaa-aaaar-qb5qq-cai` and
protected IO NNS neuron `10292412127977304661` are not Historian observation
sources and are not touched by its validation or local tests.
