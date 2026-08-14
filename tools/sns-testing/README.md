# Optional Local SNS Testing

This directory contains optional local-only helpers for the maintained official
local SNS rehearsal. These scripts are not required CI and are not used by
`verify_release`. Historical packages remain immutable evidence for their
recorded releases; `current-canonical.toml` is the source of truth for the one
package selected for the current release. Local fixture and candidate-upstream
evidence does not prove that an official capability-bearing SNS release exists
or that IO is ready for mainnet.

Follow the current official ICP/DFINITY SNS testing documentation as the source of truth. The historical standalone `dfinity/sns-testing` repository is deprecated; if the official docs reference successor tooling or a new repository/location, use that current official location.

The maintained path uses the source-built `sns-testing-init`, `sns-testing` and `sns` binaries from `rs/sns/testing` and `rs/sns/cli`; it does not depend on the dfx SNS extension.

The source flow has reached SNS-W deployment, swap finalization, reserve funding,
ledger/index evidence, controller handoff, and authentic same-source candidate
Governance/Root upgrade execution. The one-component candidate-Governance with
official-Root wire mismatch is historical. The local chunk-store authorization
gap is an upstream CLI/bootstrap defect and does not block an inline SNS
Governance proposal executed by Root. The source-shaped NNS readiness and
hash-changing upgrade fixtures are recorded in immutable completed packages.
The thin lifecycle profile and later full rehearsals use fresh topologies and
never rebind earlier evidence. A package is current only when the explicit
`deploy/local-sns-rehearsal/evidence/current-canonical.toml` selector binds it
to the repository's recorded source/artifact lineage; local package names and
fixture values are not production configuration. Each run requires the
absolute `sns-testing-init` `topology.json` and derives fresh allocation IDs
instead of reusing historical ephemeral principals.

Expected manual order:

1. Prepare the local SNS testing environment according to the current official ICP/DFINITY SNS testing documentation.
2. Run `./tools/sns-testing/check-prereqs.sh`.
3. Build/deploy IO dapp canisters into the local environment with `./tools/sns-testing/deploy-io-dapp-local.sh`.
4. Fill `tools/sns/sns_init.io.local.yaml` with local dapp canister IDs, fallback controller principals, and local SNS canister IDs.
5. Run `./tools/sns-testing/validate-local-sns-config.sh`.
6. Run `./tools/sns-testing/run-local-sns-testing.sh` only after the operator has reviewed the generated commands.

These scripts must not use `--network ic`, must not call mainnet, and must not
start a replica inside the dapp deployment step. NNS Manager execution canister
`oae4c-3iaaa-aaaar-qb5qq-cai` and protected IO NNS neuron
`10292412127977304661` are not touched.
