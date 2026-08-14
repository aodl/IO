# Fallback Controller Handoff

Manual/mainnet only. Not CI. Not a real launch. No real swap.

Fallback controllers are unresolved production decisions. This document records the handoff questions that must be answered before testflight and before any real SNS launch proposal.

Required decisions:

- final fallback controller principals;
- emergency process for rotating fallback controllers;
- recovery controller retained during testflight;
- criteria for removing developer recovery control;
- SNS root registration proposal sequence;
- rollback plan for testflight only.

Checks:

- Verify every dapp canister lists the expected fallback or recovery principal before testflight.
- Verify SNS root is added as co-controller before registration proposals.
- Verify SNS root controls the intended dapp canisters after registration.
- Verify no procedure mutates NNS Manager execution canister
  `oae4c-3iaaa-aaaar-qb5qq-cai` or protected IO NNS neuron
  `10292412127977304661`.
- Verify no required script executes mainnet commands.
