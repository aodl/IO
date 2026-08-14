# Optional Official SNS Testing Notes

These notes are optional local-only reference material. They are not part of `test_ci`, are not used by `verify_release`, and must not call mainnet.

The maintained rehearsal uses the source-built `sns` and `sns-testing`
commands after `. scripts/env.sh`, with `sns-testing-init`, PocketIC, Bazel,
and Quill where governance proposals require it. `dfx sns` is not the
prerequisite for this source flow.

In the guardrail vocabulary this is the **source-built sns** workflow.

Use `tools/sns/sns_init.io.local.yaml` only after filling local placeholders. Do not use --network ic.

The full official-readiness package is documented in:

- `docs/operations/official-sns-testing.md`
- `tools/sns/README.md`
- `tools/sns-testing/README.md`
- `tools/sns/testflight/README.md`

Do not add scripts that automatically run `dfx` in required workflows. Do not include optional SNS compatibility checks in `test_ci` or `verify_release` unless the required workflow is explicitly redesigned.
