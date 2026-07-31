# Production API surface

The stream manager exposes `redeem`, `prepare_liquid_receipt`, `complete_liquid_receipt`, `resume`, `prove_active_transfer`, `set_paused`, and `get_status`.

The NNS manager exposes `notify_jupiter_deposit`, `set_two_week_target`, `resume`, `prove_active_transfer`, `set_paused`, and `get_status`.

Commands authenticate authority or carry an exact canonical proof. No caller chooses a payout destination or asserts completion. Production DIDs exclude ticks, event processors, state dumps, forced success and debug methods.
