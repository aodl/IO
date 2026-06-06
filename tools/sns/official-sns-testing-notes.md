# Optional Official SNS Testing Notes

These notes are optional local-only reference material. They are not part of `test_ci`, are not used by `verify_release`, and must not call mainnet.

Official SNS local testing may require the `dfx sns` extension and SNS launch config inputs such as an `sns_init.yaml` file. IO does not run those commands in required workflows because the required IO toolchain remains `xtask`, `icp-cli`, Rust tests, and PocketIC.

Use `tools/sns/sns_init.io.local.yaml` only after filling local placeholders. Do not use --network ic.

The full official-readiness package is documented in:

- `docs/operations/official-sns-testing.md`
- `tools/sns/README.md`
- `tools/sns-testing/README.md`
- `tools/sns/testflight/README.md`

Do not add scripts that automatically run `dfx` in required workflows. Do not include optional SNS compatibility checks in `test_ci` or `verify_release` unless the required workflow is explicitly redesigned.
