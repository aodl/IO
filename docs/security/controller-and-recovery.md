# Controller and recovery policy

SNS governance alone pauses or requests readiness. Pausing may occur with an active operation; permissionless resume and exact proof remain available for that operation. Governance cannot mark an effect complete.

The NNS Manager is intended to execute at the protected-neuron controller
`oae4c-3iaaa-aaaar-qb5qq-cai`. That principal is valid only for this exact
authority role; it is not a general mutation target. Mainnet inspection,
installation, upgrade, or mutation requires separate explicit approval.

Ambiguous ledger effects retry only the identical typed request inside its deduplication window. Later recovery accepts one exact matching canonical block; mismatch is non-mutating. Otherwise the protocol remains Paused pending a governed upgrade.
