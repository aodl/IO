# Mainnet readiness

IO is not live. Production activation, mainnet calls, controllers, funding and
identities remain outside this local release closure.

## Current local release

The completed account-semantic release is bound to:

- source-finalization commit
  `9462a1a0df602f06fa845bd31f9fcd0adf80067a`;
- immediate artifact-recording commit
  `46713f8499cf9a63a6cd4879b1fff9c1f9ef0be5`;
- canonical evidence commit
  `1ed1399130358ed5788cae99f3d65d82cbbc70a9`;
- schema-v2 package
  `deploy/local-sns-rehearsal/evidence/2026-08-26-9462a1a-account-semantic`;
- release-manifest SHA-256
  `011113d83510e66976f5d3eabefc57ef30ba44fc49a8973fad29be043b374431`;
- package-manifest SHA-256
  `254e8866394d62ba9fbe2b0290709d2c9dbaeb8ed84c8b3446b9a1912af0eb78`;
- package `SHA256SUMS` SHA-256
  `a1915cef319b569955c5d75ad88c4a3715af8483573d2cf9b3a2af108d6bea29`.

Local source validation, repeated exact-source reproducible builds, canonical
account-semantic evidence, `test_ci`, and `verify_release` are complete. The
dated packages that precede the selected package remain immutable historical
evidence for their own releases and were not rebound.

The pooled-parent identity hardening and the subsequent execution/API
simplification postdate that completed release. Their validation is local
source evidence only: the checked-in artifacts and selected package have not
been regenerated or rebound to this source tail. Before launch, finalize a new
exact source/artifact pair and obtain fresh canonical rehearsal evidence for
that pair through the normal release process.

The exact `9462a1a` → `46713f8` → `1ed1399` history must remain intact. After
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
- fresh source/artifact finalization and canonical rehearsal evidence for the
  pooled-parent hardening and execution/API simplification tail;
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
The pooled parent does not exist until canonical liquid backing reaches the
minimum and the reviewed bootstrap protocol creates it with an exact 14-day
delay and fixed following policy. Its production memo is `0`, used only as the
deterministic staking nonce, and it follows permanent IO two-year neuron
`10_292_412_127_977_304_661`. The permanent neuron is recorded and
operationally expected to follow alpha-vote neuron
`2_947_465_672_511_369`; IO does not change that policy. This remains subject
to separately authorized mainnet verification. Readiness rejects a
memo-derived Account collision before bootstrap. Unsolicited ICP at a distinct
candidate Account is treated as unattributed pooled surplus and ordinary
reconciliation unwinds any `OverTarget` amount.

Daily pool-policy observation makes independent best-effort voting-power
refresh attempts for the permanent neuron and pooled parent. Timestamp age and
refresh failure are advisory governance-maintenance facts, not monetary
readiness, reconciliation, maturity, issuance, or redemption gates. No
additional timer or stable scheduler exists, and IO never changes the
permanent neuron's followees.

The selected package is current local authority for the account-semantic
release. Historical local rehearsal and release evidence remains immutable and
is not reinterpreted as proof of this release.

Local rehearsal uses a real SNS-created stack installed through SNS-W. Its
`IO_TEST` ledger and protocol reserve are non-canonical local evidence; IO is
not launched on mainnet.

Historian remains a rebuildable public read model. It is not canonical protocol truth
and not a value-moving authority. The IO protocol is not live and the
SNS IO ledger remains not launched. The missing/stale/error observations remain
explicit inputs to freshness, as do the index canisters.
