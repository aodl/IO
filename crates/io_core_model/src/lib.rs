//! Pure IO monetary-policy model.
//!
//! This crate intentionally has no IC/CDK dependencies. It is the canonical
//! place for solvency-preserving accounting rules.

pub const E8S_PER_TOKEN: u128 = 100_000_000;
pub const FORTY_PERCENT_BPS: u128 = 4_000;
pub const SIXTY_PERCENT_BPS: u128 = 6_000;
pub const BPS_DENOMINATOR: u128 = 10_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreamKind {
    /// ICP sent by Jupiter Faucet. Issues backed IO to Jupiter Faucet.
    JupiterFaucet,
    /// Maturity from IO's permanent 2-year NNS neuron. Issues no IO.
    TwoYearMaturity,
    /// Maturity from the pooled 2-week NNS neuron. Issues backed IO to eligible IO SNS neurons.
    TwoWeekMaturity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IoRecipientPolicy {
    JupiterFaucet,
    EligibleIoSnsNeurons,
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Split {
    pub stake_e8s: u128,
    pub liquid_e8s: u128,
}

pub fn split_40_60(amount_e8s: u128) -> Split {
    let stake_e8s = amount_e8s.saturating_mul(FORTY_PERCENT_BPS) / BPS_DENOMINATOR;
    Split {
        stake_e8s,
        liquid_e8s: amount_e8s.saturating_sub(stake_e8s),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProtocolState {
    pub liquid_icp_e8s: u128,
    pub two_year_staked_icp_e8s: u128,
    pub two_week_staked_icp_e8s: u128,
    pub total_io_supply_e8s: u128,
    pub protocol_reserve_io_e8s: u128,
    pub non_redeemable_governance_io_e8s: u128,
}

impl ProtocolState {
    pub fn new(
        total_io_supply_e8s: u128,
        protocol_reserve_io_e8s: u128,
        non_redeemable_governance_io_e8s: u128,
    ) -> Self {
        Self {
            liquid_icp_e8s: 0,
            two_year_staked_icp_e8s: 0,
            two_week_staked_icp_e8s: 0,
            total_io_supply_e8s,
            protocol_reserve_io_e8s,
            non_redeemable_governance_io_e8s,
        }
    }

    pub fn redeemable_io_supply_e8s(&self) -> Result<u128, ModelError> {
        let excluded = self
            .protocol_reserve_io_e8s
            .checked_add(self.non_redeemable_governance_io_e8s)
            .ok_or(ModelError::ArithmeticOverflow)?;
        self.total_io_supply_e8s
            .checked_sub(excluded)
            .ok_or(ModelError::ExcludedSupplyExceedsTotal)
    }

    pub fn redemption_rate(&self) -> Result<RedemptionRate, ModelError> {
        let supply = self.redeemable_io_supply_e8s()?;
        if supply == 0 || self.liquid_icp_e8s == 0 {
            Ok(RedemptionRate::one_to_one())
        } else {
            Ok(RedemptionRate::new(self.liquid_icp_e8s, supply))
        }
    }

    pub fn ensure_can_issue_from_reserve(&self, io_e8s: u128) -> Result<(), ModelError> {
        if io_e8s > self.protocol_reserve_io_e8s {
            return Err(ModelError::InsufficientProtocolReserve {
                requested_e8s: io_e8s,
                available_e8s: self.protocol_reserve_io_e8s,
            });
        }
        Ok(())
    }

    pub fn issue_io_from_reserve(&mut self, io_e8s: u128) -> Result<(), ModelError> {
        self.ensure_can_issue_from_reserve(io_e8s)?;
        self.protocol_reserve_io_e8s -= io_e8s;
        Ok(())
    }

    pub fn return_io_to_reserve(&mut self, io_e8s: u128) -> Result<(), ModelError> {
        let new_reserve = self
            .protocol_reserve_io_e8s
            .checked_add(io_e8s)
            .ok_or(ModelError::ArithmeticOverflow)?;
        if new_reserve
            .checked_add(self.non_redeemable_governance_io_e8s)
            .ok_or(ModelError::ArithmeticOverflow)?
            > self.total_io_supply_e8s
        {
            return Err(ModelError::ExcludedSupplyExceedsTotal);
        }
        self.protocol_reserve_io_e8s = new_reserve;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RedemptionRate {
    pub liquid_icp_e8s: u128,
    pub redeemable_io_e8s: u128,
}

impl RedemptionRate {
    pub fn new(liquid_icp_e8s: u128, redeemable_io_e8s: u128) -> Self {
        let divisor = gcd(liquid_icp_e8s, redeemable_io_e8s);
        Self {
            liquid_icp_e8s: liquid_icp_e8s / divisor,
            redeemable_io_e8s: redeemable_io_e8s / divisor,
        }
    }

    pub fn one_to_one() -> Self {
        Self {
            liquid_icp_e8s: E8S_PER_TOKEN,
            redeemable_io_e8s: E8S_PER_TOKEN,
        }
    }

    pub fn io_for_liquid_backing(self, liquid_icp_e8s: u128) -> Result<u128, ModelError> {
        liquid_icp_e8s
            .checked_mul(self.redeemable_io_e8s)
            .ok_or(ModelError::ArithmeticOverflow)?
            .checked_div(self.liquid_icp_e8s)
            .ok_or(ModelError::DivisionByZero)
    }

    pub fn icp_for_io(self, io_e8s: u128) -> Result<u128, ModelError> {
        io_e8s
            .checked_mul(self.liquid_icp_e8s)
            .ok_or(ModelError::ArithmeticOverflow)?
            .checked_div(self.redeemable_io_e8s)
            .ok_or(ModelError::DivisionByZero)
    }
}

fn gcd(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        let remainder = a % b;
        a = b;
        b = remainder;
    }
    a.max(1)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StreamOutcome {
    pub kind: StreamKind,
    pub split: Split,
    pub recipient_policy: IoRecipientPolicy,
    pub io_issued_e8s: u128,
    pub rate_before: RedemptionRate,
    pub rate_after: RedemptionRate,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RedemptionOutcome {
    pub io_redeemed_e8s: u128,
    pub icp_paid_e8s: u128,
    pub rate_before: RedemptionRate,
    pub rate_after: RedemptionRate,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModelError {
    ArithmeticOverflow,
    DivisionByZero,
    ExcludedSupplyExceedsTotal,
    InsufficientProtocolReserve {
        requested_e8s: u128,
        available_e8s: u128,
    },
    InsufficientLiquidReserve {
        requested_e8s: u128,
        available_e8s: u128,
    },
    InvalidBasisPoints {
        bps: u128,
    },
}

pub fn stream_policy(kind: StreamKind) -> (IoRecipientPolicy, bool) {
    match kind {
        StreamKind::JupiterFaucet => (IoRecipientPolicy::JupiterFaucet, false),
        StreamKind::TwoYearMaturity => (IoRecipientPolicy::None, false),
        StreamKind::TwoWeekMaturity => (IoRecipientPolicy::EligibleIoSnsNeurons, true),
    }
}

pub fn process_stream(
    state: &mut ProtocolState,
    kind: StreamKind,
    amount_e8s: u128,
) -> Result<StreamOutcome, ModelError> {
    let rate_before = state.redemption_rate()?;
    let split = split_40_60(amount_e8s);
    let (recipient_policy, stake_target_is_two_week) = stream_policy(kind);

    let io_issued_e8s = match recipient_policy {
        IoRecipientPolicy::JupiterFaucet | IoRecipientPolicy::EligibleIoSnsNeurons => {
            rate_before.io_for_liquid_backing(split.liquid_e8s)?
        }
        IoRecipientPolicy::None => 0,
    };

    // Pre-flight reserve checks before any state mutation. This preserves atomicity
    // in the pure model and mirrors how the canister should guard value-moving work.
    state.ensure_can_issue_from_reserve(io_issued_e8s)?;

    if stake_target_is_two_week {
        state.two_week_staked_icp_e8s = state
            .two_week_staked_icp_e8s
            .checked_add(split.stake_e8s)
            .ok_or(ModelError::ArithmeticOverflow)?;
    } else {
        state.two_year_staked_icp_e8s = state
            .two_year_staked_icp_e8s
            .checked_add(split.stake_e8s)
            .ok_or(ModelError::ArithmeticOverflow)?;
    }
    state.liquid_icp_e8s = state
        .liquid_icp_e8s
        .checked_add(split.liquid_e8s)
        .ok_or(ModelError::ArithmeticOverflow)?;
    state.issue_io_from_reserve(io_issued_e8s)?;

    let rate_after = state.redemption_rate()?;
    Ok(StreamOutcome {
        kind,
        split,
        recipient_policy,
        io_issued_e8s,
        rate_before,
        rate_after,
    })
}

pub fn redeem_io(state: &mut ProtocolState, io_e8s: u128) -> Result<RedemptionOutcome, ModelError> {
    let rate_before = state.redemption_rate()?;
    let icp_paid_e8s = rate_before.icp_for_io(io_e8s)?;
    if icp_paid_e8s > state.liquid_icp_e8s {
        return Err(ModelError::InsufficientLiquidReserve {
            requested_e8s: icp_paid_e8s,
            available_e8s: state.liquid_icp_e8s,
        });
    }

    // Pre-flight the supply transition before mutating the liquid reserve.
    let mut post_state = *state;
    post_state.liquid_icp_e8s -= icp_paid_e8s;
    post_state.return_io_to_reserve(io_e8s)?;
    *state = post_state;

    let rate_after = state.redemption_rate()?;
    Ok(RedemptionOutcome {
        io_redeemed_e8s: io_e8s,
        icp_paid_e8s,
        rate_before,
        rate_after,
    })
}

pub fn target_two_week_pool_e8s(
    active_staked_io_e8s: u128,
    rate: RedemptionRate,
    backing_bps: u128,
) -> Result<u128, ModelError> {
    if backing_bps > BPS_DENOMINATOR {
        return Err(ModelError::InvalidBasisPoints { bps: backing_bps });
    }
    let full_claim = rate.icp_for_io(active_staked_io_e8s)?;
    full_claim
        .checked_mul(backing_bps)
        .ok_or(ModelError::ArithmeticOverflow)
        .map(|v| v / BPS_DENOMINATOR)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn t(n: u128) -> u128 {
        n * E8S_PER_TOKEN
    }

    fn state() -> ProtocolState {
        ProtocolState::new(t(1_000_000), t(900_000), t(100_000))
    }

    #[test]
    fn split_rounding_preserves_total_for_small_amounts() {
        for amount in 0..1000u128 {
            let split = split_40_60(amount);
            assert_eq!(split.stake_e8s + split.liquid_e8s, amount);
            assert!(split.stake_e8s <= amount);
            assert!(split.liquid_e8s <= amount);
        }
    }

    #[test]
    fn genesis_faucet_deposit_issues_sixty_io_for_hundred_icp() {
        let mut s = state();
        let out = process_stream(&mut s, StreamKind::JupiterFaucet, t(100)).unwrap();
        assert_eq!(out.split.stake_e8s, t(40));
        assert_eq!(out.split.liquid_e8s, t(60));
        assert_eq!(out.io_issued_e8s, t(60));
        assert_eq!(s.liquid_icp_e8s, t(60));
        assert_eq!(s.two_year_staked_icp_e8s, t(40));
        assert_eq!(s.redeemable_io_supply_e8s().unwrap(), t(60));
        assert_eq!(s.redemption_rate().unwrap().icp_for_io(t(1)).unwrap(), t(1));
    }

    #[test]
    fn two_year_maturity_issues_no_io_and_increases_rate() {
        let mut s = state();
        process_stream(&mut s, StreamKind::JupiterFaucet, t(100)).unwrap();
        let out = process_stream(&mut s, StreamKind::TwoYearMaturity, t(100)).unwrap();
        assert_eq!(out.io_issued_e8s, 0);
        assert_eq!(s.liquid_icp_e8s, t(120));
        assert_eq!(s.two_year_staked_icp_e8s, t(80));
        assert_eq!(s.redeemable_io_supply_e8s().unwrap(), t(60));
        assert_eq!(s.redemption_rate().unwrap().icp_for_io(t(1)).unwrap(), t(2));
    }

    #[test]
    fn two_week_maturity_issues_backed_io_to_eligible_stakers() {
        let mut s = state();
        process_stream(&mut s, StreamKind::JupiterFaucet, t(100)).unwrap();
        process_stream(&mut s, StreamKind::TwoYearMaturity, t(100)).unwrap(); // rate = 2
        let out = process_stream(&mut s, StreamKind::TwoWeekMaturity, t(100)).unwrap();
        assert_eq!(out.split.liquid_e8s, t(60));
        assert_eq!(out.io_issued_e8s, t(30));
        assert_eq!(s.two_week_staked_icp_e8s, t(40));
        assert_eq!(s.liquid_icp_e8s, t(180));
        assert_eq!(s.redeemable_io_supply_e8s().unwrap(), t(90));
        assert_eq!(s.redemption_rate().unwrap().icp_for_io(t(1)).unwrap(), t(2));
    }

    #[test]
    fn redemption_keeps_rate_constant_when_no_rounding() {
        let mut s = state();
        process_stream(&mut s, StreamKind::JupiterFaucet, t(100)).unwrap();
        process_stream(&mut s, StreamKind::TwoYearMaturity, t(100)).unwrap(); // rate = 2
        let out = redeem_io(&mut s, t(10)).unwrap();
        assert_eq!(out.icp_paid_e8s, t(20));
        assert_eq!(s.liquid_icp_e8s, t(100));
        assert_eq!(s.redeemable_io_supply_e8s().unwrap(), t(50));
        assert_eq!(s.redemption_rate().unwrap().icp_for_io(t(1)).unwrap(), t(2));
    }

    #[test]
    fn target_two_week_pool_tracks_active_staked_io_claim() {
        let mut s = state();
        process_stream(&mut s, StreamKind::JupiterFaucet, t(100)).unwrap();
        let target = target_two_week_pool_e8s(t(30), s.redemption_rate().unwrap(), 10_000).unwrap();
        assert_eq!(target, t(30));
        let half_target =
            target_two_week_pool_e8s(t(30), s.redemption_rate().unwrap(), 5_000).unwrap();
        assert_eq!(half_target, t(15));
    }

    #[test]
    fn non_redeemable_and_protocol_reserve_supply_are_excluded() {
        let s = ProtocolState::new(t(1_000), t(700), t(100));
        assert_eq!(s.redeemable_io_supply_e8s().unwrap(), t(200));
    }

    #[test]
    fn excluded_supply_cannot_exceed_total() {
        let s = ProtocolState::new(t(100), t(80), t(30));
        assert_eq!(
            s.redeemable_io_supply_e8s(),
            Err(ModelError::ExcludedSupplyExceedsTotal)
        );
    }

    #[test]
    fn stream_failure_from_insufficient_io_reserve_is_atomic() {
        let mut s = ProtocolState::new(t(100), t(10), t(0));
        let before = s;
        let err = process_stream(&mut s, StreamKind::JupiterFaucet, t(100)).unwrap_err();
        assert_eq!(
            err,
            ModelError::InsufficientProtocolReserve {
                requested_e8s: t(60),
                available_e8s: t(10)
            }
        );
        assert_eq!(s, before);
    }

    #[test]
    fn redemption_failure_from_insufficient_liquid_reserve_is_atomic() {
        let mut s = state();
        process_stream(&mut s, StreamKind::JupiterFaucet, t(100)).unwrap();
        let before = s;
        let err = redeem_io(&mut s, t(100)).unwrap_err();
        assert_eq!(
            err,
            ModelError::InsufficientLiquidReserve {
                requested_e8s: t(100),
                available_e8s: t(60)
            }
        );
        assert_eq!(s, before);
    }

    #[test]
    fn two_year_yield_makes_later_faucet_entrants_receive_less_io() {
        let mut s = state();
        process_stream(&mut s, StreamKind::JupiterFaucet, t(100)).unwrap();
        process_stream(&mut s, StreamKind::TwoYearMaturity, t(100)).unwrap(); // rate = 2
        let out = process_stream(&mut s, StreamKind::JupiterFaucet, t(100)).unwrap();
        assert_eq!(out.io_issued_e8s, t(30));
        assert_eq!(s.redeemable_io_supply_e8s().unwrap(), t(90));
        assert_eq!(s.liquid_icp_e8s, t(180));
        assert_eq!(s.redemption_rate().unwrap().icp_for_io(t(1)).unwrap(), t(2));
    }
}

#[cfg(test)]
mod additional_edge_case_tests {
    use super::*;
    fn t(n: u128) -> u128 {
        n * E8S_PER_TOKEN
    }

    fn base_state() -> ProtocolState {
        ProtocolState::new(t(1_000_000), t(900_000), t(100_000))
    }

    #[test]
    fn stream_policies_match_protocol_design() {
        assert_eq!(
            stream_policy(StreamKind::JupiterFaucet),
            (IoRecipientPolicy::JupiterFaucet, false)
        );
        assert_eq!(
            stream_policy(StreamKind::TwoYearMaturity),
            (IoRecipientPolicy::None, false)
        );
        assert_eq!(
            stream_policy(StreamKind::TwoWeekMaturity),
            (IoRecipientPolicy::EligibleIoSnsNeurons, true)
        );
    }

    #[test]
    fn zero_value_stream_is_noop_but_still_well_defined() {
        let mut s = base_state();
        let before = s;
        let out = process_stream(&mut s, StreamKind::JupiterFaucet, 0).unwrap();
        assert_eq!(
            out.split,
            Split {
                stake_e8s: 0,
                liquid_e8s: 0
            }
        );
        assert_eq!(out.io_issued_e8s, 0);
        assert_eq!(s, before);
    }

    #[test]
    fn tiny_stream_amounts_never_lose_or_create_e8s() {
        for amount in 1..10_000u128 {
            let split = split_40_60(amount);
            assert_eq!(split.stake_e8s + split.liquid_e8s, amount);
            // For amounts not divisible by 10_000, rounding dust stays liquid rather than disappearing.
            assert_eq!(split.liquid_e8s, amount - split.stake_e8s);
        }
    }

    #[test]
    fn two_week_maturity_preserves_rate_even_after_two_year_yield_changed_it() {
        let mut s = base_state();
        process_stream(&mut s, StreamKind::JupiterFaucet, t(100)).unwrap();
        process_stream(&mut s, StreamKind::TwoYearMaturity, t(300)).unwrap();
        let rate_before = s.redemption_rate().unwrap();
        let out = process_stream(&mut s, StreamKind::TwoWeekMaturity, t(100)).unwrap();
        let rate_after = s.redemption_rate().unwrap();
        assert!(out.io_issued_e8s > 0);
        assert_eq!(rate_before, rate_after);
    }

    #[test]
    fn two_year_maturity_with_zero_redeemable_supply_does_not_create_redeemable_io() {
        let mut s = ProtocolState::new(t(1_000), t(900), t(100));
        let out = process_stream(&mut s, StreamKind::TwoYearMaturity, t(100)).unwrap();
        assert_eq!(out.io_issued_e8s, 0);
        assert_eq!(s.redeemable_io_supply_e8s().unwrap(), 0);
        assert_eq!(s.liquid_icp_e8s, t(60));
    }

    #[test]
    fn faucet_after_two_year_yield_uses_pre_deposit_rate_not_post_deposit_rate() {
        let mut s = base_state();
        process_stream(&mut s, StreamKind::JupiterFaucet, t(100)).unwrap();
        process_stream(&mut s, StreamKind::TwoYearMaturity, t(100)).unwrap();
        let rate_before = s.redemption_rate().unwrap();
        let out = process_stream(&mut s, StreamKind::JupiterFaucet, t(60)).unwrap();
        assert_eq!(out.rate_before, rate_before);
        assert_eq!(
            out.io_issued_e8s,
            rate_before.io_for_liquid_backing(t(36)).unwrap()
        );
        assert_eq!(out.io_issued_e8s, t(18));
    }

    #[test]
    fn returning_too_much_io_to_reserve_is_rejected_atomically() {
        let mut s = ProtocolState::new(t(100), t(90), t(10));
        let before = s;
        assert_eq!(
            s.return_io_to_reserve(1),
            Err(ModelError::ExcludedSupplyExceedsTotal)
        );
        assert_eq!(s, before);
    }

    #[test]
    fn redemption_of_zero_io_is_noop() {
        let mut s = base_state();
        process_stream(&mut s, StreamKind::JupiterFaucet, t(100)).unwrap();
        let before = s;
        let out = redeem_io(&mut s, 0).unwrap();
        assert_eq!(out.io_redeemed_e8s, 0);
        assert_eq!(out.icp_paid_e8s, 0);
        assert_eq!(s, before);
    }

    #[test]
    fn redemption_rounding_never_pays_more_than_liquid_reserve() {
        let mut s = ProtocolState::new(1_000, 0, 0);
        s.liquid_icp_e8s = 1;
        let out = redeem_io(&mut s, 999).unwrap();
        assert_eq!(out.icp_paid_e8s, 0);
        assert_eq!(s.liquid_icp_e8s, 1);
        assert_eq!(s.protocol_reserve_io_e8s, 999);
    }

    #[test]
    fn target_two_week_pool_supports_zero_backing_fraction() {
        let mut s = base_state();
        process_stream(&mut s, StreamKind::JupiterFaucet, t(100)).unwrap();
        assert_eq!(
            target_two_week_pool_e8s(t(100), s.redemption_rate().unwrap(), 0).unwrap(),
            0
        );
    }

    #[test]
    fn target_two_week_pool_rejects_backing_fraction_above_100_percent() {
        let mut s = base_state();
        process_stream(&mut s, StreamKind::JupiterFaucet, t(100)).unwrap();
        assert_eq!(
            target_two_week_pool_e8s(t(10), s.redemption_rate().unwrap(), 10_001),
            Err(ModelError::InvalidBasisPoints { bps: 10_001 })
        );
    }

    #[test]
    fn target_two_week_pool_overflow_is_reported() {
        let rate = RedemptionRate {
            liquid_icp_e8s: u128::MAX,
            redeemable_io_e8s: 1,
        };
        assert_eq!(
            target_two_week_pool_e8s(2, rate, 10_000),
            Err(ModelError::ArithmeticOverflow)
        );
    }

    #[test]
    fn overflow_during_stream_calculation_is_atomic() {
        let mut s = ProtocolState::new(u128::MAX, u128::MAX - 10, 0);
        s.liquid_icp_e8s = 1;
        let before = s;
        let err = process_stream(&mut s, StreamKind::JupiterFaucet, u128::MAX).unwrap_err();
        assert!(matches!(
            err,
            ModelError::ArithmeticOverflow | ModelError::InsufficientProtocolReserve { .. }
        ));
        assert_eq!(s, before);
    }

    #[test]
    fn rate_defaults_to_one_to_one_when_liquid_exists_but_no_redeemable_supply() {
        let mut s = ProtocolState::new(t(100), t(90), t(10));
        s.liquid_icp_e8s = t(1_000);
        assert_eq!(s.redeemable_io_supply_e8s().unwrap(), 0);
        assert_eq!(s.redemption_rate().unwrap(), RedemptionRate::one_to_one());
    }
}
