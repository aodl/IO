# Simplified launch threat model

| Threat | Control |
|---|---|
| Unauthorized intent | Exact caller authority or exact canonical block proof |
| Double redemption | Per-caller nonce plus request fingerprint |
| Async stale callback | Operation sequence, kind, phase, transfer fingerprint and dispatch epoch must all match |
| Double external effect | One persisted effect phase per update invocation |
| Ambiguous transfer | Identical retry inside deduplication window, then Paused and exact-block proof |
| Balance drift | Canonical pre/post balances with conservative inequalities |
| Historian compromise | Historian state is never monetary input or completion authority |
| Unsupported transfer | Creates no protocol claim and receives no automatic refund |
| Upgrade corruption | V1 envelope decode and complete self-bound validation trap |

Production activation and mainnet operations remain unavailable.
