# Pooled claim-backing E2E scenarios

These scenarios use canonical ledgers and Governance under PocketIC. They do
not authorize mainnet work.

1. Jupiter proves the source block, credits permanent capital with 40% gross
   minus its exact fee, credits liquid claim backing with 60% gross minus its
   exact fee, and releases IO at the pre-event `B/C` rate (1:1 only at true
   empty genesis).
2. Ordinary IO stakes at 0%, 50%, and 100%. Existing liquid backing moves to a
   lazy exact-14-day parent; `C` is unchanged and only transfer fees reduce
   `B`.
3. The pooled parent follows the configured local followee, casts no manual
   vote, follows the leader ballot, earns maturity, and preserves that policy
   through the existing daily voting-power refresh.
4. Pooled maturity proves actual Mint, fee-reduced permanent credit, the joint
   all-liquid/all-pool/mixed claim route, partial forfeiture/dust, and sequential
   IO recipient settlement. There is no source-level pooled liquid receipt.
5. Permanent maturity stakes 40% of ordinary maturity, disburses the remainder,
   treats the actual Mint as entirely new claim backing, dynamically routes it,
   and issues no IO.
6. Dissolve/cancellation proves precommit netting, sticky postcommit lifecycle,
   exact readiness, principal return, maturity cleanup, prospective re-entry,
   and no duplicate child or retroactive reward.
7. Three overlapping daily cohorts complete independently. Thirty-two live
   cohorts fit; a thirty-third creates no effect, and retirement frees capacity.
8. Redemption quotes `L+P+U+T` but spends only `L`. A shortfall pulls no IO;
   retry after child return succeeds exactly once.
9. Successful reward IO transfer plus failed SNS refresh still increases
   `A_backing` through the authoritative staking-account ledger balance.
10. Upgrades and lost callbacks at every submitted/proved phase preserve exact
    intents, permit canonical proof, and never double count or transfer.
11. Exact candidate tests prove component hashes, 14-day threshold, following,
    maturity, minimum stake, child lifecycle, and cleanup.

Historical inventory aliases retained by the coverage guard are:
Serialized redemption; Jupiter 40/60; Direct maturity; Exact rewards;
One unwind child; Historian and frontend; Transport ambiguity. In every
maintained monetary scenario, Every upgrade returns Paused. These aliases
identify prior test families and do not override the corrected assertions.
