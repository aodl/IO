#![cfg(test)]

//! Executable proposal model for pooled claim-backing economics.
//!
//! This crate is test-only. It does not define the active IO economics and is
//! not linked into a canister.

pub mod proposed_model {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum ModelError {
        ArithmeticOverflow,
        ExclusionsExceedSupply,
        ZeroClaimSupply,
        BackingWithoutClaims,
        UncoveredClaims,
        ActiveWithoutClaims,
        ActiveExceedsClaims,
        InsufficientBacking,
        InsufficientOperationalReserve,
        InsufficientLiquid,
        InvalidBackingState,
        InvalidTransition,
        ProofMismatch,
        SameBucket,
    }

    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct Backing {
        pub liquid: u128,
        pub pooled: u128,
        pub pending_unwind: u128,
        pub transit: u128,
        pub permanent: u128,
        pub operational_reserve: u128,
    }

    impl Backing {
        pub fn claim_backing(self) -> Result<u128, ModelError> {
            self.liquid
                .checked_add(self.pooled)
                .and_then(|value| value.checked_add(self.pending_unwind))
                .and_then(|value| value.checked_add(self.transit))
                .ok_or(ModelError::ArithmeticOverflow)
        }

        pub fn total_assets(self) -> Result<u128, ModelError> {
            self.claim_backing()?
                .checked_add(self.permanent)
                .and_then(|value| value.checked_add(self.operational_reserve))
                .ok_or(ModelError::ArithmeticOverflow)
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Supply {
        pub total_io: u128,
        pub protocol_reserve_io: u128,
        pub nonredeemable_governance_io: u128,
    }

    impl Supply {
        pub fn claim_supply(self) -> Result<u128, ModelError> {
            let exclusions = self
                .protocol_reserve_io
                .checked_add(self.nonredeemable_governance_io)
                .ok_or(ModelError::ArithmeticOverflow)?;
            self.total_io
                .checked_sub(exclusions)
                .ok_or(ModelError::ExclusionsExceedSupply)
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum ClaimRate {
        EmptyGenesis,
        BackingWithoutClaims { backing: u128 },
        Ratio { backing: u128, claims: u128 },
    }

    pub fn claim_rate(backing: u128, claims: u128) -> Result<ClaimRate, ModelError> {
        match (backing, claims) {
            (0, 0) => Ok(ClaimRate::EmptyGenesis),
            (_, 0) => Ok(ClaimRate::BackingWithoutClaims { backing }),
            (0, _) => Err(ModelError::UncoveredClaims),
            _ => Ok(ClaimRate::Ratio { backing, claims }),
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Delta {
        Increase(u128),
        Decrease(u128),
    }

    impl Delta {
        pub fn apply(self, value: u128) -> Result<u128, ModelError> {
            match self {
                Self::Increase(amount) => value
                    .checked_add(amount)
                    .ok_or(ModelError::ArithmeticOverflow),
                Self::Decrease(amount) => value
                    .checked_sub(amount)
                    .ok_or(ModelError::InsufficientBacking),
            }
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct EconomicState {
        pub backing: Backing,
        pub supply: Supply,
        pub active_stake: u128,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct AllocationInput {
        pub state: EconomicState,
        pub net_claim_backing_increment: u128,
        pub claim_supply_delta: Delta,
        pub active_stake_delta: Delta,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Allocation {
        pub post_claim_backing: u128,
        pub post_claim_supply: u128,
        pub post_active_stake: u128,
        pub target_pool: u128,
        pub to_pool: u128,
        pub to_liquid: u128,
        pub remaining_under_target: u128,
        pub resulting_over_target: u128,
    }

    pub fn target_pool(active: u128, backing: u128, claims: u128) -> Result<u128, ModelError> {
        match (backing, claims, active) {
            (0, 0, 0) => return Ok(0),
            (_, 0, active) if active > 0 => return Err(ModelError::ActiveWithoutClaims),
            (backing, 0, 0) if backing > 0 => return Err(ModelError::BackingWithoutClaims),
            (0, claims, _) if claims > 0 => return Err(ModelError::UncoveredClaims),
            (_, claims, active) if active > claims => {
                return Err(ModelError::ActiveExceedsClaims);
            }
            _ => {}
        }
        active
            .checked_mul(backing)
            .ok_or(ModelError::ArithmeticOverflow)
            .map(|product| product / claims)
    }

    pub fn allocate(input: AllocationInput) -> Result<Allocation, ModelError> {
        let pre_backing = input.state.backing.claim_backing()?;
        if input.state.backing.pooled > pre_backing {
            return Err(ModelError::InvalidBackingState);
        }
        let pre_claims = input.state.supply.claim_supply()?;
        target_pool(input.state.active_stake, pre_backing, pre_claims)?;
        let post_claim_backing = pre_backing
            .checked_add(input.net_claim_backing_increment)
            .ok_or(ModelError::ArithmeticOverflow)?;
        let post_claim_supply = input.claim_supply_delta.apply(pre_claims)?;
        let post_active_stake = input.active_stake_delta.apply(input.state.active_stake)?;
        let target_pool = target_pool(post_active_stake, post_claim_backing, post_claim_supply)?;
        let pool_need = target_pool.saturating_sub(input.state.backing.pooled);
        let to_pool = input.net_claim_backing_increment.min(pool_need);
        let to_liquid = input
            .net_claim_backing_increment
            .checked_sub(to_pool)
            .ok_or(ModelError::ArithmeticOverflow)?;
        let resulting_pool = input
            .state
            .backing
            .pooled
            .checked_add(to_pool)
            .ok_or(ModelError::ArithmeticOverflow)?;
        Ok(Allocation {
            post_claim_backing,
            post_claim_supply,
            post_active_stake,
            target_pool,
            to_pool,
            to_liquid,
            remaining_under_target: target_pool.saturating_sub(resulting_pool),
            resulting_over_target: resulting_pool.saturating_sub(target_pool),
        })
    }

    pub fn io_release_at_pre_event_rate(
        claim_backing_increment: u128,
        pre_backing: u128,
        pre_claims: u128,
    ) -> Result<u128, ModelError> {
        match (pre_backing, pre_claims) {
            (0, 0) => Ok(claim_backing_increment),
            (0, _) => Err(ModelError::UncoveredClaims),
            (_, 0) => Err(ModelError::BackingWithoutClaims),
            _ => claim_backing_increment
                .checked_mul(pre_claims)
                .ok_or(ModelError::ArithmeticOverflow)
                .map(|product| product / pre_backing),
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct ReserveDelivery {
        pub delivered_io: u128,
        pub reserve_debit: u128,
        pub total_supply_burn: u128,
        pub claim_supply_increment: u128,
    }

    pub fn reserve_delivery(
        delivered_io: u128,
        io_ledger_fee: u128,
    ) -> Result<ReserveDelivery, ModelError> {
        Ok(ReserveDelivery {
            delivered_io,
            reserve_debit: delivered_io
                .checked_add(io_ledger_fee)
                .ok_or(ModelError::ArithmeticOverflow)?,
            total_supply_burn: io_ledger_fee,
            claim_supply_increment: delivered_io,
        })
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct MaturitySplit {
        pub permanent_leg: u128,
        pub gross_claim_leg: u128,
        pub net_claim_increment: u128,
    }

    pub fn maturity_split(
        actual_mint: u128,
        claim_reducing_fees: u128,
    ) -> Result<MaturitySplit, ModelError> {
        let permanent_leg = actual_mint
            .checked_mul(40)
            .ok_or(ModelError::ArithmeticOverflow)?
            / 100;
        let gross_claim_leg = actual_mint
            .checked_sub(permanent_leg)
            .ok_or(ModelError::ArithmeticOverflow)?;
        let net_claim_increment = gross_claim_leg
            .checked_sub(claim_reducing_fees)
            .ok_or(ModelError::InsufficientBacking)?;
        Ok(MaturitySplit {
            permanent_leg,
            gross_claim_leg,
            net_claim_increment,
        })
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum ClaimLegRoute {
        AllLiquid,
        AllPool,
        Mixed,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct MaturityRouteInput {
        pub state: EconomicState,
        pub actual_mint: u128,
        pub claim_supply_delta: Delta,
        pub active_stake_delta: Delta,
        pub permanent_transfer_fee: u128,
        pub claim_transfer_fee: u128,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct MaturityRoutePlan {
        pub route: ClaimLegRoute,
        pub route_evaluations: u8,
        pub permanent_source_debit: u128,
        pub permanent_credit: u128,
        pub claim_staging_source_debit: u128,
        pub first_claim_credit: u128,
        pub liquid_to_pool_source_debit: Option<u128>,
        pub liquid_credit: u128,
        pub pooled_credit: u128,
        pub permanent_fee: u128,
        pub claim_fees: u128,
        pub post_backing: u128,
        pub post_permanent: u128,
        pub post_target: u128,
        pub remaining_under_target: u128,
        pub resulting_over_target: u128,
    }

    pub fn plan_maturity_route(input: MaturityRouteInput) -> Result<MaturityRoutePlan, ModelError> {
        let split = maturity_split(input.actual_mint, 0)?;
        let permanent_credit = split
            .permanent_leg
            .checked_sub(input.permanent_transfer_fee)
            .ok_or(ModelError::InsufficientBacking)?;
        let first_claim_credit = split
            .gross_claim_leg
            .checked_sub(input.claim_transfer_fee)
            .ok_or(ModelError::InsufficientBacking)?;
        let allocation_input = |increment| AllocationInput {
            state: input.state,
            net_claim_backing_increment: increment,
            claim_supply_delta: input.claim_supply_delta,
            active_stake_delta: input.active_stake_delta,
        };
        let one_fee = allocate(allocation_input(first_claim_credit))?;

        let (route, route_evaluations, allocation, second_source_debit) = if one_fee.to_pool == 0 {
            (ClaimLegRoute::AllLiquid, 1, one_fee, None)
        } else if one_fee.to_liquid == 0 {
            (ClaimLegRoute::AllPool, 1, one_fee, None)
        } else {
            let two_fee_credit = first_claim_credit
                .checked_sub(input.claim_transfer_fee)
                .ok_or(ModelError::InsufficientBacking)?;
            let two_fee = allocate(allocation_input(two_fee_credit))?;
            if two_fee.to_pool == 0 {
                // Paying a second fee would erase the pool delta. Keep the
                // one-fee all-liquid result and expose its bounded residual.
                let mut adjusted = one_fee;
                adjusted.to_pool = 0;
                adjusted.to_liquid = first_claim_credit;
                adjusted.remaining_under_target = adjusted
                    .target_pool
                    .saturating_sub(input.state.backing.pooled);
                adjusted.resulting_over_target = input
                    .state
                    .backing
                    .pooled
                    .saturating_sub(adjusted.target_pool);
                (ClaimLegRoute::AllLiquid, 2, adjusted, None)
            } else if two_fee.to_liquid == 0 {
                // Paying a second fee would consume the liquid remainder.
                // The direct one-fee pool route is the deterministic edge.
                let mut adjusted = one_fee;
                adjusted.to_pool = first_claim_credit;
                adjusted.to_liquid = 0;
                let resulting_pool = input
                    .state
                    .backing
                    .pooled
                    .checked_add(first_claim_credit)
                    .ok_or(ModelError::ArithmeticOverflow)?;
                adjusted.remaining_under_target =
                    adjusted.target_pool.saturating_sub(resulting_pool);
                adjusted.resulting_over_target =
                    resulting_pool.saturating_sub(adjusted.target_pool);
                (ClaimLegRoute::AllPool, 2, adjusted, None)
            } else {
                let source_debit = two_fee
                    .to_pool
                    .checked_add(input.claim_transfer_fee)
                    .ok_or(ModelError::ArithmeticOverflow)?;
                (ClaimLegRoute::Mixed, 2, two_fee, Some(source_debit))
            }
        };

        let claim_fee_count = if route == ClaimLegRoute::Mixed { 2 } else { 1 };
        let claim_fees = input
            .claim_transfer_fee
            .checked_mul(claim_fee_count)
            .ok_or(ModelError::ArithmeticOverflow)?;
        Ok(MaturityRoutePlan {
            route,
            route_evaluations,
            permanent_source_debit: split.permanent_leg,
            permanent_credit,
            claim_staging_source_debit: split.gross_claim_leg,
            first_claim_credit,
            liquid_to_pool_source_debit: second_source_debit,
            liquid_credit: allocation.to_liquid,
            pooled_credit: allocation.to_pool,
            permanent_fee: input.permanent_transfer_fee,
            claim_fees,
            post_backing: allocation.post_claim_backing,
            post_permanent: input
                .state
                .backing
                .permanent
                .checked_add(permanent_credit)
                .ok_or(ModelError::ArithmeticOverflow)?,
            post_target: allocation.target_pool,
            remaining_under_target: allocation.remaining_under_target,
            resulting_over_target: allocation.resulting_over_target,
        })
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Bucket {
        Liquid,
        Pooled,
        PendingUnwind,
        Transit,
    }

    fn bucket_value(backing: Backing, bucket: Bucket) -> u128 {
        match bucket {
            Bucket::Liquid => backing.liquid,
            Bucket::Pooled => backing.pooled,
            Bucket::PendingUnwind => backing.pending_unwind,
            Bucket::Transit => backing.transit,
        }
    }

    fn set_bucket(backing: &mut Backing, bucket: Bucket, value: u128) {
        match bucket {
            Bucket::Liquid => backing.liquid = value,
            Bucket::Pooled => backing.pooled = value,
            Bucket::PendingUnwind => backing.pending_unwind = value,
            Bucket::Transit => backing.transit = value,
        }
    }

    pub fn move_backing(
        mut backing: Backing,
        from: Bucket,
        to: Bucket,
        gross: u128,
        fee: u128,
    ) -> Result<Backing, ModelError> {
        if from == to {
            return Err(ModelError::SameBucket);
        }
        let credited = gross
            .checked_sub(fee)
            .ok_or(ModelError::InsufficientBacking)?;
        let source = bucket_value(backing, from)
            .checked_sub(gross)
            .ok_or(ModelError::InsufficientBacking)?;
        let destination = bucket_value(backing, to)
            .checked_add(credited)
            .ok_or(ModelError::ArithmeticOverflow)?;
        set_bucket(&mut backing, from, source);
        set_bucket(&mut backing, to, destination);
        Ok(backing)
    }

    pub fn assert_conservation(
        pre: Backing,
        post: Backing,
        external_mint_or_inflow: u128,
        explicit_fees: u128,
        external_payouts: u128,
    ) -> Result<(), ModelError> {
        let expected = pre
            .total_assets()?
            .checked_add(external_mint_or_inflow)
            .and_then(|value| value.checked_sub(explicit_fees))
            .and_then(|value| value.checked_sub(external_payouts))
            .ok_or(ModelError::ArithmeticOverflow)?;
        if post.total_assets()? == expected {
            Ok(())
        } else {
            Err(ModelError::InvalidBackingState)
        }
    }

    pub fn policy_a_fee(
        mut backing: Backing,
        bucket: Bucket,
        fee: u128,
    ) -> Result<Backing, ModelError> {
        let remaining = bucket_value(backing, bucket)
            .checked_sub(fee)
            .ok_or(ModelError::InsufficientBacking)?;
        set_bucket(&mut backing, bucket, remaining);
        Ok(backing)
    }

    pub fn policy_b_fee(
        mut backing: Backing,
        bucket: Bucket,
        fee: u128,
    ) -> Result<Backing, ModelError> {
        if backing.operational_reserve < fee {
            return Err(ModelError::InsufficientOperationalReserve);
        }
        backing = policy_a_fee(backing, bucket, fee)?;
        backing.operational_reserve -= fee;
        backing.liquid = backing
            .liquid
            .checked_add(fee)
            .ok_or(ModelError::ArithmeticOverflow)?;
        Ok(backing)
    }

    pub fn policy_b_or_a_fee(
        backing: Backing,
        bucket: Bucket,
        fee: u128,
    ) -> Result<Backing, ModelError> {
        if backing.operational_reserve >= fee {
            policy_b_fee(backing, bucket, fee)
        } else {
            policy_a_fee(backing, bucket, fee)
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct PolicyBMaturity {
        pub reserve_replenishment: u128,
        pub remaining_mint: u128,
        pub permanent_leg: u128,
        pub new_claim_backing: u128,
        pub post_reserve: u128,
    }

    pub fn policy_b_maturity(
        actual_mint: u128,
        current_reserve: u128,
        reserve_target: u128,
    ) -> Result<PolicyBMaturity, ModelError> {
        let reserve_replenishment = actual_mint.min(reserve_target.saturating_sub(current_reserve));
        let remaining_mint = actual_mint
            .checked_sub(reserve_replenishment)
            .ok_or(ModelError::ArithmeticOverflow)?;
        let permanent_leg = remaining_mint
            .checked_mul(40)
            .ok_or(ModelError::ArithmeticOverflow)?
            / 100;
        Ok(PolicyBMaturity {
            reserve_replenishment,
            remaining_mint,
            permanent_leg,
            new_claim_backing: remaining_mint
                .checked_sub(permanent_leg)
                .ok_or(ModelError::ArithmeticOverflow)?,
            post_reserve: current_reserve
                .checked_add(reserve_replenishment)
                .ok_or(ModelError::ArithmeticOverflow)?,
        })
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct FeeCounter {
        pub unreimbursed: u128,
    }

    pub fn policy_c_fee(
        backing: Backing,
        counter: FeeCounter,
        bucket: Bucket,
        fee: u128,
    ) -> Result<(Backing, FeeCounter), ModelError> {
        Ok((
            policy_a_fee(backing, bucket, fee)?,
            FeeCounter {
                unreimbursed: counter
                    .unreimbursed
                    .checked_add(fee)
                    .ok_or(ModelError::ArithmeticOverflow)?,
            },
        ))
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct PolicyCMaturity {
        pub reimbursement: u128,
        pub remaining_mint: u128,
        pub permanent_leg: u128,
        pub new_claim_backing: u128,
        pub post_counter: FeeCounter,
    }

    pub fn policy_c_maturity(
        actual_mint: u128,
        counter: FeeCounter,
    ) -> Result<PolicyCMaturity, ModelError> {
        let reimbursement = actual_mint.min(counter.unreimbursed);
        let remaining_mint = actual_mint
            .checked_sub(reimbursement)
            .ok_or(ModelError::ArithmeticOverflow)?;
        let permanent_leg = remaining_mint
            .checked_mul(40)
            .ok_or(ModelError::ArithmeticOverflow)?
            / 100;
        let new_claim_backing = reimbursement
            .checked_add(
                remaining_mint
                    .checked_sub(permanent_leg)
                    .ok_or(ModelError::ArithmeticOverflow)?,
            )
            .ok_or(ModelError::ArithmeticOverflow)?;
        Ok(PolicyCMaturity {
            reimbursement,
            remaining_mint,
            permanent_leg,
            new_claim_backing,
            post_counter: FeeCounter {
                unreimbursed: counter.unreimbursed - reimbursement,
            },
        })
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum HoldReason {
        BelowMinimumStake,
        FeeTolerance,
        ChildMinimum,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum ReconcilePlan {
        Hold {
            post_fee_target: u128,
            tolerance: u128,
            reason: HoldReason,
        },
        TopUp {
            post_fee_target: u128,
            credited: u128,
            source_debit: u128,
            creates_parent: bool,
        },
        Unwind {
            post_fee_target: u128,
            gross: u128,
            expected_credit: u128,
        },
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct ReconcileInput {
        pub backing: Backing,
        pub claims: u128,
        pub active_stake: u128,
        pub next_fee: u128,
        pub minimum_child_gross: u128,
        pub minimum_parent: u128,
        pub parent_exists: bool,
    }

    pub fn plan_reconciliation(input: ReconcileInput) -> Result<ReconcilePlan, ModelError> {
        let backing = input.backing.claim_backing()?;
        let raw_target = target_pool(input.active_stake, backing, input.claims)?;
        if !input.parent_exists {
            if input.backing.pooled != 0 {
                return Err(ModelError::InvalidBackingState);
            }
            if raw_target < input.minimum_parent {
                return Ok(ReconcilePlan::Hold {
                    post_fee_target: raw_target,
                    tolerance: input.minimum_parent.saturating_sub(1),
                    reason: HoldReason::BelowMinimumStake,
                });
            }
        }
        let post_fee_backing = backing
            .checked_sub(input.next_fee)
            .ok_or(ModelError::InsufficientBacking)?;
        let post_fee_raw_target = target_pool(input.active_stake, post_fee_backing, input.claims)?;
        let post_fee_target = post_fee_raw_target.max(input.minimum_parent);
        let pooled = input.backing.pooled;
        if pooled < post_fee_target {
            let credited = post_fee_target - pooled;
            if credited <= input.next_fee {
                return Ok(ReconcilePlan::Hold {
                    post_fee_target,
                    tolerance: input.next_fee,
                    reason: HoldReason::FeeTolerance,
                });
            }
            let source_debit = credited
                .checked_add(input.next_fee)
                .ok_or(ModelError::ArithmeticOverflow)?;
            if input.backing.liquid < source_debit {
                return Err(ModelError::InsufficientLiquid);
            }
            return Ok(ReconcilePlan::TopUp {
                post_fee_target,
                credited,
                source_debit,
                creates_parent: !input.parent_exists,
            });
        }
        let excess = pooled - post_fee_target;
        if excess < input.minimum_child_gross {
            return Ok(ReconcilePlan::Hold {
                post_fee_target,
                tolerance: input.minimum_child_gross.saturating_sub(1),
                reason: HoldReason::ChildMinimum,
            });
        }
        Ok(ReconcilePlan::Unwind {
            post_fee_target,
            gross: excess,
            expected_credit: excess
                .checked_sub(input.next_fee)
                .ok_or(ModelError::InsufficientBacking)?,
        })
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum CanonicalSnsState {
        Active,
        Dissolving,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum StickyStatus {
        ActiveBacked,
        ExitObserved,
        ExitCommitted,
        ReentryPending,
        LiquidReturned,
        RestakePlanned,
        RestakeCommitted,
        RestakeProved,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct StickyNeuron {
        pub status: StickyStatus,
        pub latest_sns_state: CanonicalSnsState,
        pub committed_generation: Option<u64>,
        pub reward_eligible_from_observation: Option<u64>,
    }

    impl Default for StickyNeuron {
        fn default() -> Self {
            Self {
                status: StickyStatus::ActiveBacked,
                latest_sns_state: CanonicalSnsState::Active,
                committed_generation: None,
                reward_eligible_from_observation: Some(0),
            }
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum CohortLifecycle {
        Dissolving,
        Ready,
        Returned,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum CohortProofState {
        PrincipalProved,
        DisbursementSubmitted,
        ReturnProved,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct PassiveCohort {
        pub generation: u64,
        pub principal: u128,
        pub lifecycle: CohortLifecycle,
        pub proof: CohortProofState,
        pub ready_at: u64,
    }

    impl StickyNeuron {
        pub fn reward_eligible_at(self, observation: u64) -> bool {
            self.status == StickyStatus::ActiveBacked
                && self.latest_sns_state == CanonicalSnsState::Active
                && self
                    .reward_eligible_from_observation
                    .is_some_and(|first| observation >= first)
        }
    }

    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct StickyOperationCounts {
        pub split_intents: u128,
        pub split_proofs: u128,
        pub merge_proofs: u128,
        pub disbursement_proofs: u128,
        pub restake_proofs: u128,
    }

    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct StickyFeeTotals {
        pub split: u128,
        pub merge: u128,
        pub disbursement: u128,
        pub restake: u128,
    }

    impl StickyFeeTotals {
        pub fn total(self) -> Result<u128, ModelError> {
            self.split
                .checked_add(self.merge)
                .and_then(|value| value.checked_add(self.disbursement))
                .and_then(|value| value.checked_add(self.restake))
                .ok_or(ModelError::ArithmeticOverflow)
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum ReturnedLiquidityPlan {
        LatestStateDissolving,
        RestoredWithoutTransfer {
            target: u128,
        },
        Hold {
            target: u128,
            required_credit: u128,
            tolerance: u128,
            reason: HoldReason,
        },
        Restake {
            post_fee_target: u128,
            credited: u128,
            source_debit: u128,
        },
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct ReturnedLiquidityInput {
        pub generation: u64,
        pub claims: u128,
        pub active_backing: u128,
        pub exact_restake_fee: u128,
        pub minimum_restake_credit: u128,
        pub minimum_parent: u128,
        pub next_reward_observation: u64,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct RestakeIntent {
        pub generation: u64,
        pub post_fee_target: u128,
        pub credited: u128,
        pub source_debit: u128,
        pub fee: u128,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum ActiveNnsCommand {
        Split {
            generation: u64,
            gross: u128,
            started_at: u64,
            dissolve_delay: u64,
        },
        Disburse {
            generation: u64,
        },
        Restake(RestakeIntent),
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct StickyUnwindModel {
        pub neurons: Vec<StickyNeuron>,
        pub backing: Backing,
        pub cohorts: Vec<PassiveCohort>,
        pub max_passive_cohorts: usize,
        pub next_generation: u64,
        pub active_command: Option<ActiveNnsCommand>,
        pub planned_restake: Option<RestakeIntent>,
        pub operations: StickyOperationCounts,
        pub fees: StickyFeeTotals,
    }

    impl StickyUnwindModel {
        pub fn with_neurons(
            backing: Backing,
            neuron_count: usize,
            max_passive_cohorts: usize,
        ) -> Self {
            Self {
                neurons: vec![StickyNeuron::default(); neuron_count],
                backing,
                cohorts: Vec::new(),
                max_passive_cohorts,
                next_generation: 0,
                active_command: None,
                planned_restake: None,
                operations: StickyOperationCounts::default(),
                fees: StickyFeeTotals::default(),
            }
        }

        pub fn observe_sns(
            &mut self,
            neuron_index: usize,
            state: CanonicalSnsState,
            observation: u64,
        ) -> Result<(), ModelError> {
            let neuron = self
                .neurons
                .get_mut(neuron_index)
                .ok_or(ModelError::InvalidTransition)?;
            neuron.latest_sns_state = state;
            match state {
                CanonicalSnsState::Dissolving => {
                    neuron.reward_eligible_from_observation = None;
                    neuron.status = match neuron.status {
                        StickyStatus::ActiveBacked => StickyStatus::ExitObserved,
                        StickyStatus::ExitObserved => StickyStatus::ExitObserved,
                        StickyStatus::ExitCommitted | StickyStatus::ReentryPending => {
                            StickyStatus::ExitCommitted
                        }
                        StickyStatus::LiquidReturned => StickyStatus::LiquidReturned,
                        StickyStatus::RestakePlanned => {
                            self.planned_restake = None;
                            StickyStatus::LiquidReturned
                        }
                        StickyStatus::RestakeCommitted => StickyStatus::RestakeCommitted,
                        StickyStatus::RestakeProved => StickyStatus::RestakeProved,
                    };
                }
                CanonicalSnsState::Active => {
                    neuron.status = match neuron.status {
                        StickyStatus::ExitObserved => {
                            neuron.reward_eligible_from_observation = Some(
                                observation
                                    .checked_add(1)
                                    .ok_or(ModelError::ArithmeticOverflow)?,
                            );
                            neuron.committed_generation = None;
                            StickyStatus::ActiveBacked
                        }
                        StickyStatus::ExitCommitted => StickyStatus::ReentryPending,
                        status => status,
                    };
                }
            }
            Ok(())
        }

        pub fn submit_split_intent(
            &mut self,
            neuron_index: usize,
            gross: u128,
            started_at: u64,
            dissolve_delay: u64,
        ) -> Result<u64, ModelError> {
            let neuron = self
                .neurons
                .get_mut(neuron_index)
                .ok_or(ModelError::InvalidTransition)?;
            if neuron.status != StickyStatus::ExitObserved
                || neuron.latest_sns_state != CanonicalSnsState::Dissolving
                || self.active_command.is_some()
                || self.cohorts.len() >= self.max_passive_cohorts
            {
                return Err(ModelError::InvalidTransition);
            }
            let generation = self.next_generation;
            self.next_generation = self
                .next_generation
                .checked_add(1)
                .ok_or(ModelError::ArithmeticOverflow)?;
            self.backing = move_backing(self.backing, Bucket::Pooled, Bucket::Transit, gross, 0)?;
            self.active_command = Some(ActiveNnsCommand::Split {
                generation,
                gross,
                started_at,
                dissolve_delay,
            });
            self.operations.split_intents = self
                .operations
                .split_intents
                .checked_add(1)
                .ok_or(ModelError::ArithmeticOverflow)?;
            neuron.status = StickyStatus::ExitCommitted;
            neuron.committed_generation = Some(generation);
            Ok(generation)
        }

        pub fn prove_split(&mut self, exact_fee: u128) -> Result<(), ModelError> {
            let (generation, gross, started_at, dissolve_delay) = match self.active_command {
                Some(ActiveNnsCommand::Split {
                    generation,
                    gross,
                    started_at,
                    dissolve_delay,
                }) => (generation, gross, started_at, dissolve_delay),
                _ => return Err(ModelError::InvalidTransition),
            };
            let credited = gross
                .checked_sub(exact_fee)
                .ok_or(ModelError::InsufficientBacking)?;
            self.backing = move_backing(
                self.backing,
                Bucket::Transit,
                Bucket::PendingUnwind,
                gross,
                exact_fee,
            )?;
            self.cohorts.push(PassiveCohort {
                generation,
                principal: credited,
                lifecycle: CohortLifecycle::Dissolving,
                proof: CohortProofState::PrincipalProved,
                ready_at: started_at
                    .checked_add(dissolve_delay)
                    .ok_or(ModelError::ArithmeticOverflow)?,
            });
            self.active_command = None;
            self.operations.split_proofs = self
                .operations
                .split_proofs
                .checked_add(1)
                .ok_or(ModelError::ArithmeticOverflow)?;
            self.fees.split = self
                .fees
                .split
                .checked_add(exact_fee)
                .ok_or(ModelError::ArithmeticOverflow)?;
            Ok(())
        }

        pub fn refresh_cohort_readiness(&mut self, now: u64) {
            for cohort in &mut self.cohorts {
                if cohort.lifecycle == CohortLifecycle::Dissolving && now >= cohort.ready_at {
                    cohort.lifecycle = CohortLifecycle::Ready;
                }
            }
        }

        pub fn submit_child_disbursement(
            &mut self,
            generation: u64,
            now: u64,
        ) -> Result<(), ModelError> {
            if self.active_command.is_some() {
                return Err(ModelError::InvalidTransition);
            }
            self.refresh_cohort_readiness(now);
            let cohort = self
                .cohorts
                .iter_mut()
                .find(|cohort| cohort.generation == generation)
                .ok_or(ModelError::InvalidTransition)?;
            if cohort.lifecycle != CohortLifecycle::Ready
                || cohort.proof != CohortProofState::PrincipalProved
            {
                return Err(ModelError::InvalidTransition);
            }
            cohort.proof = CohortProofState::DisbursementSubmitted;
            self.active_command = Some(ActiveNnsCommand::Disburse { generation });
            Ok(())
        }

        pub fn prove_child_disbursement(
            &mut self,
            generation: u64,
            exact_fee: u128,
        ) -> Result<(), ModelError> {
            if self.active_command != Some(ActiveNnsCommand::Disburse { generation }) {
                return Err(ModelError::InvalidTransition);
            }
            let cohort_index = self
                .cohorts
                .iter()
                .position(|cohort| cohort.generation == generation)
                .ok_or(ModelError::InvalidTransition)?;
            let principal = self.cohorts[cohort_index].principal;
            let next_backing = move_backing(
                self.backing,
                Bucket::PendingUnwind,
                Bucket::Liquid,
                principal,
                exact_fee,
            )?;
            self.backing = next_backing;
            self.cohorts[cohort_index].lifecycle = CohortLifecycle::Returned;
            self.cohorts[cohort_index].proof = CohortProofState::ReturnProved;
            self.active_command = None;
            self.operations.disbursement_proofs = self
                .operations
                .disbursement_proofs
                .checked_add(1)
                .ok_or(ModelError::ArithmeticOverflow)?;
            self.fees.disbursement = self
                .fees
                .disbursement
                .checked_add(exact_fee)
                .ok_or(ModelError::ArithmeticOverflow)?;
            for neuron in &mut self.neurons {
                if neuron.committed_generation == Some(generation) {
                    neuron.status = StickyStatus::LiquidReturned;
                }
            }
            Ok(())
        }

        pub fn plan_returned_liquidity(
            &mut self,
            input: ReturnedLiquidityInput,
        ) -> Result<ReturnedLiquidityPlan, ModelError> {
            let ReturnedLiquidityInput {
                generation,
                claims,
                active_backing,
                exact_restake_fee,
                minimum_restake_credit,
                minimum_parent,
                next_reward_observation,
            } = input;
            if self.active_command.is_some() || self.planned_restake.is_some() {
                return Err(ModelError::InvalidTransition);
            }
            let cohort = self
                .cohorts
                .iter()
                .find(|cohort| cohort.generation == generation)
                .ok_or(ModelError::InvalidTransition)?;
            if cohort.lifecycle != CohortLifecycle::Returned
                || cohort.proof != CohortProofState::ReturnProved
            {
                return Err(ModelError::InvalidTransition);
            }
            let matching: Vec<usize> = self
                .neurons
                .iter()
                .enumerate()
                .filter_map(|(index, neuron)| {
                    (neuron.committed_generation == Some(generation)).then_some(index)
                })
                .collect();
            if matching.is_empty() {
                return Err(ModelError::InvalidTransition);
            }
            if matching
                .iter()
                .any(|index| self.neurons[*index].latest_sns_state == CanonicalSnsState::Dissolving)
            {
                return Ok(ReturnedLiquidityPlan::LatestStateDissolving);
            }

            let backing = self.backing.claim_backing()?;
            let current_target = target_pool(active_backing, backing, claims)?.max(minimum_parent);
            if self.backing.pooled >= current_target {
                self.restore_generation_without_transfer(generation, next_reward_observation);
                return Ok(ReturnedLiquidityPlan::RestoredWithoutTransfer {
                    target: current_target,
                });
            }

            let post_fee_backing = backing
                .checked_sub(exact_restake_fee)
                .ok_or(ModelError::InsufficientBacking)?;
            let post_fee_target =
                target_pool(active_backing, post_fee_backing, claims)?.max(minimum_parent);
            let required_credit = post_fee_target.saturating_sub(self.backing.pooled);
            let tolerance = exact_restake_fee.max(minimum_restake_credit.saturating_sub(1));
            if required_credit <= exact_restake_fee || required_credit < minimum_restake_credit {
                return Ok(ReturnedLiquidityPlan::Hold {
                    target: post_fee_target,
                    required_credit,
                    tolerance,
                    reason: if required_credit <= exact_restake_fee {
                        HoldReason::FeeTolerance
                    } else {
                        HoldReason::BelowMinimumStake
                    },
                });
            }
            let source_debit = required_credit
                .checked_add(exact_restake_fee)
                .ok_or(ModelError::ArithmeticOverflow)?;
            if self.backing.liquid < source_debit {
                return Err(ModelError::InsufficientLiquid);
            }
            let intent = RestakeIntent {
                generation,
                post_fee_target,
                credited: required_credit,
                source_debit,
                fee: exact_restake_fee,
            };
            self.planned_restake = Some(intent);
            for index in matching {
                self.neurons[index].status = StickyStatus::RestakePlanned;
                self.neurons[index].reward_eligible_from_observation = None;
            }
            Ok(ReturnedLiquidityPlan::Restake {
                post_fee_target,
                credited: required_credit,
                source_debit,
            })
        }

        pub fn commit_restake(&mut self, generation: u64) -> Result<(), ModelError> {
            if self.active_command.is_some() {
                return Err(ModelError::InvalidTransition);
            }
            let intent = self
                .planned_restake
                .filter(|intent| intent.generation == generation)
                .ok_or(ModelError::InvalidTransition)?;
            let next_backing = move_backing(
                self.backing,
                Bucket::Liquid,
                Bucket::Transit,
                intent.source_debit,
                0,
            )?;
            self.backing = next_backing;
            self.planned_restake = None;
            self.active_command = Some(ActiveNnsCommand::Restake(intent));
            for neuron in &mut self.neurons {
                if neuron.committed_generation == Some(generation) {
                    neuron.status = StickyStatus::RestakeCommitted;
                }
            }
            Ok(())
        }

        pub fn prove_restake(
            &mut self,
            generation: u64,
            actual_credited: u128,
        ) -> Result<(), ModelError> {
            let intent = match self.active_command {
                Some(ActiveNnsCommand::Restake(intent)) if intent.generation == generation => {
                    intent
                }
                _ => return Err(ModelError::InvalidTransition),
            };
            if actual_credited != intent.credited {
                return Err(ModelError::ProofMismatch);
            }
            let next_backing = move_backing(
                self.backing,
                Bucket::Transit,
                Bucket::Pooled,
                intent.source_debit,
                intent.fee,
            )?;
            if next_backing.pooled != intent.post_fee_target {
                return Err(ModelError::ProofMismatch);
            }
            self.backing = next_backing;
            self.active_command = None;
            self.operations.restake_proofs = self
                .operations
                .restake_proofs
                .checked_add(1)
                .ok_or(ModelError::ArithmeticOverflow)?;
            self.fees.restake = self
                .fees
                .restake
                .checked_add(intent.fee)
                .ok_or(ModelError::ArithmeticOverflow)?;
            for neuron in &mut self.neurons {
                if neuron.committed_generation == Some(generation) {
                    neuron.status = StickyStatus::RestakeProved;
                }
            }
            Ok(())
        }

        pub fn finish_restake(
            &mut self,
            generation: u64,
            next_reward_observation: u64,
        ) -> Result<(), ModelError> {
            let mut found = false;
            for neuron in &mut self.neurons {
                if neuron.committed_generation == Some(generation) {
                    if neuron.status != StickyStatus::RestakeProved {
                        return Err(ModelError::InvalidTransition);
                    }
                    found = true;
                    neuron.reward_eligible_from_observation = None;
                    neuron.committed_generation = None;
                    neuron.status = if neuron.latest_sns_state == CanonicalSnsState::Active {
                        neuron.reward_eligible_from_observation = Some(next_reward_observation);
                        StickyStatus::ActiveBacked
                    } else {
                        StickyStatus::ExitObserved
                    };
                }
            }
            if !found {
                return Err(ModelError::InvalidTransition);
            }
            Ok(())
        }

        pub fn cohort(&self, generation: u64) -> Option<PassiveCohort> {
            self.cohorts
                .iter()
                .copied()
                .find(|cohort| cohort.generation == generation)
        }

        fn restore_generation_without_transfer(
            &mut self,
            generation: u64,
            next_reward_observation: u64,
        ) {
            for neuron in &mut self.neurons {
                if neuron.committed_generation == Some(generation)
                    && neuron.latest_sns_state == CanonicalSnsState::Active
                {
                    neuron.status = StickyStatus::ActiveBacked;
                    neuron.committed_generation = None;
                    neuron.reward_eligible_from_observation = Some(next_reward_observation);
                }
            }
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct LiquidityLagInputs {
        pub guaranteed_reconciliation_cadence: Option<u64>,
        pub nns_dissolve_delay: u64,
        pub max_detection_margin: u64,
        pub max_command_margin: u64,
        pub max_disbursement_margin: u64,
    }

    pub fn liquidity_lag_bound(input: LiquidityLagInputs) -> Result<Option<u64>, ModelError> {
        let Some(cadence) = input.guaranteed_reconciliation_cadence else {
            return Ok(None);
        };
        cadence
            .checked_add(input.nns_dissolve_delay)
            .and_then(|value| value.checked_add(input.max_detection_margin))
            .and_then(|value| value.checked_add(input.max_command_margin))
            .and_then(|value| value.checked_add(input.max_disbursement_margin))
            .map(Some)
            .ok_or(ModelError::ArithmeticOverflow)
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct LifecycleFeeComparison {
        pub immediate_mirroring: u128,
        pub sticky_unwind: u128,
    }

    pub fn lifecycle_fee_comparison(
        cancel_start_pairs: u128,
        split_fee: u128,
        merge_fee: u128,
        disbursement_fee: u128,
        restake_fee: u128,
    ) -> Result<LifecycleFeeComparison, ModelError> {
        let repeated_split_fees = cancel_start_pairs
            .checked_add(1)
            .and_then(|count| count.checked_mul(split_fee))
            .ok_or(ModelError::ArithmeticOverflow)?;
        let repeated_merge_fees = cancel_start_pairs
            .checked_mul(merge_fee)
            .ok_or(ModelError::ArithmeticOverflow)?;
        let immediate_mirroring = repeated_split_fees
            .checked_add(repeated_merge_fees)
            .and_then(|value| value.checked_add(disbursement_fee))
            .ok_or(ModelError::ArithmeticOverflow)?;
        let sticky_unwind = split_fee
            .checked_add(disbursement_fee)
            .and_then(|value| value.checked_add(restake_fee))
            .ok_or(ModelError::ArithmeticOverflow)?;
        Ok(LifecycleFeeComparison {
            immediate_mirroring,
            sticky_unwind,
        })
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum RedemptionReadiness {
        AwaitLiquidity { gross_quote: u128 },
        Ready { gross_quote: u128, net_payout: u128 },
    }

    pub fn redemption_readiness(
        io_amount: u128,
        backing: Backing,
        claim_supply: u128,
        payout_fee: u128,
    ) -> Result<RedemptionReadiness, ModelError> {
        if claim_supply == 0 {
            return Err(ModelError::ZeroClaimSupply);
        }
        let gross_quote = io_amount
            .checked_mul(backing.claim_backing()?)
            .ok_or(ModelError::ArithmeticOverflow)?
            / claim_supply;
        if backing.liquid < gross_quote {
            return Ok(RedemptionReadiness::AwaitLiquidity { gross_quote });
        }
        let net_payout = gross_quote
            .checked_sub(payout_fee)
            .ok_or(ModelError::InsufficientBacking)?;
        Ok(RedemptionReadiness::Ready {
            gross_quote,
            net_payout,
        })
    }
}

mod tests {
    use super::proposed_model::*;

    fn supply(claims: u128) -> Supply {
        Supply {
            total_io: claims,
            protocol_reserve_io: 0,
            nonredeemable_governance_io: 0,
        }
    }

    fn state(backing: Backing, claims: u128, active_stake: u128) -> EconomicState {
        EconomicState {
            backing,
            supply: supply(claims),
            active_stake,
        }
    }

    fn allocation(
        backing: Backing,
        claims: u128,
        active: u128,
        q: u128,
        dc: u128,
        da: u128,
    ) -> Allocation {
        allocate(AllocationInput {
            state: state(backing, claims, active),
            net_claim_backing_increment: q,
            claim_supply_delta: Delta::Increase(dc),
            active_stake_delta: Delta::Increase(da),
        })
        .unwrap()
    }

    #[test]
    fn claim_supply_and_rate_cover_boundaries_without_float() {
        assert_eq!(claim_rate(0, 0), Ok(ClaimRate::EmptyGenesis));
        assert_eq!(claim_rate(0, 100), Err(ModelError::UncoveredClaims));
        assert_eq!(
            claim_rate(100, 0),
            Ok(ClaimRate::BackingWithoutClaims { backing: 100 })
        );
        assert_eq!(
            claim_rate(100, 100),
            Ok(ClaimRate::Ratio {
                backing: 100,
                claims: 100
            })
        );
        assert_eq!(
            claim_rate(250, 100),
            Ok(ClaimRate::Ratio {
                backing: 250,
                claims: 100
            })
        );
        let large = u128::MAX / 4;
        assert_eq!(
            claim_rate(large, large),
            Ok(ClaimRate::Ratio {
                backing: large,
                claims: large
            })
        );
        assert_eq!(target_pool(large, 2, large), Ok(2));
        assert_eq!(
            Supply {
                total_io: 1_000,
                protocol_reserve_io: 200,
                nonredeemable_governance_io: 300,
            }
            .claim_supply(),
            Ok(500)
        );
        assert_eq!(
            Supply {
                total_io: 1,
                protocol_reserve_io: 1,
                nonredeemable_governance_io: 1,
            }
            .claim_supply(),
            Err(ModelError::ExclusionsExceedSupply)
        );
        assert_eq!(
            Supply {
                total_io: u128::MAX,
                protocol_reserve_io: u128::MAX,
                nonredeemable_governance_io: 1,
            }
            .claim_supply(),
            Err(ModelError::ArithmeticOverflow)
        );
        assert_eq!(target_pool(0, 0, 0), Ok(0));
        assert_eq!(
            target_pool(0, 100, 0),
            Err(ModelError::BackingWithoutClaims)
        );
        assert_eq!(target_pool(0, 0, 100), Err(ModelError::UncoveredClaims));
        assert_eq!(target_pool(1, 0, 0), Err(ModelError::ActiveWithoutClaims));
        assert_eq!(
            target_pool(101, 100, 100),
            Err(ModelError::ActiveExceedsClaims)
        );
        assert_eq!(
            target_pool(u128::MAX, 2, u128::MAX),
            Err(ModelError::ArithmeticOverflow)
        );
        assert_eq!(target_pool(1, 1, 0), Err(ModelError::ActiveWithoutClaims));
    }

    #[test]
    fn allocation_validates_pre_and_post_active_claim_bounds() {
        let invalid_pre = AllocationInput {
            state: state(
                Backing {
                    liquid: 100,
                    ..Backing::default()
                },
                100,
                101,
            ),
            net_claim_backing_increment: 1,
            claim_supply_delta: Delta::Increase(1),
            active_stake_delta: Delta::Increase(0),
        };
        assert_eq!(allocate(invalid_pre), Err(ModelError::ActiveExceedsClaims));

        let invalid_post = AllocationInput {
            state: state(
                Backing {
                    liquid: 100,
                    ..Backing::default()
                },
                100,
                100,
            ),
            net_claim_backing_increment: 1,
            claim_supply_delta: Delta::Increase(0),
            active_stake_delta: Delta::Increase(1),
        };
        assert_eq!(allocate(invalid_post), Err(ModelError::ActiveExceedsClaims));
    }

    #[test]
    fn jupiter_allocation_is_rate_and_target_derived() {
        let genesis = allocation(Backing::default(), 0, 0, 60, 60, 0);
        assert_eq!((genesis.to_pool, genesis.to_liquid), (0, 60));
        for (backing, claims, active, pooled, q) in [
            (1_000, 1_000, 0, 0, 600),
            (1_000, 1_000, 250, 250, 600),
            (2_000, 1_000, 250, 500, 600),
        ] {
            let delivered = io_release_at_pre_event_rate(q, backing, claims).unwrap();
            let result = allocation(
                Backing {
                    liquid: backing - pooled,
                    pooled,
                    ..Backing::default()
                },
                claims,
                active,
                q,
                delivered,
                0,
            );
            assert_eq!(result.target_pool, pooled);
            assert_eq!((result.to_pool, result.to_liquid), (0, q));
            assert_eq!(
                result.post_claim_backing * claims,
                backing * result.post_claim_supply
            );
        }

        let rounded = allocation(
            Backing {
                liquid: 2,
                pooled: 1,
                ..Backing::default()
            },
            2,
            1,
            1,
            io_release_at_pre_event_rate(1, 3, 2).unwrap(),
            0,
        );
        assert_eq!((rounded.to_pool, rounded.to_liquid), (1, 0));
        assert!(rounded.post_claim_backing * 2 > 3 * rounded.post_claim_supply);

        let delivery = reserve_delivery(60, 1).unwrap();
        assert_eq!(delivery.claim_supply_increment, 60);
        assert_eq!(delivery.reserve_debit, 61);
        assert_eq!(delivery.total_supply_burn, 1);
    }

    #[test]
    fn two_week_maturity_uses_actual_distribution_not_source_label() {
        let split = maturity_split(1_000, 0).unwrap();
        assert_eq!((split.permanent_leg, split.net_claim_increment), (400, 600));
        let base = Backing {
            liquid: 500,
            pooled: 500,
            ..Backing::default()
        };
        let full = allocation(base, 1_000, 500, split.net_claim_increment, 600, 600);
        assert_eq!((full.to_pool, full.to_liquid), (600, 0));
        assert_eq!(
            (full.post_claim_backing, full.post_claim_supply),
            (1_600, 1_600)
        );

        let partial = allocation(base, 1_000, 500, split.net_claim_increment, 300, 300);
        assert_eq!((partial.to_pool, partial.to_liquid), (484, 116));
        let forfeited = allocation(base, 1_000, 500, split.net_claim_increment, 0, 0);
        assert_eq!((forfeited.to_pool, forfeited.to_liquid), (300, 300));

        let after_fee = maturity_split(1_000, 1).unwrap();
        assert_eq!(after_fee.net_claim_increment, 599);
        let fee_result = allocation(base, 1_000, 500, 599, 599, 599);
        assert_eq!((fee_result.to_pool, fee_result.to_liquid), (599, 0));
        assert!(fee_result.post_claim_backing * 1_000 >= 1_000 * fee_result.post_claim_supply);
    }

    fn maturity_route_input(
        backing: Backing,
        claims: u128,
        active: u128,
        actual_mint: u128,
        delivered_io: u128,
        permanent_fee: u128,
        claim_fee: u128,
    ) -> MaturityRouteInput {
        MaturityRouteInput {
            state: state(backing, claims, active),
            actual_mint,
            claim_supply_delta: Delta::Increase(delivered_io),
            active_stake_delta: Delta::Increase(delivered_io),
            permanent_transfer_fee: permanent_fee,
            claim_transfer_fee: claim_fee,
        }
    }

    #[test]
    fn maturity_claim_leg_uses_one_or_two_physical_fees_by_final_route() {
        let all_liquid = plan_maturity_route(maturity_route_input(
            Backing {
                liquid: 1_000,
                ..Backing::default()
            },
            1_000,
            0,
            1_000,
            0,
            10,
            10,
        ))
        .unwrap();
        assert_eq!(all_liquid.route, ClaimLegRoute::AllLiquid);
        assert_eq!(all_liquid.route_evaluations, 1);
        assert_eq!(
            (
                all_liquid.permanent_source_debit,
                all_liquid.permanent_credit,
                all_liquid.claim_staging_source_debit,
                all_liquid.first_claim_credit,
                all_liquid.liquid_credit,
                all_liquid.pooled_credit,
                all_liquid.liquid_to_pool_source_debit,
            ),
            (400, 390, 600, 590, 590, 0, None)
        );
        assert_eq!(
            (all_liquid.post_backing, all_liquid.post_permanent),
            (1_590, 390)
        );

        let all_pool = plan_maturity_route(maturity_route_input(
            Backing {
                pooled: 1_000,
                ..Backing::default()
            },
            1_000,
            1_000,
            1_000,
            590,
            10,
            10,
        ))
        .unwrap();
        assert_eq!(all_pool.route, ClaimLegRoute::AllPool);
        assert_eq!((all_pool.liquid_credit, all_pool.pooled_credit), (0, 590));
        assert_eq!(all_pool.claim_fees, 10);
        assert_eq!(all_pool.post_target, 1_590);

        let mixed = plan_maturity_route(maturity_route_input(
            Backing {
                liquid: 500,
                pooled: 500,
                ..Backing::default()
            },
            1_000,
            500,
            1_000,
            0,
            10,
            10,
        ))
        .unwrap();
        assert_eq!(mixed.route, ClaimLegRoute::Mixed);
        assert_eq!(mixed.route_evaluations, 2);
        assert_eq!((mixed.liquid_credit, mixed.pooled_credit), (290, 290));
        assert_eq!(mixed.liquid_to_pool_source_debit, Some(300));
        assert_eq!(mixed.claim_fees, 20);
        assert_eq!(
            (mixed.post_backing, mixed.post_permanent, mixed.post_target),
            (1_580, 390, 790)
        );
    }

    #[test]
    fn maturity_route_fee_floor_edges_terminate_without_assumed_fee_oscillation() {
        let liquid_edge = plan_maturity_route(maturity_route_input(
            Backing {
                liquid: 50,
                pooled: 50,
                ..Backing::default()
            },
            100,
            50,
            8,
            0,
            1,
            2,
        ))
        .unwrap();
        assert_eq!(liquid_edge.route, ClaimLegRoute::AllLiquid);
        assert_eq!(liquid_edge.route_evaluations, 2);
        assert_eq!(
            (liquid_edge.post_target, liquid_edge.pooled_credit),
            (51, 0)
        );
        assert_eq!(liquid_edge.remaining_under_target, 1);
        assert_eq!(liquid_edge.claim_fees, 2);

        let pool_edge = plan_maturity_route(maturity_route_input(
            Backing {
                liquid: 52,
                pooled: 50,
                ..Backing::default()
            },
            100,
            50,
            8,
            0,
            1,
            2,
        ))
        .unwrap();
        assert_eq!(pool_edge.route, ClaimLegRoute::AllPool);
        assert_eq!(pool_edge.route_evaluations, 2);
        assert_eq!((pool_edge.post_target, pool_edge.pooled_credit), (52, 3));
        assert_eq!(pool_edge.resulting_over_target, 1);
        assert_eq!(pool_edge.claim_fees, 2);
        assert!(liquid_edge.route_evaluations <= 2 && pool_edge.route_evaluations <= 2);
    }

    #[test]
    fn permanent_maturity_split_follows_target_delta() {
        for (active, pooled, expected_pool) in [
            (0, 0, 0),
            (250, 250, 150),
            (500, 500, 300),
            (1_000, 1_000, 600),
        ] {
            let result = allocation(
                Backing {
                    liquid: 1_000 - pooled,
                    pooled,
                    ..Backing::default()
                },
                1_000,
                active,
                600,
                0,
                0,
            );
            assert_eq!(result.to_pool, expected_pool);
            assert_eq!(result.to_liquid, 600 - expected_pool);
        }

        let under = allocation(
            Backing {
                liquid: 900,
                pooled: 100,
                ..Backing::default()
            },
            1_000,
            500,
            600,
            0,
            0,
        );
        assert_eq!((under.to_pool, under.remaining_under_target), (600, 100));

        let over = allocation(
            Backing {
                liquid: 100,
                pooled: 900,
                ..Backing::default()
            },
            1_000,
            500,
            600,
            0,
            0,
        );
        assert_eq!((over.to_pool, over.to_liquid), (0, 600));
        assert_eq!(over.resulting_over_target, 100);

        let pending = allocation(
            Backing {
                liquid: 200,
                pooled: 500,
                pending_unwind: 200,
                transit: 100,
                ..Backing::default()
            },
            1_000,
            500,
            600,
            0,
            0,
        );
        assert_eq!((pending.to_pool, pending.to_liquid), (300, 300));
    }

    #[test]
    fn bounded_rounding_loops_never_allocate_more_than_q() {
        for backing in 1..=40 {
            for claims in 1..=40 {
                for active in 0..=claims {
                    for q in 0..=20 {
                        let pooled = target_pool(active, backing, claims).unwrap().min(backing);
                        let result = allocation(
                            Backing {
                                liquid: backing - pooled,
                                pooled,
                                ..Backing::default()
                            },
                            claims,
                            active,
                            q,
                            0,
                            0,
                        );
                        assert_eq!(result.to_pool + result.to_liquid, q);
                        assert!(result.to_pool <= q);
                    }
                }
            }
        }
    }

    #[test]
    fn conservation_covers_reallocation_fees_payouts_and_permanent_exclusion() {
        let pre = Backing {
            liquid: 500,
            pooled: 300,
            pending_unwind: 100,
            transit: 100,
            permanent: 1_000,
            operational_reserve: 50,
        };
        let frozen = move_backing(pre, Bucket::Liquid, Bucket::Transit, 100, 0).unwrap();
        assert_eq!(frozen.claim_backing(), pre.claim_backing());
        let credited = move_backing(frozen, Bucket::Transit, Bucket::Pooled, 200, 1).unwrap();
        assert_eq!(
            credited.claim_backing(),
            Ok(pre.claim_backing().unwrap() - 1)
        );
        assert_conservation(pre, credited, 0, 1, 0).unwrap();
        let fabricated = Backing {
            liquid: pre.liquid + 1,
            ..pre
        };
        assert_eq!(
            assert_conservation(pre, fabricated, 0, 0, 0),
            Err(ModelError::InvalidBackingState)
        );

        let mut post_mint = credited;
        post_mint.permanent += 40;
        post_mint.liquid += 60;
        assert_conservation(credited, post_mint, 100, 0, 0).unwrap();
        let post_payout = policy_a_fee(post_mint, Bucket::Liquid, 10).unwrap();
        assert_conservation(post_mint, post_payout, 0, 0, 10).unwrap();
        assert_eq!(
            post_mint.claim_backing().unwrap() - credited.claim_backing().unwrap(),
            60
        );
    }

    #[test]
    fn policies_a_b_and_c_have_distinct_state_and_timing() {
        let base = Backing {
            liquid: 100,
            operational_reserve: 10,
            ..Backing::default()
        };
        let a = policy_a_fee(base, Bucket::Liquid, 1).unwrap();
        assert_eq!(
            (a.claim_backing().unwrap(), a.operational_reserve),
            (99, 10)
        );

        let b = policy_b_fee(base, Bucket::Liquid, 1).unwrap();
        assert_eq!(
            (b.claim_backing().unwrap(), b.operational_reserve),
            (100, 9)
        );
        let depleted = Backing {
            operational_reserve: 0,
            ..base
        };
        assert_eq!(
            policy_b_fee(depleted, Bucket::Liquid, 1),
            Err(ModelError::InsufficientOperationalReserve)
        );
        assert_eq!(
            policy_b_or_a_fee(depleted, Bucket::Liquid, 1)
                .unwrap()
                .claim_backing(),
            Ok(99)
        );
        assert_eq!(
            policy_b_maturity(100, 2, 10).unwrap(),
            PolicyBMaturity {
                reserve_replenishment: 8,
                remaining_mint: 92,
                permanent_leg: 36,
                new_claim_backing: 56,
                post_reserve: 10,
            }
        );

        let (c_backing, c_counter) =
            policy_c_fee(base, FeeCounter { unreimbursed: 0 }, Bucket::Liquid, 1).unwrap();
        assert_eq!(
            (c_backing.claim_backing().unwrap(), c_counter.unreimbursed),
            (99, 1)
        );
        let insufficient = policy_c_maturity(4, FeeCounter { unreimbursed: 10 }).unwrap();
        assert_eq!(
            (
                insufficient.reimbursement,
                insufficient.new_claim_backing,
                insufficient.post_counter.unreimbursed
            ),
            (4, 4, 6)
        );
        assert_eq!(
            policy_c_fee(
                base,
                FeeCounter {
                    unreimbursed: u128::MAX
                },
                Bucket::Liquid,
                1
            ),
            Err(ModelError::ArithmeticOverflow)
        );
        let (twice_backing, twice_counter) =
            policy_c_fee(c_backing, c_counter, Bucket::Liquid, 2).unwrap();
        assert_eq!(
            (
                twice_backing.claim_backing().unwrap(),
                twice_counter.unreimbursed
            ),
            (97, 3)
        );
    }

    #[test]
    fn delayed_reimbursement_restores_assets_not_original_holder_rate() {
        let initial = Backing {
            liquid: 100,
            ..Backing::default()
        };
        let (after_fee, counter) =
            policy_c_fee(initial, FeeCounter { unreimbursed: 0 }, Bucket::Liquid, 1).unwrap();
        assert_eq!(
            (after_fee.claim_backing().unwrap(), counter.unreimbursed),
            (99, 1)
        );
        let mut after_issuance = after_fee;
        after_issuance.liquid += 99;
        let claims = 200;
        assert_eq!(after_issuance.claim_backing(), Ok(198));
        let reimbursement = policy_c_maturity(1, counter).unwrap();
        after_issuance.liquid += reimbursement.new_claim_backing;
        assert_eq!(after_issuance.claim_backing(), Ok(199));
        assert_eq!(
            claim_rate(199, claims),
            Ok(ClaimRate::Ratio {
                backing: 199,
                claims: 200
            })
        );
        assert_ne!(199 * 100, 200 * 100);
    }

    #[test]
    fn post_fee_planning_avoids_direction_reversal() {
        let under = ReconcileInput {
            backing: Backing {
                liquid: 600,
                pooled: 400,
                ..Backing::default()
            },
            claims: 1_000,
            active_stake: 500,
            next_fee: 10,
            minimum_child_gross: 110,
            minimum_parent: 100,
            parent_exists: true,
        };
        let plan = plan_reconciliation(under).unwrap();
        let (target, credited, debit) = match plan {
            ReconcilePlan::TopUp {
                post_fee_target,
                credited,
                source_debit,
                creates_parent,
            } => {
                assert!(!creates_parent);
                (post_fee_target, credited, source_debit)
            }
            other => panic!("expected top-up, got {other:?}"),
        };
        assert_eq!((target, credited, debit), (495, 95, 105));
        let after = Backing {
            liquid: under.backing.liquid - debit,
            pooled: under.backing.pooled + credited,
            ..under.backing
        };
        assert_eq!(after.claim_backing(), Ok(990));
        assert_eq!(target_pool(500, 990, 1_000), Ok(after.pooled));

        let small = ReconcileInput {
            backing: Backing {
                liquid: 509,
                pooled: 491,
                ..Backing::default()
            },
            ..under
        };
        assert!(matches!(
            plan_reconciliation(small),
            Ok(ReconcilePlan::Hold {
                tolerance: 10,
                reason: HoldReason::FeeTolerance,
                ..
            })
        ));

        let over = ReconcileInput {
            backing: Backing {
                liquid: 300,
                pooled: 700,
                ..Backing::default()
            },
            ..under
        };
        let plan = plan_reconciliation(over).unwrap();
        let (target, gross, credit) = match plan {
            ReconcilePlan::Unwind {
                post_fee_target,
                gross,
                expected_credit,
            } => (post_fee_target, gross, expected_credit),
            other => panic!("expected unwind, got {other:?}"),
        };
        assert_eq!((target, gross, credit), (495, 205, 195));
        let after = Backing {
            pooled: over.backing.pooled - gross,
            pending_unwind: credit,
            ..over.backing
        };
        assert_eq!(
            target_pool(500, after.claim_backing().unwrap(), 1_000),
            Ok(after.pooled)
        );

        let below_child_minimum = ReconcileInput {
            backing: Backing {
                liquid: 400,
                pooled: 600,
                ..Backing::default()
            },
            ..under
        };
        assert!(matches!(
            plan_reconciliation(below_child_minimum),
            Ok(ReconcilePlan::Hold {
                tolerance: 109,
                reason: HoldReason::ChildMinimum,
                ..
            })
        ));
    }

    #[test]
    fn parent_bootstrap_holds_below_minimum_and_creates_lazily_with_exact_fee() {
        let no_parent = ReconcileInput {
            backing: Backing {
                liquid: 1_000,
                ..Backing::default()
            },
            claims: 1_000,
            active_stake: 50,
            next_fee: 10,
            minimum_child_gross: 110,
            minimum_parent: 100,
            parent_exists: false,
        };
        assert_eq!(
            plan_reconciliation(no_parent),
            Ok(ReconcilePlan::Hold {
                post_fee_target: 50,
                tolerance: 99,
                reason: HoldReason::BelowMinimumStake,
            })
        );
        assert_eq!(no_parent.backing.liquid, 1_000);

        let at_minimum = ReconcileInput {
            active_stake: 100,
            ..no_parent
        };
        assert_eq!(
            plan_reconciliation(at_minimum),
            Ok(ReconcilePlan::TopUp {
                post_fee_target: 100,
                credited: 100,
                source_debit: 110,
                creates_parent: true,
            })
        );

        let zero_active = ReconcileInput {
            active_stake: 0,
            ..no_parent
        };
        assert!(matches!(
            plan_reconciliation(zero_active),
            Ok(ReconcilePlan::Hold {
                reason: HoldReason::BelowMinimumStake,
                ..
            })
        ));

        let existing_parent = ReconcileInput {
            backing: Backing {
                liquid: 900,
                pooled: 100,
                ..Backing::default()
            },
            active_stake: 0,
            parent_exists: true,
            ..no_parent
        };
        assert!(matches!(
            plan_reconciliation(existing_parent),
            Ok(ReconcilePlan::Hold {
                post_fee_target: 100,
                reason: HoldReason::ChildMinimum,
                ..
            })
        ));
    }

    #[test]
    fn redemption_quotes_total_backing_but_waits_for_liquid() {
        let backing = Backing {
            liquid: 100,
            pooled: 500,
            pending_unwind: 200,
            transit: 200,
            permanent: 1_000,
            operational_reserve: 50,
        };
        assert_eq!(backing.claim_backing(), Ok(1_000));
        assert_eq!(
            redemption_readiness(200, backing, 1_000, 10),
            Ok(RedemptionReadiness::AwaitLiquidity { gross_quote: 200 })
        );
        let liquid = move_backing(backing, Bucket::PendingUnwind, Bucket::Liquid, 200, 0).unwrap();
        assert_eq!(
            redemption_readiness(200, liquid, 1_000, 10),
            Ok(RedemptionReadiness::Ready {
                gross_quote: 200,
                net_payout: 190
            })
        );
        assert_eq!(backing.operational_reserve, 50);
    }

    #[test]
    fn overflow_and_decrease_paths_fail_closed() {
        let overflowing = AllocationInput {
            state: state(
                Backing {
                    liquid: u128::MAX,
                    ..Backing::default()
                },
                1,
                1,
            ),
            net_claim_backing_increment: 1,
            claim_supply_delta: Delta::Increase(0),
            active_stake_delta: Delta::Increase(0),
        };
        assert_eq!(allocate(overflowing), Err(ModelError::ArithmeticOverflow));
        let decreasing = AllocationInput {
            state: state(
                Backing {
                    liquid: 10,
                    ..Backing::default()
                },
                10,
                10,
            ),
            net_claim_backing_increment: 0,
            claim_supply_delta: Delta::Decrease(11),
            active_stake_delta: Delta::Decrease(0),
        };
        assert_eq!(allocate(decreasing), Err(ModelError::InsufficientBacking));
        assert_eq!(
            maturity_split(u128::MAX, 0),
            Err(ModelError::ArithmeticOverflow)
        );
        assert_eq!(
            move_backing(Backing::default(), Bucket::Liquid, Bucket::Liquid, 0, 0),
            Err(ModelError::SameBucket)
        );
    }

    #[test]
    fn model_has_no_floating_point_or_unminted_maturity_backing() {
        let source = include_str!("lib.rs");
        assert!(!source.contains(&["f", "32"].concat()));
        assert!(!source.contains(&["f", "64"].concat()));

        let before = Backing {
            liquid: 10,
            pooled: 20,
            permanent: 30,
            ..Backing::default()
        };
        let observed_but_unminted_maturity = 1_000;
        assert!(observed_but_unminted_maturity > 0);
        assert_eq!(before.claim_backing(), Ok(30));
        assert_eq!(before.total_assets(), Ok(60));
        let actual_mint = maturity_split(100, 0).unwrap();
        assert_eq!(
            (actual_mint.permanent_leg, actual_mint.net_claim_increment),
            (40, 60)
        );
    }

    const DAY: u64 = 86_400;
    const TWO_WEEK_DELAY: u64 = 14 * DAY;

    fn sticky_base(neuron_count: usize) -> StickyUnwindModel {
        StickyUnwindModel::with_neurons(
            Backing {
                liquid: 500,
                pooled: 500,
                ..Backing::default()
            },
            neuron_count,
            2,
        )
    }

    fn commit_cohort(
        model: &mut StickyUnwindModel,
        neuron_index: usize,
        observation: u64,
        started_at: u64,
    ) -> u64 {
        model
            .observe_sns(neuron_index, CanonicalSnsState::Dissolving, observation)
            .unwrap();
        let generation = model
            .submit_split_intent(neuron_index, 110, started_at, TWO_WEEK_DELAY)
            .unwrap();
        model.prove_split(10).unwrap();
        generation
    }

    fn returned_active_cohort() -> (StickyUnwindModel, u64) {
        let mut model = sticky_base(1);
        let generation = commit_cohort(&mut model, 0, 1, 0);
        model.observe_sns(0, CanonicalSnsState::Active, 2).unwrap();
        model
            .submit_child_disbursement(generation, TWO_WEEK_DELAY)
            .unwrap();
        model.prove_child_disbursement(generation, 10).unwrap();
        (model, generation)
    }

    fn returned_liquidity_input(
        generation: u64,
        active_backing: u128,
        minimum_restake_credit: u128,
        next_reward_observation: u64,
    ) -> ReturnedLiquidityInput {
        ReturnedLiquidityInput {
            generation,
            claims: 1_000,
            active_backing,
            exact_restake_fee: 10,
            minimum_restake_credit,
            minimum_parent: 0,
            next_reward_observation,
        }
    }

    #[test]
    fn precommit_cancellation_is_fee_free_and_not_retroactively_eligible() {
        let mut model = sticky_base(1);
        let initial = model.backing;
        assert!(model.neurons[0].reward_eligible_at(0));
        model
            .observe_sns(0, CanonicalSnsState::Dissolving, 1)
            .unwrap();
        assert_eq!(model.neurons[0].status, StickyStatus::ExitObserved);
        model.observe_sns(0, CanonicalSnsState::Active, 2).unwrap();
        assert_eq!(model.neurons[0].status, StickyStatus::ActiveBacked);
        assert!(!model.neurons[0].reward_eligible_at(2));
        assert!(model.neurons[0].reward_eligible_at(3));
        assert_eq!(model.backing, initial);
        assert!(model.cohorts.is_empty());
        assert_eq!(model.operations, StickyOperationCounts::default());
        assert_eq!(model.fees.total(), Ok(0));
    }

    #[test]
    fn overlapping_passive_cohorts_keep_independent_ready_times_and_global_liquidity() {
        let mut model = sticky_base(2);
        let generation_a = commit_cohort(&mut model, 0, 1, 0);
        let generation_b = commit_cohort(&mut model, 1, 2, DAY);
        assert_eq!((generation_a, generation_b), (0, 1));
        assert_eq!(model.cohorts.len(), 2);
        assert_eq!(model.backing.pending_unwind, 200);

        model.observe_sns(0, CanonicalSnsState::Active, 3).unwrap();
        assert_eq!(model.neurons[0].status, StickyStatus::ReentryPending);
        model.observe_sns(1, CanonicalSnsState::Active, 4).unwrap();
        model
            .observe_sns(1, CanonicalSnsState::Dissolving, 5)
            .unwrap();
        assert_eq!(model.neurons[1].status, StickyStatus::ExitCommitted);
        assert_eq!(model.operations.split_intents, 2);
        assert_eq!(
            model.submit_split_intent(1, 110, 2 * DAY, TWO_WEEK_DELAY),
            Err(ModelError::InvalidTransition)
        );

        model.refresh_cohort_readiness(TWO_WEEK_DELAY);
        assert_eq!(
            model.cohort(generation_a).unwrap().lifecycle,
            CohortLifecycle::Ready
        );
        assert_eq!(
            model.cohort(generation_b).unwrap().lifecycle,
            CohortLifecycle::Dissolving
        );
        model
            .submit_child_disbursement(generation_a, TWO_WEEK_DELAY)
            .unwrap();
        assert_eq!(
            model.submit_child_disbursement(generation_b, TWO_WEEK_DELAY + DAY),
            Err(ModelError::InvalidTransition),
            "only one NNS command may be active"
        );
        model.prove_child_disbursement(generation_a, 10).unwrap();
        assert_eq!(
            model.cohort(generation_a).unwrap().lifecycle,
            CohortLifecycle::Returned
        );
        assert_eq!(
            model.cohort(generation_b).unwrap().lifecycle,
            CohortLifecycle::Dissolving
        );
        assert_eq!(
            (model.backing.liquid, model.backing.pending_unwind),
            (590, 100)
        );
        model.refresh_cohort_readiness(TWO_WEEK_DELAY + DAY);
        assert_eq!(
            model.cohort(generation_b).unwrap().lifecycle,
            CohortLifecycle::Ready
        );
        model
            .submit_child_disbursement(generation_b, TWO_WEEK_DELAY + DAY)
            .unwrap();
        model.prove_child_disbursement(generation_b, 10).unwrap();
        assert_eq!(
            model.cohort(generation_b).unwrap().lifecycle,
            CohortLifecycle::Returned
        );
        assert_eq!(
            (model.backing.liquid, model.backing.pending_unwind),
            (680, 0)
        );
        assert_eq!(model.operations.split_intents, 2);
        assert_eq!(model.operations.merge_proofs, 0);
    }

    #[test]
    fn reentry_requires_target_credit_but_exact_and_over_target_need_no_transfer() {
        for (active_backing, expected_target) in [(398, 390), (300, 294)] {
            let (mut model, generation) = returned_active_cohort();
            assert_eq!(
                model
                    .plan_returned_liquidity(returned_liquidity_input(
                        generation,
                        active_backing,
                        1,
                        10,
                    ))
                    .unwrap(),
                ReturnedLiquidityPlan::RestoredWithoutTransfer {
                    target: expected_target,
                }
            );
            assert!(!model.neurons[0].reward_eligible_at(9));
            assert!(model.neurons[0].reward_eligible_at(10));
            assert_eq!(model.operations.restake_proofs, 0);
        }

        let (mut fee_tolerated, fee_generation) = returned_active_cohort();
        assert_eq!(
            fee_tolerated
                .plan_returned_liquidity(returned_liquidity_input(fee_generation, 407, 1, 10,))
                .unwrap(),
            ReturnedLiquidityPlan::Hold {
                target: 394,
                required_credit: 4,
                tolerance: 10,
                reason: HoldReason::FeeTolerance,
            }
        );
        assert!(!fee_tolerated.neurons[0].reward_eligible_at(u64::MAX));

        let (mut minimum_tolerated, minimum_generation) = returned_active_cohort();
        assert_eq!(
            minimum_tolerated
                .plan_returned_liquidity(
                    returned_liquidity_input(minimum_generation, 500, 100, 10,)
                )
                .unwrap(),
            ReturnedLiquidityPlan::Hold {
                target: 485,
                required_credit: 95,
                tolerance: 99,
                reason: HoldReason::BelowMinimumStake,
            }
        );
        assert!(!minimum_tolerated.neurons[0].reward_eligible_at(u64::MAX));
    }

    #[test]
    fn restake_plan_is_discardable_but_committed_ambiguous_effect_must_be_proved() {
        let (mut planned, generation) = returned_active_cohort();
        let before_plan = planned.backing;
        assert!(matches!(
            planned.plan_returned_liquidity(returned_liquidity_input(generation, 500, 1, 4,)),
            Ok(ReturnedLiquidityPlan::Restake {
                credited: 95,
                source_debit: 105,
                ..
            })
        ));
        assert_eq!(planned.neurons[0].status, StickyStatus::RestakePlanned);
        planned
            .observe_sns(0, CanonicalSnsState::Dissolving, 3)
            .unwrap();
        assert_eq!(planned.planned_restake, None);
        assert_eq!(planned.backing, before_plan);
        assert_eq!(planned.neurons[0].status, StickyStatus::LiquidReturned);

        let (mut committed, generation) = returned_active_cohort();
        committed
            .plan_returned_liquidity(returned_liquidity_input(generation, 500, 1, 4))
            .unwrap();
        committed.commit_restake(generation).unwrap();
        assert_eq!(
            (committed.backing.liquid, committed.backing.transit),
            (485, 105)
        );
        committed
            .observe_sns(0, CanonicalSnsState::Dissolving, 3)
            .unwrap();
        assert_eq!(committed.neurons[0].status, StickyStatus::RestakeCommitted);
        assert_eq!(
            committed.commit_restake(generation),
            Err(ModelError::InvalidTransition)
        );
        assert_eq!(
            committed.prove_restake(generation, 94),
            Err(ModelError::ProofMismatch)
        );
        assert_eq!(committed.backing.transit, 105);
        committed.prove_restake(generation, 95).unwrap();
        assert_eq!(committed.neurons[0].status, StickyStatus::RestakeProved);
        assert_eq!(
            (committed.backing.transit, committed.backing.pooled),
            (0, 485)
        );
        assert_eq!(committed.operations.restake_proofs, 1);
        committed.finish_restake(generation, 4).unwrap();
        assert_eq!(committed.neurons[0].status, StickyStatus::ExitObserved);
        assert!(!committed.neurons[0].reward_eligible_at(u64::MAX));
        let next_generation = committed
            .submit_split_intent(0, 110, 20 * DAY, TWO_WEEK_DELAY)
            .unwrap();
        assert_eq!(next_generation, generation + 1);
    }

    #[test]
    fn mixed_generations_remain_ineligible_until_global_target_is_fully_restored() {
        let mut model = sticky_base(2);
        let generation_a = commit_cohort(&mut model, 0, 1, 2 * DAY);
        let generation_b = commit_cohort(&mut model, 1, 2, 0);
        model.observe_sns(1, CanonicalSnsState::Active, 3).unwrap();
        assert_eq!(model.neurons[1].status, StickyStatus::ReentryPending);
        model
            .submit_child_disbursement(generation_b, TWO_WEEK_DELAY)
            .unwrap();
        model.prove_child_disbursement(generation_b, 10).unwrap();

        assert_eq!(
            model.cohort(generation_a).unwrap().lifecycle,
            CohortLifecycle::Dissolving
        );
        assert_eq!(
            model.cohort(generation_b).unwrap().lifecycle,
            CohortLifecycle::Returned
        );
        assert!(!model.neurons[0].reward_eligible_at(u64::MAX));
        assert!(!model.neurons[1].reward_eligible_at(u64::MAX));

        model.backing =
            move_backing(model.backing, Bucket::Liquid, Bucket::Pooled, 100, 0).unwrap();
        assert_eq!(
            (model.backing.pooled, target_pool(500, 970, 1_000).unwrap()),
            (380, 485)
        );
        assert!(!model.neurons[1].reward_eligible_at(u64::MAX));

        model.backing =
            move_backing(model.backing, Bucket::Liquid, Bucket::Pooled, 105, 0).unwrap();
        assert_eq!(model.backing.pooled, 485);
        assert_eq!(
            model
                .plan_returned_liquidity(returned_liquidity_input(generation_b, 500, 1, 20,))
                .unwrap(),
            ReturnedLiquidityPlan::RestoredWithoutTransfer { target: 485 }
        );
        assert!(!model.neurons[0].reward_eligible_at(u64::MAX));
        assert!(!model.neurons[1].reward_eligible_at(19));
        assert!(model.neurons[1].reward_eligible_at(20));
        assert_eq!(model.operations.restake_proofs, 0);
    }

    #[test]
    fn candidate_liquidity_bound_is_formula_derived_and_unresolved_without_cadence() {
        let unresolved = LiquidityLagInputs {
            guaranteed_reconciliation_cadence: None,
            nns_dissolve_delay: TWO_WEEK_DELAY,
            max_detection_margin: DAY,
            max_command_margin: DAY,
            max_disbursement_margin: DAY,
        };
        assert_eq!(liquidity_lag_bound(unresolved), Ok(None));

        let candidate = LiquidityLagInputs {
            guaranteed_reconciliation_cadence: Some(DAY),
            ..unresolved
        };
        assert_eq!(liquidity_lag_bound(candidate), Ok(Some(18 * DAY)));
        assert_eq!(
            liquidity_lag_bound(LiquidityLagInputs {
                guaranteed_reconciliation_cadence: Some(u64::MAX),
                ..candidate
            }),
            Err(ModelError::ArithmeticOverflow)
        );
    }

    #[test]
    fn sticky_fee_count_is_bounded_per_committed_generation() {
        let comparison = lifecycle_fee_comparison(3, 10, 10, 10, 10).unwrap();
        assert_eq!(comparison.immediate_mirroring, 80);
        assert_eq!(comparison.sticky_unwind, 30);
        for flip_flops in 1..=100 {
            let fees = lifecycle_fee_comparison(flip_flops, 10, 10, 10, 10).unwrap();
            assert_eq!(fees.sticky_unwind, 30);
            assert!(fees.immediate_mirroring > fees.sticky_unwind);
        }
        assert_eq!(
            lifecycle_fee_comparison(u128::MAX, 1, 1, 1, 1),
            Err(ModelError::ArithmeticOverflow)
        );
    }
}
