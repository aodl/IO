# Permissionless progression

Launch monetary execution has no timer or scanner scheduler. A keeper calls `resume`; one invocation performs at most one external monetary or governance effect. The persisted typed phase, immutable intent fingerprint, operation sequence and dispatch epoch determine the only legal continuation.

A recent submitted attempt returns Busy. An eligible retry reissues the identical request inside the ledger deduplication window. Expired ambiguity pauses and requires exact-block proof or a governed upgrade. Unsupported transfers create no protocol claim.
