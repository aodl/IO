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

The pure allocator owns only its tiny canonical SNS neuron-ID value and has no production dependency on the broad governance-types crate. The stream manager owns narrow pinned DTOs for `list_neurons`, bounded proposal evidence, `get_proposal`, and exact payout-time `get_neuron`. It excludes protocol-owned and Jupiter-governance staking Accounts, zero stake, every delay other than exactly 1,209,600 seconds, and dissolving neurons.

Stream-manager governance snapshot tests fetch local/mock SNS governance-shaped neuron pages through `SnsGovernanceClient`, apply this policy, and report excluded neurons alongside raw reward observations. Invalid or noncanonical SNS neuron IDs are treated as exclusions with conversion errors; they are not mapped to a fallback numeric ID. Ordinary reward cohorts require exact 32-byte canonical SNS neuron IDs.

Production-shaped proposal ballots encode direct and followed participation identically as `Yes = 1` or `No = 2`. Capture anchors the latest proposal and bounded open IDs; close reads only the cohort window and uses the strict capture-exclusive, close-inclusive decision interval. These evidence corrections do not change allocator arithmetic or any expected allocation vector.
