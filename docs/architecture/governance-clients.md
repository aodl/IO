# Governance boundaries

The stream manager authenticates SNS governance only for pause/readiness control and authenticates the configured NNS manager for liquid receipts. It does not re-prove NNS neuron internals.

The NNS manager alone submits commands concerning protected NNS neurons. Every governance effect is a typed persisted phase, and one update invocation submits at most one effect. Historian observations are advisory only.
