# ADR: one cohort deadline timer

## Decision

The stream manager installs one one-shot timer for the exact
`active_reward_cohort.closes_at_timestamp_seconds`. The callback invokes the
same idempotent close-due transition exposed to permissionless keepers. After
an upgrade, the timer is reconstructed from the unchanged stable cohort
deadline.

## Rationale

Closing at the stored deadline prevents an operator-dependent stretched or
backdated reward interval. A one-shot callback removes the operational need to
poll while preserving the protocol's small state surface: one active cohort,
one pending closed cohort, and one transient timer identifier.

The timer is coordination-only. It cannot transfer tokens, submit governance
commands directly, retry an operation, or discover monetary activity. Closing
still obtains bounded canonical SNS evidence and uses the same full typed
compare-and-swap rules as a keeper call.

## Simplicity constraints

- There is exactly one transient timer slot.
- The implementation uses `set_timer`, never an interval timer.
- Capturing a cohort replaces the prior one-shot timer; closing clears it.
- No general scheduler, retry queue, ledger scan, or history scan is added.
- A pending cohort blocks closing a later active cohort. The resulting visible
  gap is preferable to merging, stretching, or fabricating reward periods.
- Post-upgrade reconstruction changes neither capture nor close timestamps.

IO remains Paused and inert; this decision does not activate production.
