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

    pub fn claim_rate(backing: u128, claims: u128) -> ClaimRate {
        match (backing, claims) {
            (0, 0) => ClaimRate::EmptyGenesis,
            (_, 0) => ClaimRate::BackingWithoutClaims { backing },
            _ => ClaimRate::Ratio { backing, claims },
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
        if claims == 0 {
            return Err(ModelError::ZeroClaimSupply);
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
            (0, _) => Err(ModelError::BackingWithoutClaims),
            (_, 0) => Err(ModelError::ZeroClaimSupply),
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
    pub enum ReconcilePlan {
        Hold {
            post_fee_target: u128,
            tolerance: u128,
        },
        TopUp {
            post_fee_target: u128,
            credited: u128,
            source_debit: u128,
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
    }

    pub fn plan_reconciliation(input: ReconcileInput) -> Result<ReconcilePlan, ModelError> {
        let backing = input.backing.claim_backing()?;
        let post_fee_backing = backing
            .checked_sub(input.next_fee)
            .ok_or(ModelError::InsufficientBacking)?;
        let raw_target = target_pool(input.active_stake, post_fee_backing, input.claims)?;
        let post_fee_target = raw_target.max(input.minimum_parent);
        let pooled = input.backing.pooled;
        if pooled < post_fee_target {
            let credited = post_fee_target - pooled;
            if credited <= input.next_fee {
                return Ok(ReconcilePlan::Hold {
                    post_fee_target,
                    tolerance: input.next_fee,
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
            });
        }
        let excess = pooled - post_fee_target;
        if excess < input.minimum_child_gross {
            return Ok(ReconcilePlan::Hold {
                post_fee_target,
                tolerance: input.minimum_child_gross.saturating_sub(1),
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
        RestakePending,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct StickyNeuron {
        pub status: StickyStatus,
        pub latest_sns_state: CanonicalSnsState,
        pub reward_eligible_from_observation: Option<u64>,
    }

    impl Default for StickyNeuron {
        fn default() -> Self {
            Self {
                status: StickyStatus::ActiveBacked,
                latest_sns_state: CanonicalSnsState::Active,
                reward_eligible_from_observation: Some(0),
            }
        }
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
        },
        Restake {
            post_fee_target: u128,
            credited: u128,
            source_debit: u128,
        },
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct StickyUnwindModel {
        pub neuron: StickyNeuron,
        pub backing: Backing,
        pub frozen_split_gross: u128,
        pub aggregate_child_principal: u128,
        pub planned_restake_credit: u128,
        pub planned_restake_debit: u128,
        pub planned_post_fee_target: u128,
        pub operations: StickyOperationCounts,
        pub fees: StickyFeeTotals,
    }

    impl StickyUnwindModel {
        pub fn new(backing: Backing) -> Self {
            Self {
                neuron: StickyNeuron::default(),
                backing,
                frozen_split_gross: 0,
                aggregate_child_principal: 0,
                planned_restake_credit: 0,
                planned_restake_debit: 0,
                planned_post_fee_target: 0,
                operations: StickyOperationCounts::default(),
                fees: StickyFeeTotals::default(),
            }
        }

        pub fn observe_sns(
            &mut self,
            state: CanonicalSnsState,
            observation: u64,
        ) -> Result<(), ModelError> {
            self.neuron.latest_sns_state = state;
            match state {
                CanonicalSnsState::Dissolving => {
                    self.neuron.reward_eligible_from_observation = None;
                    self.neuron.status = match self.neuron.status {
                        StickyStatus::ActiveBacked => StickyStatus::ExitObserved,
                        StickyStatus::ExitObserved => StickyStatus::ExitObserved,
                        StickyStatus::ExitCommitted | StickyStatus::ReentryPending => {
                            StickyStatus::ExitCommitted
                        }
                        StickyStatus::LiquidReturned => StickyStatus::LiquidReturned,
                        StickyStatus::RestakePending => {
                            self.clear_planned_restake();
                            StickyStatus::LiquidReturned
                        }
                    };
                }
                CanonicalSnsState::Active => {
                    self.neuron.status = match self.neuron.status {
                        StickyStatus::ExitObserved => {
                            self.neuron.reward_eligible_from_observation = Some(
                                observation
                                    .checked_add(1)
                                    .ok_or(ModelError::ArithmeticOverflow)?,
                            );
                            StickyStatus::ActiveBacked
                        }
                        StickyStatus::ExitCommitted => StickyStatus::ReentryPending,
                        status => status,
                    };
                }
            }
            Ok(())
        }

        pub fn submit_split_intent(&mut self, gross: u128) -> Result<(), ModelError> {
            if self.neuron.status != StickyStatus::ExitObserved
                || self.neuron.latest_sns_state != CanonicalSnsState::Dissolving
                || self.frozen_split_gross != 0
                || self.aggregate_child_principal != 0
            {
                return Err(ModelError::InvalidTransition);
            }
            self.backing = move_backing(self.backing, Bucket::Pooled, Bucket::Transit, gross, 0)?;
            self.frozen_split_gross = gross;
            self.operations.split_intents = self
                .operations
                .split_intents
                .checked_add(1)
                .ok_or(ModelError::ArithmeticOverflow)?;
            self.neuron.status = StickyStatus::ExitCommitted;
            Ok(())
        }

        pub fn prove_split(&mut self, exact_fee: u128) -> Result<(), ModelError> {
            if self.frozen_split_gross == 0 || self.aggregate_child_principal != 0 {
                return Err(ModelError::InvalidTransition);
            }
            let credited = self
                .frozen_split_gross
                .checked_sub(exact_fee)
                .ok_or(ModelError::InsufficientBacking)?;
            self.backing = move_backing(
                self.backing,
                Bucket::Transit,
                Bucket::PendingUnwind,
                self.frozen_split_gross,
                exact_fee,
            )?;
            self.frozen_split_gross = 0;
            self.aggregate_child_principal = credited;
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

        pub fn prove_child_disbursement(&mut self, exact_fee: u128) -> Result<(), ModelError> {
            if self.aggregate_child_principal == 0 || self.frozen_split_gross != 0 {
                return Err(ModelError::InvalidTransition);
            }
            self.backing = move_backing(
                self.backing,
                Bucket::PendingUnwind,
                Bucket::Liquid,
                self.aggregate_child_principal,
                exact_fee,
            )?;
            self.aggregate_child_principal = 0;
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
            self.neuron.status = StickyStatus::LiquidReturned;
            Ok(())
        }

        pub fn plan_returned_liquidity(
            &mut self,
            claims: u128,
            active_stake: u128,
            exact_restake_fee: u128,
            minimum_restake_credit: u128,
            minimum_parent: u128,
            next_reward_observation: u64,
        ) -> Result<ReturnedLiquidityPlan, ModelError> {
            if self.neuron.status != StickyStatus::LiquidReturned {
                return Err(ModelError::InvalidTransition);
            }
            if self.neuron.latest_sns_state == CanonicalSnsState::Dissolving {
                return Ok(ReturnedLiquidityPlan::LatestStateDissolving);
            }

            let backing = self.backing.claim_backing()?;
            let current_target = target_pool(active_stake, backing, claims)?.max(minimum_parent);
            if self.backing.pooled == current_target {
                self.neuron.status = StickyStatus::ActiveBacked;
                self.neuron.reward_eligible_from_observation = Some(next_reward_observation);
                return Ok(ReturnedLiquidityPlan::RestoredWithoutTransfer {
                    target: current_target,
                });
            }
            if self.backing.pooled > current_target {
                return Ok(ReturnedLiquidityPlan::Hold {
                    target: current_target,
                    required_credit: 0,
                    tolerance: self.backing.pooled - current_target,
                });
            }

            let post_fee_backing = backing
                .checked_sub(exact_restake_fee)
                .ok_or(ModelError::InsufficientBacking)?;
            let post_fee_target =
                target_pool(active_stake, post_fee_backing, claims)?.max(minimum_parent);
            let required_credit = post_fee_target.saturating_sub(self.backing.pooled);
            let tolerance = exact_restake_fee.max(minimum_restake_credit.saturating_sub(1));
            if required_credit <= exact_restake_fee || required_credit < minimum_restake_credit {
                return Ok(ReturnedLiquidityPlan::Hold {
                    target: post_fee_target,
                    required_credit,
                    tolerance,
                });
            }
            let source_debit = required_credit
                .checked_add(exact_restake_fee)
                .ok_or(ModelError::ArithmeticOverflow)?;
            if self.backing.liquid < source_debit {
                return Err(ModelError::InsufficientLiquid);
            }
            self.planned_restake_credit = required_credit;
            self.planned_restake_debit = source_debit;
            self.planned_post_fee_target = post_fee_target;
            self.neuron.status = StickyStatus::RestakePending;
            self.neuron.reward_eligible_from_observation = None;
            Ok(ReturnedLiquidityPlan::Restake {
                post_fee_target,
                credited: required_credit,
                source_debit,
            })
        }

        pub fn prove_restake(
            &mut self,
            actual_credited: u128,
            exact_fee: u128,
            next_reward_observation: u64,
        ) -> Result<(), ModelError> {
            if self.neuron.status != StickyStatus::RestakePending
                || actual_credited != self.planned_restake_credit
                || actual_credited
                    .checked_add(exact_fee)
                    .ok_or(ModelError::ArithmeticOverflow)?
                    != self.planned_restake_debit
            {
                return Err(ModelError::ProofMismatch);
            }
            self.backing = move_backing(
                self.backing,
                Bucket::Liquid,
                Bucket::Pooled,
                self.planned_restake_debit,
                exact_fee,
            )?;
            if self.backing.pooled != self.planned_post_fee_target {
                return Err(ModelError::ProofMismatch);
            }
            self.operations.restake_proofs = self
                .operations
                .restake_proofs
                .checked_add(1)
                .ok_or(ModelError::ArithmeticOverflow)?;
            self.fees.restake = self
                .fees
                .restake
                .checked_add(exact_fee)
                .ok_or(ModelError::ArithmeticOverflow)?;
            self.clear_planned_restake();
            self.neuron.status = StickyStatus::ActiveBacked;
            self.neuron.reward_eligible_from_observation = Some(next_reward_observation);
            Ok(())
        }

        fn clear_planned_restake(&mut self) {
            self.planned_restake_credit = 0;
            self.planned_restake_debit = 0;
            self.planned_post_fee_target = 0;
        }
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
        assert_eq!(claim_rate(0, 0), ClaimRate::EmptyGenesis);
        assert_eq!(
            claim_rate(0, 100),
            ClaimRate::Ratio {
                backing: 0,
                claims: 100
            }
        );
        assert_eq!(
            claim_rate(100, 0),
            ClaimRate::BackingWithoutClaims { backing: 100 }
        );
        assert_eq!(
            claim_rate(100, 100),
            ClaimRate::Ratio {
                backing: 100,
                claims: 100
            }
        );
        assert_eq!(
            claim_rate(250, 100),
            ClaimRate::Ratio {
                backing: 250,
                claims: 100
            }
        );
        let large = u128::MAX / 4;
        assert_eq!(
            claim_rate(large, large),
            ClaimRate::Ratio {
                backing: large,
                claims: large
            }
        );
        assert_eq!(target_pool(large, 2, 2), Ok(large));
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
        assert_eq!(target_pool(101, 100, 100), Ok(101));
        assert_eq!(
            target_pool(u128::MAX, 2, 1),
            Err(ModelError::ArithmeticOverflow)
        );
        assert_eq!(target_pool(1, 1, 0), Err(ModelError::ZeroClaimSupply));
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
                for active in 0..=50 {
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
            ClaimRate::Ratio {
                backing: 199,
                claims: 200
            }
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
        };
        let plan = plan_reconciliation(under).unwrap();
        let (target, credited, debit) = match plan {
            ReconcilePlan::TopUp {
                post_fee_target,
                credited,
                source_debit,
            } => (post_fee_target, credited, source_debit),
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
            Ok(ReconcilePlan::Hold { tolerance: 10, .. })
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
            Ok(ReconcilePlan::Hold { tolerance: 109, .. })
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

    fn sticky_base() -> StickyUnwindModel {
        StickyUnwindModel::new(Backing {
            liquid: 500,
            pooled: 500,
            ..Backing::default()
        })
    }

    fn prove_one_sticky_child(model: &mut StickyUnwindModel) {
        model.observe_sns(CanonicalSnsState::Dissolving, 1).unwrap();
        model.submit_split_intent(110).unwrap();
        model.prove_split(10).unwrap();
        assert_eq!(model.aggregate_child_principal, 100);
    }

    #[test]
    fn sticky_start_then_cancel_before_split_is_fee_free() {
        let mut model = sticky_base();
        let initial = model.backing;
        assert!(model.neuron.reward_eligible_at(0));

        model.observe_sns(CanonicalSnsState::Dissolving, 1).unwrap();
        assert_eq!(model.neuron.status, StickyStatus::ExitObserved);
        assert!(!model.neuron.reward_eligible_at(1));
        model.observe_sns(CanonicalSnsState::Active, 2).unwrap();

        assert_eq!(model.neuron.status, StickyStatus::ActiveBacked);
        assert!(!model.neuron.reward_eligible_at(2));
        assert!(model.neuron.reward_eligible_at(3));
        assert_eq!(model.backing, initial);
        assert_eq!(model.backing.pending_unwind, 0);
        assert_eq!(model.operations, StickyOperationCounts::default());
        assert_eq!(model.fees.total(), Ok(0));
    }

    #[test]
    fn sticky_committed_cancel_continues_without_merge_or_rewards() {
        let mut model = sticky_base();
        model.observe_sns(CanonicalSnsState::Dissolving, 1).unwrap();
        model.submit_split_intent(110).unwrap();
        assert_eq!((model.backing.pooled, model.backing.transit), (390, 110));

        model.observe_sns(CanonicalSnsState::Active, 2).unwrap();
        assert_eq!(model.neuron.status, StickyStatus::ReentryPending);
        model.prove_split(10).unwrap();

        assert_eq!(model.aggregate_child_principal, 100);
        assert_eq!(model.backing.pending_unwind, 100);
        assert_eq!(model.operations.split_intents, 1);
        assert_eq!(model.operations.split_proofs, 1);
        assert_eq!(model.operations.merge_proofs, 0);
        assert_eq!(model.fees.split, 10);
        assert!(!model.neuron.reward_eligible_at(u64::MAX));
    }

    #[test]
    fn sticky_redissolve_before_disbursement_returns_liquid_without_restake() {
        let mut model = sticky_base();
        prove_one_sticky_child(&mut model);
        model.observe_sns(CanonicalSnsState::Active, 2).unwrap();
        model.observe_sns(CanonicalSnsState::Dissolving, 3).unwrap();
        assert_eq!(model.neuron.status, StickyStatus::ExitCommitted);

        model.prove_child_disbursement(10).unwrap();
        assert_eq!(model.neuron.status, StickyStatus::LiquidReturned);
        assert_eq!(
            model
                .plan_returned_liquidity(1_000, 400, 10, 1, 0, 4)
                .unwrap(),
            ReturnedLiquidityPlan::LatestStateDissolving
        );
        assert_eq!((model.backing.liquid, model.backing.pooled), (590, 390));
        assert_eq!(model.operations.split_proofs, 1);
        assert_eq!(model.operations.disbursement_proofs, 1);
        assert_eq!(model.operations.restake_proofs, 0);
        assert_eq!(model.operations.merge_proofs, 0);
        assert_eq!(model.fees.total(), Ok(20));
    }

    #[test]
    fn sticky_active_reentry_restakes_only_global_target_delta_after_proof() {
        let mut model = sticky_base();
        let initial = model.backing;
        prove_one_sticky_child(&mut model);
        model.observe_sns(CanonicalSnsState::Active, 2).unwrap();
        model.prove_child_disbursement(10).unwrap();

        let plan = model
            .plan_returned_liquidity(1_000, 500, 10, 1, 0, 4)
            .unwrap();
        assert_eq!(
            plan,
            ReturnedLiquidityPlan::Restake {
                post_fee_target: 485,
                credited: 95,
                source_debit: 105,
            }
        );
        assert_eq!(model.neuron.status, StickyStatus::RestakePending);
        assert_eq!(
            model.prove_restake(94, 10, 4),
            Err(ModelError::ProofMismatch)
        );
        assert!(!model.neuron.reward_eligible_at(4));
        model.prove_restake(95, 10, 4).unwrap();

        assert_eq!((model.backing.liquid, model.backing.pooled), (485, 485));
        assert_eq!(model.backing.claim_backing(), Ok(970));
        assert_eq!(target_pool(500, 970, 1_000), Ok(485));
        assert_ne!(95, 100, "the original child amount is not auto-restaked");
        assert_eq!(model.operations.restake_proofs, 1);
        assert_eq!(model.operations.merge_proofs, 0);
        assert_eq!(model.fees.total(), Ok(30));
        assert_conservation(initial, model.backing, 0, 30, 0).unwrap();
        assert!(model.neuron.reward_eligible_at(4));
    }

    #[test]
    fn sticky_flip_flops_net_before_commit_and_do_not_multiply_committed_effects() {
        let mut before_commit = sticky_base();
        before_commit
            .observe_sns(CanonicalSnsState::Dissolving, 1)
            .unwrap();
        before_commit
            .observe_sns(CanonicalSnsState::Active, 2)
            .unwrap();
        before_commit
            .observe_sns(CanonicalSnsState::Dissolving, 3)
            .unwrap();
        before_commit.submit_split_intent(110).unwrap();
        before_commit.prove_split(10).unwrap();
        assert_eq!(before_commit.operations.split_intents, 1);

        for observation in 4..=21 {
            let state = if observation % 2 == 0 {
                CanonicalSnsState::Active
            } else {
                CanonicalSnsState::Dissolving
            };
            before_commit.observe_sns(state, observation).unwrap();
        }
        assert_eq!(before_commit.operations.split_intents, 1);
        assert_eq!(before_commit.operations.split_proofs, 1);
        assert_eq!(before_commit.operations.merge_proofs, 0);
        assert_eq!(before_commit.aggregate_child_principal, 100);
        assert_eq!(before_commit.fees.total(), Ok(10));
        assert!(!before_commit.neuron.reward_eligible_at(u64::MAX));

        before_commit
            .observe_sns(CanonicalSnsState::Active, 22)
            .unwrap();
        before_commit.prove_child_disbursement(10).unwrap();
        assert!(matches!(
            before_commit.plan_returned_liquidity(1_000, 500, 10, 1, 0, 23),
            Ok(ReturnedLiquidityPlan::Restake {
                credited: 95,
                source_debit: 105,
                ..
            })
        ));
        assert_eq!(before_commit.operations.split_intents, 1);
        assert_eq!(before_commit.operations.disbursement_proofs, 1);
        assert_eq!(before_commit.operations.merge_proofs, 0);
    }

    #[test]
    fn sticky_mixed_cohort_uses_latest_global_target_without_child_ownership() {
        let still_dissolving = StickyNeuron {
            status: StickyStatus::ExitCommitted,
            latest_sns_state: CanonicalSnsState::Dissolving,
            reward_eligible_from_observation: None,
        };
        let mut aggregate = sticky_base();
        prove_one_sticky_child(&mut aggregate);
        aggregate.observe_sns(CanonicalSnsState::Active, 2).unwrap();
        aggregate.prove_child_disbursement(10).unwrap();

        let plan = aggregate
            .plan_returned_liquidity(1_000, 600, 10, 1, 0, 4)
            .unwrap();
        assert_eq!(
            plan,
            ReturnedLiquidityPlan::Restake {
                post_fee_target: 582,
                credited: 192,
                source_debit: 202,
            }
        );
        aggregate.prove_restake(192, 10, 4).unwrap();

        assert_eq!(aggregate.backing.pooled, 582);
        assert_eq!(target_pool(600, 970, 1_000), Ok(582));
        assert!(!still_dissolving.reward_eligible_at(u64::MAX));
        assert!(aggregate.neuron.reward_eligible_at(4));
        assert_eq!(aggregate.operations.split_proofs, 1);
        assert_eq!(aggregate.aggregate_child_principal, 0);
    }

    #[test]
    fn sticky_tolerance_and_fee_comparison_bound_reversal_churn() {
        let mut fee_batched = StickyUnwindModel::new(Backing {
            liquid: 509,
            pooled: 491,
            ..Backing::default()
        });
        fee_batched.neuron.status = StickyStatus::LiquidReturned;
        let before = fee_batched.backing;
        assert_eq!(
            fee_batched
                .plan_returned_liquidity(1_000, 500, 10, 1, 0, 1)
                .unwrap(),
            ReturnedLiquidityPlan::Hold {
                target: 495,
                required_credit: 4,
                tolerance: 10,
            }
        );
        assert_eq!(fee_batched.backing, before);
        assert_eq!(fee_batched.operations.restake_proofs, 0);
        assert_eq!(fee_batched.fees.total(), Ok(0));

        let mut minimum_batched = StickyUnwindModel::new(Backing {
            liquid: 600,
            pooled: 400,
            ..Backing::default()
        });
        minimum_batched.neuron.status = StickyStatus::LiquidReturned;
        assert_eq!(
            minimum_batched
                .plan_returned_liquidity(1_000, 500, 10, 100, 0, 1)
                .unwrap(),
            ReturnedLiquidityPlan::Hold {
                target: 495,
                required_credit: 95,
                tolerance: 99,
            }
        );
        assert_eq!(minimum_batched.operations.restake_proofs, 0);
        assert_eq!(minimum_batched.fees.total(), Ok(0));

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
