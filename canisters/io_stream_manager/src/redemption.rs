use candid::{CandidType, Principal};
use serde::Deserialize;

use crate::{
    state::Account,
    transfer::{deterministic_memo, LedgerMethod, OwnTransferAttempt, TransferState},
};

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct RedeemArgs {
    pub from_subaccount: Option<Vec<u8>>,
    pub io_amount_e8s: u128,
    pub min_icp_out_e8s: u128,
    pub max_io_fee_e8s: u128,
    pub max_icp_fee_e8s: u128,
    pub expires_at_nanos: u64,
    pub nonce: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum RedemptionPhase {
    Prepared,
    PullingIo,
    IoInReserve,
    PayingIcp,
    Completed,
    Stuck,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct CanonicalRedemptionSnapshot {
    pub total_supply_e8s: u128,
    pub reserve_io_e8s: u128,
    pub excluded_io_e8s: u128,
    pub liquid_icp_e8s: u128,
    pub io_fee_e8s: u128,
    pub icp_fee_e8s: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct RedemptionOperation {
    pub caller: Principal,
    pub nonce: u64,
    pub account: Account,
    pub io_amount_e8s: u128,
    pub gross_icp_e8s: u128,
    pub net_icp_e8s: u128,
    pub snapshot: CanonicalRedemptionSnapshot,
    pub io_pull: OwnTransferAttempt,
    pub icp_payout: OwnTransferAttempt,
    pub phase: RedemptionPhase,
}

pub fn calculate(
    caller: Principal,
    args: &RedeemArgs,
    snapshot: CanonicalRedemptionSnapshot,
    io_ledger: Principal,
    icp_ledger: Principal,
    reserve: Account,
    liquid: Account,
    now: u64,
) -> Result<RedemptionOperation, String> {
    let account = Account {
        owner: caller,
        subaccount: args.from_subaccount.clone(),
    };
    account.validate()?;
    if args.io_amount_e8s == 0 || args.expires_at_nanos < now {
        return Err("redemption is zero or expired".into());
    }
    if snapshot.io_fee_e8s > args.max_io_fee_e8s || snapshot.icp_fee_e8s > args.max_icp_fee_e8s {
        return Err("current ledger fee exceeds caller maximum".into());
    }
    let redeemable_supply = snapshot
        .total_supply_e8s
        .checked_sub(snapshot.reserve_io_e8s)
        .and_then(|v| v.checked_sub(snapshot.excluded_io_e8s))
        .ok_or("canonical supply exclusions exceed total supply")?;
    if redeemable_supply == 0 || args.io_amount_e8s > redeemable_supply {
        return Err("insufficient redeemable IO supply".into());
    }
    let gross = snapshot
        .liquid_icp_e8s
        .checked_mul(args.io_amount_e8s)
        .ok_or("redemption multiplication overflow")?
        / redeemable_supply;
    let net = gross
        .checked_sub(snapshot.icp_fee_e8s)
        .ok_or("gross ICP does not cover payout fee")?;
    if net == 0 || net < args.min_icp_out_e8s {
        return Err("minimum ICP output not met".into());
    }
    let io_memo = deterministic_memo(b"io-redemption-pull-v1", caller, args.nonce);
    let icp_memo = deterministic_memo(b"io-redemption-pay-v1", caller, args.nonce);
    let io_pull = OwnTransferAttempt {
        ledger: io_ledger,
        method: LedgerMethod::Icrc2TransferFrom,
        source_subaccount: None,
        source_account: Some(account.clone()),
        destination: reserve,
        amount_e8s: args.io_amount_e8s,
        fee_e8s: snapshot.io_fee_e8s,
        memo: io_memo,
        created_at_time_nanos: now,
        state: TransferState::Prepared,
    };
    let icp_payout = OwnTransferAttempt {
        ledger: icp_ledger,
        method: LedgerMethod::IcpTransfer,
        source_subaccount: liquid.subaccount.clone(),
        source_account: None,
        destination: account.clone(),
        amount_e8s: net,
        fee_e8s: snapshot.icp_fee_e8s,
        memo: icp_memo,
        created_at_time_nanos: now,
        state: TransferState::Prepared,
    };
    io_pull.validate()?;
    icp_payout.validate()?;
    Ok(RedemptionOperation {
        caller,
        nonce: args.nonce,
        account,
        io_amount_e8s: args.io_amount_e8s,
        gross_icp_e8s: gross,
        net_icp_e8s: net,
        snapshot,
        io_pull,
        icp_payout,
        phase: RedemptionPhase::Prepared,
    })
}

pub fn verify_postconditions(
    operation: &RedemptionOperation,
    post: &CanonicalRedemptionSnapshot,
) -> Result<(), String> {
    let minimum_reserve = operation
        .snapshot
        .reserve_io_e8s
        .checked_add(operation.io_amount_e8s)
        .ok_or("reserve postcondition overflow")?;
    let maximum_supply = operation
        .snapshot
        .total_supply_e8s
        .checked_sub(operation.snapshot.io_fee_e8s)
        .ok_or("supply postcondition underflow")?;
    let minimum_liquid = operation
        .snapshot
        .liquid_icp_e8s
        .checked_sub(operation.gross_icp_e8s)
        .ok_or("liquid postcondition underflow")?;
    if post.reserve_io_e8s < minimum_reserve {
        return Err("adverse IO reserve decrease after redemption".into());
    }
    if post.total_supply_e8s > maximum_supply {
        return Err("IO supply did not reflect the transfer_from fee burn".into());
    }
    if post.excluded_io_e8s != operation.snapshot.excluded_io_e8s {
        return Err("excluded IO balances changed during redemption".into());
    }
    if post.liquid_icp_e8s < minimum_liquid {
        return Err("liquid ICP decreased beyond the persisted payout".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn principal(id: u8) -> Principal {
        Principal::from_slice(&[id])
    }

    #[test]
    fn exact_fee_burn_equation_and_account_binding() {
        let args = RedeemArgs {
            from_subaccount: Some(vec![7; 32]),
            io_amount_e8s: 100,
            min_icp_out_e8s: 189,
            max_io_fee_e8s: 2,
            max_icp_fee_e8s: 10,
            expires_at_nanos: 2,
            nonce: 0,
        };
        let operation = calculate(
            principal(1),
            &args,
            CanonicalRedemptionSnapshot {
                total_supply_e8s: 1_000,
                reserve_io_e8s: 400,
                excluded_io_e8s: 100,
                liquid_icp_e8s: 1_000,
                io_fee_e8s: 2,
                icp_fee_e8s: 10,
            },
            principal(2),
            principal(3),
            Account {
                owner: principal(4),
                subaccount: None,
            },
            Account {
                owner: principal(4),
                subaccount: Some(vec![9; 32]),
            },
            1,
        )
        .unwrap();
        assert_eq!(operation.gross_icp_e8s, 200);
        assert_eq!(operation.net_icp_e8s, 190);
        assert_eq!(operation.account.owner, principal(1));
        assert_eq!(operation.icp_payout.destination, operation.account);
    }

    #[test]
    fn conservative_postconditions_accept_donations_and_extra_burns() {
        let args = RedeemArgs {
            from_subaccount: None,
            io_amount_e8s: 100,
            min_icp_out_e8s: 1,
            max_io_fee_e8s: 2,
            max_icp_fee_e8s: 10,
            expires_at_nanos: 2,
            nonce: 0,
        };
        let snapshot = CanonicalRedemptionSnapshot {
            total_supply_e8s: 1_000,
            reserve_io_e8s: 400,
            excluded_io_e8s: 100,
            liquid_icp_e8s: 1_000,
            io_fee_e8s: 2,
            icp_fee_e8s: 10,
        };
        let operation = calculate(
            principal(1),
            &args,
            snapshot,
            principal(2),
            principal(3),
            Account {
                owner: principal(4),
                subaccount: None,
            },
            Account {
                owner: principal(4),
                subaccount: None,
            },
            1,
        )
        .unwrap();
        let post = CanonicalRedemptionSnapshot {
            total_supply_e8s: 997,
            reserve_io_e8s: 501,
            excluded_io_e8s: 100,
            liquid_icp_e8s: 800,
            io_fee_e8s: 2,
            icp_fee_e8s: 10,
        };
        assert_eq!(verify_postconditions(&operation, &post), Ok(()));
        let mut adverse = post;
        adverse.reserve_io_e8s = 499;
        assert!(verify_postconditions(&operation, &adverse).is_err());
    }
}
