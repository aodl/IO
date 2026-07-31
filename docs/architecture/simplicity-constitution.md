# IO Simplicity Constitution

Status: normative for the IO launch architecture.

IO is not live. The reserved production canisters are inert and this document does
not authorize production activation, installation, upgrade, funding, or controller
changes.

## Constitutional rules

1. Explicit intent beats inferred intent.
2. Canonical balances beat replicated balances.
3. One active operation beats reservation algebra.
4. Typed operation variants beat optional-field bags.
5. Exact `Account` roles beat liability classification.
6. Prove our own effects, not every global event.
7. The historian explains global history and is never monetary authority.
8. Safe pause beats automatic rare-case recovery.
9. NNS authority and proof remain in the NNS manager.
10. Unsupported activity is not a launch feature.
11. Prelaunch experiments create no permanent migration obligation.
12. Replacement code must delete the replaced path.
13. Every complexity exception requires an ADR and demonstrated need.

These rules are requirements, not aspirations. A replacement phase is incomplete
while its old production monetary path remains reachable. Experimental branches
are research records, not compatibility obligations.

## Mandatory future-change checklist

Every proposed monetary change must answer:

- Can explicit intent replace observation?
- Can `Account` topology replace state classification?
- Can serialization remove concurrency state?
- Can a safe pause handle the rare event?
- Does this duplicate a canonical fact?
- Does this introduce a second representation of the same economic state?
- What old code and states does it delete?
- What actual incident or measured need justifies the automation?
- Can an auditor describe the state transition on one page?

An answer that adds a second monetary implementation, trusts historian state, or
lets a caller assert completion is automatically rejected.

## Deliberate launch constraints

Launch execution is serialized. Temporary `Busy` responses, safe pauses for rare
ambiguity, delayed NNS maturity payout, one pending unwind child, coalesced target
updates, and the absence of automatic unsolicited-transfer refunds are intended
properties. They are not incomplete work.

Unsupported direct transfers create no protocol claim. Intentional ledger fee
changes use pause, drain, governance, configuration update, verification, and
unpause. Launch value-moving canisters support only stable schema V1.
