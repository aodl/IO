# Stream Manager

The Stream Manager owns the IO reserve, spendable liquid ICP backing, semantic
redemption staging, canonical SNS structural/reward observation, one pending
entitlement batch, and one serialized monetary operation.

Its canonical snapshot brackets IO/ICP ledger and SNS reads with two identical
NNS observations. It derives claim-bearing supply `C`, liquid backing `L`,
claim-bearing Dynamic-parent principal `P`, live-child principal `U`, exact in-transit backing
`T`, total backing `B=L+P+U+T`, structural active stake `A_backing`, and the
prospectively eligible subset `A_reward`. Governance supplies neuron identity
and structural state; the IO ledger staking Account supplies stake value.

Each successful 12-hour structural observation refreshes the bounded, sorted
neuron registry and stores one latest reconciliation checkpoint without
crediting a reward event. Daily reward observation remains separately fenced by
the canonical event deadline and 300-second margin. One ephemeral earliest-
deadline timer wakes structural, reward, or 60-second retry work. There is no
target queue or second monetary scheduler. Reward allocation is allowed only
when Dynamic claim principal covers `floor(A_reward*B/C)`.

The user transfers IO into one fixed staging Account, which is deliberately not
reserve and therefore remains in `C`. A bounded account-filtered index scan
discovers candidates, but only canonical ledger proof authorizes the amount and
source Account. Stream waits without debt until a fresh coherent `B/C` quote,
matching fees, and sufficient liquid ICP exist. Exact payout success retires
the staged amount economically; the following exact staging-to-reserve sweep
replaces that temporary `C` exclusion with physical ledger accounting. The
permissionless no-argument endpoint and timer share the same bounded worker,
monetary slot, and scheduler.

Jupiter and two-week maturity enter through one paired-backing receipt. The
receipt is identified by the authenticated NNS Manager's operation sequence,
exact claim credit, and recipient policy: Jupiter or one frozen entitlement
generation. It freezes pre-inflow economics and the bounded recipient vector
before the credit becomes redeemable. Two-year maturity is ordinary unpaired
yield and enters liquid backing without a receipt. IO-ledger staking balances
remain authoritative when an ancillary SNS `ClaimOrRefresh` is delayed.
Public progress is coarse and action-oriented. Internal phase names remain
operator diagnostics, and multi-recipient settlement stays bounded to one
recipient transfer per resume.

Install and post-upgrade state are Paused. Reviewed unpause reconstructs the
Stream scheduler from semantic checkpoints; the NNS Manager independently
reconstructs exact operation/ready-child recovery deadlines. IO remains inert
and prelaunch.
