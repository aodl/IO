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
