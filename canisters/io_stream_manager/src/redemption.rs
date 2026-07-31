use candid::{CandidType, Principal};
use serde::Deserialize;

use crate::{
    state::{Account, OperationSequence, StreamConfig},
    transfer::{deterministic_memo, OwnTransferIntent, TransferAttempt},
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
    PullSubmitted,
    IoInReserve,
    PayoutSubmitted,
    PayoutSucceeded,
    Completed,
    Stuck,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct CanonicalRedemptionSnapshot {
    pub total_supply_e8s: u128,
    pub reserve_io_e8s: u128,
    pub excluded_io_balances: Vec<(Account, u128)>,
    pub liquid_icp_e8s: u128,
    pub io_fee_e8s: u128,
    pub icp_fee_e8s: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct RedemptionOperation {
    pub sequence: OperationSequence,
    pub request_fingerprint: Vec<u8>,
    pub caller: Principal,
    pub nonce: u64,
    pub account: Account,
    pub io_amount_e8s: u128,
    pub gross_icp_e8s: u128,
    pub net_icp_e8s: u128,
    pub snapshot: CanonicalRedemptionSnapshot,
    pub io_pull: TransferAttempt,
    pub icp_payout: TransferAttempt,
    pub phase: RedemptionPhase,
}

pub fn calculate(
    caller: Principal,
    args: &RedeemArgs,
    snapshot: CanonicalRedemptionSnapshot,
    config: &StreamConfig,
    now: u64,
    sequence: OperationSequence,
) -> Result<RedemptionOperation, String> {
    let account = Account {
        owner: caller,
        subaccount: args.from_subaccount.clone(),
    };
    account.validate()?;
    if args.io_amount_e8s == 0 || args.expires_at_nanos < now {
        return Err("redemption is zero or expired".into());
    }
    if args.expires_at_nanos.saturating_sub(now) > config.maximum_request_lifetime_nanos {
        return Err("redemption expiry exceeds launch lifetime bound".into());
    }
    if account.effective_eq(&config.io_reserve)? {
        return Err("reserve account cannot redeem".into());
    }
    if config
        .excluded_io_accounts
        .iter()
        .any(|excluded| account.effective_eq(excluded).unwrap_or(false))
    {
        return Err("excluded account cannot redeem".into());
    }
    if snapshot.io_fee_e8s > args.max_io_fee_e8s || snapshot.icp_fee_e8s > args.max_icp_fee_e8s {
        return Err("current ledger fee exceeds caller maximum".into());
    }
    let excluded = snapshot
        .excluded_io_balances
        .iter()
        .try_fold(0u128, |total, (_, balance)| total.checked_add(*balance))
        .ok_or("excluded balance sum overflow")?;
    let quote = io_core_model::redemption_quote(
        args.io_amount_e8s,
        snapshot.io_fee_e8s,
        snapshot.total_supply_e8s,
        snapshot.reserve_io_e8s,
        excluded,
        snapshot.liquid_icp_e8s,
        snapshot.icp_fee_e8s,
    )
    .map_err(|error| format!("redemption quote failed: {error:?}"))?;
    if quote.net_icp_e8s < args.min_icp_out_e8s {
        return Err("minimum ICP output not met".into());
    }
    let io_memo = deterministic_memo(b"io-redemption-pull-v1", caller, args.nonce);
    let icp_memo = deterministic_memo(b"io-redemption-pay-v1", caller, args.nonce);
    let io_pull = TransferAttempt::prepared(OwnTransferIntent::Icrc2TransferFrom {
        ledger: config.io_ledger,
        spender_subaccount: [0; 32],
        from: account.clone(),
        to: config.io_reserve.clone(),
        amount: args.io_amount_e8s,
        fee: snapshot.io_fee_e8s,
        memo: io_memo,
        created_at_time: now,
    })?;
    let icp_payout = TransferAttempt::prepared(OwnTransferIntent::Icrc1 {
        ledger: config.icp_ledger,
        from_subaccount: config.liquid_icp.canonical()?.subaccount,
        to: account.clone(),
        amount: quote.net_icp_e8s,
        fee: snapshot.icp_fee_e8s,
        memo: icp_memo,
        created_at_time: now,
    })?;
    let request_fingerprint = request_fingerprint(caller, args, &account);
    Ok(RedemptionOperation {
        sequence,
        request_fingerprint,
        caller,
        nonce: args.nonce,
        account,
        io_amount_e8s: args.io_amount_e8s,
        gross_icp_e8s: quote.gross_icp_e8s,
        net_icp_e8s: quote.net_icp_e8s,
        snapshot,
        io_pull,
        icp_payout,
        phase: RedemptionPhase::Prepared,
    })
}

pub fn request_fingerprint(caller: Principal, args: &RedeemArgs, account: &Account) -> Vec<u8> {
    use sha2::{Digest, Sha256};
    let canonical = account.canonical().expect("validated redemption account");
    Sha256::digest(
        candid::encode_one((caller, canonical.subaccount, args))
            .expect("redemption request must encode"),
    )
    .to_vec()
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
    if post.excluded_io_balances.len() != operation.snapshot.excluded_io_balances.len() {
        return Err("excluded IO balance set changed during redemption".into());
    }
    for ((pre_account, pre), (post_account, post)) in operation
        .snapshot
        .excluded_io_balances
        .iter()
        .zip(&post.excluded_io_balances)
    {
        if !pre_account.effective_eq(post_account)? || post < pre {
            return Err("excluded IO account decreased during redemption".into());
        }
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

    fn config(liquid_subaccount: Option<Vec<u8>>) -> StreamConfig {
        StreamConfig {
            io_ledger: principal(2),
            icp_ledger: principal(3),
            nns_manager: principal(5),
            nns_receipt_source: Account {
                owner: principal(5),
                subaccount: None,
            },
            sns_governance: principal(6),
            io_reserve: Account {
                owner: principal(4),
                subaccount: None,
            },
            liquid_icp: Account {
                owner: principal(4),
                subaccount: liquid_subaccount,
            },
            excluded_io_accounts: Vec::new(),
            minimum_redemption_io_e8s: 1,
            expected_io_fee_e8s: 2,
            expected_icp_fee_e8s: 10,
            maximum_request_lifetime_nanos: 1_000,
            retry_delay_nanos: 10,
            ledger_deduplication_window_nanos: 1_000,
        }
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
                excluded_io_balances: vec![(
                    Account {
                        owner: principal(8),
                        subaccount: None,
                    },
                    100,
                )],
                liquid_icp_e8s: 1_000,
                io_fee_e8s: 2,
                icp_fee_e8s: 10,
            },
            &config(Some(vec![9; 32])),
            1,
            OperationSequence(1),
        )
        .unwrap();
        assert_eq!(operation.gross_icp_e8s, 200);
        assert_eq!(operation.net_icp_e8s, 190);
        assert_eq!(operation.account.owner, principal(1));
        assert!(
            matches!(&operation.icp_payout.intent, OwnTransferIntent::Icrc1 { to, .. } if to == &operation.account)
        );
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
            excluded_io_balances: vec![(
                Account {
                    owner: principal(8),
                    subaccount: None,
                },
                100,
            )],
            liquid_icp_e8s: 1_000,
            io_fee_e8s: 2,
            icp_fee_e8s: 10,
        };
        let operation = calculate(
            principal(1),
            &args,
            snapshot,
            &config(None),
            1,
            OperationSequence(1),
        )
        .unwrap();
        let post = CanonicalRedemptionSnapshot {
            total_supply_e8s: 997,
            reserve_io_e8s: 501,
            excluded_io_balances: vec![(
                Account {
                    owner: principal(8),
                    subaccount: None,
                },
                100,
            )],
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
