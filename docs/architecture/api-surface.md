# Production API surface

The stream manager exposes `redeem`, `prepare_liquid_receipt`,
`complete_liquid_receipt`, `resume`, `prove_active_transfer`,
`resume_reward_work`, `resume_reward_backing`, `set_paused`, and `get_status`.
Reward observation is permissionless, non-monetary, and idempotent. Reward
backing revalidates reviewed SNS Governance, reconciles the NNS target, freezes at most one
entitlement batch only when preparation can start, and advances the
authenticated two-week NNS path independently of later daily observations.

The NNS manager exposes `notify_jupiter_deposit`,
`reconcile_two_week_backing_readiness`, `prepare_two_week_maturity`,
`start_maturity`, `resume_maturity`,
`prove_maturity_mint`, `resume`, `prove_active_transfer`, `set_paused`, and
`get_status`. Two-week maturity preparation authenticates only the configured
stream manager and binds one frozen entitlement-batch generation and exact
target. Reconciliation is authenticated and idempotently persists target
changes; its generation is independent of entitlement-batch generation. The
generic maturity start is limited to two-year work.

Commands authenticate authority or carry an exact canonical proof. No caller chooses a payout destination or asserts completion. Production DIDs exclude ticks, event processors, state dumps, forced success and debug methods.
