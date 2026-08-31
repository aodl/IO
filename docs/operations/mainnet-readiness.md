# Mainnet readiness

IO is not live. Production activation, mainnet calls, controllers, funding and
identities remain outside this local release closure.

## Current local release

The completed anchored-dynamic release is bound to:

- source-finalization commit
  `b6a26f223b4c37f021ea398f3003b4d149683ee9`;
- immediate artifact-recording commit
  `af7e0791384808b1f8304e7048b86ade9a328306`;
- canonical evidence commit
  `dd0de91ec1388da13da9c4598b9a4df82f894cca`;
- schema-v2 package
  `deploy/local-sns-rehearsal/evidence/2026-08-31-b6a26f2-anchored-dynamic`;
- release-manifest SHA-256
  `70e843404812cd0955b20ccb586e8a4760990c944146a55708ebb00649600fd2`;
- package-manifest SHA-256
  `4e1eacb9d94c381dd93161243e8405df6021ef05639595518cb5367561554a29`;
- package `SHA256SUMS` SHA-256
  `61b727deca1e8c98b1435ca333c0b88b7e81b30ce7d981ca8d208c2a8bba173b`.

Local source validation, repeated exact-source reproducible builds, and a
fresh canonical Layer A/B/C rehearsal are complete for this exact pair. Final
local release gates and exact-head hosted workflows remain required. Every
earlier package remains immutable historical evidence for its own release and
was not rebound.

The exact `b6a26f2` → `af7e079` → `dd0de91` history remains immutable historical
evidence. A machine-proved zero-tree ancestry reconciliation is a provenance
operation only and must precede a new source-finalization, immediate artifact
child and fresh evidence package; it cannot remain inside the selected release
tail. Content-bearing base merges require normal source review. After hosted
exact-head CI and explicit integration authorization, integration remains a
direct fast-forward only, without squash or rebase.

## Remaining launch blockers

Local completion does not establish mainnet readiness. Launch remains blocked
on:

- an officially reviewed SNS release containing
  `latest_reward_event_participation`, with refreshed official Wasm and DID
  pins;
- final production SNS/tokenomics/configuration values and identities;
- hosted test, security and reproducible-build CI on the exact documentation
  tail head;
- independent external security review;
- separately authorized review of protected/mainnet positions where required;
- mainnet testflight; and
- production controller, cycles, domain and funding work.

Production templates deliberately retain unresolved values and cannot be used
as install arguments.

The source roles are `io_stream_manager`, `io_nns_neuron_manager`,
`io_historian`, and `io_frontend`; naming them here records wiring identity and
does not activate any canister.
Their reserved production IDs are `thset-pqaaa-aaaar-qb7wa-cai`,
`oae4c-3iaaa-aaaar-qb5qq-cai`, `tjqj3-uaaaa-aaaar-qb7xa-cai`, and
`torpp-zyaaa-aaaar-qb7xq-cai`, respectively.

The NNS Manager execution identity is the existing protected-neuron controller
`oae4c-3iaaa-aaaar-qb5qq-cai`. The permanent neuron remains protected input.
The Dynamic 14-day IO neuron exists before Ready. Its production memo is `0`,
used only as the deterministic staking nonce, and it follows permanent IO
two-year neuron
`10_292_412_127_977_304_661`. The permanent neuron is recorded and
operationally expected to follow alpha-vote neuron
`2_947_465_672_511_369`; IO does not change that policy. This remains subject
to separately authorized mainnet verification. Bootstrap requires at least the
10 ICP anchor, rejects a permanent-Account collision, excludes the anchor from
claim backing, and classifies any positive unexplained residual as excluded
surplus without creating IO or increasing the replenishment entitlement.

Pool-policy observation makes independent best-effort voting-power
refresh attempts for the permanent neuron and pooled parent. Timestamp age and
refresh failure are advisory governance-maintenance facts, not monetary
readiness, reconciliation, maturity, issuance, or redemption gates. Stream uses
one derived earliest-deadline one-shot scheduler for 12-hour structural work,
daily reward observation, and 60-second recovery. NNS Manager uses one derived
recovery/ready-child wake; neither adds a stable timer field or monetary slot.
IO never changes the permanent neuron's followees.

The selected package is current local authority for the anchored-dynamic
release. Historical local rehearsal and release evidence remains immutable and
is not reinterpreted as proof of this release.

Local rehearsal uses a real SNS-created stack installed through SNS-W. Its
`IO_TEST` ledger and protocol reserve are non-canonical local evidence; IO is
not launched on mainnet.

Historian remains a rebuildable public read model. It is not canonical protocol truth
and not a value-moving authority. The IO protocol is not live and the
SNS IO ledger remains not launched. The missing/stale/error observations remain
explicit inputs to freshness, as do the index canisters.
