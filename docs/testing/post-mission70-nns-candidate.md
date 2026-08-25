# Post-Mission-70 NNS Governance candidate evidence

## Status

This is the active NNS Governance boundary evidence for the pooled
claim-backing source. It is not launch authorization, release lineage, or a
mainnet action.

## Official release lineage

NNS proposal 141441 introduced Mission 70. The subsequently executed
Governance `InstallCode` proposals inspected for this evidence include 141738,
141771, 141779, 142447, 142679, 142936, 143410, 143577, and 143660. Proposal
143660 executed on 2026-08-24 and is the latest executed Governance
`InstallCode` proposal found in the public official proposal metadata through
proposal 143685 on 2026-08-25.

The selected candidate is proposal 143660 at source commit
`c748b8e76b90ceef329c055e6f7b38a00aae8745`, following
`8aa4680e378f3248e7e7b9b8237915aded999bd9`. Its official compressed Wasm,
reproduced raw Wasm, and source Governance DID identities are respectively:

- `e4e9e99730dbee3a6fb9a95b40b10b512ad4831c9d2f6efb51d3f0a5d243b503`
- `573af1cde5bf55a5e4dbf2d47f8dd340f7a73a107eebbc645fe1202b97f61e85`
- `6e9a397f4bf0adc913980ef6c176e765534617d0ce59d52e7bcc66add2b0cd71`

The compressed proposal artifact was downloaded from the official IC release
location, decompressed, and checked against those compressed and raw hashes.
The DID at the pinned source commit matched the DID hash. The candidate lock
records the exact digest-qualified official build image and the production
`local+stamped` profile; the Governance test feature was disabled.

## Decisive controlled proofs

`exact_post_m70_upgrade_rewards_fourteen_day_boundary` installs the old
IO-pinned Governance Wasm and source-shaped NNS state, upgrades it with the
exact selected production Wasm, and exercises deterministic local XRC history.
Its serial PocketIC evidence records the following exact run values:

| Observation | Controlled value |
| --- | --- |
| Old voting threshold | 15,778,800 seconds |
| Upgraded voting threshold | 1,209,600 seconds |
| Exact neuron dissolve delay | 1,209,600 seconds |
| Below-threshold control | 1,209,599 seconds |
| Proposal/reward round | Proposal 1 / round 126 |
| Ordinary maturity before/after | 0 / 6,411,740,158,210,222 e8s |
| Direct top-up amount / fee / block | 200,000,000 e8s / 10,000 e8s / block 7 |
| Cached stake before/after | 10,000,000,000,000,000 / 10,000,000,200,000,000 e8s |
| Split children | 6,670,980,498,903,551,809; 17,951,363,335,400,986,306; 1,238,449,259,371,036,481 |
| Split gross / credited | 200,010,000 / 200,000,000 e8s |
| Inherited delay | 1,209,600 seconds for every child |
| Merge child start / parent stake before/after | 1,631,608,226 / 9,999,999,599,970,000 / 9,999,999,799,960,000 e8s |
| Parent maturity before three splits | 3,847,044,094,926,134 ordinary / 2,564,696,063,284,088 staked e8s |
| Selected child / inherited maturity | 17,951,363,335,400,986,306 / 76,944,727 ordinary / 51,296,484 staked e8s |
| Selected split / disburse-child StartDissolving / readiness | 1,631,608,226 / 1,631,608,263 / 1,632,817,863 seconds |
| Continuing child start / readiness | 1,631,608,264 / 1,632,817,864 seconds |
| Dissolving-child ordinary maturity after reward | 82,914,450 e8s |
| Early/exact disbursement | At 1,632,817,862, error type 11 reports the child still dissolving; succeeds at 1,632,817,863 in block 12 |
| Post-disbursement retained maturity | Zero cached stake / 134,210,934 ordinary / zero staked e8s |
| Zero-principal cleanup | One-second delay increase; 134,210,934 maturity merged to parent; zero ICP blocks and zero ICP fee |
| Nominal maturity | 6,411,739,901,727,798 e8s |
| Finalization delay | 604,800 seconds |
| Maturity modulation | 0 permyriad under deterministic fixture inputs |
| Actual Mint / Mint block | 6,411,739,901,727,798 e8s / block 13; spend succeeds in block 14 |

