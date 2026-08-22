use candid::{CandidType, Principal};
use serde::Deserialize;

use crate::{
    state::{Account, OperationSequence, RedemptionResult, StreamConfig},
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
    CompletionPrepared,
    CallerResultApplied,
    Stuck,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct CanonicalRedeemRequestV1 {
    pub effective_subaccount: [u8; 32],
    pub io_amount_e8s: u128,
    pub min_icp_out_e8s: u128,
    pub max_io_fee_e8s: u128,
    pub max_icp_fee_e8s: u128,
    pub expires_at_nanos: u64,
    pub nonce: u64,
}

impl CanonicalRedeemRequestV1 {
    pub fn from_args(args: &RedeemArgs) -> Result<Self, String> {
        let effective_subaccount = match &args.from_subaccount {
            None => [0; 32],
            Some(bytes) => bytes
                .as_slice()
                .try_into()
                .map_err(|_| "subaccount must contain exactly 32 bytes")?,
        };
        Ok(Self {
            effective_subaccount,
            io_amount_e8s: args.io_amount_e8s,
            min_icp_out_e8s: args.min_icp_out_e8s,
            max_io_fee_e8s: args.max_io_fee_e8s,
            max_icp_fee_e8s: args.max_icp_fee_e8s,
            expires_at_nanos: args.expires_at_nanos,
            nonce: args.nonce,
        })
    }

    pub fn account(&self, caller: Principal) -> Account {
        Account {
            owner: caller,
            subaccount: (self.effective_subaccount != [0; 32])
                .then(|| self.effective_subaccount.to_vec()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct RedemptionPreparation {
    pub sequence: OperationSequence,
    pub captured_control_epoch: u64,
    pub request_fingerprint: Vec<u8>,
    pub request: CanonicalRedeemRequestV1,
    pub caller: Principal,
    pub account: Account,
    pub prepared_at_nanos: u64,
}

impl RedemptionPreparation {
    pub fn validate(&self) -> Result<(), String> {
        if self.caller == Principal::anonymous() {
            return Err("invalid redemption preparation identity".into());
        }
        self.account.validate()?;
        if self.account != self.request.account(self.caller) {
            return Err("preparation account is not canonical".into());
        }
        if self.request_fingerprint.len() != 32 || self.prepared_at_nanos == 0 {
            return Err("invalid preparation fingerprint or timestamp".into());
        }
        if self.request_fingerprint != request_fingerprint(self.caller, &self.request) {
            return Err("preparation request fingerprint does not match its request".into());
        }
        if self.request.io_amount_e8s == 0 || self.request.expires_at_nanos < self.prepared_at_nanos
        {
            return Err("invalid prepared redemption bounds".into());
        }
        Ok(())
    }
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
    pub icp_payout: Option<TransferAttempt>,
    pub completion_result: Option<RedemptionResult>,
    pub phase: RedemptionPhase,
}

impl RedemptionOperation {
    pub fn validate(&self, config: &StreamConfig) -> Result<(), String> {
        if self.request_fingerprint.len() != 32 {
            return Err("invalid redemption sequence or fingerprint".into());
        }
        if self.caller == Principal::anonymous() || self.account.owner != self.caller {
            return Err("invalid redemption caller account".into());
        }
        self.account.validate()?;
        self.io_pull.validate()?;
        if self.io_amount_e8s < config.minimum_redemption_io_e8s
            || self.snapshot.io_fee_e8s != config.expected_io_fee_e8s
            || self.snapshot.icp_fee_e8s != config.expected_icp_fee_e8s
            || self.gross_icp_e8s.checked_sub(self.snapshot.icp_fee_e8s) != Some(self.net_icp_e8s)
        {
            return Err("redemption economics do not match approved configuration".into());
        }
        if self.account.effective_eq(&config.io_reserve)?
            || config
                .excluded_io_accounts
                .iter()
                .try_fold(false, |matched, account| {
                    self.account
                        .effective_eq(account)
                        .map(|same| matched || same)
                })?
        {
            return Err("redemption source has a forbidden account role".into());
        }
        if self.snapshot.excluded_io_balances.len() != config.excluded_io_accounts.len()
            || self
                .snapshot
                .excluded_io_balances
                .iter()
                .zip(&config.excluded_io_accounts)
                .any(|((actual, _), expected)| !actual.effective_eq(expected).unwrap_or(false))
        {
            return Err("redemption snapshot excluded accounts do not match configuration".into());
        }
        let expected_pull_memo =
            deterministic_memo(b"io-redemption-pull-v1", self.caller, self.nonce);
        match &self.io_pull.intent {
            OwnTransferIntent::Icrc2TransferFrom {
                ledger,
                spender_subaccount,
                from,
                to,
                amount,
                fee,
                memo,
                ..
            } if *ledger == config.io_ledger
                && *spender_subaccount == [0; 32]
                && from.effective_eq(&self.account)?
                && to.effective_eq(&config.io_reserve)?
                && *amount == self.io_amount_e8s
                && *fee == self.snapshot.io_fee_e8s
                && *memo == expected_pull_memo => {}
            _ => return Err("IO pull intent does not match redemption context".into()),
        }
        if let Some(payout) = &self.icp_payout {
            payout.validate()?;
            let expected_payout_memo =
                deterministic_memo(b"io-redemption-pay-v1", self.caller, self.nonce);
            let liquid_subaccount = config.liquid_icp.canonical()?.subaccount;
            match &payout.intent {
                OwnTransferIntent::Icrc1 {
                    ledger,
                    from_subaccount,
                    to,
                    amount,
                    fee,
                    memo,
                    created_at_time,
                } if *ledger == config.icp_ledger
                    && *from_subaccount == liquid_subaccount
                    && to.effective_eq(&self.account)?
                    && *amount == self.net_icp_e8s
                    && *fee == self.snapshot.icp_fee_e8s
                    && *memo == expected_payout_memo
                    && *created_at_time >= self.io_pull.intent.created_at_time() => {}
                _ => return Err("ICP payout intent does not match redemption context".into()),
            }
        }
        if matches!(
            self.phase,
            RedemptionPhase::PayoutSubmitted
                | RedemptionPhase::PayoutSucceeded
                | RedemptionPhase::CompletionPrepared
                | RedemptionPhase::CallerResultApplied
        ) && self.icp_payout.is_none()
        {
            return Err("payout phase requires an immutable payout intent".into());
        }
        if matches!(
            self.phase,
            RedemptionPhase::CompletionPrepared | RedemptionPhase::CallerResultApplied
        ) != self.completion_result.is_some()
        {
            return Err("completion phase/result mismatch".into());
        }
        let io_succeeded = self.io_pull.succeeded_block().ok();
        let payout_succeeded = self
            .icp_payout
            .as_ref()
            .and_then(|attempt| attempt.succeeded_block().ok());
        match self.phase {
            RedemptionPhase::Prepared
                if !matches!(self.io_pull.state, crate::transfer::TransferState::Prepared)
                    || self.icp_payout.is_some() =>
            {
                return Err("prepared redemption has incompatible transfer state".into())
            }
            RedemptionPhase::PullSubmitted
                if !matches!(
                    self.io_pull.state,
                    crate::transfer::TransferState::Submitted { .. }
                ) || self.icp_payout.is_some() =>
            {
                return Err("pull-submitted redemption has incompatible transfer state".into())
            }
            RedemptionPhase::IoInReserve if io_succeeded.is_none() || self.icp_payout.is_some() => {
                return Err("IO-in-reserve redemption lacks exact pull success".into())
            }
            RedemptionPhase::PayoutSubmitted
                if io_succeeded.is_none()
                    || !matches!(
                        self.icp_payout.as_ref().map(|value| &value.state),
                        Some(crate::transfer::TransferState::Submitted { .. })
                    ) =>
            {
                return Err("payout-submitted redemption has incompatible transfer state".into())
            }
            RedemptionPhase::PayoutSucceeded
            | RedemptionPhase::CompletionPrepared
            | RedemptionPhase::CallerResultApplied
                if io_succeeded.is_none() || payout_succeeded.is_none() =>
            {
                return Err("completed-effect redemption lacks exact transfer success".into())
            }
            _ => {}
        }
        if let Some(result) = &self.completion_result {
            if result.request_fingerprint != self.request_fingerprint
                || result.nonce != self.nonce
                || Some(result.io_block) != io_succeeded
                || Some(result.icp_block) != payout_succeeded
                || result.net_icp_e8s != self.net_icp_e8s
                || result.gross_icp_e8s != self.gross_icp_e8s
                || result.io_fee_e8s != self.snapshot.io_fee_e8s
                || result.icp_fee_e8s != self.snapshot.icp_fee_e8s
                || result.completed_at_nanos == 0
            {
                return Err("redemption completion result does not match operation".into());
            }
        }
        Ok(())
    }
}

pub fn calculate(
    preparation: &RedemptionPreparation,
    snapshot: CanonicalRedemptionSnapshot,
    config: &StreamConfig,
) -> Result<RedemptionOperation, String> {
    preparation.validate()?;
    let caller = preparation.caller;
    let args = &preparation.request;
    let account = preparation.account.clone();
    account.validate()?;
    if args.io_amount_e8s == 0 || args.expires_at_nanos < preparation.prepared_at_nanos {
        return Err("redemption is zero or expired".into());
    }
    if args
        .expires_at_nanos
        .checked_sub(preparation.prepared_at_nanos)
        .is_none_or(|lifetime| lifetime > config.maximum_request_lifetime_nanos)
    {
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
    let claims = io_core_model::claim_supply(
        snapshot.total_supply_e8s,
        snapshot.reserve_io_e8s,
        &[excluded],
    )
    .map_err(|error| format!("claim supply failed: {error:?}"))?;
    let quote = io_core_model::redemption_quote(
        io_core_model::EconomicState {
            backing: io_core_model::Backing {
                liquid: snapshot.liquid_icp_e8s,
                ..io_core_model::Backing::default()
            },
            claims,
            active_backing: 0,
            active_reward: 0,
        },
        args.io_amount_e8s,
        snapshot.io_fee_e8s,
        snapshot.icp_fee_e8s,
    )
    .map_err(|error| format!("redemption quote failed: {error:?}"))?;
    if quote.net_icp < args.min_icp_out_e8s {
        return Err("minimum ICP output not met".into());
    }
    let io_memo = deterministic_memo(b"io-redemption-pull-v1", caller, args.nonce);
    let io_pull = TransferAttempt::prepared(OwnTransferIntent::Icrc2TransferFrom {
        ledger: config.io_ledger,
        spender_subaccount: [0; 32],
        from: account.clone(),
        to: config.io_reserve.clone(),
        amount: args.io_amount_e8s,
        fee: snapshot.io_fee_e8s,
        memo: io_memo,
        created_at_time: preparation.prepared_at_nanos,
    })?;
    Ok(RedemptionOperation {
        sequence: preparation.sequence,
        request_fingerprint: preparation.request_fingerprint.clone(),
        caller,
        nonce: args.nonce,
        account,
        io_amount_e8s: args.io_amount_e8s,
        gross_icp_e8s: quote.gross_icp,
        net_icp_e8s: quote.net_icp,
        snapshot,
        io_pull,
        icp_payout: None,
        completion_result: None,
        phase: RedemptionPhase::Prepared,
    })
}

pub fn request_fingerprint(caller: Principal, request: &CanonicalRedeemRequestV1) -> Vec<u8> {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(b"io-canonical-redeem-request-v1\0");
    hasher.update(candid::encode_one((caller, request)).expect("redemption request must encode"));
    hasher.finalize().to_vec()
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

pub fn verify_pre_payout_conditions(
    operation: &RedemptionOperation,
    fresh: &CanonicalRedemptionSnapshot,
) -> Result<(), String> {
    if fresh.io_fee_e8s != operation.snapshot.io_fee_e8s {
        return Err("IO fee changed before redemption payout".into());
    }
    if fresh.icp_fee_e8s != operation.snapshot.icp_fee_e8s {
        return Err("ICP fee changed before redemption payout".into());
    }
    let minimum_reserve = operation
        .snapshot
        .reserve_io_e8s
        .checked_add(operation.io_amount_e8s)
        .ok_or("reserve pre-payout check overflow")?;
    if fresh.reserve_io_e8s < minimum_reserve {
        return Err("IO reserve does not reflect the exact redemption pull".into());
    }
    let maximum_supply = operation
        .snapshot
        .total_supply_e8s
        .checked_sub(operation.snapshot.io_fee_e8s)
        .ok_or("supply pre-payout check underflow")?;
    if fresh.total_supply_e8s > maximum_supply {
        return Err("IO total supply increased or did not reflect the pull fee burn".into());
    }
    if fresh.excluded_io_balances.len() != operation.snapshot.excluded_io_balances.len() {
        return Err("excluded IO account set changed before payout".into());
    }
    for ((expected_account, expected_balance), (fresh_account, fresh_balance)) in operation
        .snapshot
        .excluded_io_balances
        .iter()
        .zip(&fresh.excluded_io_balances)
    {
        if !expected_account.effective_eq(fresh_account)? {
            return Err("excluded IO account identity/order changed before payout".into());
        }
        if fresh_balance < expected_balance {
            return Err("excluded IO balance decreased before payout".into());
        }
    }
    if fresh.liquid_icp_e8s < operation.snapshot.liquid_icp_e8s {
        return Err("liquid ICP backing decreased before payout".into());
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
            jupiter_receipt_source: Account {
                owner: principal(5),
                subaccount: Some(vec![1; 32]),
            },
            two_week_receipt_source: Account {
                owner: principal(5),
                subaccount: Some(vec![2; 32]),
            },
            jupiter_io_account: Account {
                owner: principal(7),
                subaccount: None,
            },
            sns_governance: principal(6),
            sns_root: Principal::from_slice(&[5; 29]),
            expected_sns_governance_module_hash: vec![0; 32],
            approved_reward_event_duration_seconds: 86_400,
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

    fn preparation(args: &RedeemArgs) -> RedemptionPreparation {
        let request = CanonicalRedeemRequestV1::from_args(args).unwrap();
        RedemptionPreparation {
            sequence: OperationSequence(1),
            captured_control_epoch: 1,
            request_fingerprint: request_fingerprint(principal(1), &request),
            account: request.account(principal(1)),
            request,
            caller: principal(1),
            prepared_at_nanos: 1,
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
            &preparation(&args),
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
        )
        .unwrap();
        assert_eq!(operation.gross_icp_e8s, 200);
        assert_eq!(operation.net_icp_e8s, 190);
        assert_eq!(operation.account.owner, principal(1));
        assert!(operation.icp_payout.is_none());
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
        let operation = calculate(&preparation(&args), snapshot, &config(None)).unwrap();
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

    #[test]
    fn pre_payout_conditions_reject_adverse_and_accept_conservative_drift() {
        let args = RedeemArgs {
            from_subaccount: None,
            io_amount_e8s: 100,
            min_icp_out_e8s: 1,
            max_io_fee_e8s: 2,
            max_icp_fee_e8s: 10,
            expires_at_nanos: 2,
            nonce: 0,
        };
        let excluded = Account {
            owner: principal(8),
            subaccount: None,
        };
        let snapshot = CanonicalRedemptionSnapshot {
            total_supply_e8s: 1_000,
            reserve_io_e8s: 400,
            excluded_io_balances: vec![(excluded.clone(), 100)],
            liquid_icp_e8s: 1_000,
            io_fee_e8s: 2,
            icp_fee_e8s: 10,
        };
        let operation = calculate(&preparation(&args), snapshot, &config(None)).unwrap();
        let valid = CanonicalRedemptionSnapshot {
            total_supply_e8s: 998,
            reserve_io_e8s: 500,
            excluded_io_balances: vec![(excluded.clone(), 100)],
            liquid_icp_e8s: 1_000,
            io_fee_e8s: 2,
            icp_fee_e8s: 10,
        };
        assert_eq!(verify_pre_payout_conditions(&operation, &valid), Ok(()));

        let mut cases = Vec::new();
        let mut value = valid.clone();
        value.excluded_io_balances[0].1 = 99;
        cases.push(value);
        let mut value = valid.clone();
        value.excluded_io_balances[0].0.owner = principal(9);
        cases.push(value);
        let mut value = valid.clone();
        value.total_supply_e8s = 999;
        cases.push(value);
        let mut value = valid.clone();
        value.liquid_icp_e8s = 999;
        cases.push(value);
        let mut value = valid.clone();
        value.io_fee_e8s = 3;
        cases.push(value);
        let mut value = valid.clone();
        value.icp_fee_e8s = 11;
        cases.push(value);
        let mut value = valid.clone();
        value.reserve_io_e8s = 499;
        cases.push(value);
        for adverse in cases {
            assert!(verify_pre_payout_conditions(&operation, &adverse).is_err());
        }

        let mut favorable = valid.clone();
        favorable.liquid_icp_e8s += 1;
        favorable.excluded_io_balances[0].1 += 1;
        favorable.reserve_io_e8s += 1;
        favorable.total_supply_e8s -= 1;
        assert_eq!(verify_pre_payout_conditions(&operation, &favorable), Ok(()));
    }

    #[test]
    fn null_and_zero_subaccounts_have_one_request_identity() {
        let mut null = RedeemArgs {
            from_subaccount: None,
            io_amount_e8s: 100,
            min_icp_out_e8s: 1,
            max_io_fee_e8s: 2,
            max_icp_fee_e8s: 10,
            expires_at_nanos: 2,
            nonce: 0,
        };
        let canonical_null = CanonicalRedeemRequestV1::from_args(&null).unwrap();
        null.from_subaccount = Some(vec![0; 32]);
        let canonical_zero = CanonicalRedeemRequestV1::from_args(&null).unwrap();
        assert_eq!(canonical_null, canonical_zero);
        assert_eq!(
            request_fingerprint(principal(1), &canonical_null),
            request_fingerprint(principal(1), &canonical_zero)
        );
    }

    #[test]
    fn semantic_validation_rejects_fingerprint_intent_and_phase_corruption() {
        let args = RedeemArgs {
            from_subaccount: None,
            io_amount_e8s: 100,
            min_icp_out_e8s: 1,
            max_io_fee_e8s: 2,
            max_icp_fee_e8s: 10,
            expires_at_nanos: 2,
            nonce: 0,
        };
        let mut bad_preparation = preparation(&args);
        bad_preparation.request_fingerprint[0] ^= 1;
        assert!(bad_preparation.validate().is_err());

        let config = config(None);
        let mut operation = calculate(
            &preparation(&args),
            CanonicalRedemptionSnapshot {
                total_supply_e8s: 1_000,
                reserve_io_e8s: 400,
                excluded_io_balances: Vec::new(),
                liquid_icp_e8s: 1_000,
                io_fee_e8s: 2,
                icp_fee_e8s: 10,
            },
            &config,
        )
        .unwrap();
        assert_eq!(operation.validate(&config), Ok(()));
        operation.phase = RedemptionPhase::IoInReserve;
        assert!(operation.validate(&config).is_err());
        operation.phase = RedemptionPhase::Prepared;
        if let OwnTransferIntent::Icrc2TransferFrom { to, .. } = &mut operation.io_pull.intent {
            *to = Account {
                owner: principal(9),
                subaccount: None,
            };
        }
        operation.io_pull.fingerprint = operation.io_pull.intent.fingerprint();
        assert!(operation.validate(&config).is_err());
    }
}
