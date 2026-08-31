# Mainnet readiness

IO is not live. Production activation, mainnet calls, controllers, funding and
identities remain outside this local release closure.

## Current local release

The completed anchored-dynamic release is bound to:

- source-finalization commit
  `92113419eb2eecacd2d4064b30e091c644c4cb71`;
- immediate artifact-recording commit
  `3f03ae7e9a3b85ee7824f50375ec58e70b9e0879`;
- canonical evidence commit
  `3de6d473504ed418aa28408c94e6e08bd23521ec`;
- schema-v2 package
  `deploy/local-sns-rehearsal/evidence/2026-08-31-9211341-anchored-dynamic`;
- release-manifest SHA-256
  `6315bf85b944c36d90d8dd22c375618db3566150bb56c23d24ae74b033bedfa1`;
- package-manifest SHA-256
  `35fbc6a145f18af1bc33d83a43abd5071869ff4285f857e7d48d5d5118acf5dc`;
- package `SHA256SUMS` SHA-256
  `9e4a45920a869b900c86913ff1288b12ab3aadb059ab17c2cd49466fce044750`.

Local source validation, repeated exact-source reproducible builds, and a
fresh canonical Layer A/B/C rehearsal are complete for this exact pair. Final
local release gates and exact-head hosted workflows remain required. Every
earlier package remains immutable historical evidence for its own release and
was not rebound.

The exact `9211341` → `3f03ae7` → `3de6d47` history must remain intact. After
hosted exact-head CI and explicit integration authorization, use a direct
fast-forward only; do not squash, rebase or create a merge commit for this
release tail.

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
