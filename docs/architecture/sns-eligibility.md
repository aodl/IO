# SNS Eligibility

SNS eligibility is modelled in `io-governance-types` as a pure snapshot function over SNS neuron records.

Inputs include SNS neurons, protocol-owned neuron IDs, Jupiter governance neuron IDs, a minimum dissolve delay, a strict non-dissolving flag, and a timestamp. The output is a `SnsNeuronEligibility` per observed neuron with either eligible stake or an exclusion reason.

Eligibility rules:

- user-owned neurons can be eligible;
- dissolve delay must be at least two weeks;
- strict mode excludes dissolving neurons;
- Jupiter governance neurons are excluded;
- protocol-owned neurons are excluded;
- zero-stake neurons are excluded.

Normal user-staked IO remains redeemable supply even while locked in SNS neurons. Eligibility affects reward allocation and 2-week pool targeting; it does not remove user IO from redeemable supply.

The model is fixture-tested and does not call live SNS governance. Production-shaped SNS governance DTOs now decode official-shaped `list_neurons` and `list_proposals` records into the same internal `SnsNeuron` and `SnsProposal` types used by this policy. That mapping preserves the existing exclusions for protocol-owned neurons, Jupiter governance neurons, zero stake, insufficient dissolve delay, and dissolving neurons.

Stream-manager governance snapshot tests fetch local/mock SNS governance-shaped neuron pages through `SnsGovernanceClient`, apply this policy, and report excluded neurons alongside valid reward snapshots. Invalid/non-eight-byte SNS neuron IDs are treated as exclusions with conversion errors; they are not mapped to a fallback numeric ID. Eight-byte SNS neuron IDs are a local/mock compatibility bridge for the existing `NeuronSnapshot` reward identity.

Production-shaped SNS proposal records are mapped into the existing participation summary model, including direct votes, followed votes, proposal topics, decision timestamps, reward eligibility, and open/closed filtering. This milestone does not change reward allocation economics or participation policy.