The same run proves the exact neuron receives a ballot, the one-second-below
control does not, the exact neuron casts a yes vote and receives ordinary
voting maturity, three children coexist, one stops and merges, another
disburses at the exact boundary, and the third continues independently. The
Mint increases the Stream-like staging balance by the exact Mint amount and is
spendable.

`exact_post_m70_fourteen_day_parent_follows_and_earns_maturity` is the
production-policy control: a separate proposer creates a Motion, the pooled
parent registers no manual vote, its configured leader votes yes, and the
parent ballot follows yes. The exact 14-day parent then receives nonzero
ordinary maturity. A subsequent `RefreshVotingPower` preserves the fixed
topic-0, Governance-topic, and SNS-management-topic following policy.

The selected child's split and `StartDissolving` are distinct: split creates a
non-dissolving child with the inherited delay, while the later command
establishes its canonical readiness timestamp. The child accrues additional
ordinary reward maturity while dissolving, and inherited staked maturity
converts to ordinary maturity at dissolution. Principal disbursement leaves
zero cached stake and retains all maturity. Increasing that zero-principal
child's delay by one second then permits a merge that moves all child maturity
to the pooled parent. The cleanup leaves the child empty/finalizable, creates
no ICP ledger block, and charges no ICP fee.

`exact_post_m70_minimum_stake_boundaries` records these controlled boundaries:

- Claiming against 99,999,999 e8s fails with error type 14: `Account does not
  have enough funds to stake a neuron. Please make sure that account has at
  least 100000000 e8s (was 99999999 e8s)`.
- Exactly 100,000,000 e8s creates neuron 17,047,225,741,041,935,755 with that
  exact cached stake.
- A source that pays the 10,000-e8s transfer fee separately needs exactly
  100,010,000 e8s gross to place 100,000,000 e8s in the staking account; the
  claim creates neuron 6,003,521,757,219,431,476.
- A 100,009,999-e8s split fails with error type 14 and reports the exact
  100,010,000-e8s minimum. A 100,010,000-e8s split creates neuron
  7,051,052,760,892,872,357 with exactly 100,000,000 e8s cached stake.
- The parent retains 300,010,000 e8s. Attempting to split 200,010,001 e8s,
  which would leave 99,999,999 e8s, and attempting to split its entire
  300,010,000 e8s both fail with error type 14 and the minimum-parent-stake
  explanation.

## Mechanical conclusions

- A separately endowed reward-backing fund is not mechanically required.
- A direct Stream-owned ICP transfer followed by NNS Manager refresh and
  cached-stake proof is supported.
- Candidate A--an actual maturity Mint followed by explicit restaking--is
  supported.
- `StakeMaturity` remains distinct staked-maturity state and is not ordinary
  reusable cached principal.
- Multiple passive children are mechanically supported.
- SNS-state detection latency is additional to the 14-day NNS dissolve delay.
- Exact SNS/NNS unlock alignment is not guaranteed.
- Split and `StartDissolving` are separate canonical lifecycle steps.
- A dissolving child can accrue ordinary maturity; inherited staked maturity
  converts to ordinary maturity at dissolution.
- Zero-principal child maturity can be merged completely into the pooled
  parent without an ICP ledger block or fee, leaving an empty/finalizable child.

These are upstream-mechanics conclusions only; they do not select or implement
IO's future orchestration or monetary policy.

## Component boundary

The Governance pin is independent of the maintained ICP Ledger pin. Each
component retains its own exact source revision, compressed and raw artifact
hashes, and DID hash in `nns-boundary-pin.md` and the executable manifest.

Source-supported facts are not classified as controlled-run proof.
