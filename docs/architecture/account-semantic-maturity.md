# Account-semantic maturity and proof budget

## Status

Accepted for the pre-launch source state.

## Custody rule

Fungible ICP already held by IO is classified by its current protocol-controlled
Account, not by upstream provenance. The NNS Manager owns two deterministic,
domain-separated subaccounts:

- `two_week_maturity_staging()`: paired backing for a frozen entitlement batch;
- `two_year_maturity_staging()`: unpaired protocol yield.

Before `DisburseMaturity(100%)`, the manager persists the relevant balance.
After the canonical finalization boundary it freezes the positive balance delta
once. Donations received between baseline and capture inherit the Account's
semantics. Later receipts remain outside the frozen operation. The two Accounts
cannot satisfy each other's work.

## Economics

For captured ICP `M`, one checked production function computes:

```text
permanent_gross = floor(M * 40 / 100)
claim_gross = M - permanent_gross
permanent_credit = permanent_gross - permanent_fee
claim_credit = claim_gross - claim_fee
```

The two gross debits sum exactly to `M`. Jupiter and two-week maturity are paired
inflows: Stream freezes the pre-inflow `B/C` economics before `claim_credit`
becomes redeemable and releases at most `floor(claim_credit * C0 / B0)` IO.
Jupiter selects its configured recipient; two-week maturity selects one frozen
entitlement generation. Two-year maturity issues no IO and needs no paired
receipt; its claim credit is ordinary liquid yield. Every claim increment enters
liquid before ordinary pool reconciliation.

## Proof budget

IO proves ambiguous irreversible effects, not fungible-asset provenance after
custody. Exact outgoing Ledger transfer recovery, redemption replay, Jupiter
authorization, parent cached-stake reflection, NNS Split recovery, child
Disburse recovery, entitlement-generation binding, bounded recipient settlement,
and strict same-schema upgrade validation remain. Without those mechanisms an
ambiguous retry could duplicate value, create a second child, settle a different
recipient set, or decode an incompatible monetary state.

Maturity Mint proof, Mint source attribution, cross-kind block replay, maturity
source-operation identity, receipt source Account/block fields, broad receipt
fingerprints, and two-year paired-receipt state do not remain. Canonical Account
balance, semantic Account identity, exact operation sequence/amount/kind, and the
outgoing-effect proofs supply all required safety facts.

Claim backing remains:

```text
B = L + P + U + T
```

where permanent capital is excluded and paired claim credit remains quarantined
until Stream has frozen its matching IO obligation.
