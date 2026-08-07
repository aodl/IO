# IO repository agent rules

- Use `tools/scripts/test-sns-framework --source official` by default.
- Use `--source local` only when work explicitly requires the sibling IC
  checkout. Treat `../ic` as read/build input: never modify, reset, clean,
  switch, pull, rebase, or commit it.
- Never silently substitute official Governance for a requested local build.
- Report the resolved bundle manifest, source mode, profile, IC commit and dirty
  state when applicable, tracked-diff hash, component overrides, Governance DID
  hash, and artifact hashes for every SNS variant result.
- Candidate Wasms are not official releases. Do not commit Wasm bundles, SNS
  variant caches, resolver staging directories, or generated build artifacts.
- For long SNS variant work, append commands, patch checkpoints, results, and
  resource heartbeats to
  `/home/codexdev/.codex-io/sns-framework/PROGRESS.md` using the external
  monitored-command workflow.
- Do not push, deploy, or perform a mainnet operation unless the user explicitly
  authorizes that exact action in the current task.
