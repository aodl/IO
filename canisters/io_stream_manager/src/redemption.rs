use candid::{CandidType, Principal};
use serde::Deserialize;

use crate::{
    state::{Account, OperationSequence, StreamConfig, StructuralStakeState},
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
    PayoutOwed,
    PayoutSubmitted,
    PayoutSucceeded,
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
pub struct ClaimSnapshot {
    pub total_supply_e8s: u128,
    pub reserve_io_e8s: u128,
    pub excluded_io_balances: Vec<(Account, u128)>,
    pub claim_supply_e8s: u128,
    pub liquid_icp_e8s: u128,
    pub pooled_principal_e8s: u128,
    pub unwinding_net_backing_e8s: u128,
    pub transit_backing_e8s: u128,
    pub total_claim_backing_e8s: u128,
    pub nns_control_epoch: u64,
    pub nns_operation_sequence: u64,
    pub last_completed_pool_operation_sequence: Option<u64>,
    pub nns_fingerprint: Vec<u8>,
    pub pool_staking_account: Account,
    pub anchor_target_e8s: u128,
    pub anchor_available_e8s: u128,
    pub excluded_dynamic_surplus_e8s: u128,
    pub stream_control_epoch: u64,
    pub observation_fingerprint: Vec<u8>,
    pub io_fee_e8s: u128,
    pub icp_fee_e8s: u128,
}

impl Default for ClaimSnapshot {
    fn default() -> Self {
        let account = Account {
            owner: Principal::anonymous(),
            subaccount: None,
        };
        Self {
            total_supply_e8s: 0,
            reserve_io_e8s: 0,
            excluded_io_balances: Vec::new(),
            claim_supply_e8s: 0,
            liquid_icp_e8s: 0,
            pooled_principal_e8s: 0,
            unwinding_net_backing_e8s: 0,
            transit_backing_e8s: 0,
            total_claim_backing_e8s: 0,
            nns_control_epoch: 0,
            nns_operation_sequence: 0,
            last_completed_pool_operation_sequence: None,
            nns_fingerprint: Vec::new(),
            pool_staking_account: account,
            anchor_target_e8s: 0,
            anchor_available_e8s: 0,
            excluded_dynamic_surplus_e8s: 0,
            stream_control_epoch: 0,
            observation_fingerprint: Vec::new(),
            io_fee_e8s: 0,
            icp_fee_e8s: 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct StructuralStakeObservation {
    pub sns_neuron_id: Vec<u8>,
    pub staking_account: Account,
    pub state: StructuralStakeState,
    pub ledger_balance_e8s: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct FrozenRedemptionEconomics {
    pub total_supply_e8s: u128,
    pub reserve_io_e8s: u128,
    pub excluded_io_balances: Vec<(Account, u128)>,
    pub claim_supply_e8s: u128,
    pub liquid_icp_e8s: u128,
    pub total_claim_backing_e8s: u128,
    pub observation_fingerprint: Vec<u8>,
    pub io_fee_e8s: u128,
    pub icp_fee_e8s: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct PreparedRedemption {
    pub request_fingerprint: Vec<u8>,
    pub caller: Principal,
    pub account: Account,
    pub request: CanonicalRedeemRequestV1,
    pub prepared_at_nanos: u64,
    pub push_memo: Vec<u8>,
    pub reserve: Account,
    pub gross_icp_e8s: u128,
    pub net_icp_e8s: u128,
    pub snapshot: FrozenRedemptionEconomics,
}

impl PreparedRedemption {
    pub fn validate(&self, config: &StreamConfig) -> Result<(), String> {
        if self.caller == Principal::anonymous()
            || self.account != self.request.account(self.caller)
            || self.request_fingerprint != request_fingerprint(self.caller, &self.request)
            || self.request_fingerprint.len() != 32
            || self.push_memo
                != deterministic_memo(b"io-redemption-push-v1", self.caller, self.request.nonce)
            || !self.reserve.effective_eq(&config.io_reserve)?
            || self.prepared_at_nanos == 0
            || self.request.expires_at_nanos < self.prepared_at_nanos
            || self.snapshot.observation_fingerprint.len() != 32
            || self.snapshot.io_fee_e8s != config.expected_io_fee_e8s
            || self.snapshot.icp_fee_e8s != config.expected_icp_fee_e8s
            || self.gross_icp_e8s.checked_sub(self.snapshot.icp_fee_e8s) != Some(self.net_icp_e8s)
        {
            return Err("prepared push redemption is inconsistent".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct PushedRedemption {
    pub prepared: PreparedRedemption,
    pub io_block: u128,
    pub transfer_created_at_nanos: u64,
}

impl PushedRedemption {
    pub fn validate(&self, config: &StreamConfig) -> Result<(), String> {
        self.prepared.validate(config)?;
        if self.transfer_created_at_nanos < self.prepared.prepared_at_nanos
            || self.transfer_created_at_nanos > self.prepared.request.expires_at_nanos
        {
            return Err("proved push occurred outside its preparation window".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct RedemptionOperation {
    pub sequence: OperationSequence,
    pub pushed: PushedRedemption,
    pub icp_payout: Option<TransferAttempt>,
    pub phase: RedemptionPhase,
}

impl RedemptionOperation {
    pub fn validate(&self, config: &StreamConfig) -> Result<(), String> {
        self.pushed.validate(config)?;
        if let Some(payout) = &self.icp_payout {
            payout.validate()?;
            let OwnTransferIntent::Icrc1 {
                ledger,
                from_subaccount,
                to,
                amount,
                fee,
                memo,
                created_at_time,
            } = &payout.intent;
            let expected_source = config.liquid_icp.canonical()?;
            if *ledger != config.icp_ledger
                || *from_subaccount != expected_source.subaccount
                || !to.effective_eq(&self.pushed.prepared.account)?
                || *amount != self.pushed.prepared.net_icp_e8s
                || *fee != self.pushed.prepared.snapshot.icp_fee_e8s
                || *memo
                    != deterministic_memo(
                        b"io-redemption-pay-v1",
                        self.pushed.prepared.caller,
                        self.pushed.prepared.request.nonce,
                    )
                || *created_at_time < self.pushed.transfer_created_at_nanos
            {
                return Err("payout intent does not match the proved push".into());
            }
        }
        match self.phase {
            RedemptionPhase::PayoutOwed if self.icp_payout.is_some() => {
                return Err("owed payout already contains a transfer intent".into())
            }
            RedemptionPhase::PayoutSubmitted
                if !matches!(
                    self.icp_payout.as_ref().map(|attempt| &attempt.state),
                    Some(crate::transfer::TransferState::Submitted { .. })
                ) =>
            {
                return Err("submitted payout lacks exact submitted state".into())
            }
            RedemptionPhase::PayoutSucceeded
                if self
                    .icp_payout
                    .as_ref()
                    .and_then(|attempt| attempt.succeeded_block().ok())
                    .is_none() =>
            {
                return Err("successful payout lacks exact block proof".into())
            }
            _ => {}
        }
        Ok(())
    }
}

pub fn prepare(
    caller: Principal,
    request: CanonicalRedeemRequestV1,
    snapshot: ClaimSnapshot,
    config: &StreamConfig,
    now_nanos: u64,
) -> Result<PreparedRedemption, String> {
    let account = request.account(caller);
    if caller == Principal::anonymous()
        || request.io_amount_e8s < config.minimum_redemption_io_e8s
        || request.expires_at_nanos < now_nanos
        || request
            .expires_at_nanos
            .checked_sub(now_nanos)
            .is_none_or(|duration| duration > config.maximum_request_lifetime_nanos)
        || account.effective_eq(&config.io_reserve)?
        || config
            .nonredeemable_governance_io_accounts
            .iter()
            .any(|excluded| account.effective_eq(excluded).unwrap_or(false))
        || snapshot.io_fee_e8s > request.max_io_fee_e8s
        || snapshot.icp_fee_e8s > request.max_icp_fee_e8s
    {
        return Err("redemption preparation violates caller or launch bounds".into());
    }
    let quote = quote_for_amount(request.io_amount_e8s, &snapshot)?;
    if quote.net_icp < request.min_icp_out_e8s {
        return Err("minimum ICP output not met".into());
    }
    let nonce = request.nonce;
    let prepared = PreparedRedemption {
        request_fingerprint: request_fingerprint(caller, &request),
        caller,
        account,
        request,
        prepared_at_nanos: now_nanos,
        push_memo: deterministic_memo(b"io-redemption-push-v1", caller, nonce),
        reserve: config.io_reserve.clone(),
        gross_icp_e8s: quote.gross_icp,
        net_icp_e8s: quote.net_icp,
        snapshot: FrozenRedemptionEconomics {
            total_supply_e8s: snapshot.total_supply_e8s,
            reserve_io_e8s: snapshot.reserve_io_e8s,
            excluded_io_balances: snapshot.excluded_io_balances,
            claim_supply_e8s: snapshot.claim_supply_e8s,
            liquid_icp_e8s: snapshot.liquid_icp_e8s,
            total_claim_backing_e8s: snapshot.total_claim_backing_e8s,
            observation_fingerprint: snapshot.observation_fingerprint,
            io_fee_e8s: snapshot.io_fee_e8s,
            icp_fee_e8s: snapshot.icp_fee_e8s,
        },
    };
    prepared.validate(config)?;
    Ok(prepared)
}

fn quote_for_amount(
    amount: u128,
    snapshot: &ClaimSnapshot,
) -> Result<io_core_model::RedemptionQuote, String> {
    if io_core_model::claim_backing(io_core_model::Backing {
        liquid: snapshot.liquid_icp_e8s,
        pooled: snapshot.pooled_principal_e8s,
        unwinding: snapshot.unwinding_net_backing_e8s,
        transit: snapshot.transit_backing_e8s,
    })
    .map_err(|error| format!("claim backing failed: {error:?}"))?
        != snapshot.total_claim_backing_e8s
    {
        return Err("canonical total claim backing is inconsistent".into());
    }
    io_core_model::redemption_quote(
        io_core_model::EconomicState {
            backing: io_core_model::Backing {
                liquid: snapshot.liquid_icp_e8s,
                pooled: snapshot.pooled_principal_e8s,
                unwinding: snapshot.unwinding_net_backing_e8s,
                transit: snapshot.transit_backing_e8s,
            },
            claims: snapshot.claim_supply_e8s,
            active_backing: 0,
            active_reward: 0,
        },
        amount,
        snapshot.io_fee_e8s,
        snapshot.icp_fee_e8s,
    )
    .map_err(|error| format!("redemption quote failed: {error:?}"))
}

pub fn request_fingerprint(caller: Principal, request: &CanonicalRedeemRequestV1) -> Vec<u8> {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(b"io-canonical-push-redemption-v1\0");
    hasher.update(candid::encode_one((caller, request)).expect("redemption request must encode"));
    hasher.finalize().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(config: &StreamConfig, liquid: u128) -> ClaimSnapshot {
        ClaimSnapshot {
            total_supply_e8s: 2_000_000_000,
            reserve_io_e8s: 1_000_000_000,
            claim_supply_e8s: 1_000_000_000,
            liquid_icp_e8s: liquid,
            pooled_principal_e8s: 1_000_000_000u128.saturating_sub(liquid),
            total_claim_backing_e8s: 1_000_000_000,
            io_fee_e8s: config.expected_io_fee_e8s,
            icp_fee_e8s: config.expected_icp_fee_e8s,
            observation_fingerprint: vec![7; 32],
            pool_staking_account: config.liquid_icp.clone(),
            anchor_target_e8s: 1_000_000_000,
            anchor_available_e8s: 1_000_000_000,
            ..ClaimSnapshot::default()
        }
    }

    fn request(now: u64) -> CanonicalRedeemRequestV1 {
        CanonicalRedeemRequestV1 {
            effective_subaccount: [9; 32],
            io_amount_e8s: 100_000_000,
            min_icp_out_e8s: 99_990_000,
            max_io_fee_e8s: 10_000,
            max_icp_fee_e8s: 10_000,
            expires_at_nanos: now + 100,
            nonce: 4,
        }
    }

    #[test]
    fn preparation_freezes_an_exact_push_without_a_liquidity_gate() {
        let (_, state) = crate::state::tests::valid_state();
        let now = 1_000;
        let mut request = request(now);
        request.min_icp_out_e8s = 99_999_990;
        let prepared = prepare(
            candid::Principal::from_slice(&[33; 29]),
            request,
            snapshot(&state.config, 1),
            &state.config,
            now,
        )
        .unwrap();
        assert_eq!(prepared.gross_icp_e8s, 100_000_000);
        assert_eq!(prepared.net_icp_e8s, 99_999_990);
        assert_eq!(prepared.reserve, state.config.io_reserve);
        prepared.validate(&state.config).unwrap();
    }

    #[test]
    fn push_must_be_canonically_inside_the_preparation_window() {
        let (_, state) = crate::state::tests::valid_state();
        let now = 1_000;
        let caller = candid::Principal::from_slice(&[34; 29]);
        let prepared = prepare(
            caller,
            request(now),
            snapshot(&state.config, 0),
            &state.config,
            now,
        )
        .unwrap();
        let mut pushed = PushedRedemption {
            prepared,
            io_block: 77,
            transfer_created_at_nanos: now + 50,
        };
        pushed.validate(&state.config).unwrap();
        pushed.transfer_created_at_nanos = now + 101;
        assert!(pushed.validate(&state.config).is_err());
    }

    #[test]
    fn accepted_payout_intent_is_exactly_bound_to_the_proved_push() {
        let (_, state) = crate::state::tests::valid_state();
        let now = 1_000;
        let caller = candid::Principal::from_slice(&[35; 29]);
        let pushed = PushedRedemption {
            prepared: prepare(
                caller,
                request(now),
                snapshot(&state.config, 1_000_000_000),
                &state.config,
                now,
            )
            .unwrap(),
            io_block: 88,
            transfer_created_at_nanos: now,
        };
        let mut operation = RedemptionOperation {
            sequence: OperationSequence(1),
            pushed,
            icp_payout: None,
            phase: RedemptionPhase::PayoutOwed,
        };
        operation.validate(&state.config).unwrap();
        let prepared = &operation.pushed.prepared;
        operation.icp_payout = Some(
            TransferAttempt::prepared(OwnTransferIntent::Icrc1 {
                ledger: state.config.icp_ledger,
                from_subaccount: state.config.liquid_icp.canonical().unwrap().subaccount,
                to: prepared.account.clone(),
                amount: prepared.net_icp_e8s,
                fee: prepared.snapshot.icp_fee_e8s,
                memo: deterministic_memo(b"io-redemption-pay-v1", caller, 4),
                created_at_time: now,
            })
            .unwrap(),
        );
        operation.phase = RedemptionPhase::PayoutSubmitted;
        operation.icp_payout.as_mut().unwrap().state = crate::transfer::TransferState::Submitted {
            epoch: crate::state::DispatchEpoch(1),
            first_submitted_at: now,
            last_submitted_at: now,
        };
        operation.validate(&state.config).unwrap();
        let OwnTransferIntent::Icrc1 { amount, .. } =
            &mut operation.icp_payout.as_mut().unwrap().intent;
        *amount += 1;
        assert!(operation.validate(&state.config).is_err());
    }
}
