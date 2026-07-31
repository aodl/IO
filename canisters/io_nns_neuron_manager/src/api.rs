use candid::{CandidType, Principal};
use serde::Deserialize;

use crate::{
    jupiter,
    state::{self, Lifecycle, NnsOperation, TwoWeekTarget},
};

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum ApiError {
    Unauthorized,
    Inert,
    Paused,
    Busy,
    Invalid(String),
    Stuck(String),
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NotifyJupiterDepositArgs {
    pub block_index: u128,
    pub sequence: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct SetTwoWeekTargetArgs {
    pub target_e8s: u128,
    pub generation: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct Status {
    pub lifecycle: Lifecycle,
    pub active_operation: Option<String>,
    pub next_jupiter_sequence: u64,
    pub latest_target_generation: u64,
    pub has_pending_two_year_maturity: bool,
    pub has_pending_two_week_maturity: bool,
    pub has_pending_unwind: bool,
}

fn ready() -> Result<crate::state::NnsStateV1, ApiError> {
    let state = state::read();
    match state.lifecycle {
        Lifecycle::Ready => Ok(state),
        Lifecycle::Inert => Err(ApiError::Inert),
        Lifecycle::Paused => Err(ApiError::Paused),
    }
}

pub async fn notify_jupiter_deposit(
    caller: Principal,
    args: NotifyJupiterDepositArgs,
) -> Result<(), ApiError> {
    let state = ready()?;
    if caller != state.config.jupiter {
        return Err(ApiError::Unauthorized);
    }
    if state.active_operation.is_some() {
        return Err(ApiError::Busy);
    }
    if args.sequence != state.next_jupiter_sequence || args.block_index == 0 {
        return Err(ApiError::Invalid(
            "Jupiter sequence or exact block is invalid".into(),
        ));
    }
    // No state mutation is allowed until the canonical ICP block decoder proves
    // source, staging destination, amount, fee, and sequence memo.
    Err(ApiError::Stuck(format!(
        "exact ICP block {} awaits canonical decoder for memo {}",
        args.block_index,
        jupiter::sequence_memo(args.sequence)
    )))
}

pub fn set_two_week_target(caller: Principal, args: SetTwoWeekTargetArgs) -> Result<(), ApiError> {
    let mut state = ready()?;
    if caller != state.config.stream_manager {
        return Err(ApiError::Unauthorized);
    }
    let expected = state
        .latest_target_generation
        .checked_add(1)
        .ok_or_else(|| ApiError::Invalid("target generation overflow".into()))?;
    if args.generation != expected {
        return Err(ApiError::Invalid(format!(
            "expected target generation {expected}"
        )));
    }
    state.latest_two_week_target = Some(TwoWeekTarget {
        generation: args.generation,
        target_e8s: args.target_e8s,
    });
    state.latest_target_generation = args.generation;
    state::write(state);
    Ok(())
}

pub async fn resume() -> Result<(), ApiError> {
    match state::read().active_operation {
        None => Ok(()),
        Some(NnsOperation::JupiterDeposit { .. }) => Err(ApiError::Stuck(
            "Jupiter operation requires exact deterministic continuation".into(),
        )),
        Some(_) => Err(ApiError::Stuck(
            "NNS operation requires deterministic governance continuation".into(),
        )),
    }
}

pub async fn prove_active_transfer(_block_index: u128) -> Result<(), ApiError> {
    Err(ApiError::Invalid(
        "no Stuck active ledger transfer matches the supplied proof".into(),
    ))
}

pub fn get_status() -> Status {
    let state = state::read();
    Status {
        lifecycle: state.lifecycle,
        active_operation: state.active_operation.map(|operation| match operation {
            NnsOperation::JupiterDeposit { .. } => "JupiterDeposit".into(),
            NnsOperation::PoolTopUp { .. } => "PoolTopUp".into(),
            NnsOperation::PoolMergeBack { .. } => "PoolMergeBack".into(),
        }),
        next_jupiter_sequence: state.next_jupiter_sequence,
        latest_target_generation: state.latest_target_generation,
        has_pending_two_year_maturity: state.pending_two_year_maturity.is_some(),
        has_pending_two_week_maturity: state.pending_two_week_maturity.is_some(),
        has_pending_unwind: state.pending_unwind.is_some(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_updates_are_strict_and_coalesced() {
        let principal = Principal::from_slice(&[1]);
        let account = crate::state::Account {
            owner: principal,
            subaccount: None,
        };
        state::initialize(crate::state::NnsStateV1 {
            config: crate::state::NnsConfig {
                sns_governance: principal,
                stream_manager: principal,
                jupiter: principal,
                icp_ledger: principal,
                nns_governance: principal,
                two_year_neuron_id: 1,
                two_week_neuron_id: 2,
                jupiter_account: account.clone(),
                staging_account: account.clone(),
                operational_fee_account: account.clone(),
                stream_liquid_account: account,
                expected_icp_fee_e8s: 10_000,
            },
            lifecycle: Lifecycle::Ready,
            active_operation: None,
            next_jupiter_sequence: 0,
            latest_two_week_target: None,
            latest_target_generation: 0,
            pending_two_year_maturity: None,
            pending_two_week_maturity: None,
            pending_unwind: None,
        })
        .unwrap();
        set_two_week_target(
            principal,
            SetTwoWeekTargetArgs {
                target_e8s: 10,
                generation: 1,
            },
        )
        .unwrap();
        set_two_week_target(
            principal,
            SetTwoWeekTargetArgs {
                target_e8s: 20,
                generation: 2,
            },
        )
        .unwrap();
        let state = state::read();
        assert_eq!(state.latest_two_week_target.unwrap().target_e8s, 20);
    }
}
