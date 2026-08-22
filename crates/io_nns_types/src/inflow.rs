use candid::CandidType;
use io_accounts::Account;
use io_reward_policy::{ClaimRoutePlan, TwoWeekSettlementPlan};
use serde::Deserialize;
use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum BackingInflowKind {
    PermanentMaturity,
    PooledMaturity,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct PrepareBackingInflowArgs {
    pub kind: BackingInflowKind,
    pub source_operation_id: Vec<u8>,
    pub actual_mint_e8s: u128,
    pub maturity_generation: u64,
    pub staging_account: Account,
    pub mint_block: u128,
    pub permanent_transfer_fee_e8s: u128,
    pub claim_transfer_fee_e8s: u128,
    pub nns_fingerprint: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct FrozenRewardRecipient {
    pub sns_neuron_id: Vec<u8>,
    pub destination: Account,
    pub io_e8s: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum FrozenInflowEconomics {
    Permanent {
        route: ClaimRoutePlan,
    },
    Pooled {
        settlement: Box<TwoWeekSettlementPlan>,
        recipients: Vec<FrozenRewardRecipient>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct BackingInflowPermit {
    pub stream_operation_sequence: u64,
    pub source_operation_id: Vec<u8>,
    pub actual_mint_e8s: u128,
    pub maturity_generation: u64,
    pub staging_account: Account,
    pub mint_block: u128,
    pub permanent_destination: Account,
    pub pool_destination: Account,
    pub expected_parent_before_e8s: u128,
    pub liquid_destination: Account,
    pub permanent_transfer_fee_e8s: u128,
    pub claim_transfer_fee_e8s: u128,
    pub economics: FrozenInflowEconomics,
    pub nns_fingerprint: Vec<u8>,
    pub snapshot_fingerprint: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum BackingEffect {
    PermanentCredit,
    FirstClaimCredit,
    PooledCredit,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct ProveBackingEffectArgs {
    pub stream_operation_sequence: u64,
    pub effect: BackingEffect,
    pub block_index: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum BackingInflowProgress {
    AwaitingNnsEffects(Box<BackingInflowPermit>),
    AwaitingPooledTransfer,
    AwaitingPooledProof {
        block_index: u128,
    },
    SettlingRewards,
    Completed {
        source_operation_id: Vec<u8>,
        distributed_io_e8s: u128,
    },
    Stuck(String),
}

impl BackingInflowPermit {
    pub fn route(&self) -> ClaimRoutePlan {
        match &self.economics {
            FrozenInflowEconomics::Permanent { route } => *route,
            FrozenInflowEconomics::Pooled { settlement, .. } => settlement.route,
        }
    }

    pub fn permanent_credit(&self) -> u128 {
        match &self.economics {
            FrozenInflowEconomics::Permanent { .. } => 0,
            FrozenInflowEconomics::Pooled { settlement, .. } => settlement.permanent_credit,
        }
    }

    pub fn first_claim_credit(&self) -> Option<u128> {
        let route = self.route();
        match route.route {
            io_reward_policy::ClaimRoute::Mixed => {
                route.claim_credit.checked_add(self.claim_transfer_fee_e8s)
            }
            _ => Some(route.claim_credit),
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        self.staging_account.validate()?;
        self.permanent_destination.validate()?;
        self.pool_destination.validate()?;
        self.liquid_destination.validate()?;
        let route = self.route();
        if self.source_operation_id.is_empty()
            || self.source_operation_id.len() > 64
            || self.actual_mint_e8s == 0
            || self.maturity_generation == 0
            || self.permanent_transfer_fee_e8s == 0
            || self.claim_transfer_fee_e8s == 0
            || self.snapshot_fingerprint.len() != 32
            || self.nns_fingerprint.len() != 32
            || route.claim_credit == 0
            || route.fee_count == 0
            || route.fee_count > 2
            || route.claim_credit
                != route
                    .liquid_credit
                    .checked_add(route.pooled_credit)
                    .ok_or("claim route credit overflow")?
            || match route.route {
                io_reward_policy::ClaimRoute::AllLiquid => {
                    route.fee_count != 1 || route.liquid_credit == 0 || route.pooled_credit != 0
                }
                io_reward_policy::ClaimRoute::AllPool => {
                    route.fee_count != 1 || route.liquid_credit != 0 || route.pooled_credit == 0
                }
                io_reward_policy::ClaimRoute::Mixed => {
                    route.fee_count != 2
                        || route.liquid_credit == 0
                        || route.pooled_credit <= self.claim_transfer_fee_e8s
                }
            }
        {
            return Err("backing-inflow permit is malformed".into());
        }
        let fees = self
            .claim_transfer_fee_e8s
            .checked_mul(u128::from(route.fee_count))
            .ok_or("claim fee total overflow")?;
        match &self.economics {
            FrozenInflowEconomics::Permanent { .. } => {
                if self.permanent_destination == self.pool_destination
                    || self
                        .actual_mint_e8s
                        .checked_sub(fees)
                        .is_none_or(|credit| credit != route.claim_credit)
                {
                    return Err("permanent maturity route does not conserve its Mint".into());
                }
            }
            FrozenInflowEconomics::Pooled {
                settlement,
                recipients,
            } => {
                let split = io_core_model::split_40_60(self.actual_mint_e8s)
                    .map_err(|error| format!("pooled maturity split failed: {error:?}"))?;
                let distributed = recipients.iter().try_fold(0u128, |sum, recipient| {
                    recipient.destination.validate()?;
                    sum.checked_add(recipient.io_e8s)
                        .ok_or_else(|| "reward recipient total overflow".to_string())
                })?;
                let allocations_match = recipients
                    .iter()
                    .map(|recipient| (&recipient.sns_neuron_id, recipient.io_e8s))
                    .eq(settlement
                        .rewards
                        .allocations
                        .iter()
                        .map(|allocation| (&allocation.sns_neuron_id, allocation.io_e8s)));
                if split.permanent.checked_sub(self.permanent_transfer_fee_e8s)
                    != Some(settlement.permanent_credit)
                    || split.claim.checked_sub(fees) != Some(route.claim_credit)
                    || settlement.route != route
                    || settlement.distributed_io != distributed
                    || !allocations_match
                {
                    return Err("pooled maturity settlement is internally inconsistent".into());
                }
            }
        }
        Ok(())
    }
}

pub fn effect_memo(source_operation_id: &[u8], effect: BackingEffect) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(b"io-backing-inflow-v1");
    hasher.update(source_operation_id);
    hasher.update([match effect {
        BackingEffect::PermanentCredit => 1,
        BackingEffect::FirstClaimCredit => 2,
        BackingEffect::PooledCredit => 3,
    }]);
    hasher.finalize().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use candid::Principal;
    use io_core_model::{Backing, EconomicState};
    use io_reward_policy::{plan_two_week_settlement, ClaimRoute, TwoWeekSettlementInput};

    fn account(byte: u8) -> Account {
        Account {
            owner: Principal::from_slice(&[byte; 29]),
            subaccount: Some(vec![byte; 32]),
        }
    }

    #[test]
    fn pooled_one_fee_permit_freezes_the_all_pool_regression() {
        let settlement = plan_two_week_settlement(TwoWeekSettlementInput {
            state: EconomicState {
                backing: Backing {
                    liquid: 50_029_990_000,
                    pooled: 49_970_010_000,
                    unwinding: 0,
                    transit: 0,
                },
                claims: 100_000_000_000,
                active_backing: 50_000_000_000,
                active_reward: 50_000_000_000,
            },
            actual_mint: 100_000_000,
            permanent_transfer_fee: 10_000,
            claim_transfer_fee: 10_000,
            parent_exists: true,
            minimum_parent_credit: 100_000_000,
            policy_credit_total: 1,
            entitlements: &[],
            reward_eligible_ids: &[],
            reserve_io_capacity: 1_000_000_000,
            io_fee: 10_000,
            snapshot_fingerprint: [9; 32],
        })
        .unwrap();
        assert_eq!(settlement.route.route, ClaimRoute::AllPool);
        assert_eq!(settlement.route.fee_count, 1);
        assert_eq!(settlement.route.pooled_credit, 59_990_000);
        assert_eq!(settlement.route.over_target, 5_000);
        assert_eq!(settlement.route.claim_credit - 59_980_000, 10_000);

        let permit = BackingInflowPermit {
            stream_operation_sequence: 7,
            source_operation_id: b"pooled-7".to_vec(),
            actual_mint_e8s: 100_000_000,
            maturity_generation: 7,
            staging_account: account(1),
            mint_block: 11,
            permanent_destination: account(2),
            pool_destination: account(3),
            expected_parent_before_e8s: 49_970_010_000,
            liquid_destination: account(4),
            permanent_transfer_fee_e8s: 10_000,
            claim_transfer_fee_e8s: 10_000,
            economics: FrozenInflowEconomics::Pooled {
                settlement: Box::new(settlement),
                recipients: vec![],
            },
            nns_fingerprint: vec![8; 32],
            snapshot_fingerprint: vec![9; 32],
        };
        assert_eq!(permit.validate(), Ok(()));
        assert_eq!(permit.first_claim_credit(), Some(59_990_000));
        let mut tampered = permit;
        tampered.claim_transfer_fee_e8s += 1;
        assert!(tampered.validate().is_err());
    }

    #[test]
    fn effect_memos_are_source_and_phase_specific() {
        let source = b"operation-1";
        let permanent = effect_memo(source, BackingEffect::PermanentCredit);
        assert_eq!(permanent.len(), 32);
        assert_ne!(
            permanent,
            effect_memo(source, BackingEffect::FirstClaimCredit)
        );
        assert_ne!(
            permanent,
            effect_memo(b"operation-2", BackingEffect::PermanentCredit)
        );
    }
}
