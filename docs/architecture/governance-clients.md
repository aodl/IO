# Governance boundaries

The Stream Manager authenticates SNS governance for lifecycle control and
canonical reward observations, and authenticates the configured NNS Manager
for exact backing inflows. It never accepts caller-supplied monetary facts.

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

SNS generic-function validators are pure proposal-submission preflights. They
do not lock manager state, and target execution repeats every authoritative
local and inter-canister check after voting. At the reviewed SNS source
boundary, any normal target-method reply is recorded as successful execution;
SNS Governance does not interpret a Candid `Err` returned in the reply bytes.
The three registered IO targets—Stream lifecycle, NNS lifecycle, and protected
two-year maturity—therefore reject at the inter-canister transport boundary
when the requested action reached neither its postcondition nor durable
acceptance. After exact work is persisted, `Pending` is ordinary permissionless
continuation. A deliberately persisted Paused/Stuck safety response is retained
and surfaced even though SNS may record that target call as executed.

The one immediate NNS monetary-operation slot remains intentional. In
particular, structural reward observation may legitimately start Pool
reconciliation. Maturity validation rejects while Pool is active; execution
also rejects if Pool wins after validation. Operators should inspect current
manager status before proposing maturity, then submit a new proposal after Pool
finishes rather than expecting maturity to preempt or queue behind it.
