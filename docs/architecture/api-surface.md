# Production API surface

Stream exposes narrow redemption/resume/proof, Jupiter receipt, backing-inflow,
reward observation/backing, lifecycle, and status methods. NNS exposes narrow
Jupiter, maturity, pooled reconciliation, resume/proof, lifecycle, status, and
claim-backing observation methods.

Callers never choose a monetary destination, parent memo, followee, neuron, or
transfer amount. Permissionless calls can wake already-defined work but provide
no monetary facts. Production DIDs exclude ticks, forced outcomes, state dumps,
generic voting, and debug methods.
