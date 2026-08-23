#![cfg(test)]

//! Executable proposal model for pooled claim-backing economics.
//!
//! This crate is test-only. It does not define the active IO economics and is
//! not linked into a canister.

pub mod proposed_model {
    use io_core_model::{Backing as CoreBacking, EconomicState as CoreState, EconomicsError};

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum ModelError {
        ArithmeticOverflow,
        ExclusionsExceedSupply,
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
        DuplicateGeneration,
        DuplicateChild,
        CohortCapacityExhausted,
        RewardActiveExceedsBacking,
        RewardBackingUnderTarget,
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
            io_core_model::claim_backing(self.core()).map_err(Into::into)
        }

        pub fn total_assets(self) -> Result<u128, ModelError> {
            self.claim_backing()?
                .checked_add(self.permanent)
                .and_then(|value| value.checked_add(self.operational_reserve))
                .ok_or(ModelError::ArithmeticOverflow)
        }
        fn core(self) -> CoreBacking {
            CoreBacking {
                liquid: self.liquid,
                pooled: self.pooled,
                unwinding: self.pending_unwind,
                transit: self.transit,
            }
        }
    }

    impl From<EconomicsError> for ModelError {
        fn from(value: EconomicsError) -> Self {
            match value {
                EconomicsError::ArithmeticOverflow => Self::ArithmeticOverflow,
                EconomicsError::ExclusionsExceedSupply => Self::ExclusionsExceedSupply,
                EconomicsError::BackingWithoutClaims => Self::BackingWithoutClaims,
                EconomicsError::UncoveredClaims => Self::UncoveredClaims,
                EconomicsError::ActiveExceedsClaims => Self::ActiveExceedsClaims,
                EconomicsError::RewardActiveExceedsBacking => Self::RewardActiveExceedsBacking,
                EconomicsError::RewardBackingUnderTarget => Self::RewardBackingUnderTarget,
                _ => Self::InvalidBackingState,
            }
        }
    }

    pub fn target_pool(active: u128, backing: u128, claims: u128) -> Result<u128, ModelError> {
        io_core_model::target(active, backing, claims).map_err(Into::into)
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

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum HoldReason {
        BelowMinimumStake,
        FeeTolerance,
        ChildMinimum,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum CanonicalSnsState {
        Active,
        Dissolving,
        LiquidOrDissolved,
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
        CanonicalDissolving,
        DisbursementSubmitted,
        PrincipalReturned,
        MaturityHandled,
        CleanupComplete,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct PassiveCohort {
        pub generation: u64,
        pub child_neuron_id: u64,
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
        pub disbursement_proofs: u128,
        pub restake_proofs: u128,
    }

    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct StickyFeeTotals {
        pub split: u128,
        pub disbursement: u128,
        pub restake: u128,
    }

    impl StickyFeeTotals {
        pub fn total(self) -> Result<u128, ModelError> {
            self.split
                .checked_add(self.disbursement)
                .and_then(|value| value.checked_add(self.restake))
                .ok_or(ModelError::ArithmeticOverflow)
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum ReturnedLiquidityPlan {
        NoActiveMembersNeedRestake,
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
        SplitCommitted {
            generation: u64,
            gross: u128,
        },
        SplitProved {
            generation: u64,
            child_neuron_id: u64,
            principal: u128,
        },
        StartDissolvingCommitted {
            generation: u64,
            child_neuron_id: u64,
            principal: u128,
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
        pub const NNS_DISSOLVE_DELAY_SECONDS: u64 = 14 * 86_400;

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
            let generation = self
                .neurons
                .get(neuron_index)
                .ok_or(ModelError::InvalidTransition)?
                .committed_generation;
            if state != CanonicalSnsState::Active
                && self.neurons[neuron_index].status == StickyStatus::RestakePlanned
            {
                let intent = self
                    .planned_restake
                    .take()
                    .filter(|intent| Some(intent.generation) == generation)
                    .ok_or(ModelError::InvalidTransition)?;
                for member in &mut self.neurons {
                    if member.committed_generation == Some(intent.generation)
                        && member.status == StickyStatus::RestakePlanned
                    {
                        member.status = StickyStatus::LiquidReturned;
                    }
                }
            }
            let returned = generation.is_some_and(|generation| {
                self.cohorts.iter().any(|cohort| {
                    cohort.generation == generation
                        && matches!(
                            cohort.proof,
                            CohortProofState::PrincipalReturned
                                | CohortProofState::MaturityHandled
                                | CohortProofState::CleanupComplete
                        )
                })
            });
            let neuron = &mut self.neurons[neuron_index];
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
                        StickyStatus::RestakePlanned => StickyStatus::LiquidReturned,
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
                CanonicalSnsState::LiquidOrDissolved => {
                    neuron.reward_eligible_from_observation = None;
                    match neuron.status {
                        StickyStatus::RestakeCommitted | StickyStatus::RestakeProved => {}
                        _ => {
                            if returned {
                                neuron.committed_generation = None;
                            }
                            neuron.status = StickyStatus::ReentryPending;
                        }
                    }
                }
            }
            Ok(())
        }

        pub fn submit_split_intent(
            &mut self,
            neuron_indices: &[usize],
            gross: u128,
        ) -> Result<u64, ModelError> {
            if neuron_indices.is_empty() || self.active_command.is_some() {
                return Err(ModelError::InvalidTransition);
            }
            if self.cohorts.len() >= self.max_passive_cohorts {
                return Err(ModelError::CohortCapacityExhausted);
            }
            let mut unique = neuron_indices.to_vec();
            unique.sort_unstable();
            unique.dedup();
            if unique.len() != neuron_indices.len()
                || unique.iter().any(|index| {
                    self.neurons.get(*index).is_none_or(|neuron| {
                        neuron.status != StickyStatus::ExitObserved
                            || neuron.latest_sns_state != CanonicalSnsState::Dissolving
                    })
                })
            {
                return Err(ModelError::InvalidTransition);
            }
            let generation = self.next_generation;
            if self
                .cohorts
                .iter()
                .any(|cohort| cohort.generation == generation)
            {
                return Err(ModelError::DuplicateGeneration);
            }
            self.next_generation = self
                .next_generation
                .checked_add(1)
                .ok_or(ModelError::ArithmeticOverflow)?;
            self.backing = move_backing(self.backing, Bucket::Pooled, Bucket::Transit, gross, 0)?;
            self.active_command = Some(ActiveNnsCommand::SplitCommitted { generation, gross });
            self.operations.split_intents = self
                .operations
                .split_intents
                .checked_add(1)
                .ok_or(ModelError::ArithmeticOverflow)?;
            for index in unique {
                self.neurons[index].status = StickyStatus::ExitCommitted;
                self.neurons[index].committed_generation = Some(generation);
            }
            Ok(generation)
        }

        pub fn prove_split(
            &mut self,
            child_neuron_id: u64,
            exact_fee: u128,
        ) -> Result<(), ModelError> {
            let (generation, gross) = match self.active_command {
                Some(ActiveNnsCommand::SplitCommitted { generation, gross }) => (generation, gross),
                _ => return Err(ModelError::InvalidTransition),
            };
            if self.cohorts.iter().any(|cohort| {
                cohort.child_neuron_id == child_neuron_id || cohort.generation == generation
            }) {
                return Err(ModelError::DuplicateChild);
            }
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
            self.active_command = Some(ActiveNnsCommand::SplitProved {
                generation,
                child_neuron_id,
                principal: credited,
            });
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

        pub fn commit_start_dissolving(&mut self, generation: u64) -> Result<(), ModelError> {
            let (child_neuron_id, principal) = match self.active_command {
                Some(ActiveNnsCommand::SplitProved {
                    generation: active_generation,
                    child_neuron_id,
                    principal,
                }) if active_generation == generation => (child_neuron_id, principal),
                _ => return Err(ModelError::InvalidTransition),
            };
            self.active_command = Some(ActiveNnsCommand::StartDissolvingCommitted {
                generation,
                child_neuron_id,
                principal,
            });
            Ok(())
        }

        pub fn prove_start_dissolving_rejected(
            &mut self,
            generation: u64,
            child_neuron_id: u64,
        ) -> Result<(), ModelError> {
            let principal = match self.active_command {
                Some(ActiveNnsCommand::StartDissolvingCommitted {
                    generation: active_generation,
                    child_neuron_id: active_child,
                    principal,
                }) if active_generation == generation && active_child == child_neuron_id => {
                    principal
                }
                _ => return Err(ModelError::ProofMismatch),
            };
            self.active_command = Some(ActiveNnsCommand::SplitProved {
                generation,
                child_neuron_id,
                principal,
            });
            Ok(())
        }

        pub fn prove_start_dissolving(
            &mut self,
            generation: u64,
            child_neuron_id: u64,
            canonical_ready_at: u64,
        ) -> Result<u64, ModelError> {
            let principal = match self.active_command {
                Some(ActiveNnsCommand::StartDissolvingCommitted {
                    generation: active_generation,
                    child_neuron_id: active_child,
                    principal,
                }) if active_generation == generation && active_child == child_neuron_id => {
                    principal
                }
                _ => return Err(ModelError::ProofMismatch),
            };
            if self.cohorts.len() >= self.max_passive_cohorts {
                return Err(ModelError::CohortCapacityExhausted);
            }
            if self
                .cohorts
                .iter()
                .any(|cohort| cohort.generation == generation)
            {
                return Err(ModelError::DuplicateGeneration);
            }
            if self
                .cohorts
                .iter()
                .any(|cohort| cohort.child_neuron_id == child_neuron_id)
            {
                return Err(ModelError::DuplicateChild);
            }
            let effective_start = canonical_ready_at
                .checked_sub(Self::NNS_DISSOLVE_DELAY_SECONDS)
                .ok_or(ModelError::ProofMismatch)?;
            self.cohorts.push(PassiveCohort {
                generation,
                child_neuron_id,
                principal,
                lifecycle: CohortLifecycle::Dissolving,
                proof: CohortProofState::CanonicalDissolving,
                ready_at: canonical_ready_at,
            });
            self.active_command = None;
            Ok(effective_start)
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
                || cohort.proof != CohortProofState::CanonicalDissolving
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
            self.cohorts[cohort_index].proof = CohortProofState::PrincipalReturned;
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
                || !matches!(
                    cohort.proof,
                    CohortProofState::PrincipalReturned
                        | CohortProofState::MaturityHandled
                        | CohortProofState::CleanupComplete
                )
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
            let active: Vec<usize> = matching
                .iter()
                .copied()
                .filter(|index| self.neurons[*index].latest_sns_state == CanonicalSnsState::Active)
                .collect();
            if active.is_empty() {
                return Ok(ReturnedLiquidityPlan::NoActiveMembersNeedRestake);
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
            for index in active {
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
            let live_returned_cohort = self.cohorts.iter().any(|cohort| {
                cohort.generation == generation
                    && cohort.lifecycle == CohortLifecycle::Returned
                    && matches!(
                        cohort.proof,
                        CohortProofState::PrincipalReturned
                            | CohortProofState::MaturityHandled
                            | CohortProofState::CleanupComplete
                    )
            });
            let has_current_active_member = self.neurons.iter().any(|neuron| {
                neuron.committed_generation == Some(generation)
                    && neuron.latest_sns_state == CanonicalSnsState::Active
                    && neuron.status == StickyStatus::RestakePlanned
            });
            if !live_returned_cohort || !has_current_active_member {
                return Err(ModelError::InvalidTransition);
            }
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
                if neuron.committed_generation == Some(generation)
                    && neuron.latest_sns_state == CanonicalSnsState::Active
                {
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
                if neuron.committed_generation == Some(generation)
                    && neuron.status == StickyStatus::RestakeCommitted
                {
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
                if neuron.committed_generation == Some(generation)
                    && neuron.status == StickyStatus::RestakeProved
                {
                    if neuron.status != StickyStatus::RestakeProved {
                        return Err(ModelError::InvalidTransition);
                    }
                    found = true;
                    neuron.reward_eligible_from_observation = None;
                    neuron.committed_generation = None;
                    neuron.status = match neuron.latest_sns_state {
                        CanonicalSnsState::Active => {
                            neuron.reward_eligible_from_observation = Some(next_reward_observation);
                            StickyStatus::ActiveBacked
                        }
                        CanonicalSnsState::Dissolving => StickyStatus::ExitObserved,
                        CanonicalSnsState::LiquidOrDissolved => StickyStatus::ReentryPending,
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

        pub fn prove_child_maturity_handled(&mut self, generation: u64) -> Result<(), ModelError> {
            let cohort = self
                .cohorts
                .iter_mut()
                .find(|cohort| cohort.generation == generation)
                .ok_or(ModelError::InvalidTransition)?;
            if cohort.proof != CohortProofState::PrincipalReturned {
                return Err(ModelError::InvalidTransition);
            }
            cohort.proof = CohortProofState::MaturityHandled;
            Ok(())
        }

        pub fn prove_child_cleanup_complete(&mut self, generation: u64) -> Result<(), ModelError> {
            let cohort = self
                .cohorts
                .iter_mut()
                .find(|cohort| cohort.generation == generation)
                .ok_or(ModelError::InvalidTransition)?;
            if cohort.proof != CohortProofState::MaturityHandled {
                return Err(ModelError::InvalidTransition);
            }
            cohort.proof = CohortProofState::CleanupComplete;
            Ok(())
        }

        pub fn retire_cohort(&mut self, generation: u64) -> Result<PassiveCohort, ModelError> {
            let index = self
                .cohorts
                .iter()
                .position(|cohort| cohort.generation == generation)
                .ok_or(ModelError::InvalidTransition)?;
            let cohort = self.cohorts[index];
            if cohort.lifecycle != CohortLifecycle::Returned
                || cohort.proof != CohortProofState::CleanupComplete
                || self
                    .neurons
                    .iter()
                    .any(|neuron| neuron.committed_generation == Some(generation))
                || self.active_command.is_some_and(|command| {
                    command.generation() == generation
                        || command.child_neuron_id() == Some(cohort.child_neuron_id)
                })
                || self
                    .planned_restake
                    .is_some_and(|intent| intent.generation == generation)
            {
                return Err(ModelError::InvalidTransition);
            }
            Ok(self.cohorts.remove(index))
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

    impl ActiveNnsCommand {
        fn generation(self) -> u64 {
            match self {
                Self::SplitCommitted { generation, .. }
                | Self::SplitProved { generation, .. }
                | Self::StartDissolvingCommitted { generation, .. }
                | Self::Disburse { generation }
                | Self::Restake(RestakeIntent { generation, .. }) => generation,
            }
        }

        fn child_neuron_id(self) -> Option<u64> {
            match self {
                Self::SplitProved {
                    child_neuron_id, ..
                }
                | Self::StartDissolvingCommitted {
                    child_neuron_id, ..
                } => Some(child_neuron_id),
                _ => None,
            }
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct LiquidityLagInputs {
        pub maximum_detection_reconciliation_interval: Option<u64>,
        pub nns_dissolve_delay: u64,
        pub max_detection_margin: u64,
        pub max_command_margin: u64,
        pub max_disbursement_margin: u64,
    }

    pub fn liquidity_lag_bound(input: LiquidityLagInputs) -> Result<Option<u64>, ModelError> {
        let Some(interval) = input.maximum_detection_reconciliation_interval else {
            return Ok(None);
        };
        interval
            .checked_add(input.nns_dissolve_delay)
            .and_then(|value| value.checked_add(input.max_detection_margin))
            .and_then(|value| value.checked_add(input.max_command_margin))
            .and_then(|value| value.checked_add(input.max_disbursement_margin))
            .map(Some)
            .ok_or(ModelError::ArithmeticOverflow)
    }

    pub fn cohort_capacity_bound(
        maximum_unresolved_cohort_lifetime: u64,
        minimum_committed_generation_spacing: u64,
        reviewed_operational_margin: u64,
    ) -> Result<u64, ModelError> {
        if minimum_committed_generation_spacing == 0 {
            return Err(ModelError::InvalidTransition);
        }
        let rounded = maximum_unresolved_cohort_lifetime
            .checked_add(minimum_committed_generation_spacing - 1)
            .ok_or(ModelError::ArithmeticOverflow)?
            / minimum_committed_generation_spacing;
        rounded
            .checked_add(reviewed_operational_margin)
            .ok_or(ModelError::ArithmeticOverflow)
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct RewardCoverage {
        pub backing_target: u128,
        pub reward_target: u128,
    }

    pub fn require_reward_coverage(
        active_backing: u128,
        reward_eligible_active: u128,
        backing: u128,
        claims: u128,
        pooled: u128,
    ) -> Result<RewardCoverage, ModelError> {
        let state = CoreState {
            backing: CoreBacking {
                liquid: backing
                    .checked_sub(pooled)
                    .ok_or(ModelError::InvalidBackingState)?,
                pooled,
                ..CoreBacking::default()
            },
            claims,
            active_backing,
            active_reward: reward_eligible_active,
        };
        let backing_target = io_core_model::target(active_backing, backing, claims)?;
        let reward_target = io_core_model::reward_target(state)?;
        io_core_model::rewards_covered(state)?;
        Ok(RewardCoverage {
            backing_target,
            reward_target,
        })
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
        EmptyGenesis,
        BackingWithoutClaims { backing: u128 },
        AwaitLiquidity { gross_quote: u128 },
        Ready { gross_quote: u128, net_payout: u128 },
    }

    pub fn redemption_readiness(
        io_amount: u128,
        backing: Backing,
        claim_supply: u128,
        payout_fee: u128,
    ) -> Result<RedemptionReadiness, ModelError> {
        let claim_backing = backing.claim_backing()?;
        let state = CoreState {
            backing: backing.core(),
            claims: claim_supply,
            active_backing: 0,
            active_reward: 0,
        };
        match io_core_model::claim_rate(state)? {
            io_core_model::ClaimRate::EmptyGenesis => {
                return Ok(RedemptionReadiness::EmptyGenesis);
            }
            io_core_model::ClaimRate::BackingWithoutClaims => {
                return Ok(RedemptionReadiness::BackingWithoutClaims {
                    backing: claim_backing,
                });
            }
            io_core_model::ClaimRate::Ratio { .. } => {}
        }
        let quote = io_core_model::redemption_quote(state, io_amount, 0, payout_fee)?;
        if io_core_model::require_liquidity(quote, backing.liquid).is_err() {
            return Ok(RedemptionReadiness::AwaitLiquidity {
                gross_quote: quote.gross_icp,
            });
        }
        Ok(RedemptionReadiness::Ready {
            gross_quote: quote.gross_icp,
            net_payout: quote.net_icp,
        })
    }
}

mod tests {
    use super::proposed_model::*;

    #[test]
    fn production_economics_are_the_scenario_oracle() {
        let backing = io_core_model::Backing {
            liquid: 100,
            pooled: 500,
            unwinding: 200,
            transit: 200,
        };
        let state = io_core_model::EconomicState {
            backing,
            claims: 1_000,
            active_backing: 500,
            active_reward: 400,
        };
        let quote = io_core_model::redemption_quote(state, 200, 0, 10).unwrap();
        assert_eq!((quote.gross_icp, quote.net_icp), (200, 190));
        assert!(io_core_model::require_liquidity(quote, backing.liquid).is_err());
        assert_eq!(io_core_model::target(500, 1_000, 1_000), Ok(500));
        assert_eq!(io_core_model::reward_target(state), Ok(400));
    }

    #[test]
    fn production_pooled_maturity_enters_liquid_once() {
        let plan = io_reward_policy::plan_pooled_maturity(io_reward_policy::PooledMaturityInput {
            pre_backing: 100_000_000_000,
            pre_claims: 100_000_000_000,
            actual_mint: 100_000_000,
            permanent_transfer_fee: 10_000,
            claim_transfer_fee: 10_000,
            policy_credit_total: 1,
            entitlements: &[],
            reserve_io_capacity: u128::MAX,
            io_fee: 10_000,
            snapshot_fingerprint: [1; 32],
        })
        .unwrap();
        assert_eq!(plan.permanent_credit, 39_990_000);
        assert_eq!(plan.claim.claim_credit, 59_990_000);
        assert_eq!(plan.claim.maximum_io_pool, 59_990_000);
        assert_eq!(plan.claim.post_backing, 100_059_990_000);
    }
    const DAY: u64 = 86_400;
    const TWO_WEEK_DELAY: u64 = 14 * DAY;

    fn sticky_base(neuron_count: usize, capacity: usize) -> StickyUnwindModel {
        StickyUnwindModel::with_neurons(
            Backing {
                liquid: 500,
                pooled: 500,
                ..Backing::default()
            },
            neuron_count,
            capacity,
        )
    }

    fn commit_cohort(
        model: &mut StickyUnwindModel,
        neuron_indices: &[usize],
        child_neuron_id: u64,
        ready_at: u64,
    ) -> u64 {
        for index in neuron_indices {
            model
                .observe_sns(*index, CanonicalSnsState::Dissolving, 1)
                .unwrap();
        }
        let generation = model.submit_split_intent(neuron_indices, 110).unwrap();
        model.prove_split(child_neuron_id, 10).unwrap();
        model.commit_start_dissolving(generation).unwrap();
        assert_eq!(
            model
                .prove_start_dissolving(generation, child_neuron_id, ready_at)
                .unwrap(),
            ready_at - TWO_WEEK_DELAY
        );
        generation
    }

    fn return_cohort(model: &mut StickyUnwindModel, generation: u64, now: u64) {
        model.submit_child_disbursement(generation, now).unwrap();
        model.prove_child_disbursement(generation, 10).unwrap();
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
    fn split_and_start_dissolving_are_distinct_recoverable_active_phases() {
        let mut model = sticky_base(3, 2);
        for index in 0..3 {
            model
                .observe_sns(index, CanonicalSnsState::Dissolving, 1)
                .unwrap();
        }
        let generation = model.submit_split_intent(&[0, 1, 2], 110).unwrap();
        assert_eq!(generation, 0);
        assert!(model.cohorts.is_empty());
        model.prove_split(7_001, 10).unwrap();
        assert_eq!(
            model.active_command,
            Some(ActiveNnsCommand::SplitProved {
                generation,
                child_neuron_id: 7_001,
                principal: 100,
            })
        );
        assert!(model.cohorts.is_empty());

        model.commit_start_dissolving(generation).unwrap();
        assert!(matches!(
            model.active_command,
            Some(ActiveNnsCommand::StartDissolvingCommitted { .. })
        ));
        model
            .prove_start_dissolving_rejected(generation, 7_001)
            .unwrap();
        assert!(matches!(
            model.active_command,
            Some(ActiveNnsCommand::SplitProved { .. })
        ));
        model.commit_start_dissolving(generation).unwrap();
        assert_eq!(
            model.prove_start_dissolving(generation, 7_002, TWO_WEEK_DELAY + 37),
            Err(ModelError::ProofMismatch)
        );
        assert!(
            model.active_command.is_some(),
            "callback loss retains the slot"
        );
        let effective_start = model
            .prove_start_dissolving(generation, 7_001, TWO_WEEK_DELAY + 37)
            .unwrap();
        assert_eq!(effective_start, 37);
        assert_eq!(model.cohorts.len(), 1);
        assert_eq!(
            model.cohort(generation).unwrap().ready_at,
            TWO_WEEK_DELAY + 37
        );
        assert!(model.active_command.is_none());
        assert!(model
            .neurons
            .iter()
            .all(|neuron| neuron.committed_generation == Some(generation)));
    }

    #[test]
    fn overlapping_cohorts_have_independent_canonical_clocks_and_pooled_returns() {
        let mut model = sticky_base(2, 2);
        let a = commit_cohort(&mut model, &[0], 7_001, TWO_WEEK_DELAY);
        model
            .observe_sns(1, CanonicalSnsState::Dissolving, 2)
            .unwrap();
        model.next_generation = a;
        assert_eq!(
            model.submit_split_intent(&[1], 110),
            Err(ModelError::DuplicateGeneration)
        );
        model.next_generation = a + 1;
        let b = model.submit_split_intent(&[1], 110).unwrap();
        assert_eq!(
            model.prove_split(7_001, 10),
            Err(ModelError::DuplicateChild)
        );
        model.prove_split(7_002, 10).unwrap();
        model.commit_start_dissolving(b).unwrap();
        model
            .prove_start_dissolving(b, 7_002, TWO_WEEK_DELAY + DAY)
            .unwrap();
        model.refresh_cohort_readiness(TWO_WEEK_DELAY);
        assert_eq!(model.cohort(a).unwrap().lifecycle, CohortLifecycle::Ready);
        assert_eq!(
            model.cohort(b).unwrap().lifecycle,
            CohortLifecycle::Dissolving
        );
        return_cohort(&mut model, a, TWO_WEEK_DELAY);
        assert_eq!(
            (model.backing.liquid, model.backing.pending_unwind),
            (590, 100)
        );
        assert_eq!(
            model.cohort(b).unwrap().lifecycle,
            CohortLifecycle::Dissolving
        );
        return_cohort(&mut model, b, TWO_WEEK_DELAY + DAY);
        assert_eq!(
            (model.backing.liquid, model.backing.pending_unwind),
            (680, 0)
        );
    }

    #[test]
    fn aggregate_generation_processes_active_and_dissolving_members_separately() {
        let mut model = sticky_base(3, 2);
        let generation = commit_cohort(&mut model, &[0, 1, 2], 8_001, TWO_WEEK_DELAY);
        model.observe_sns(1, CanonicalSnsState::Active, 2).unwrap();
        model.observe_sns(2, CanonicalSnsState::Active, 2).unwrap();
        model
            .observe_sns(2, CanonicalSnsState::Dissolving, 3)
            .unwrap();
        assert_eq!(model.operations.split_intents, 1);
        assert_eq!(model.cohorts.len(), 1);
        assert!(model.neurons.iter().all(|member| {
            member.committed_generation == Some(generation) && !member.reward_eligible_at(u64::MAX)
        }));

        return_cohort(&mut model, generation, TWO_WEEK_DELAY);
        assert_eq!(
            model
                .plan_returned_liquidity(returned_liquidity_input(generation, 398, 1, 10))
                .unwrap(),
            ReturnedLiquidityPlan::RestoredWithoutTransfer { target: 390 }
        );
        assert!(model.neurons[1].reward_eligible_at(10));
        assert_eq!(model.neurons[1].committed_generation, None);
        assert!(!model.neurons[0].reward_eligible_at(u64::MAX));
        assert!(!model.neurons[2].reward_eligible_at(u64::MAX));
        assert_eq!(model.operations.split_intents, 1);

        model.observe_sns(2, CanonicalSnsState::Active, 11).unwrap();
        assert!(matches!(
            model.plan_returned_liquidity(returned_liquidity_input(generation, 500, 1, 12)),
            Ok(ReturnedLiquidityPlan::Restake { .. })
        ));
        model
            .observe_sns(2, CanonicalSnsState::Dissolving, 12)
            .unwrap();
        assert!(model.planned_restake.is_none());
        assert_eq!(model.operations.split_intents, 1);

        model
            .observe_sns(0, CanonicalSnsState::LiquidOrDissolved, 13)
            .unwrap();
        model
            .observe_sns(2, CanonicalSnsState::LiquidOrDissolved, 13)
            .unwrap();
        assert!(model
            .neurons
            .iter()
            .all(|member| member.committed_generation.is_none()));
        assert!(!model.neurons[0].reward_eligible_at(u64::MAX));
        assert!(!model.neurons[2].reward_eligible_at(u64::MAX));
        model.observe_sns(0, CanonicalSnsState::Active, 14).unwrap();
        assert_eq!(model.neurons[0].status, StickyStatus::ReentryPending);
        assert_eq!(model.neurons[0].committed_generation, None);
    }

    #[test]
    fn cohort_retirement_requires_return_maturity_cleanup_and_no_references() {
        let mut model = sticky_base(2, 1);
        let generation = commit_cohort(&mut model, &[0], 9_001, TWO_WEEK_DELAY);
        assert_eq!(
            model.submit_split_intent(&[1], 110),
            Err(ModelError::CohortCapacityExhausted)
        );
        return_cohort(&mut model, generation, TWO_WEEK_DELAY);
        assert_eq!(
            model.retire_cohort(generation),
            Err(ModelError::InvalidTransition),
            "returned but uncleaned remains live"
        );
        model.prove_child_maturity_handled(generation).unwrap();
        model.prove_child_cleanup_complete(generation).unwrap();
        assert_eq!(
            model.retire_cohort(generation),
            Err(ModelError::InvalidTransition),
            "a referenced cleaned cohort remains live"
        );
        model
            .observe_sns(1, CanonicalSnsState::Dissolving, 2)
            .unwrap();
        assert_eq!(
            model.submit_split_intent(&[1], 110),
            Err(ModelError::CohortCapacityExhausted)
        );
        model
            .observe_sns(0, CanonicalSnsState::LiquidOrDissolved, 2)
            .unwrap();
        let before = (model.backing, model.neurons.clone());
        let retired = model.retire_cohort(generation).unwrap();
        assert_eq!((retired.generation, retired.child_neuron_id), (0, 9_001));
        assert_eq!((model.backing, model.neurons.clone()), before);

        let next = commit_cohort(&mut model, &[1], 9_002, TWO_WEEK_DELAY + DAY);
        assert_eq!(next, generation + 1);
        assert_eq!(model.cohorts.len(), 1);
    }

    #[test]
    fn liquid_member_invalidates_uncommitted_restake_and_allows_retirement() {
        let mut model = sticky_base(1, 1);
        let generation = commit_cohort(&mut model, &[0], 9_101, TWO_WEEK_DELAY);
        model.observe_sns(0, CanonicalSnsState::Active, 2).unwrap();
        return_cohort(&mut model, generation, TWO_WEEK_DELAY);
        assert!(matches!(
            model.plan_returned_liquidity(returned_liquidity_input(generation, 500, 1, 4)),
            Ok(ReturnedLiquidityPlan::Restake {
                credited: 95,
                source_debit: 105,
                ..
            })
        ));
        model.prove_child_maturity_handled(generation).unwrap();
        model.prove_child_cleanup_complete(generation).unwrap();
        assert_eq!(
            model.retire_cohort(generation),
            Err(ModelError::InvalidTransition),
            "a planned restake and its member reference keep the cohort live"
        );

        let backing_before_invalidation = model.backing;
        model
            .observe_sns(0, CanonicalSnsState::LiquidOrDissolved, 3)
            .unwrap();
        assert!(model.planned_restake.is_none());
        assert_eq!(model.neurons[0].committed_generation, None);
        assert_eq!(model.backing, backing_before_invalidation);
        assert_eq!(model.backing.transit, 0);

        assert_eq!(
            model.retire_cohort(generation).unwrap().generation,
            generation
        );
        assert_eq!(
            model.commit_restake(generation),
            Err(ModelError::InvalidTransition)
        );
        assert_eq!(model.backing.transit, 0);
    }

    #[test]
    fn member_loss_invalidates_global_restake_snapshot_before_replanning_subset() {
        let mut model = sticky_base(2, 1);
        let generation = commit_cohort(&mut model, &[0, 1], 9_102, TWO_WEEK_DELAY);
        model.observe_sns(0, CanonicalSnsState::Active, 2).unwrap();
        model.observe_sns(1, CanonicalSnsState::Active, 2).unwrap();
        return_cohort(&mut model, generation, TWO_WEEK_DELAY);
        assert!(matches!(
            model.plan_returned_liquidity(returned_liquidity_input(generation, 500, 1, 4)),
            Ok(ReturnedLiquidityPlan::Restake {
                credited: 95,
                source_debit: 105,
                ..
            })
        ));

        model
            .observe_sns(0, CanonicalSnsState::LiquidOrDissolved, 3)
            .unwrap();
        assert!(model.planned_restake.is_none());
        assert_eq!(model.neurons[0].committed_generation, None);
        assert_eq!(model.neurons[1].status, StickyStatus::LiquidReturned);
        assert_eq!(model.neurons[1].committed_generation, Some(generation));
        assert_eq!(model.backing.transit, 0);

        assert_eq!(
            model
                .plan_returned_liquidity(returned_liquidity_input(generation, 450, 1, 5))
                .unwrap(),
            ReturnedLiquidityPlan::Restake {
                post_fee_target: 436,
                credited: 46,
                source_debit: 56,
            }
        );
    }

    #[test]
    fn aggregate_restake_commit_is_irreversible_and_counted_once() {
        let mut model = sticky_base(2, 2);
        let generation = commit_cohort(&mut model, &[0, 1], 10_001, TWO_WEEK_DELAY);
        model.observe_sns(0, CanonicalSnsState::Active, 2).unwrap();
        model.observe_sns(1, CanonicalSnsState::Active, 2).unwrap();
        return_cohort(&mut model, generation, TWO_WEEK_DELAY);
        assert!(matches!(
            model.plan_returned_liquidity(returned_liquidity_input(generation, 500, 1, 4)),
            Ok(ReturnedLiquidityPlan::Restake {
                credited: 95,
                source_debit: 105,
                ..
            })
        ));
        model.commit_restake(generation).unwrap();
        let committed_command = model.active_command;
        let backing_after_commit = model.backing;
        model
            .observe_sns(1, CanonicalSnsState::Dissolving, 3)
            .unwrap();
        assert_eq!(model.neurons[1].status, StickyStatus::RestakeCommitted);
        assert_eq!(model.neurons[1].committed_generation, Some(generation));
        assert_eq!(model.active_command, committed_command);
        assert_eq!(model.backing, backing_after_commit);
        assert!(!model.neurons[1].reward_eligible_at(u64::MAX));
        assert_eq!(
            model.prove_restake(generation, 94),
            Err(ModelError::ProofMismatch)
        );
        assert_eq!(model.backing.transit, 105);
        model.prove_restake(generation, 95).unwrap();
        assert_eq!(model.operations.restake_proofs, 1);
        assert_eq!(model.fees.restake, 10);
        assert_eq!((model.backing.pooled, model.backing.transit), (485, 0));
        assert_eq!(
            model.prove_restake(generation, 95),
            Err(ModelError::InvalidTransition)
        );
        model.finish_restake(generation, 4).unwrap();
        assert!(model.neurons[0].reward_eligible_at(4));
        assert_eq!(model.neurons[1].status, StickyStatus::ExitObserved);
        assert!(!model.neurons[1].reward_eligible_at(u64::MAX));
        let next = model.submit_split_intent(&[1], 110).unwrap();
        assert_eq!(next, generation + 1);
    }

    #[test]
    fn committed_restake_survives_liquid_observation_until_proof_and_finish() {
        let mut model = sticky_base(1, 1);
        let generation = commit_cohort(&mut model, &[0], 10_002, TWO_WEEK_DELAY);
        model.observe_sns(0, CanonicalSnsState::Active, 2).unwrap();
        return_cohort(&mut model, generation, TWO_WEEK_DELAY);
        model
            .plan_returned_liquidity(returned_liquidity_input(generation, 500, 1, 4))
            .unwrap();
        model.commit_restake(generation).unwrap();
        let committed_command = model.active_command;
        let backing_after_commit = model.backing;

        model
            .observe_sns(0, CanonicalSnsState::LiquidOrDissolved, 3)
            .unwrap();
        assert_eq!(
            model.neurons[0].latest_sns_state,
            CanonicalSnsState::LiquidOrDissolved
        );
        assert_eq!(model.neurons[0].status, StickyStatus::RestakeCommitted);
        assert_eq!(model.neurons[0].committed_generation, Some(generation));
        assert_eq!(model.active_command, committed_command);
        assert_eq!(model.backing, backing_after_commit);
        assert!(!model.neurons[0].reward_eligible_at(u64::MAX));

        model.prove_restake(generation, 95).unwrap();
        assert_eq!(model.neurons[0].status, StickyStatus::RestakeProved);
        assert_eq!(model.neurons[0].committed_generation, Some(generation));
        assert_eq!(model.operations.restake_proofs, 1);
        assert_eq!(model.fees.restake, 10);
        assert_eq!((model.backing.pooled, model.backing.transit), (485, 0));

        model.finish_restake(generation, 4).unwrap();
        assert_eq!(model.neurons[0].status, StickyStatus::ReentryPending);
        assert_eq!(model.neurons[0].committed_generation, None);
        assert!(!model.neurons[0].reward_eligible_at(u64::MAX));
    }

    #[test]
    fn proved_restake_survives_liquid_observation_until_finish() {
        let mut model = sticky_base(1, 1);
        let generation = commit_cohort(&mut model, &[0], 10_003, TWO_WEEK_DELAY);
        model.observe_sns(0, CanonicalSnsState::Active, 2).unwrap();
        return_cohort(&mut model, generation, TWO_WEEK_DELAY);
        model
            .plan_returned_liquidity(returned_liquidity_input(generation, 500, 1, 4))
            .unwrap();
        model.commit_restake(generation).unwrap();
        model.prove_restake(generation, 95).unwrap();
        let backing_after_proof = model.backing;

        model
            .observe_sns(0, CanonicalSnsState::LiquidOrDissolved, 3)
            .unwrap();
        assert_eq!(
            model.neurons[0].latest_sns_state,
            CanonicalSnsState::LiquidOrDissolved
        );
        assert_eq!(model.neurons[0].status, StickyStatus::RestakeProved);
        assert_eq!(model.neurons[0].committed_generation, Some(generation));
        assert_eq!(model.backing, backing_after_proof);
        assert_eq!(model.operations.restake_proofs, 1);
        assert_eq!(model.fees.restake, 10);
        assert!(!model.neurons[0].reward_eligible_at(u64::MAX));

        model.finish_restake(generation, 4).unwrap();
        assert_eq!(model.neurons[0].status, StickyStatus::ReentryPending);
        assert_eq!(model.neurons[0].committed_generation, None);
        assert_eq!(model.backing, backing_after_proof);
        assert_eq!(model.operations.restake_proofs, 1);
        assert_eq!(model.fees.restake, 10);
        assert!(!model.neurons[0].reward_eligible_at(u64::MAX));
    }

    #[test]
    fn reward_coverage_separates_structural_backing_from_reward_eligibility() {
        assert_eq!(
            require_reward_coverage(50, 0, 1_000, 1_000, 0),
            Ok(RewardCoverage {
                backing_target: 50,
                reward_target: 0,
            }),
            "below-minimum structural stake is backed but earns no rewards"
        );
        assert_eq!(
            require_reward_coverage(600, 500, 1_000, 1_000, 500),
            Ok(RewardCoverage {
                backing_target: 600,
                reward_target: 500,
            }),
            "pending re-entry is excluded while existing eligibility stays covered"
        );
        assert_eq!(
            require_reward_coverage(500, 500, 1_000, 1_000, 499),
            Err(ModelError::RewardBackingUnderTarget)
        );
        for pooled in [500, 501] {
            assert!(require_reward_coverage(500, 500, 1_000, 1_000, pooled).is_ok());
        }
        assert_eq!(
            require_reward_coverage(600, 600, 1_000, 1_000, 599),
            Err(ModelError::RewardBackingUnderTarget),
            "re-entry cannot join A_reward until its expanded target is covered"
        );
        assert!(require_reward_coverage(600, 600, 1_000, 1_000, 600).is_ok());
        assert_eq!(
            require_reward_coverage(500, 501, 1_000, 1_000, 501),
            Err(ModelError::RewardActiveExceedsBacking)
        );
    }

    #[test]
    fn reward_coverage_rejects_pooled_principal_above_total_backing() {
        assert_eq!(
            require_reward_coverage(500, 500, 1_000, 1_000, 1_001),
            Err(ModelError::InvalidBackingState)
        );
    }

    #[test]
    fn redemption_uses_strict_economic_state_validation() {
        let uncovered = Backing::default();
        for fee in [0, 10] {
            assert_eq!(
                redemption_readiness(100, uncovered, 1_000, fee),
                Err(ModelError::UncoveredClaims)
            );
        }
        assert_eq!(
            redemption_readiness(
                0,
                Backing {
                    liquid: 100,
                    ..Backing::default()
                },
                0,
                10,
            ),
            Ok(RedemptionReadiness::BackingWithoutClaims { backing: 100 })
        );
        assert_eq!(
            redemption_readiness(0, Backing::default(), 0, 0),
            Ok(RedemptionReadiness::EmptyGenesis)
        );
        assert_eq!(
            redemption_readiness(
                200,
                Backing {
                    liquid: 100,
                    pooled: 900,
                    ..Backing::default()
                },
                1_000,
                10,
            ),
            Ok(RedemptionReadiness::AwaitLiquidity { gross_quote: 200 })
        );
    }

    #[test]
    fn cadence_and_capacity_bounds_remain_formula_derived() {
        let unresolved = LiquidityLagInputs {
            maximum_detection_reconciliation_interval: None,
            nns_dissolve_delay: TWO_WEEK_DELAY,
            max_detection_margin: DAY,
            max_command_margin: DAY,
            max_disbursement_margin: DAY,
        };
        assert_eq!(liquidity_lag_bound(unresolved), Ok(None));
        let candidate = LiquidityLagInputs {
            maximum_detection_reconciliation_interval: Some(DAY),
            ..unresolved
        };
        assert_eq!(liquidity_lag_bound(candidate), Ok(Some(18 * DAY)));
        assert_eq!(cohort_capacity_bound(18 * DAY, DAY, 2), Ok(20));
        assert_eq!(cohort_capacity_bound(DAY + 1, DAY, 0), Ok(2));
        assert_eq!(
            cohort_capacity_bound(DAY, 0, 0),
            Err(ModelError::InvalidTransition)
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
    }
}
