# Governance boundaries

The stream manager authenticates SNS governance only for pause/readiness control and authenticates the configured NNS manager for liquid receipts. It does not re-prove NNS neuron internals.

The NNS Manager alone submits commands concerning the two-year protected NNS neuron and the distinct two-week reward-backing NNS neuron. Every governance effect is a typed persisted phase, and one update invocation submits at most one effect. Historian observations are advisory only.
