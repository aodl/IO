# Testflight Upgrade Proposal Template

Manual/mainnet only. Not CI. Not a real launch. No real swap.

Use this template to prepare an SNS-governed dapp canister upgrade proposal during testflight.

Target canister:

- `io_stream_manager`
- `io_nns_neuron_manager`
- `io_historian`
- `frontend`

Required artifact references:

- manifest: `release-artifacts/manifest.json`
- raw Wasm path: `TODO_TESTFLIGHT_RAW_WASM_PATH`
- raw Wasm SHA-256: `TODO_TESTFLIGHT_RAW_WASM_SHA256`
- gzipped Wasm SHA-256: `TODO_TESTFLIGHT_GZ_WASM_SHA256`
- git commit: `TODO_TESTFLIGHT_GIT_COMMIT`

Proposal fields:

- title: `TODO_TESTFLIGHT_UPGRADE_TITLE`
- summary: `TODO_TESTFLIGHT_UPGRADE_SUMMARY`
- forum URL: `TODO_FINAL_SNS_PROPOSAL_FORUM_URL`
- target canister ID: `TODO_TESTFLIGHT_TARGET_CANISTER_ID`

Verification:

- Confirm the target canister is registered with SNS root.
- Confirm the proposal references the manifest entry for the exact target canister.
- Confirm the observed module hash changes after execution.
- Confirm stable pending journal state survives upgrade for value-moving canisters.
- Confirm production DIDs for value-moving canisters remain `service : (InitArgs) -> {}`.
