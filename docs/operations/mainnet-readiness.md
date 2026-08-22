# Mainnet readiness

IO is not live. Production activation, mainnet calls, controllers, funding,
identities, release artifacts, and canonical evidence are outside this source
tranche.

The source implements pooled claim backing, but launch remains blocked on final
production SNS identities, the pooled-parent memo and followee, fresh
source/artifact lineage, fresh corrected local-SNS evidence, hosted exact-head
CI, mainnet testflight, controllers/cycles/domain work, and independent review.
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
delay and fixed following policy.

Historical local rehearsal and release evidence describes the superseded
economics and remains immutable. Readiness must report that corrected-economics
evidence is missing; it must never reinterpret old evidence as proof of the
pooled model.

Local rehearsal uses a real SNS-created stack installed through SNS-W. Its
`IO_TEST` ledger and protocol reserve are non-canonical local evidence; IO is
not launched on mainnet.

Historian remains a rebuildable public read model. It is not canonical protocol truth
and not a value-moving authority. The IO protocol is not live and the
SNS IO ledger remains not launched. The missing/stale/error observations remain
explicit inputs to freshness, as do the index canisters.
