use candid::{CandidType, Principal};
use serde::Deserialize;

use crate::{
    api::ApiError,
    redemption::CanonicalRedemptionSnapshot,
    state::{Account, StreamConfig},
};
use io_receipt_types::PrepareLiquidReceiptArgs;

pub fn request_fingerprint(args: &PrepareLiquidReceiptArgs) -> Vec<u8> {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(b"io-liquid-receipt-request-v1\0");
    hasher.update(candid::encode_one(args).expect("receipt request must encode"));
    hasher.finalize().to_vec()
}

pub fn receipt_memo(manager: Principal, sequence: u64) -> Vec<u8> {
    crate::transfer::deterministic_memo(b"io-liquid-receipt-v1", manager, sequence)
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct BackingSnapshot {
    pub total_io_supply_e8s: u128,
    pub reserve_io_e8s: u128,
    pub excluded_io_balances: Vec<(Account, u128)>,
    pub liquid_icp_e8s: u128,
    pub io_fee_e8s: u128,
    pub observed_at_nanos: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct ReceiptPreparation {
    pub request: PrepareLiquidReceiptArgs,
    pub request_fingerprint: Vec<u8>,
    pub authority: Principal,
    pub captured_control_epoch: u64,
    pub prepared_at_nanos: u64,
}

impl BackingSnapshot {
    pub(crate) fn validate(&self, config: &StreamConfig) -> Result<(), String> {
        if self.observed_at_nanos == 0
            || self.io_fee_e8s != config.expected_io_fee_e8s
            || self.excluded_io_balances.len() != config.nonredeemable_governance_io_accounts.len()
            || self
                .reserve_io_e8s
                .checked_add(
                    self.excluded_io_balances
                        .iter()
                        .try_fold(0u128, |sum, (_, balance)| sum.checked_add(*balance))
                        .ok_or("backing snapshot excluded balance overflow")?,
                )
                .is_none_or(|accounted| accounted > self.total_io_supply_e8s)
        {
            return Err("invalid pre-receipt backing snapshot".into());
        }
        for ((account, _), expected) in self
            .excluded_io_balances
            .iter()
            .zip(&config.nonredeemable_governance_io_accounts)
        {
            if !account.effective_eq(expected)? {
                return Err("backing snapshot excluded account mismatch".into());
            }
        }
        Ok(())
    }
}

impl ReceiptPreparation {
    pub fn validate(&self, config: &StreamConfig) -> Result<(), String> {
        if self.authority != config.nns_manager
            || self.authority == Principal::anonymous()
            || self.prepared_at_nanos == 0
            || self.request_fingerprint != request_fingerprint(&self.request)
            || self.request_fingerprint.len() != 32
            || self.request.source_operation_id.is_empty()
            || self.request.source_operation_id.len() > 64
            || self.request.liquid_amount_e8s == 0
        {
            return Err("invalid receipt preparation".into());
        }
        Ok(())
    }
}

pub(crate) fn validate_post_receipt_snapshot(
    before: &BackingSnapshot,
    after: &CanonicalRedemptionSnapshot,
    liquid_receipt_e8s: u128,
    allowed_reserve_debit_e8s: u128,
) -> Result<(), ApiError> {
    let required_liquid = before
        .liquid_icp_e8s
        .checked_add(liquid_receipt_e8s)
        .ok_or_else(|| ApiError::Invalid("post-receipt liquid requirement overflow".into()))?;
    let minimum_reserve = before
        .reserve_io_e8s
        .checked_sub(allowed_reserve_debit_e8s)
        .ok_or_else(|| ApiError::Invalid("protocol reserve debit exceeds snapshot".into()))?;
    if after.total_supply_e8s > before.total_io_supply_e8s
        || after.reserve_io_e8s < minimum_reserve
        || after.liquid_icp_e8s < required_liquid
        || after.io_fee_e8s != before.io_fee_e8s
        || after.excluded_io_balances.len() != before.excluded_io_balances.len()
    {
        return Err(ApiError::Invalid(
            "canonical post-receipt balances violate immutable backing snapshot".into(),
        ));
    }
    for ((before_account, before_balance), (after_account, after_balance)) in before
        .excluded_io_balances
        .iter()
        .zip(&after.excluded_io_balances)
    {
        if !before_account
            .effective_eq(after_account)
            .map_err(ApiError::Invalid)?
            || after_balance < before_balance
        {
            return Err(ApiError::Invalid(
                "excluded balance decreased after receipt preparation".into(),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn backing() -> BackingSnapshot {
        BackingSnapshot {
            total_io_supply_e8s: 1_000,
            reserve_io_e8s: 200,
            excluded_io_balances: vec![(
                Account {
                    owner: Principal::from_slice(&[7]),
                    subaccount: Some(vec![1; 32]),
                },
                100,
            )],
            liquid_icp_e8s: 700,
            io_fee_e8s: 10,
            observed_at_nanos: 1,
        }
    }

    fn after_snapshot() -> CanonicalRedemptionSnapshot {
        let before = backing();
        CanonicalRedemptionSnapshot {
            total_supply_e8s: before.total_io_supply_e8s,
            reserve_io_e8s: before.reserve_io_e8s,
            excluded_io_balances: before.excluded_io_balances,
            liquid_icp_e8s: before.liquid_icp_e8s + 70,
            io_fee_e8s: before.io_fee_e8s,
            icp_fee_e8s: 10,
            ..Default::default()
        }
    }

    #[test]
    fn immutable_pre_receipt_rate_ignores_later_liquid_donation() {
        let before = backing();
        let mut after = after_snapshot();
        after.liquid_icp_e8s += 500;
        after.reserve_io_e8s += 25;
        after.excluded_io_balances[0].1 += 10;
        after.total_supply_e8s -= 3;
        validate_post_receipt_snapshot(&before, &after, 70, 0).unwrap();

        let redeemable =
            before.total_io_supply_e8s - before.reserve_io_e8s - before.excluded_io_balances[0].1;
        assert_eq!(
            io_core_model::backed_io(70, before.liquid_icp_e8s, redeemable),
            Ok(70)
        );
    }

    #[test]
    fn post_receipt_snapshot_rejects_missing_receipt_or_unapproved_decrease() {
        let before = backing();
        let mut after = after_snapshot();
        after.liquid_icp_e8s -= 1;
        assert!(validate_post_receipt_snapshot(&before, &after, 70, 0).is_err());

        let mut after = after_snapshot();
        after.reserve_io_e8s -= 1;
        assert!(validate_post_receipt_snapshot(&before, &after, 70, 0).is_err());

        let mut after = after_snapshot();
        after.total_supply_e8s += 1;
        assert!(validate_post_receipt_snapshot(&before, &after, 70, 0).is_err());

        let mut after = after_snapshot();
        after.excluded_io_balances[0].1 -= 1;
        assert!(validate_post_receipt_snapshot(&before, &after, 70, 0).is_err());
    }
}
