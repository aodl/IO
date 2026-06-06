# IO SNS Testflight Package

This package is for a future manual mainnet SNS testflight. It is not CI, not a real launch, has no real swap, and is not production launch configuration.

Testflight proves that IO operators can run post-decentralization workflows against a mock SNS:

- deploy and configure IO dapp canisters for the testflight environment;
- deploy a mock SNS from a reviewed testflight `sns_init.yaml`;
- add SNS root as a co-controller while retaining recovery control;
- register `io_stream_manager`, `io_nns_neuron_manager`, `io_historian`, and `frontend` as dapp canisters;
- submit and execute SNS-governed upgrade proposals;
- verify proposal artifacts against `release-artifacts/manifest.json`;
- verify the frontend and historian use testflight canister IDs;
- verify SNS root controls the intended dapp canisters.

Testflight does not prove the NNS proposal path, SNS-W deployment, decentralization swap, final tokenomics, or final production handoff.

Manual dangerous mainnet actions must be reviewed outside repo automation. Required scripts must not execute those actions. The existing canister that owns IO NNS neuron 6345890886899317159 is not touched.

Files:

- `sns_init.testflight.template.yaml`: manual testflight config template with placeholders.
- `proposal-checklist.md`: proposal readiness checklist.
- `upgrade-proposal-template.md`: upgrade proposal preparation template.
- `fallback-controller-handoff.md`: recovery and controller handoff notes.
