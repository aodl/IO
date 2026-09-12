# Governance boundaries

The Stream Manager reads canonical SNS governance reward observations and
authenticates the configured NNS Manager for exact backing inflows. It never
accepts caller-supplied monetary facts. Lifecycle readiness is automatic.

The NNS Manager alone submits commands for the permanent neuron, pooled
exact-14-day parent, and bounded passive unwind children. The parent has one
fixed configured following policy; readiness verifies it. Daily pool-policy
observation independently attempts best-effort voting-power refresh for the
permanent neuron and pooled parent without another timer. Refresh failure does
not invalidate policy observation or gate monetary work. Every potentially
irreversible Governance effect has a typed persisted immutable intent before
submission. Definite success is immediately re-observed once and may continue
to the next proved fixed step; ambiguity or a missing postcondition stops
dependent work. Historian observations are advisory only.

Production pins that parent policy to permanent IO two-year neuron
`10_292_412_127_977_304_661`, never alpha-vote directly. The permanent neuron
is recorded and operationally expected to follow alpha-vote neuron
`2_947_465_672_511_369`. This remains subject to separately authorized mainnet
verification; IO code does not change the permanent neuron's followees.

SNS Governance governs SNS policy and canister upgrades, but routine IO
lifecycle, deterministic continuation, and protected two-year maturity do not
use generic functions or proposals. Each manager automatically recovers known
durable work, revalidates while Paused, and enters Ready when its invariants
hold. Two-year ordinary maturity is offered to the existing maturity state
machine once per week. A Paused/Stuck safety response remains fail-closed and
is rechecked automatically after external correction or upgrade.

The one immediate NNS monetary-operation slot remains intentional. In
particular, structural reward observation may legitimately start Pool
reconciliation. The weekly maturity trigger observes Busy and leaves Pool
intact; the recovery timer completes it independently and the next interval is
another ordinary maturity opportunity. Passive TwoWeek maturity and non-ready
unwind children do not reserve the immediate slot. Nothing preempts or queues
behind that slot.
