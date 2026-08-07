# ADR: Simplicity budget recalibration

- Status: accepted
- Date: 2026-08-07
- Baseline commit: `d6bf55012e9abf4c5e79c18510c114eb48c3273f`

## Context

The prior 10,500-line combined production-Rust ceiling was derived from an
incomplete component list. It omitted production-reachable code and therefore
could report an apparent simplification when code merely moved into a newly
reachable boundary. Applying the complete counting definition to the frozen
pre-feature commit gives an honest baseline of 10,449 production lines.

The launch reward design adds 601 lines over that corrected baseline: 163 in
the stream manager, 440 in the SNS reward boundary, and 6 in the ledger
boundary, offset by an 8-line reduction in reward policy. The narrow SNS reward
boundary consumes canonical Governance reward shares. It replaces local
proposal and ballot reconstruction and avoids making the broad
`io-governance-types` crate a production dependency.

The resulting design retains one top-level stream operation slot and has no
proposal scanner, ballot archive, maturity entitlement signal, old reward path,
generic journal, or generic liability engine.

## Decision

The one-time combined production-Rust ceiling is 11,100 lines. This decision
does not change any component or per-file limit and does not authorize a new
production abstraction merely because the accepted total has 50 lines of
headroom at the reviewed checkpoint.

The combined count includes all production Rust reachable from the two
value-moving canisters in these components:

- stream manager;
- NNS manager;
- `io-accounts`;
- ledger boundary;
- SNS reward boundary;
- economics;
- reward policy;
- NNS types;
- receipt types.

The definition may not be weakened, narrowed, or otherwise changed to pass the
gate. Every later increase requires a separate accepted complexity-exception
ADR that records the complete component delta and demonstrates deletion-first
work.

## Consequences

The corrected baseline remains 10,449 lines and is the comparison point for
future monetary work. The accepted ceiling is a guardrail, not an allocation:
new code must still be justified by protocol behavior and must delete replaced
paths. Moving code between counted components does not reduce combined
complexity.

This decision does not activate production. IO remains Paused, inert and not
live, and no deployment, controller, identity, funding or mainnet operation is
authorized by this ADR.
