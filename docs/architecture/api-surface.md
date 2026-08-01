# Production API surface

The stream manager exposes `redeem`, `prepare_liquid_receipt`, `complete_liquid_receipt`, `resume`, `prove_active_transfer`, `set_paused`, and `get_status`.

The NNS manager exposes `notify_jupiter_deposit`, `set_two_week_target`, `prepare_two_week_maturity`, `start_maturity`, `resume_maturity`, `prove_maturity_mint`, `resume`, `prove_active_transfer`, `set_paused`, and `get_status`. Two-week maturity preparation authenticates only the configured stream manager and binds one closed cohort generation; the generic maturity start is limited to two-year work.

Commands authenticate authority or carry an exact canonical proof. No caller chooses a payout destination or asserts completion. Production DIDs exclude ticks, event processors, state dumps, forced success and debug methods.
