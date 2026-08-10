# IO launch product decisions

No value in this register may be inferred from the deterministic local rehearsal. Local values are test fixtures only.

| Decision | Status | Required resolution |
| --- | --- | --- |
| Final description, logo, project URL and forum URL | NEEDS USER DECISION | Approved public assets and URLs. |
| Final voting-power economics | NEEDS USER DECISION | Final bonus, voting period, proposal and following policy. |
| Minimum voting delay `1,209,600` versus intentional `1,209,599` | NEEDS USER DECISION | Explicit governance choice; local source tooling accepted `1,209,599`. Ordinary IO reward eligibility remains exactly `1,209,600` and is a separate invariant. |
| Swap minimum, maximum, minimum participant count and per-participant limits | NEEDS USER DECISION | Final audited values. |
| Neuron basket and vesting schedule | NEEDS USER DECISION | Final basket count, interval and vesting. |
| Confirmation text and restricted countries | NEEDS USER DECISION | Legal/product-approved text and list. |
| Developer neurons and treasury distribution | NEEDS USER DECISION | Principals, stakes, memos, dissolve delays and allocations. |
| Jupiter Governance neuron and other-neuron policy | NEEDS USER DECISION | Exact neuron/controller/following policy. |
| Fallback controllers | NEEDS USER DECISION | Final principals and recovery policy. |
| Initial protocol reserve | NEEDS USER DECISION | Amount and treasury proposal policy. |
| Two-week-staker reward-backing NNS neuron ID and seeded principal | MAINNET AUDIT REQUIRED | Audit the existing protected position under separately authorized mainnet work; do not infer from local fixtures. |
| Final IO/ICP fee floats | NEEDS USER DECISION | Audited amounts consistent with final ledger fees. |
| Final Account/subaccount mapping | NEEDS USER DECISION | Exact owners and 32-byte subaccounts for reserve, liquid ICP, maturity staging and Jupiter. |
| Final Governance and module hashes | EXTERNAL | Official candidate adoption/release first, followed by reviewed pin updates. |
| Final dapp and SNS canister IDs | MAINNET AUDIT REQUIRED | Allocated/observed only during separately authorized launch work. |
| Candidate reward-share field | EXTERNAL | Official DFINITY review and mutually compatible SNS release. |
| Production token symbol | NEEDS USER DECISION | The pinned DFINITY SNS implementation accepts symbols of 3–10 characters, so two-character `IO` is not valid. Choose a valid 3–10 character production symbol or obtain and adopt an upstream SNS validation change. `IOLO` is only a local fixture and does not decide the production symbol. |

Decisions already fixed by architecture are not reopened here: native SNS reward rates are zero, reward rounds are 86,400 seconds, dissolve-delay and age bonuses are zero for the accepted IO reward policy, ordinary reward eligibility is exact 1,209,600-second non-dissolving, protected NNS delay is 252,460,800 seconds, two-year maturity issues no IO, and Jupiter is exact 40/60.
