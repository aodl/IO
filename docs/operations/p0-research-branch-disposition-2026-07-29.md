# P0 Research Branch Disposition

This record freezes `p0-monetary-safety-foundation` at `63f47cde49a1b7a5465765765e067c8e1698ba1b` as research scaffolding.

It is not a merge candidate and must not receive additional corrective commits. Sound decisions from this branch may be reimplemented or selectively transplanted onto `p0-monetary-safety-foundation-v2`, created from the repaired `production-readiness-truth-pass` head.

Verified ancestry:

- `production-readiness-truth-pass` reference: `d9fb7dc12e2458fc17faa9291adf1ee57388d7ea`
- research reference: `63f47cde49a1b7a5465765765e067c8e1698ba1b`
- `d9fb7dc12e2458fc17faa9291adf1ee57388d7ea` is an ancestor of `63f47cde49a1b7a5465765765e067c8e1698ba1b`

Complete research-only history after `d9fb7dc`:

```text
202f8cc docs: accept SNS fee burn and canonical supply authority
5dab3c8 test: automate maintained official local SNS rehearsal
9aae617 fix: reconcile fee-burn supply and reserve accounting
39e9cb4 refactor: unify durable monetary transfer attempts
7cb553a feat: add canonical ledger and archive transfer proof
2c6c529 fix: harden exact-account issuance and redemption settlement
4461148 fix: unify wiring and gate execution on canonical reconciliation
ffb479e docs: record P0 monetary safety evidence
71a3290 test: align fee-burn wiring validation
9bde2a9 test: align zero-liquid stream fixtures
b175ea9 test: align zero-liquid pocketic fixture
63f47cd test: align production wiring fixture fee mode
```
