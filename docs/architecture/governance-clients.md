# Governance boundaries

The Stream Manager authenticates SNS governance for lifecycle control and
canonical reward observations, and authenticates the configured NNS Manager
for exact backing inflows. It never accepts caller-supplied monetary facts.

The NNS Manager alone submits commands for the permanent neuron, pooled
exact-14-day parent, and bounded passive unwind children. The parent has one
fixed configured following policy; readiness verifies it. Daily pool-policy
observation independently attempts best-effort voting-power refresh for the
permanent neuron and pooled parent without another timer. Refresh failure does
not invalidate policy observation or gate monetary work. Every potentially
irreversible Governance effect has a typed persisted immutable intent before
submission. Definite success is immediately re-observed once and may continue
to the next proved fixed step; ambiguity or a missing postcondition stops
dependent work. Historian observations are advisory only.

Production pins that parent policy to permanent IO two-year neuron
`10_292_412_127_977_304_661`, never alpha-vote directly. The permanent neuron
is recorded and operationally expected to follow alpha-vote neuron
`2_947_465_672_511_369`. This remains subject to separately authorized mainnet
verification; IO code does not change the permanent neuron's followees.
