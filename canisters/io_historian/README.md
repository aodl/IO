# IO Historian

`io_historian` is a bounded, rebuildable public read model. It cannot authorize
issuance, redemption, backing movement, NNS commands, SNS lifecycle, or launch.

The dashboard projects claim-bearing IO supply; liquid, pooled, unwinding, and
in-transit claim backing; total backing and its `B/C` rate; structural and
reward-eligible stake; pooled target/delta; live cohort count/oldest readiness;
observation freshness; available liquid; and permanent productive capital as a
separate non-claim-backing value.

Ledger balances remain canonical. Stream status supplies the latest canonical
pooled checkpoint, while public NNS observation supplies permanent stake and
staked maturity. Missing, stale, inconsistent, or error observations are never
converted to zero monetary facts.

Production configuration is constructor/upgrade-only and bounded. `null` on
first install remains prelaunch; a validated configuration arms one
non-overlapping one-shot refresh; `null` on same-schema upgrade preserves it.
Old development states are rejected.

The read-only production API exposes `version`, `get_public_status`,
`get_dashboard_state`, `get_protocol_snapshot`, and `get_claim_rate`.
There are no production ingestion, configuration, refresh, or debug methods.

Useful checks:

```bash
cargo test -p io-historian
cargo run -p xtask -- historian_tests
cargo run -p xtask -- validate_historian_freshness
cargo run -p xtask -- did_surface
```

IO remains prelaunch. Historian freshness is operator evidence, not monetary
authority or launch readiness.
