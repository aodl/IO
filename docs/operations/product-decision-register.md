# IO launch product decisions

No production principal or economic quantity may be inferred from the deterministic local rehearsal. Local values are fixtures only.

| Decision | Hard technical/official constraint | Local fixture | Depends on decision | Change class |
| --- | --- | --- | --- | --- |
| Production token symbol | Pinned SNS validation accepts 3–10 characters; two-character `IO` is rejected. | `IOLO` | Final SNS init and frontend labels | Launch config/assets only, unless upstream validation changes |
| Voting threshold | Governance launch policy and reward eligibility are separate. Ordinary IO eligibility uses 1,296,060 seconds (15 days + 1 minute); NNS Dynamic children remain exactly 1,209,600 seconds. | Source CLI voting minimum remains fixture-only. | SNS parameters and eligibility explanation | Resolved product timing; structural scheduler proof is normative |
| Swap bounds/participants | Must satisfy official SNS min/max/per-participant/count relationships and treasury allocation. | Completed local swap uses deliberately non-production values. | SNS init, funding plan, audit | Launch config/economics decision |
| Neuron basket/vesting | Must fit official basket count/interval and allocation totals. | Local swap basket is test-only. | Genesis neurons, decentralization, audit | Launch config/economics decision |
| Developer/treasury distribution | Allocations must conserve configured token supply and use reviewed principals/memos/delays. | Local treasury funds reserve/user only for proof. | SNS init, reserve funding, audit | Launch config/economics decision |
| Fallback controllers | Exact principals and recovery policy must be reviewed before SNS creation. | Local development principal only. | SNS init, recovery plan, controller audit | Launch config/controller decision |
| Initial protocol reserve | Must be consistent with total supply, excluded Accounts, swap distribution, redemption policy, and audited treasury proposal. | 10,000,000,000 e8s funding fixture. | Redemption capacity, historian config, treasury proposal | Launch config/economics decision; monetary algorithm unchanged |
| Internal ICP fees | Claim-backing fees consume the excluded Dynamic anchor; permanent fees create exact permanent shortfall; reimbursement-transfer fees are fresh TwoYear maturity cost; external fees create no entitlement. | Canonical local ledger fee. | NNS/Stream fee capacity and TwoYear replenishment | Anchored fee policy is frozen by the replacement ADR |
| Account/subaccount map | Exact `Account` owners and 32-byte subaccounts must be unique where required and consistent across install args/historian config. | Reserve `01×32`, liquid `02×32`, maturity `03×32`, Jupiter `04×32`. | Both managers, ledgers, historian, audit | Launch config only; changing semantics would require code review |
| Cycles strategy | SNS Root must retain upgrade/control authority; the reserve/alert/incident policy is defined in `cycles-management.md`, while the top-up source and automation authority still need approval. | Local canisters receive test cycles. | Approved funding source and monitored operating runbook | Operations/product decision |
| Public metadata/assets | Official URL, forum URL, description, logo rights, confirmation text, restricted countries, and custom domain require approval. | Local-only names/URLs. | SNS init/frontend/legal review | Launch config/assets/legal decision |
| Jupiter Governance neuron policy | Must preserve frozen 40/60 architecture and reviewed following/controller policy. | Local deterministic neuron. | Genesis distribution and audit | Launch config/governance decision |
| Dynamic parent memo/followee/anchor | Memo `0` is the fixed deterministic staking nonce; the parent follows protected two-year neuron `10_292_412_127_977_304_661`; the excluded replenishable anchor target is exactly 10 ICP. Validation requires the followee to equal `two_year_neuron_id`. The permanent neuron's expected alpha-vote following remains subject to separately authorized mainnet verification. | Historical local memo values remain fixture-only; replacement rehearsal pre-seeds at least 10 ICP. | Production NNS-manager args/readiness, collision/dust guards, fee capacity | Resolved production configuration; no mainnet call is implied |
| Final module hashes | Official reward-share Governance release must exist; all official Wasm/DID bytes must be pinned and rerun. | Candidate Governance/Root at IC `4320fdf2e613844eabae1927b1a23b98da3a7bc6`. | SNS creation, historian expected hashes, audit | Official/upstream then reviewed pins |
| Final dapp/SNS IDs | Available only after reviewed allocation/creation. | Completed local IDs are evidence-only. | Install args, historian config, controllers, frontend | Mainnet-only observation/configuration |
| Project copyright notice | Do not invent the legal owner/name. Apache-2.0 license text is already canonical. | No local substitute. | Optional NOTICE/header attribution | Product/legal decision; no protocol effect |

The anchored replacement ADR supersedes Policy-A claim erosion, lazy parent
bootstrap, shared SNS/NNS 14-day timing, the 32-cohort product limit, and ICRC-2
pull redemption. Retained frozen decisions include reserve-transfer issuance,
`B/C` redemption, Jupiter/two-week paired 40/60, TwoYear no-IO issuance, daily
reward policy, sticky irreversible-effect recovery, following/voting-power
maintenance, ambiguity proofs, zero native SNS reward rates, 86,400-second
reward rounds, and zero SNS dissolve-delay/age bonuses.
