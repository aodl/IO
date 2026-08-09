# SNS Eligibility

Production SNS eligibility is owned by the narrow `io-sns-reward-boundary` and
the stream manager. The value-moving graph does not depend on
`io-governance-types`.

Inputs include SNS neurons, protocol-owned neuron IDs, Jupiter governance neuron IDs, a minimum dissolve delay, a strict non-dissolving flag, and a timestamp. The output is a `SnsNeuronEligibility` per observed neuron with either eligible stake or an exclusion reason.

Eligibility rules:

- user-owned neurons can be eligible;
- dissolve delay must be at least two weeks;
- strict mode excludes dissolving neurons;
- Jupiter governance neurons are excluded;
- protocol-owned neurons are excluded;
- zero-stake neurons are excluded.

Normal user-staked IO remains redeemable supply even while locked in SNS neurons. Eligibility affects reward allocation and 2-week pool targeting; it does not remove user IO from redeemable supply.

The pure allocator owns only its tiny canonical SNS neuron-ID value. The SNS
boundary owns narrow pinned DTOs for latest reward events, paginated neurons,
exact-neuron reads, Root module-hash evidence, and reward parameters. The stream
manager excludes protocol-owned and Jupiter-governance staking Accounts, zero
stake, every delay other than exactly 1,209,600 seconds, and dissolving neurons.

Stream-manager governance snapshot tests fetch local/mock SNS governance-shaped
neuron pages through `SnsGovernanceClient`, apply this policy, and report
excluded neurons alongside canonical reward observations. Invalid or
noncanonical SNS neuron IDs fail closed; they are not mapped to a fallback
numeric ID. Entitlement entries require exact 32-byte canonical SNS neuron IDs.

For a proposal-bearing event the boundary reads Governance's canonical reward
shares; IO does not retain proposal DTOs or reconstruct direct/followed voting.
When no proposal settled, current eligible cached IO stake determines each neuron's
fraction of the fixed daily credit. That fallback is selected only by an empty
canonical `settled_proposals` list; zero proposal shares never trigger it. For
a proposal-bearing event, current-event shares from excluded and ineligible
neurons remain in the canonical denominator, so their fractions are forfeited
rather than redistributed.
