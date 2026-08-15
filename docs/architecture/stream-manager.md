# Stream manager

The stream manager owns the IO reserve, liquid ICP Account, direct ICRC-2
redemption, proof-bound liquid receipts, daily reward-entitlement observation,
one immutable pending entitlement batch, and serialized recipient settlement.

Daily observation has no external value effect. It verifies the exact SNS Root,
Governance principal, reviewed module hash, zero native reward rates, 86,400
second round duration, and approved zero voting-power bonus parameters. It then
reads one stable reward-event boundary around paginated neuron reads and commits
the event's canonical weights in one state mutation after rechecking the durable
checkpoint. A stale callback mutates nothing.

One transient one-shot timer marks reward work due and calls the same idempotent
method available to permissionless keepers. Failures leave work due. Successful
processing schedules the next observation. There is no interval timer, retry
scheduler, proposal timer, task queue, or event archive.

Backing is asynchronous. One live accumulator can continue receiving daily
weights while one frozen batch moves through the two-week-staker reward-backing
NNS 40/60 maturity path, actual ICP receipt, and sequential IO transfers. A second pending
batch is not created. Missing reward events add no credits, advance only through
a typed skip record, and leave undistributed backing in reserve.

Redemption pulls IO from the authenticated caller Account directly to reserve.
A separate `resume` pays ICP and later verifies postconditions. Reward
observation never occupies the redemption operation slot, so governance
availability and payout delays do not block redemption.

Install and post-upgrade state are Paused. Reviewed unpause is required before
the one-shot observation timer is installed. IO remains inert and prelaunch.
