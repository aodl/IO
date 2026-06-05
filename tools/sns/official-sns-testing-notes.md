# Optional Official SNS Testing Notes

These notes are optional reference material for future local SNS compatibility work. They are not part of `test_ci`, are not used by `verify_release`, and must not call mainnet.

Official SNS local testing may require the `dfx sns` extension and SNS launch config inputs such as an `sns_init.yaml` file. IO does not run those commands in required workflows because the required IO toolchain remains `xtask`, `icp-cli`, Rust tests, and PocketIC.

Any manual official SNS testing must be local-only:

```bash
# Optional, local-only reference example. Do not use --network ic.
dfx sns validate-config-file tools/sns/sns_init.io.local.yaml
```

Some official flows include steps such as `add-nns-root` or SNS initialization validation that do not have an `icp-cli` equivalent. Multi-canister IO launch topology likely needs adaptation before the official flow can validate the complete IO setup.

Do not add scripts that automatically run `dfx`. Do not include optional SNS compatibility checks in `test_ci` or `verify_release` unless the required workflow is explicitly redesigned.
