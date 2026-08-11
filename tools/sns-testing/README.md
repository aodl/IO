# Optional Local SNS Testing

This directory contains optional local-only helpers for the maintained official local SNS rehearsal. These scripts are not required CI and are not used by `verify_release`. The immutable completed 2026-08-11 sanitized package is historical local evidence; it is not proof that an official capability-bearing SNS release exists or that IO is ready for mainnet.

Follow the current official ICP/DFINITY SNS testing documentation as the source of truth. The historical standalone `dfinity/sns-testing` repository is deprecated; if the official docs reference successor tooling or a new repository/location, use that current official location.

The maintained path uses the source-built `sns-testing-init`, `sns-testing` and `sns` binaries from `rs/sns/testing` and `rs/sns/cli`; it does not depend on the dfx SNS extension.

The source flow has reached SNS-W deployment, swap finalization, reserve funding,
ledger/index evidence, controller handoff, and authentic same-source candidate
Governance/Root upgrade execution. The one-component candidate-Governance with
official-Root wire mismatch is historical. The local chunk-store authorization
gap is an upstream CLI/bootstrap defect and does not block an inline SNS
Governance proposal executed by Root. The source-shaped NNS readiness and
hash-changing upgrade fixtures are recorded in the completed 2026-08-11
package. The thin lifecycle profile is a restart-safe tooling proof and does
not invalidate that canonical rehearsal. It requires the absolute
`sns-testing-init` `topology.json` and derives fresh allocation IDs instead of
reusing historical ephemeral principals.

Expected manual order:

1. Prepare the local SNS testing environment according to the current official ICP/DFINITY SNS testing documentation.
2. Run `./tools/sns-testing/check-prereqs.sh`.
3. Build/deploy IO dapp canisters into the local environment with `./tools/sns-testing/deploy-io-dapp-local.sh`.
4. Fill `tools/sns/sns_init.io.local.yaml` with local dapp canister IDs, fallback controller principals, and local SNS canister IDs.
5. Run `./tools/sns-testing/validate-local-sns-config.sh`.
6. Run `./tools/sns-testing/run-local-sns-testing.sh` only after the operator has reviewed the generated commands.

These scripts must not use `--network ic`, must not call mainnet, and must not start a replica inside the dapp deployment step. The existing canister that owns IO NNS neuron 6345890886899317159 is not touched.
