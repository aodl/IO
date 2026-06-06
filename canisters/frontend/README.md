# frontend

Placeholder UI canister.

## Role

- UI surface.
- Should consume historian/read-model APIs, not value-moving canister internals.
- Expected historian entry points for the dashboard milestone are `get_dashboard_state` and `get_public_status` on `io_historian`.
- Frontend text is not protocol truth. Canonical value-moving facts remain in ledgers, indexes, governance canisters, and reviewed canister state transitions.

The frontend must not call `io_stream_manager` or `io_nns_neuron_manager` debug/state methods. Those canisters keep production DIDs constructor-only, while `io_historian` owns bounded public query APIs for dashboard consumption.
