# Controller and recovery policy

SNS governance alone pauses or requests readiness. Pausing may occur with an
active operation. Non-redemption flows retain their existing flow-specific
resume authorization. An active Stream redemption requires SNS Governance for
both `resume` and exact proof. Permissionless `process_redemptions` is only an
external-call-free wake hint; it is not a redemption recovery authority.
Governance cannot choose transfer facts or mark an unmatched effect complete.

The NNS Manager is intended to execute at the protected-neuron controller
`oae4c-3iaaa-aaaar-qb5qq-cai`. That principal is valid only for this exact
authority role; it is not a general mutation target. Mainnet inspection,
installation, upgrade, or mutation requires separate explicit approval.

Ambiguous ledger effects retry only the identical typed request inside its deduplication window. Later recovery accepts one exact matching canonical block; mismatch is non-mutating. Otherwise the protocol remains Paused pending a governed upgrade.
