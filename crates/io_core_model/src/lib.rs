//! Small, stateless launch economics for IO.

pub const E8S_PER_TOKEN: u128 = 100_000_000;
pub const FORTY_PERCENT_BPS: u128 = 4_000;
pub const SIXTY_PERCENT_BPS: u128 = 6_000;
pub const BPS_DENOMINATOR: u128 = 10_000;
pub const TWO_WEEK_SECONDS: u64 = 1_209_600;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Split {
    pub stake_e8s: u128,
    pub liquid_e8s: u128,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RedemptionQuote {
    pub redeemable_io_e8s: u128,
    pub gross_icp_e8s: u128,
    pub net_icp_e8s: u128,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EconomicsError {
    ArithmeticOverflow,
    ExclusionsExceedSupply,
    ZeroRedeemableSupply,
    RedemptionExceedsSupply,
    PayoutDoesNotCoverFee,
    ZeroBackingRate,
}

pub fn split_40_60(amount_e8s: u128) -> Result<Split, EconomicsError> {
    let stake_e8s = amount_e8s
        .checked_mul(FORTY_PERCENT_BPS)
        .ok_or(EconomicsError::ArithmeticOverflow)?
        / BPS_DENOMINATOR;
    let liquid_e8s = amount_e8s
        .checked_sub(stake_e8s)
        .ok_or(EconomicsError::ArithmeticOverflow)?;
    Ok(Split {
        stake_e8s,
        liquid_e8s,
    })
}

pub fn redemption_quote(
    redeemed_io_e8s: u128,
    io_transfer_fee_e8s: u128,
    total_io_supply_e8s: u128,
    reserve_io_e8s: u128,
    excluded_io_e8s: u128,
    liquid_icp_e8s: u128,
    icp_payout_fee_e8s: u128,
) -> Result<RedemptionQuote, EconomicsError> {
    let exclusions = reserve_io_e8s
        .checked_add(excluded_io_e8s)
        .ok_or(EconomicsError::ArithmeticOverflow)?;
    let redeemable_io_e8s = total_io_supply_e8s
        .checked_sub(exclusions)
        .ok_or(EconomicsError::ExclusionsExceedSupply)?;
    if redeemable_io_e8s == 0 {
        return Err(EconomicsError::ZeroRedeemableSupply);
    }
    let required = redeemed_io_e8s
        .checked_add(io_transfer_fee_e8s)
        .ok_or(EconomicsError::ArithmeticOverflow)?;
    if required > redeemable_io_e8s {
        return Err(EconomicsError::RedemptionExceedsSupply);
    }
    let gross_icp_e8s = redeemed_io_e8s
        .checked_mul(liquid_icp_e8s)
        .ok_or(EconomicsError::ArithmeticOverflow)?
        / redeemable_io_e8s;
    let net_icp_e8s = gross_icp_e8s
        .checked_sub(icp_payout_fee_e8s)
        .filter(|net| *net > 0)
        .ok_or(EconomicsError::PayoutDoesNotCoverFee)?;
    Ok(RedemptionQuote {
        redeemable_io_e8s,
        gross_icp_e8s,
        net_icp_e8s,
    })
}

pub fn backed_io(
    liquid_receipt_e8s: u128,
    liquid_icp_before_e8s: u128,
    redeemable_io_before_e8s: u128,
) -> Result<u128, EconomicsError> {
    if liquid_icp_before_e8s == 0 || redeemable_io_before_e8s == 0 {
        return Err(EconomicsError::ZeroBackingRate);
    }
    liquid_receipt_e8s
        .checked_mul(redeemable_io_before_e8s)
        .ok_or(EconomicsError::ArithmeticOverflow)
        .map(|value| value / liquid_icp_before_e8s)
}

pub fn two_week_target(active_staked_io_e8s: u128) -> u128 {
    active_staked_io_e8s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_is_exact_and_checked() {
        assert_eq!(
            split_40_60(101).unwrap(),
            Split {
                stake_e8s: 40,
                liquid_e8s: 61
            }
        );
        assert_eq!(
            split_40_60(u128::MAX),
            Err(EconomicsError::ArithmeticOverflow)
        );
    }

    #[test]
    fn quote_uses_pre_pull_supply_and_requires_fee_capacity() {
        let quote = redemption_quote(100, 2, 1_000, 400, 100, 1_000, 10).unwrap();
        assert_eq!(quote.redeemable_io_e8s, 500);
        assert_eq!(quote.gross_icp_e8s, 200);
        assert_eq!(quote.net_icp_e8s, 190);
        assert_eq!(
            redemption_quote(499, 2, 1_000, 400, 100, 1_000, 10),
            Err(EconomicsError::RedemptionExceedsSupply)
        );
    }

    #[test]
    fn backing_has_no_zero_rate_fallback() {
        assert_eq!(backed_io(10, 0, 10), Err(EconomicsError::ZeroBackingRate));
        assert_eq!(backed_io(100, 1_000, 500), Ok(50));
    }
}
