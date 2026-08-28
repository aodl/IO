# Governance boundaries

The Stream Manager authenticates SNS governance for lifecycle control and
canonical reward observations, and authenticates the configured NNS Manager
for exact backing inflows. It never accepts caller-supplied monetary facts.

The NNS Manager alone submits commands for the permanent neuron, pooled
exact-14-day parent, and bounded passive unwind children. The parent has one
fixed configured following policy; readiness verifies it and daily
reconciliation refreshes voting power without another timer. Every governance
effect is a typed persisted phase, and one update submits at most one effect.
Historian observations are advisory only.

Production pins that parent policy to permanent IO two-year neuron
`10_292_412_127_977_304_661`, never alpha-vote directly. The permanent neuron
is operationally expected to retain its separately audited following of
alpha-vote neuron `2_947_465_672_511_369`; IO code does not change the
permanent neuron's followees.
