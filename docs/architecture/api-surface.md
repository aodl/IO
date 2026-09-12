# Production API surface

Stream exposes narrow redemption/resume/proof, unified claim-receipt,
reward-observation/backing, and status methods. NNS exposes narrow
Jupiter, Stream-bound two-week maturity, pooled reconciliation, resume/proof, status, and
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
flows expose `Pending`, `Completed`, and `Stuck`; the no-argument redemption
wake returns only local acceptance and no global completion. Unwind additionally exposes
`AwaitingTransferProof`. Claim receipts retain `AwaitingLiquidProof` because it
carries the exact cross-canister permit, while bounded recipient settlement is
coarse `Pending`. Detailed phase names remain diagnostic status text and are
not workflow compatibility types.

Routine IO execution has no SNS generic-function validator/update pairs.
Install and upgrade timers recover persisted work and revalidate readiness
automatically. Exact accepted work may return `Pending` and continue through
the permissionless deterministic resume surface. Stream's rare exact-block
proof is also caller-independent: the complete persisted transfer intent and
canonical ledger block determine acceptance. Authority checks remain on calls
that create new cross-canister economic intent. No public governance queue or
internal-choreography API is added.
