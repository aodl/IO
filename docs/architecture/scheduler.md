# Permissionless progression

Launch monetary execution has no monetary timer or scanner scheduler. A keeper
calls `resume`; fixed-size work may perform sequential external effects only
after each earlier immutable intent is persisted and its success is
canonically proved. Ambiguity or a missing postcondition stops dependent work.
Variable recipient and cohort fan-out remains bounded per invocation. The sole
timer exception is one daily reward-event
observation timer. It sets durable reward work due and calls the same
permissionless, idempotent `resume_reward_work` path available to keepers. The
observation cannot transfer value, remains due after retryable failure, and
installs its next one-shot deadline only after consuming or explicitly skipping
one canonical event. Reviewed unpause reconstructs at most one timer after an
upgrade; post-upgrade state remains Paused until then.

The persisted typed monetary phase, immutable intent fingerprint, operation
sequence and dispatch epoch determine the only legal monetary continuation.
Persisting a checkpoint is a recovery boundary, not by itself a reason to
return `Pending`.

A recent submitted attempt returns Busy. An eligible retry reissues the identical request inside the ledger deduplication window. Expired ambiguity pauses and requires exact-block proof or a governed upgrade. Unsupported transfers create no protocol claim.
