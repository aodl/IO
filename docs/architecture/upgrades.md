# Launch upgrades

Stream and NNS value-moving state each have one explicit stable envelope containing only `V1`. Install always starts Paused. Upgrade reopens and validates the complete V1 state against the running canister principal; corrupt or mismatched state traps.

There is no prelaunch value-moving migration chain. Future post-launch migrations begin from V1. Historian retains its independent observation-state migration history.

An SNS Governance module upgrade requires a separate reviewed production
follow-up: a same-Wasm stream-manager upgrade with reviewed `UpgradeArgs` will
replace the expected Governance module hash. Post-upgrade remains Paused, and
unpause must revalidate the replacement hash, zero reward rates, daily round,
zero bonus parameters, and total-neuron maximum. This tranche exposes no
arbitrary public configuration setter.
