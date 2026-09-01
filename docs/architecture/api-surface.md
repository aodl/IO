# Production API surface

Stream exposes narrow redemption/resume/proof, unified claim-receipt,
reward-observation/backing, lifecycle, and status methods. NNS exposes narrow
Jupiter, maturity, pooled reconciliation, resume/proof, lifecycle, status, and
claim-backing observation methods. The Stream-only claim-asset and pool-policy
observations retain their caller authorization. A separate permissionless
Dynamic-backing status update performs canonical reads and exposes only the
redacted parent partition and policy; it has no monetary effect or durable
phase.

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

The validator/update pairs registered as SNS generic functions share payloads
but have different roles. A validator is a pure, local submission-time
preflight. The update revalidates at execution time and uses a transport reject
when an authenticated governance request was not durably accepted, because the
reviewed SNS implementation treats every normal reply as execution success
without decoding the target's application-level `Result`. Typed unauthorized
responses remain part of the ordinary public API. Exact accepted work may
return `Pending` and continue through the existing permissionless resume
surface; no public governance queue or internal-choreography API is added.
