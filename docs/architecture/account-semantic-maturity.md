# Account-semantic maturity and proof budget

## Status

Accepted for the pre-launch source state.

## Custody rule

Fungible ICP already held by IO is classified by its current protocol-controlled
Account, not by upstream provenance. The NNS Manager owns two deterministic,
domain-separated subaccounts:

- `two_week_maturity_staging()`: paired backing for a frozen entitlement batch;
- `two_year_maturity_staging()`: unpaired protocol yield.

After `DisburseMaturity(100%)` passes the canonical finalization boundary, the
manager freezes the complete positive balance of the relevant Account once.
Delivery debits exactly the frozen capture. Donations present before capture
are included, while value arriving after capture remains in the Account and is
consumed by the next operation together with its later maturity receipt. The
two Accounts cannot satisfy each other's work.

## Economics

For Jupiter and TwoWeek captured ICP `M`, one checked production function
computes the ordinary gross legs:

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
receipt. It does not apply 40/60 to the complete capture immediately. In
priority order it restores the Dynamic anchor deficit, restores
permanent-capital fee shortfall, and pays the exact restoration-transfer fees
from fresh maturity. A too-small usable amount remains in the semantic staging
Account. Only the valid remainder receives the ordinary gross 40/60 split.
Restoration fees do not create recursive debt; ordinary new claim/permanent
delivery fees use anchor/permanent accounting for a later cycle. Every delivered
claim increment enters liquid before ordinary pool reconciliation.

If the maximum backed IO for a two-week capture exceeds the available protocol
reserve, receipt preparation returns `InsufficientIoReserve`. No IO is issued,
no partial recipient cursor is created, and the operation remains pending until
redemption returns enough IO to reserve. The frozen ICP cannot become redeemable
backing before that all-or-nothing obligation is persisted, so the pause cannot
dilute existing claims.

## Proof budget

IO proves ambiguous irreversible effects, not fungible-asset provenance after
custody. Exact outgoing Ledger transfer recovery, redemption replay, Jupiter
authorization, parent cached-stake reflection, NNS Split recovery, child
Disburse recovery, entitlement-generation binding, bounded recipient settlement,
and strict same-schema upgrade validation remain. Without those mechanisms an
ambiguous retry could duplicate value, create a second child, settle a different
recipient set, or decode an incompatible monetary state.

| Mechanism | Result | Concrete safety purpose |
| --- | --- | --- |
| Prepared redemption hash, exact incoming block and payout proof | Keep | Bind a caller replay, prove one memo-bound ICRC-1 push, and prevent ambiguous ICP payout retries from transferring twice. |
| Generic outgoing Ledger transfer proof | Keep | An ambiguous retry without the exact block can send controlled ICP or IO twice. |
| Jupiter source proof | Keep | Preserve the external faucet authorization boundary before its ICP enters custody. |
| Parent refresh proof | Keep | Prevent crediting a stake transfer until canonical cached stake reflects it. |
| Split recovery | Keep | Prevent a possible-effect retry from creating a second child neuron. |
| Child Disburse proof | Keep | Prevent a possible-effect retry from paying child principal twice. |
| Entitlement generation and bounded settlement cursor | Keep | Prevent a receipt replay from changing the frozen recipients or settling one recipient twice. |
| Same-schema stable-state proof | Keep | Reject incompatible launch state and preserve every irreversible-effect checkpoint across upgrade. |
| Maturity Mint/source-operation proof | Delete | The semantic staging Account balance is already controlled monetary authority. |
| Receipt source Account/block proof | Delete | Recipient policy, exact paired amount, operation ID, and exact outgoing transfer effects fully bind settlement. |
| Broad provenance fingerprints | Delete | They duplicated small immutable intent or attributed fungible ICP after custody. |
| Two-year paired receipt | Delete | Two-year claim credit is yield and creates no IO obligation. |

Canonical Account balance, semantic Account identity, exact operation
sequence/amount/kind, and the retained outgoing-effect proofs supply all required
safety facts.

Claim backing remains:

```text
B = L + P + U + T
```

where permanent capital is excluded and paired claim credit remains quarantined
until Stream has frozen its matching IO obligation.
For the Dynamic parent, `P` is only its accounted claim-bearing component;
anchor available and unexplained positive surplus are excluded.
