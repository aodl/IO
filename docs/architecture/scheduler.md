# Permissionless progression

Launch monetary execution has no monetary timer or scanner scheduler. A keeper calls `resume`; one invocation performs at most one external monetary or governance effect. The sole timer exception is the one-shot active-cohort deadline described in [ADR: one cohort deadline timer](adr-cohort-deadline-timer.md). It calls the same idempotent close-due transition, cannot transfer value, and is reconstructed from stable state after upgrade. The persisted typed phase, immutable intent fingerprint, operation sequence and dispatch epoch determine the only legal monetary continuation.

A recent submitted attempt returns Busy. An eligible retry reissues the identical request inside the ledger deduplication window. Expired ambiguity pauses and requires exact-block proof or a governed upgrade. Unsupported transfers create no protocol claim.
