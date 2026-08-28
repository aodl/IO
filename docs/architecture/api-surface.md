# Production API surface

Stream exposes narrow redemption/resume/proof, unified claim-receipt,
reward-observation/backing, lifecycle, and status methods. NNS exposes narrow
Jupiter, maturity, pooled reconciliation, resume/proof, lifecycle, status, and
claim-backing observation methods.

Callers never choose a monetary destination, parent memo, followee, neuron, or
transfer amount. Permissionless `resume` calls can wake already-defined maturity
work and read the relevant semantic staging balance, but callers provide no Mint
block or source identity. External proof arguments remain only for genuinely
ambiguous outgoing transfers. Production DIDs exclude ticks, forced outcomes,
state dumps, generic voting, and debug methods.

Public progress describes real caller action and blocking boundaries rather
than durable internal choreography. Stream redemption and NNS Jupiter/maturity
flows expose `Pending`, `Completed`, and `Stuck`; unwind additionally exposes
`AwaitingTransferProof`. Claim receipts retain `AwaitingLiquidProof` because it
carries the exact cross-canister permit, while bounded recipient settlement is
coarse `Pending`. Detailed phase names remain diagnostic status text and are
not workflow compatibility types.
