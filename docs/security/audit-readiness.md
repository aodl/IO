# Audit Readiness

Use this checklist before requesting an external audit.

## Protocol And Accounting

- [ ] IO economics remain unchanged: `redeemable_io_supply = total_io_supply - protocol_reserve_io - non_redeemable_governance_io`.
- [ ] Redemption rate remains `liquid_icp_reserve / redeemable_io_supply`.
- [ ] TwoYearMaturity does not issue IO.
- [ ] 2-week maturity issuance is backed and tested.
- [ ] Redemptions are idempotent and cannot double-pay.

## Tests And Guardrails

- [ ] Accounting unit tests pass.
- [ ] Journal/retry tests pass.
- [ ] Upgrade-before-retry tests pass with live PocketIC.
- [ ] `cargo run -p xtask -- did_surface` passes.
- [ ] `cargo run -p xtask -- validate_install_args` passes.
- [ ] `cargo run -p xtask -- verify_artifacts` passes.
- [ ] `cargo run -p xtask -- security_scan_required` passes.
- [ ] `cargo run -p xtask -- test_ci` passes with `POCKET_IC_BIN`.

## Operations

- [ ] Controller and recovery plan is current.
- [ ] Emergency runbook is current.
- [ ] Release checklist has been followed.
- [ ] Artifact hashes match the governance proposal payload.
- [ ] Production DIDs for value-moving canisters remain install-args-only.

## Open Production Integration Gaps

- [ ] Real ICP ledger/index client reviewed.
- [ ] Real IO/SNS ledger/index client reviewed.
- [ ] Real NNS governance client reviewed.
- [ ] Real SNS governance client reviewed.
- [ ] Install args validated against real principals.
- [ ] Controller handoff proposal reviewed.
- [ ] Stable-state migration strategy reviewed if state grows.
- [ ] Certified historian/frontend plan reviewed.
