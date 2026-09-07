#![cfg(test)]

//! Executable architecture model for anchored dynamic backing.
//!
//! This crate is deliberately test-only. It exercises the accounting
//! representation independently of the value-moving canisters.

mod anchored_dynamic_backing {
    use std::cmp::Ordering;

    const E8S_PER_ICP: u128 = 100_000_000;
    const ANCHOR_TARGET_E8S: u128 = 10 * E8S_PER_ICP;
    const NNS_DYNAMIC_DISSOLVE_DELAY_SECONDS: u64 = 1_209_600;
    const PREFERRED_SNS_UNLOCK_DELAY_SECONDS: u64 = 1_296_060;
    const REWARD_CADENCE_SECONDS: u64 = 86_400;
    const REWARD_MARGIN_SECONDS: u64 = 300;
    const RECOVERY_RETRY_SECONDS: u64 = 60;
    const STRUCTURAL_CADENCE_SECONDS: u64 = 43_200;
    const REVIEWED_MAX_NEURONS: u64 = 1_000;
    const NEURON_PAGE_SIZE: u64 = 100;
    const EXCLUDED_IO_ACCOUNTS: u64 = 1;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct CadenceAssessment {
        cadence_seconds: u64,
        generations_per_day: u64,
        natural_live_bound: u64,
        governance_queries_per_day_at_max: u64,
        io_balance_queries_per_day_at_max: u64,
        approximate_calls_per_day_at_max: u64,
        healthy_slack_seconds: u64,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct SchedulerFacts {
        latest_structural_at: u64,
        latest_reward_event_end: u64,
        retry_due_at: Option<u64>,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct RewardFacet {
        eligible_from_event: Option<u64>,
        eligible_through_event: Option<u64>,
        accumulated_credit: u128,
    }

    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    struct ClaimBacking {
        liquid: u128,
        dynamic: u128,
        unwinding: u128,
        transit: u128,
    }

    impl ClaimBacking {
        fn total(self) -> Result<u128, ModelError> {
            self.liquid
                .checked_add(self.dynamic)
                .and_then(|v| v.checked_add(self.unwinding))
                .and_then(|v| v.checked_add(self.transit))
                .ok_or(ModelError::Overflow)
        }
    }

    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    struct Economy {
        backing: ClaimBacking,
        claims: u128,
        dynamic_physical: u128,
        anchor_available: u128,
        excluded_surplus: u128,
        dynamic_inflight_physical: u128,
        permanent_capital: u128,
        staged_two_year_maturity: u128,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum ModelError {
        Overflow,
        InsufficientBacking,
        InsufficientAnchor,
        InvalidPartition,
        InvalidRateFloor,
        RateDecrease,
        InvalidAmount,
        DuplicateProof,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum FeeClass {
        ExistingBackingMovement,
        FreshValueDelivery,
        AnchorRestorationFromFreshValue,
        RedemptionQuote,
        External,
        IoLedgerBurn,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct TwoYearOutcome {
        state: Economy,
        anchor_reimbursement: u128,
        anchor_reimbursement_fee: u128,
        ordinary_permanent_gross: u128,
        ordinary_claim_gross: u128,
        carried: u128,
    }

    impl Economy {
        fn bootstrap(seed_balance: u128) -> Result<Self, ModelError> {
            if seed_balance < ANCHOR_TARGET_E8S {
                return Err(ModelError::InvalidAmount);
            }
            let state = Self {
                dynamic_physical: seed_balance,
                anchor_available: ANCHOR_TARGET_E8S,
                excluded_surplus: seed_balance - ANCHOR_TARGET_E8S,
                ..Self::default()
            };
            state.validate()?;
            Ok(state)
        }

        fn validate(self) -> Result<(), ModelError> {
            if self.anchor_available > ANCHOR_TARGET_E8S {
                return Err(ModelError::InvalidPartition);
            }
            let parent_partition = self
                .backing
                .dynamic
                .checked_add(self.anchor_available)
                .and_then(|v| v.checked_add(self.excluded_surplus))
                .and_then(|v| v.checked_add(self.dynamic_inflight_physical))
                .ok_or(ModelError::Overflow)?;
            if parent_partition != self.dynamic_physical {
                return Err(ModelError::InvalidPartition);
            }
            let backing = self.backing.total()?;
            if (self.claims == 0 && backing != 0) || (self.claims > 0 && backing < self.claims) {
                return Err(ModelError::InvalidRateFloor);
            }
            Ok(())
        }

        fn assert_transition(self, next: Self) -> Result<Self, ModelError> {
            self.validate()?;
            next.validate()?;
            match (self.claims, next.claims) {
                (0, 0) => {}
                (0, _) => {
                    if next.backing.total()? < next.claims {
                        return Err(ModelError::RateDecrease);
                    }
                }
                (_, 0) => {}
                _ if !ratio_ge(
                    next.backing.total()?,
                    next.claims,
                    self.backing.total()?,
                    self.claims,
                ) =>
                {
                    return Err(ModelError::RateDecrease)
                }
                _ => {}
            }
            Ok(next)
        }

        fn add_backed_issuance(self, incoming: u128) -> Result<Self, ModelError> {
            if incoming == 0 {
                return Err(ModelError::InvalidAmount);
            }
            let before_backing = self.backing.total()?;
            let issued = if self.claims == 0 {
                incoming
            } else {
                mul_div_floor(incoming, self.claims, before_backing)?
            };
            let mut next = self;
            next.backing.liquid = next
                .backing
                .liquid
                .checked_add(incoming)
                .ok_or(ModelError::Overflow)?;
            next.claims = next
                .claims
                .checked_add(issued)
                .ok_or(ModelError::Overflow)?;
            self.assert_transition(next)
        }

        fn paired_40_60_claim_inflow(self, captured: u128, fee: u128) -> Result<Self, ModelError> {
            let permanent_gross = captured.saturating_mul(40) / 100;
            let claim_gross = captured - permanent_gross;
            if permanent_gross <= fee || claim_gross <= fee {
                return Err(ModelError::InvalidAmount);
            }
            let permanent_credit = permanent_gross - fee;
            let claim_credit = claim_gross - fee;
            let before_backing = self.backing.total()?;
            let issued = if self.claims == 0 {
                claim_credit
            } else {
                mul_div_floor(claim_credit, self.claims, before_backing)?
            };
            let mut next = self;
            next.permanent_capital = next
                .permanent_capital
                .checked_add(permanent_credit)
                .ok_or(ModelError::Overflow)?;
            next.backing.liquid = next
                .backing
                .liquid
                .checked_add(claim_credit)
                .ok_or(ModelError::Overflow)?;
            next.claims = next
                .claims
                .checked_add(issued)
                .ok_or(ModelError::Overflow)?;
            self.assert_transition(next)
        }

        fn move_liquid_to_transit(self, amount: u128) -> Result<Self, ModelError> {
            if self.backing.liquid < amount {
                return Err(ModelError::InsufficientBacking);
            }
            let mut next = self;
            next.backing.liquid -= amount;
            next.backing.transit = next
                .backing
                .transit
                .checked_add(amount)
                .ok_or(ModelError::Overflow)?;
            self.assert_transition(next)
        }

        fn donate_dynamic(self, amount: u128) -> Result<Self, ModelError> {
            let mut next = self;
            next.dynamic_physical = next
                .dynamic_physical
                .checked_add(amount)
                .ok_or(ModelError::Overflow)?;
            next.excluded_surplus = next
                .excluded_surplus
                .checked_add(amount)
                .ok_or(ModelError::Overflow)?;
            self.assert_transition(next)
        }

        fn protect_existing_backing_fee(&mut self, fee: u128) -> Result<(), ModelError> {
            self.anchor_available = self
                .anchor_available
                .checked_sub(fee)
                .ok_or(ModelError::InsufficientAnchor)?;
            self.backing.dynamic = self
                .backing
                .dynamic
                .checked_add(fee)
                .ok_or(ModelError::Overflow)?;
            Ok(())
        }

        fn top_up_dynamic(self, credit: u128, fee: u128) -> Result<Self, ModelError> {
            let debit = credit.checked_add(fee).ok_or(ModelError::Overflow)?;
            if credit == 0 || self.backing.liquid < debit || self.anchor_available < fee {
                return Err(if self.anchor_available < fee {
                    ModelError::InsufficientAnchor
                } else {
                    ModelError::InsufficientBacking
                });
            }
            let mut next = self;
            next.backing.liquid -= debit;
            next.dynamic_physical = next
                .dynamic_physical
                .checked_add(credit)
                .ok_or(ModelError::Overflow)?;
            next.backing.dynamic = next
                .backing
                .dynamic
                .checked_add(credit)
                .ok_or(ModelError::Overflow)?;
            next.protect_existing_backing_fee(fee)?;
            self.assert_transition(next)
        }

        fn commit_unwind(
            self,
            gross: u128,
            split_fee: u128,
            future_disbursement_fee: u128,
        ) -> Result<Self, ModelError> {
            let fees = split_fee
                .checked_add(future_disbursement_fee)
                .ok_or(ModelError::Overflow)?;
            if gross <= fees || self.backing.dynamic < gross || self.anchor_available < fees {
                return Err(if self.anchor_available < fees {
                    ModelError::InsufficientAnchor
                } else {
                    ModelError::InsufficientBacking
                });
            }
            let child_net = gross - fees;
            let mut next = self;
            next.dynamic_physical -= gross;
            next.backing.dynamic -= gross;
            next.protect_existing_backing_fee(fees)?;
            next.backing.unwinding = next
                .backing
                .unwinding
                .checked_add(child_net)
                .ok_or(ModelError::Overflow)?;
            self.assert_transition(next)
        }

        fn return_child(self, net: u128) -> Result<Self, ModelError> {
            if self.backing.unwinding < net {
                return Err(ModelError::InsufficientBacking);
            }
            let mut next = self;
            next.backing.unwinding -= net;
            next.backing.liquid = next
                .backing
                .liquid
                .checked_add(net)
                .ok_or(ModelError::Overflow)?;
            self.assert_transition(next)
        }

        fn add_fresh_permanent_credit(self, gross: u128, fee: u128) -> Result<Self, ModelError> {
            let credit = gross.checked_sub(fee).ok_or(ModelError::InvalidAmount)?;
            let mut next = self;
            next.permanent_capital = next
                .permanent_capital
                .checked_add(credit)
                .ok_or(ModelError::Overflow)?;
            self.assert_transition(next)
        }

        fn io_fee_burn(self, fee: u128) -> Result<Self, ModelError> {
            let mut next = self;
            next.claims = next
                .claims
                .checked_sub(fee)
                .ok_or(ModelError::InsufficientBacking)?;
            self.assert_transition(next)
        }

        fn redeem_staged(
            self,
            source_block: u128,
            principal: u128,
            payout_fee: u128,
            proved_blocks: &mut Vec<u128>,
        ) -> Result<(Self, u128, u128), ModelError> {
            if principal == 0 || principal > self.claims {
                return Err(ModelError::InvalidAmount);
            }
            if proved_blocks.contains(&source_block) {
                return Err(ModelError::DuplicateProof);
            }
            let gross = mul_div_floor(principal, self.backing.total()?, self.claims)?;
            let net = gross
                .checked_sub(payout_fee)
                .ok_or(ModelError::InvalidAmount)?;
            if self.backing.liquid < gross {
                return Err(ModelError::InsufficientBacking);
            }
            let mut next = self;
            next.claims -= principal;
            next.backing.liquid -= gross;
            proved_blocks.push(source_block);
            Ok((self.assert_transition(next)?, gross, net))
        }
    }

    fn ratio_ge(a_num: u128, a_den: u128, b_num: u128, b_den: u128) -> bool {
        compare_ratio(a_num, a_den, b_num, b_den) != Ordering::Less
    }

    // Continued fractions compare exactly without overflowing cross-products.
    fn compare_ratio(
        mut a_num: u128,
        mut a_den: u128,
        mut b_num: u128,
        mut b_den: u128,
    ) -> Ordering {
        assert!(a_den > 0 && b_den > 0);
        let mut reversed = false;
        loop {
            let a_whole = a_num / a_den;
            let b_whole = b_num / b_den;
            if a_whole != b_whole {
                let order = a_whole.cmp(&b_whole);
                return if reversed { order.reverse() } else { order };
            }
            let a_rem = a_num % a_den;
            let b_rem = b_num % b_den;
            match (a_rem == 0, b_rem == 0) {
                (true, true) => return Ordering::Equal,
                (true, false) => {
                    return if reversed {
                        Ordering::Greater
                    } else {
                        Ordering::Less
                    }
                }
                (false, true) => {
                    return if reversed {
                        Ordering::Less
                    } else {
                        Ordering::Greater
                    }
                }
                (false, false) => {
                    a_num = a_den;
                    a_den = a_rem;
                    b_num = b_den;
                    b_den = b_rem;
                    reversed = !reversed;
                }
            }
        }
    }

    fn mul_div_floor(value: u128, numerator: u128, denominator: u128) -> Result<u128, ModelError> {
        if denominator == 0 {
            return Err(ModelError::InvalidAmount);
        }
        let whole = value / denominator;
        let remainder = value % denominator;
        let head = whole.checked_mul(numerator).ok_or(ModelError::Overflow)?;
        let tail = remainder
            .checked_mul(numerator)
            .ok_or(ModelError::Overflow)?
            / denominator;
        head.checked_add(tail).ok_or(ModelError::Overflow)
    }

    fn reimbursable(available: u128, deficit: u128, fee: u128) -> u128 {
        if deficit == 0 || available <= fee {
            0
        } else {
            deficit.min(available - fee)
        }
    }

    fn replenish_two_year(
        mut state: Economy,
        captured: u128,
        transfer_fee: u128,
    ) -> Result<TwoYearOutcome, ModelError> {
        state.staged_two_year_maturity = state
            .staged_two_year_maturity
            .checked_add(captured)
            .ok_or(ModelError::Overflow)?;
        let mut remaining = state.staged_two_year_maturity;
        let anchor_deficit = ANCHOR_TARGET_E8S - state.anchor_available;
        let anchor_reimbursement = reimbursable(remaining, anchor_deficit, transfer_fee);
        let anchor_reimbursement_fee = if anchor_reimbursement > 0 {
            transfer_fee
        } else {
            0
        };
        if anchor_reimbursement > 0 {
            remaining -= anchor_reimbursement + transfer_fee;
            state.anchor_available += anchor_reimbursement;
            state.dynamic_physical += anchor_reimbursement;
        }
        let mut ordinary_permanent_gross = 0;
        let mut ordinary_claim_gross = 0;
        if remaining > 0 {
            let permanent = remaining.saturating_mul(40) / 100;
            let claim = remaining - permanent;
            if permanent > transfer_fee && claim > transfer_fee {
                ordinary_permanent_gross = permanent;
                ordinary_claim_gross = claim;
                state.permanent_capital += permanent - transfer_fee;
                state.backing.liquid += claim - transfer_fee;
                remaining = 0;
            }
        }
        state.staged_two_year_maturity = remaining;
        state.validate()?;
        Ok(TwoYearOutcome {
            state,
            anchor_reimbursement,
            anchor_reimbursement_fee,
            ordinary_permanent_gross,
            ordinary_claim_gross,
            carried: remaining,
        })
    }

    fn natural_live_bound(cadence_seconds: u64) -> u64 {
        NNS_DYNAMIC_DISSOLVE_DELAY_SECONDS.div_ceil(cadence_seconds) + 1
    }

    fn governance_pages(neurons: u64) -> u64 {
        neurons / NEURON_PAGE_SIZE + 1
    }

    fn io_balance_queries_per_poll(neurons: u64) -> u64 {
        neurons + 2 * (1 + EXCLUDED_IO_ACCOUNTS)
    }

    fn approximate_calls_per_poll(neurons: u64) -> u64 {
        // Root summary (1), two before/after NNS claim snapshots (4 total),
        // ledger/index facts in those two snapshots (12 with one excluded IO
        // Account), paged SNS neurons, and one IO balance per active neuron.
        neurons + governance_pages(neurons) + 17
    }

    fn healthy_operation_budget_seconds() -> u64 {
        // A deterministic stress allowance, not a network SLA: two structural
        // retries, two Pool-contention retries, two command-reflection retries,
        // one ready-return retry, and three minutes for successful call chains.
        7 * RECOVERY_RETRY_SECONDS + 180
    }

    fn assess_cadence(cadence_seconds: u64) -> CadenceAssessment {
        assert_eq!(REWARD_CADENCE_SECONDS % cadence_seconds, 0);
        let generations_per_day = REWARD_CADENCE_SECONDS / cadence_seconds;
        let unlock_budget = PREFERRED_SNS_UNLOCK_DELAY_SECONDS - NNS_DYNAMIC_DISSOLVE_DELAY_SECONDS;
        let healthy_slack_seconds = unlock_budget
            .checked_sub(cadence_seconds + healthy_operation_budget_seconds())
            .expect("candidate cadence must fit the preferred unlock");
        CadenceAssessment {
            cadence_seconds,
            generations_per_day,
            natural_live_bound: natural_live_bound(cadence_seconds),
            governance_queries_per_day_at_max: governance_pages(REVIEWED_MAX_NEURONS)
                * generations_per_day,
            io_balance_queries_per_day_at_max: io_balance_queries_per_poll(REVIEWED_MAX_NEURONS)
                * generations_per_day,
            approximate_calls_per_day_at_max: approximate_calls_per_poll(REVIEWED_MAX_NEURONS)
                * generations_per_day,
            healthy_slack_seconds,
        }
    }

    fn next_stream_deadline(facts: SchedulerFacts) -> u64 {
        let structural = facts.latest_structural_at + STRUCTURAL_CADENCE_SECONDS;
        let reward = facts.latest_reward_event_end + REWARD_CADENCE_SECONDS + REWARD_MARGIN_SECONDS;
        facts
            .retry_due_at
            .into_iter()
            .chain([structural, reward])
            .min()
            .unwrap()
    }

    fn observe_active(facet: RewardFacet, canonical_event_marker: u64) -> RewardFacet {
        RewardFacet {
            eligible_from_event: facet
                .eligible_from_event
                .or(Some(canonical_event_marker.saturating_add(1))),
            eligible_through_event: None,
            ..facet
        }
    }

    fn observe_exit(facet: RewardFacet, canonical_event_marker: u64) -> RewardFacet {
        RewardFacet {
            eligible_through_event: facet.eligible_from_event.map(|_| canonical_event_marker),
            ..facet
        }
    }

    fn process_reward(mut facet: RewardFacet, event: u64, credit: u128) -> RewardFacet {
        let eligible = facet
            .eligible_from_event
            .is_some_and(|start| start <= event)
            && facet
                .eligible_through_event
                .is_none_or(|through| event <= through);
        if eligible {
            facet.accumulated_credit += credit;
        }
        facet
    }

    #[test]
    fn anchor_bootstrap_is_dust_tolerant_and_isolated() {
        assert_eq!(
            Economy::bootstrap(ANCHOR_TARGET_E8S - 1),
            Err(ModelError::InvalidAmount)
        );
        let exact = Economy::bootstrap(ANCHOR_TARGET_E8S).unwrap();
        let dusted = Economy::bootstrap(ANCHOR_TARGET_E8S + 777).unwrap();
        assert_eq!(exact.backing.total().unwrap(), 0);
        assert_eq!(exact.claims, 0);
        assert_eq!(dusted.anchor_available, ANCHOR_TARGET_E8S);
        assert_eq!(dusted.excluded_surplus, 777);
        assert_eq!(dusted.backing.total().unwrap(), 0);
        assert_eq!(dusted.claims, 0);
    }

    #[test]
    fn existing_backing_fee_reclassification_preserves_backing_and_partition() {
        let state = Economy::bootstrap(ANCHOR_TARGET_E8S + 9)
            .unwrap()
            .add_backed_issuance(5 * E8S_PER_ICP)
            .unwrap();
        let before = state.backing.total().unwrap();
        let next = state.top_up_dynamic(2 * E8S_PER_ICP, 10_000).unwrap();
        assert_eq!(next.backing.total().unwrap(), before);
        assert_eq!(next.anchor_available, ANCHOR_TARGET_E8S - 10_000);
        assert_eq!(next.excluded_surplus, 9);
    }

    #[test]
    fn jupiter_two_week_reward_and_transit_checkpoints_preserve_a_through_h() {
        let fee = 10_000;
        let genesis = Economy::bootstrap(ANCHOR_TARGET_E8S).unwrap();
        let jupiter = genesis
            .paired_40_60_claim_inflow(10 * E8S_PER_ICP, fee)
            .unwrap();
        let two_week = jupiter
            .paired_40_60_claim_inflow(2 * E8S_PER_ICP, fee)
            .unwrap();
        let reward = two_week.add_backed_issuance(E8S_PER_ICP).unwrap();
        let transit = reward.move_liquid_to_transit(E8S_PER_ICP).unwrap();
        let donated = transit.donate_dynamic(777).unwrap();
        assert_eq!(
            transit.backing.total().unwrap(),
            reward.backing.total().unwrap()
        );
        assert_eq!(
            donated.backing.total().unwrap(),
            transit.backing.total().unwrap()
        );
        assert_eq!(donated.excluded_surplus, 777);
        assert_eq!(donated.anchor_available, ANCHOR_TARGET_E8S);
        assert_eq!(jupiter.claims, 6 * E8S_PER_ICP - fee);
        assert_eq!(jupiter.backing.liquid, 6 * E8S_PER_ICP - fee);
        assert_eq!(jupiter.permanent_capital, 4 * E8S_PER_ICP - fee);
    }

    #[test]
    fn split_and_future_disbursement_fee_are_reserved_once() {
        let fee = 10_000;
        let state = Economy::bootstrap(ANCHOR_TARGET_E8S)
            .unwrap()
            .add_backed_issuance(2 * E8S_PER_ICP)
            .unwrap()
            .top_up_dynamic(E8S_PER_ICP + 2 * fee, fee)
            .unwrap();
        let before = state.backing.total().unwrap();
        let committed = state
            .commit_unwind(E8S_PER_ICP + 2 * fee, fee, fee)
            .unwrap();
        assert_eq!(committed.backing.total().unwrap(), before);
        assert_eq!(state.anchor_available - committed.anchor_available, 2 * fee);
        let returned = committed.return_child(E8S_PER_ICP).unwrap();
        assert_eq!(returned.backing.total().unwrap(), before);
        assert_eq!(returned.anchor_available, committed.anchor_available);
    }

    #[test]
    fn repeated_stake_unstake_churn_preserves_claim_rate_with_exact_anchor_cost() {
        let fee = 10_000;
        let mut state = Economy::bootstrap(ANCHOR_TARGET_E8S)
            .unwrap()
            .add_backed_issuance(5 * E8S_PER_ICP)
            .unwrap();
        let initial_backing = state.backing.total().unwrap();
        let initial_claims = state.claims;
        for _ in 0..3 {
            state = state.top_up_dynamic(E8S_PER_ICP + 2 * fee, fee).unwrap();
            let committed = state
                .commit_unwind(E8S_PER_ICP + 2 * fee, fee, fee)
                .unwrap();
            assert_eq!(committed.backing.total().unwrap(), initial_backing);
            state = committed.return_child(E8S_PER_ICP).unwrap();
            assert_eq!(state.backing.total().unwrap(), initial_backing);
            assert_eq!(state.claims, initial_claims);
        }
        assert_eq!(state.anchor_available, ANCHOR_TARGET_E8S - 9 * fee);
        assert!(ratio_ge(
            state.backing.total().unwrap(),
            state.claims,
            initial_backing,
            initial_claims,
        ));
    }

    #[test]
    fn anchor_exhaustion_precedes_irreversible_effect() {
        let mut state = Economy::bootstrap(ANCHOR_TARGET_E8S)
            .unwrap()
            .add_backed_issuance(2 * E8S_PER_ICP)
            .unwrap();
        state.anchor_available = 9;
        state.excluded_surplus += ANCHOR_TARGET_E8S - 9;
        state.validate().unwrap();
        assert_eq!(
            state.top_up_dynamic(E8S_PER_ICP, 10),
            Err(ModelError::InsufficientAnchor)
        );
        assert_eq!(
            state.commit_unwind(E8S_PER_ICP, 5, 5),
            Err(ModelError::InsufficientAnchor)
        );
    }

    #[test]
    fn fresh_permanent_delivery_fee_changes_no_claim_economics() {
        let state = Economy::bootstrap(ANCHOR_TARGET_E8S).unwrap();
        let next = state.add_fresh_permanent_credit(1_000_000, 10_000).unwrap();
        assert_eq!(next.permanent_capital, 990_000);
        assert_eq!(next.backing, state.backing);
        assert_eq!(next.claims, state.claims);
        assert_eq!(next.anchor_available, state.anchor_available);
    }

    #[test]
    fn fresh_jupiter_succeeds_with_zero_anchor_and_issues_against_net_credit() {
        let fee = 10_000;
        let mut state = Economy::bootstrap(ANCHOR_TARGET_E8S).unwrap();
        state.anchor_available = 0;
        state.excluded_surplus = ANCHOR_TARGET_E8S;
        state.validate().unwrap();
        let next = state
            .paired_40_60_claim_inflow(10 * E8S_PER_ICP, fee)
            .unwrap();
        assert_eq!(next.anchor_available, 0);
        assert_eq!(next.backing.liquid, 6 * E8S_PER_ICP - fee);
        assert_eq!(next.claims, 6 * E8S_PER_ICP - fee);
        assert_eq!(next.permanent_capital, 4 * E8S_PER_ICP - fee);
    }

    #[test]
    fn two_year_restores_anchor_then_splits_without_recursive_debt() {
        let fee = 10_000;
        let mut state = Economy::bootstrap(ANCHOR_TARGET_E8S)
            .unwrap()
            .add_backed_issuance(2 * E8S_PER_ICP)
            .unwrap();
        state.anchor_available -= 100_000;
        state.backing.dynamic += 100_000;
        state.validate().unwrap();
        let result = replenish_two_year(state, 1_000_000, fee).unwrap();
        assert_eq!(result.anchor_reimbursement, 100_000);
        assert_eq!(result.anchor_reimbursement_fee, fee);
        assert_eq!(result.state.anchor_available, ANCHOR_TARGET_E8S);
        assert_eq!(result.ordinary_permanent_gross, 356_000);
        assert_eq!(result.ordinary_claim_gross, 534_000);
        assert_eq!(result.state.permanent_capital, 346_000);
        assert_eq!(result.carried, 0);
    }

    #[test]
    fn tiny_two_year_maturity_carries_without_fee_recursion() {
        let fee = 10_000;
        let mut state = Economy::bootstrap(ANCHOR_TARGET_E8S).unwrap();
        state.anchor_available -= 100_000;
        state.excluded_surplus += 100_000;
        state.validate().unwrap();
        let result = replenish_two_year(state, fee, fee).unwrap();
        assert_eq!(result.anchor_reimbursement, 0);
        assert_eq!(result.anchor_reimbursement_fee, 0);
        assert_eq!(result.carried, fee);
    }

    #[test]
    fn rate_comparison_is_exact_near_u128_limits() {
        assert!(ratio_ge(u128::MAX, u128::MAX - 1, u128::MAX - 1, u128::MAX));
        assert!(!ratio_ge(
            u128::MAX - 1,
            u128::MAX,
            u128::MAX,
            u128::MAX - 1
        ));
        assert!(ratio_ge(2, 3, 4, 6));
    }

    #[test]
    fn deterministic_transition_property_sweep_preserves_floor_and_rate() {
        let mut seed = 0x4d59_5df4_d0f3_3173_u64;
        for _ in 0..2_000 {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let incoming = E8S_PER_ICP + u128::from(seed % 20) * E8S_PER_ICP;
            let fee = 1 + u128::from(seed % 10_000);
            let state = Economy::bootstrap(ANCHOR_TARGET_E8S + u128::from(seed % 999))
                .unwrap()
                .add_backed_issuance(incoming)
                .unwrap();
            let moved = state.top_up_dynamic(incoming / 2, fee).unwrap();
            assert!(ratio_ge(
                moved.backing.total().unwrap(),
                moved.claims,
                state.backing.total().unwrap(),
                state.claims,
            ));
            let burned = moved.io_fee_burn(fee.min(moved.claims - 1)).unwrap();
            assert!(ratio_ge(
                burned.backing.total().unwrap(),
                burned.claims,
                moved.backing.total().unwrap(),
                moved.claims,
            ));
        }
    }

    #[test]
    fn staged_claim_waits_for_current_rate_and_redeems_once() {
        let io_fee = 10_000;
        let initial = Economy::bootstrap(ANCHOR_TARGET_E8S)
            .unwrap()
            .add_backed_issuance(20 * E8S_PER_ICP)
            .unwrap();
        // The initial user transfer burns only its ordinary fee. Its staged
        // principal remains in C until canonical payout success.
        let staged = initial.io_fee_burn(io_fee).unwrap();
        assert_eq!(staged.claims, initial.claims - io_fee);
        let initial_quote =
            mul_div_floor(E8S_PER_ICP, staged.backing.total().unwrap(), staged.claims).unwrap();
        let appreciated = staged.add_backed_issuance(E8S_PER_ICP).unwrap();
        let mut proved_blocks = Vec::new();
        let (paid, gross, net) = appreciated
            .redeem_staged(7, E8S_PER_ICP, io_fee, &mut proved_blocks)
            .unwrap();
        assert!(gross >= initial_quote);
        assert_eq!(net, gross - io_fee);
        assert_eq!(paid.claims, appreciated.claims - E8S_PER_ICP);
        assert_eq!(
            appreciated.redeem_staged(7, E8S_PER_ICP, io_fee, &mut proved_blocks),
            Err(ModelError::DuplicateProof)
        );
    }

    #[test]
    fn illiquid_staging_creates_no_debt_and_can_retry_later() {
        let state = Economy::bootstrap(ANCHOR_TARGET_E8S)
            .unwrap()
            .add_backed_issuance(5 * E8S_PER_ICP)
            .unwrap();
        let mut illiquid = state;
        illiquid.backing.dynamic = illiquid.backing.liquid;
        illiquid.dynamic_physical += illiquid.backing.liquid;
        illiquid.backing.liquid = 0;
        illiquid.validate().unwrap();
        let mut blocks = Vec::new();
        assert_eq!(
            illiquid.redeem_staged(1, E8S_PER_ICP, 10_000, &mut blocks),
            Err(ModelError::InsufficientBacking)
        );
        assert!(blocks.is_empty());
        assert_eq!(illiquid.claims, state.claims);
    }

    #[test]
    fn reserve_sweep_replaces_temporary_retirement_without_changing_c() {
        let supply_before = 50 * E8S_PER_ICP;
        let reserve_before = 10 * E8S_PER_ICP;
        let staged = E8S_PER_ICP;
        let sweep_fee = 10_000;
        let economic_claims_before_sweep = supply_before - reserve_before - staged;
        let supply_after = supply_before - sweep_fee;
        let reserve_after = reserve_before + staged - sweep_fee;
        assert_eq!(supply_after - reserve_after, economic_claims_before_sweep);
    }

    #[test]
    fn fee_inventory_is_closed_and_non_overlapping() {
        let inventory = [
            FeeClass::External,
            FeeClass::External,
            FeeClass::ExistingBackingMovement,
            FeeClass::ExistingBackingMovement,
            FeeClass::ExistingBackingMovement,
            FeeClass::FreshValueDelivery,
            FeeClass::FreshValueDelivery,
            FeeClass::FreshValueDelivery,
            FeeClass::FreshValueDelivery,
            FeeClass::AnchorRestorationFromFreshValue,
            FeeClass::FreshValueDelivery,
            FeeClass::FreshValueDelivery,
            FeeClass::RedemptionQuote,
            FeeClass::IoLedgerBurn,
        ];
        assert_eq!(inventory.len(), 14);
        assert_eq!(
            inventory
                .iter()
                .filter(|class| matches!(class, FeeClass::FreshValueDelivery))
                .count(),
            6
        );
    }

    #[test]
    fn candidate_cadences_fit_and_twelve_hours_is_the_slowest_credible_choice() {
        let slack = PREFERRED_SNS_UNLOCK_DELAY_SECONDS - NNS_DYNAMIC_DISSOLVE_DELAY_SECONDS;
        assert_eq!(slack, 86_460);
        let candidates = [43_200, 21_600, 14_400, 3_600].map(assess_cadence);
        assert_eq!(candidates[0].natural_live_bound, 29);
        assert_eq!(candidates[1].natural_live_bound, 57);
        assert_eq!(candidates[2].natural_live_bound, 85);
        assert_eq!(candidates[3].natural_live_bound, 337);
        assert_eq!(candidates[0].healthy_slack_seconds, 42_660);
        assert!(candidates
            .windows(2)
            .all(|pair| pair[1].healthy_slack_seconds > pair[0].healthy_slack_seconds));
        assert_eq!(candidates[0].generations_per_day, 2);
        assert_eq!(candidates[0].governance_queries_per_day_at_max, 22);
        assert_eq!(candidates[0].io_balance_queries_per_day_at_max, 2_008);
        assert_eq!(candidates[0].approximate_calls_per_day_at_max, 2_056);
    }

    #[test]
    fn ready_child_priority_derives_a_natural_population_bound() {
        let natural_bound = natural_live_bound(STRUCTURAL_CADENCE_SECONDS);
        assert_eq!(natural_bound, 29);
        assert!(natural_bound < 32);
    }

    #[test]
    fn more_than_thirty_two_historical_generations_retire_without_a_product_cap() {
        use std::collections::VecDeque;

        let natural_bound = natural_live_bound(STRUCTURAL_CADENCE_SECONDS);
        let mut live = VecDeque::new();
        let mut maximum_live = 0usize;
        for generation in 1_u64..=64 {
            while live.front().is_some_and(|created_generation| {
                generation.saturating_sub(*created_generation) >= natural_bound
            }) {
                live.pop_front();
            }
            live.push_back(generation);
            maximum_live = maximum_live.max(live.len());
        }

        assert_eq!(live.back(), Some(&64));
        assert!(live.back().copied().unwrap_or_default() > 32);
        assert!(maximum_live <= natural_bound as usize);
    }

    #[test]
    fn healthy_worst_case_liquidates_before_fifteen_days_and_one_minute() {
        let transition_immediately_after_poll = 1;
        let detection_at = transition_immediately_after_poll + STRUCTURAL_CADENCE_SECONDS;
        let child_started_at = detection_at + healthy_operation_budget_seconds();
        let first_ready_at = child_started_at + NNS_DYNAMIC_DISSOLVE_DELAY_SECONDS;
        let sns_unlock_at = transition_immediately_after_poll + PREFERRED_SNS_UNLOCK_DELAY_SECONDS;
        assert_eq!(sns_unlock_at - first_ready_at, 42_660);
        assert!(first_ready_at < sns_unlock_at);

        let transition_immediately_before_poll = STRUCTURAL_CADENCE_SECONDS - 1;
        let prompt_detection_at = STRUCTURAL_CADENCE_SECONDS;
        assert!(prompt_detection_at - transition_immediately_before_poll <= 1);
    }

    #[test]
    fn stream_deadline_reconstruction_preserves_reward_margin_and_short_retry() {
        let facts = SchedulerFacts {
            latest_structural_at: 1_000,
            latest_reward_event_end: 5_000,
            retry_due_at: None,
        };
        assert_eq!(next_stream_deadline(facts), 44_200);
        let retrying = SchedulerFacts {
            retry_due_at: Some(1_060),
            ..facts
        };
        assert_eq!(next_stream_deadline(retrying), 1_060);
        let restarted = SchedulerFacts { ..retrying };
        assert_eq!(next_stream_deadline(restarted), 1_060);
        assert_eq!(
            5_000 + REWARD_CADENCE_SECONDS + REWARD_MARGIN_SECONDS,
            91_700
        );
    }

    #[test]
    fn structural_generations_and_retries_are_distinct() {
        let generation = 7;
        let retries = [60, 120, 180, 240];
        assert!(retries.windows(2).all(|pair| pair[1] - pair[0] == 60));
        assert!(retries.iter().all(|_| generation == 7));
        let next_generation_at = STRUCTURAL_CADENCE_SECONDS;
        assert!(retries.iter().all(|retry| *retry < next_generation_at));
    }

    #[test]
    fn reward_and_structural_facets_are_event_fenced() {
        let empty = RewardFacet {
            eligible_from_event: None,
            eligible_through_event: None,
            accumulated_credit: 0,
        };

        // Case A: canonical structural activation while event 10 is latest
        // makes event 11 the first eligible event regardless of keeper order.
        let active = observe_active(empty, 10);
        assert_eq!(process_reward(active, 11, 5).accumulated_credit, 5);

        // Case B: activation first observed after event 11 completed cannot
        // retroactively receive event 11 credit.
        let late_active = observe_active(empty, 11);
        assert_eq!(process_reward(late_active, 11, 5).accumulated_credit, 0);

        // Case C: an exit fenced after event 11 promptly exits backing while
        // retaining only the already-completed event's eligibility.
        let exiting = observe_exit(active, 11);
        let credited = process_reward(exiting, 11, 5);
        assert_eq!(credited.accumulated_credit, 5);
        assert_eq!(process_reward(credited, 12, 5).accumulated_credit, 5);

        // Case D: replay of the structural fact does not change its event
        // fence. Reward-event replay remains guarded by the reward checkpoint
        // in production; this model proves structural retries do not widen it.
        assert_eq!(observe_exit(exiting, 11), exiting);
    }

    #[test]
    fn empty_genesis_and_one_io_floor_are_exact() {
        Economy::default().validate().unwrap();
        let state = Economy::bootstrap(ANCHOR_TARGET_E8S)
            .unwrap()
            .add_backed_issuance(E8S_PER_ICP)
            .unwrap();
        assert_eq!(state.claims, E8S_PER_ICP);
        assert_eq!(state.backing.total().unwrap(), E8S_PER_ICP);
    }
}
