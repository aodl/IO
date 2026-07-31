# Launch upgrades

Stream and NNS value-moving state each have one explicit stable envelope containing only `V1`. Install always starts Paused. Upgrade reopens and validates the complete V1 state against the running canister principal; corrupt or mismatched state traps.

There is no prelaunch value-moving migration chain. Future post-launch migrations begin from V1. Historian retains its independent observation-state migration history.
