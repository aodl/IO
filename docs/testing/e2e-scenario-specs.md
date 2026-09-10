# Pooled claim-backing E2E scenarios

These scenarios use canonical ledgers and Governance under PocketIC. They do
not authorize mainnet work.

1. Jupiter proves the source block, credits permanent capital with 40% gross
   minus its exact fee, credits liquid claim backing with 60% gross minus its
   exact fee, and releases IO at the pre-event `B/C` rate (1:1 only at true
   empty genesis).
2. Readiness proves the memo-0 Dynamic parent, fixed followee, exact 14-day
   delay, 10 ICP anchor, and dust-tolerant physical/economic partition before
   ordinary IO stakes at 0%, 50%, and 100%.
3. Dynamic top-up, Split, and committed child-disbursement fees consume exact
   anchor capacity once, preserve `B`, and stop before effect when capacity is
   insufficient. The parent follows the configured local followee and retains
   best-effort voting-power refresh.
4. Two-week maturity captures its complete fixed semantic Account balance,
   including donations, then uses the shared Jupiter 40/60 paired path with
   partial forfeiture/dust and sequential IO recipient settlement.
5. Two-year maturity captures only its own semantic Account, restores any
   anchor deficit plus the restoration fee from fresh capture, applies gross
   40/60 only to the usable remainder, credits both ordinary legs net of their
   own fees without reducing anchor again, routes claim yield to Stream liquid,
   and issues no IO or paired receipt.
6. Dissolve/cancellation proves precommit netting, sticky postcommit lifecycle,
   exact readiness, principal return, maturity cleanup, prospective re-entry,
   and no duplicate child or retroactive reward.
7. More than 32 historical structural generations complete without a product
   cap. One aggregate child is created per generation, and an overdue or
   ambiguous ready child is serviced before another Split.
8. Redemption receives an ordinary memo-free ICRC-1 transfer into one semantic
   staging Account and keeps it in `C`. A bounded index scan discovers the
   block, the canonical ledger exact-proves it, and Stream waits without debt
   until liquid ICP covers a fresh monotone-rate quote. Payout goes to the exact
   source Account once; paid-but-unswept `X` is excluded from `C` once, and the
   exact reserve sweep replaces that exclusion without changing post-payout `C`.
9. Successful reward IO transfer plus failed SNS refresh still increases
   `A_backing` through the authoritative staking-account ledger balance.
10. Upgrades and lost callbacks at every submitted/proved phase preserve exact
    intents, permit canonical proof, and never double count or transfer.
11. Independent 12-hour structural synchronization and daily reward processing
    preserve event-fenced eligibility; exact child-ready and 60-second recovery
    timers reconstruct across upgrade. Exact candidate tests prove component
    hashes, 14-day NNS threshold, 15-day-plus-one-minute SNS product delay,
    following, maturity, minimum stake, child lifecycle, and cleanup.

Historical inventory aliases retained by the coverage guard are:
Serialized redemption; Jupiter 40/60; Direct maturity; Exact rewards;
One unwind child; Historian and frontend; Transport ambiguity. In every
maintained monetary scenario, Every upgrade returns Paused. These aliases
identify prior test families and do not override the corrected assertions.
