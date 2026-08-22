//! Checked, stateless launch economics for IO claim backing.

pub const E8S_PER_TOKEN: u128 = 100_000_000;
pub const TWO_WEEK_SECONDS: u64 = 1_209_600;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Backing {
    pub liquid: u128,
    pub pooled: u128,
    pub unwinding: u128,
    pub transit: u128,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EconomicState {
    pub backing: Backing,
    pub claims: u128,
    pub active_backing: u128,
    pub active_reward: u128,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClaimRate {
    EmptyGenesis,
    BackingWithoutClaims,
    Ratio { backing: u128, claims: u128 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RedemptionQuote {
    pub gross_icp: u128,
    pub net_icp: u128,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LiquidityShortfall {
    pub gross_icp: u128,
    pub net_icp: u128,
    pub available_liquid: u128,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Split {
    pub permanent: u128,
    pub claim: u128,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReconcilePlan {
    Hold {
        target: u128,
    },
    TopUp {
        target: u128,
        credit: u128,
        debit: u128,
    },
    Unwind {
        target: u128,
        gross: u128,
        expected_credit: u128,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EconomicsError {
    ArithmeticOverflow,
    ExclusionsExceedSupply,
    BackingWithoutClaims,
    UncoveredClaims,
    ActiveExceedsClaims,
    RewardActiveExceedsBacking,
    RedemptionExceedsSupply,
    PayoutDoesNotCoverFee,
    InsufficientBacking,
    InsufficientLiquidity(LiquidityShortfall),
    RewardBackingUnderTarget,
}

pub fn checked_add(a: u128, b: u128) -> Result<u128, EconomicsError> {
    a.checked_add(b).ok_or(EconomicsError::ArithmeticOverflow)
}

fn sub(a: u128, b: u128) -> Result<u128, EconomicsError> {
    a.checked_sub(b).ok_or(EconomicsError::InsufficientBacking)
}

fn ratio(value: u128, numerator: u128, denominator: u128) -> Result<u128, EconomicsError> {
    value
        .checked_mul(numerator)
        .ok_or(EconomicsError::ArithmeticOverflow)
        .map(|product| product / denominator)
}

pub fn claim_supply(total: u128, reserve: u128, excluded: &[u128]) -> Result<u128, EconomicsError> {
    let excluded = excluded
        .iter()
        .try_fold(reserve, |sum, value| checked_add(sum, *value))?;
    total
        .checked_sub(excluded)
        .ok_or(EconomicsError::ExclusionsExceedSupply)
}

pub fn claim_backing(backing: Backing) -> Result<u128, EconomicsError> {
    checked_add(
        checked_add(backing.liquid, backing.pooled)?,
        checked_add(backing.unwinding, backing.transit)?,
    )
}

pub fn claim_rate(state: EconomicState) -> Result<ClaimRate, EconomicsError> {
    let backing = claim_backing(state.backing)?;
    if state.active_reward > state.active_backing {
        return Err(EconomicsError::RewardActiveExceedsBacking);
    }
    if state.active_backing > state.claims {
        return Err(EconomicsError::ActiveExceedsClaims);
    }
    match (backing, state.claims, state.active_backing) {
        (0, 0, 0) => Ok(ClaimRate::EmptyGenesis),
        (_, 0, 0) => Ok(ClaimRate::BackingWithoutClaims),
        (0, _, _) => Err(EconomicsError::UncoveredClaims),
        _ => Ok(ClaimRate::Ratio {
            backing,
            claims: state.claims,
        }),
    }
}

pub fn redemption_quote(
    state: EconomicState,
    redeemed: u128,
    io_fee: u128,
    payout_fee: u128,
) -> Result<RedemptionQuote, EconomicsError> {
    let ClaimRate::Ratio { backing, claims } = claim_rate(state)? else {
        return Err(EconomicsError::BackingWithoutClaims);
    };
    if checked_add(redeemed, io_fee)? > claims {
        return Err(EconomicsError::RedemptionExceedsSupply);
    }
    let gross_icp = ratio(redeemed, backing, claims)?;
    let net_icp = gross_icp
        .checked_sub(payout_fee)
        .filter(|value| *value > 0)
        .ok_or(EconomicsError::PayoutDoesNotCoverFee)?;
    Ok(RedemptionQuote { gross_icp, net_icp })
}

pub fn require_liquidity(quote: RedemptionQuote, liquid: u128) -> Result<(), EconomicsError> {
    if liquid >= quote.gross_icp {
        return Ok(());
    }
    Err(EconomicsError::InsufficientLiquidity(LiquidityShortfall {
        gross_icp: quote.gross_icp,
        net_icp: quote.net_icp,
        available_liquid: liquid,
    }))
}

pub fn backed_io(increment: u128, backing: u128, claims: u128) -> Result<u128, EconomicsError> {
    match (backing, claims) {
        (0, 0) => Ok(increment),
        (0, _) => Err(EconomicsError::UncoveredClaims),
        (_, 0) => Err(EconomicsError::BackingWithoutClaims),
        _ => ratio(increment, claims, backing),
    }
}

pub fn target(active: u128, backing: u128, claims: u128) -> Result<u128, EconomicsError> {
    if active > claims {
        return Err(EconomicsError::ActiveExceedsClaims);
    }
    match (backing, claims, active) {
        (0, 0, 0) => Ok(0),
        (_, 0, 0) => Err(EconomicsError::BackingWithoutClaims),
        (0, _, _) => Err(EconomicsError::UncoveredClaims),
        _ => ratio(active, backing, claims),
    }
}

pub fn reward_target(state: EconomicState) -> Result<u128, EconomicsError> {
    let backing = claim_backing(state.backing)?;
    claim_rate(state)?;
    target(state.active_reward, backing, state.claims)
}

pub fn rewards_covered(state: EconomicState) -> Result<(), EconomicsError> {
    (state.backing.pooled >= reward_target(state)?)
        .then_some(())
        .ok_or(EconomicsError::RewardBackingUnderTarget)
}

pub fn split_40_60(amount: u128) -> Result<Split, EconomicsError> {
    let permanent = ratio(amount, 4_000, 10_000)?;
    Ok(Split {
        permanent,
        claim: sub(amount, permanent)?,
    })
}

pub fn reconcile(
    state: EconomicState,
    fee: u128,
    minimum_parent: u128,
    minimum_child_gross: u128,
) -> Result<ReconcilePlan, EconomicsError> {
    let backing = claim_backing(state.backing)?;
    let raw = target(state.active_backing, backing, state.claims)?;
    if state.backing.pooled == 0 && raw < minimum_parent {
        return Ok(ReconcilePlan::Hold { target: raw });
    }
    let floor = if state.backing.pooled > 0 {
        minimum_parent
    } else {
        0
    };
    let post_fee = target(state.active_backing, sub(backing, fee)?, state.claims)?.max(floor);
    if state.backing.pooled < post_fee {
        let credit = post_fee - state.backing.pooled;
        return if credit <= fee || (state.backing.pooled == 0 && credit < minimum_parent) {
            Ok(ReconcilePlan::Hold { target: post_fee })
        } else {
            Ok(ReconcilePlan::TopUp {
                target: post_fee,
                credit,
                debit: checked_add(credit, fee)?,
            })
        };
    }
    let gross = state.backing.pooled - post_fee;
    if gross < minimum_child_gross {
        Ok(ReconcilePlan::Hold { target: post_fee })
    } else {
        Ok(ReconcilePlan::Unwind {
            target: post_fee,
            gross,
            expected_credit: sub(gross, fee)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(liquid: u128, pooled: u128, claims: u128, active: u128) -> EconomicState {
        EconomicState {
            backing: Backing {
                liquid,
                pooled,
                ..Backing::default()
            },
            claims,
            active_backing: active,
            active_reward: 0,
        }
    }

    #[test]
    fn strict_claim_states_and_checked_totals() {
        assert_eq!(claim_supply(1_000, 400, &[100, 50]), Ok(450));
        assert_eq!(
            claim_supply(100, 80, &[21]),
            Err(EconomicsError::ExclusionsExceedSupply)
        );
        assert_eq!(
            claim_backing(Backing {
                liquid: 1,
                pooled: 2,
                unwinding: 3,
                transit: 4
            }),
            Ok(10)
        );
        assert_eq!(claim_rate(state(0, 0, 0, 0)), Ok(ClaimRate::EmptyGenesis));
        assert_eq!(
            claim_rate(state(10, 0, 0, 0)),
            Ok(ClaimRate::BackingWithoutClaims)
        );
        assert_eq!(
            claim_rate(state(0, 0, 1, 0)),
            Err(EconomicsError::UncoveredClaims)
        );
        assert_eq!(
            claim_rate(state(1, 0, 1, 2)),
            Err(EconomicsError::ActiveExceedsClaims)
        );
        let mut invalid_reward = state(1, 0, 1, 1);
        invalid_reward.active_reward = 2;
        assert_eq!(
            claim_rate(invalid_reward),
            Err(EconomicsError::RewardActiveExceedsBacking)
        );
        assert_eq!(
            claim_backing(Backing {
                liquid: u128::MAX,
                pooled: 1,
                ..Backing::default()
            }),
            Err(EconomicsError::ArithmeticOverflow)
        );
    }

    #[test]
    fn redemption_uses_total_backing_and_gates_on_liquid() {
        let quote = redemption_quote(state(100, 900, 500, 0), 100, 2, 10).unwrap();
        assert_eq!((quote.gross_icp, quote.net_icp), (200, 190));
        assert_eq!(require_liquidity(quote, 200), Ok(()));
        assert_eq!(
            require_liquidity(quote, 100),
            Err(EconomicsError::InsufficientLiquidity(LiquidityShortfall {
                gross_icp: 200,
                net_icp: 190,
                available_liquid: 100
            }))
        );
    }

    #[test]
    fn backed_release_handles_genesis_and_appreciated_rate() {
        assert_eq!(backed_io(100, 0, 0), Ok(100));
        assert_eq!(backed_io(100, 1_000, 500), Ok(50));
        assert_eq!(backed_io(1, 0, 1), Err(EconomicsError::UncoveredClaims));
        assert_eq!(
            backed_io(1, 1, 0),
            Err(EconomicsError::BackingWithoutClaims)
        );
    }

    #[test]
    fn targets_and_reward_coverage_are_separate() {
        let mut value = state(500, 500, 1_000, 600);
        value.active_reward = 500;
        assert_eq!(target(600, 1_000, 1_000), Ok(600));
        assert_eq!(reward_target(value), Ok(500));
        assert_eq!(rewards_covered(value), Ok(()));
        value.backing.pooled = 499;
        value.backing.liquid = 501;
        assert_eq!(
            rewards_covered(value),
            Err(EconomicsError::RewardBackingUnderTarget)
        );
    }

    #[test]
    fn split_and_reconciliation_are_fee_aware() {
        assert_eq!(
            split_40_60(101),
            Ok(Split {
                permanent: 40,
                claim: 61
            })
        );
        assert_eq!(
            reconcile(state(1_000, 0, 1_000, 50), 10, 100, 110),
            Ok(ReconcilePlan::Hold { target: 50 })
        );
        assert!(matches!(
            reconcile(state(600, 400, 1_000, 500), 10, 100, 110),
            Ok(ReconcilePlan::TopUp { .. })
        ));
        assert!(matches!(
            reconcile(state(100, 900, 1_000, 500), 10, 100, 110),
            Ok(ReconcilePlan::Unwind { .. })
        ));
    }
}
