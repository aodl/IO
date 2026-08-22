use candid::{CandidType, Principal};
use serde::Deserialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum BackingNotReadyReason {
    Paused,
    Busy,
    ReconciliationPending,
    BelowThreshold,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CallError {
    Pending(String),
    Paused,
    Waiting(String),
    Invalid(String),
}

#[derive(CandidType)]
struct PrepareMaturityArgs {
    entitlement_batch_generation: u64,
    target_e8s: u128,
}

#[derive(CandidType, Deserialize)]
enum MaturityProgress {
    Observed,
}

#[derive(Debug, CandidType, Deserialize)]
enum NnsError {
    Unauthorized,
    Paused,
    Busy,
    Invalid(String),
    Pending(String),
    Stuck(String),
    BelowMaturityThreshold {
        remaining_e8s: u64,
        minimum_e8s: u64,
    },
}

pub async fn prepare_maturity(
    manager: Principal,
    generation: u64,
    target_e8s: u128,
) -> Result<(), CallError> {
    nns_call::<_, MaturityProgress>(
        manager,
        "prepare_two_week_maturity",
        PrepareMaturityArgs {
            entitlement_batch_generation: generation,
            target_e8s,
        },
    )
    .await
    .map(|MaturityProgress::Observed| ())
}

async fn nns_call<A, R>(manager: Principal, method: &str, arg: A) -> Result<R, CallError>
where
    A: CandidType,
    R: CandidType + for<'de> Deserialize<'de>,
{
    let result: Result<R, NnsError> = ic_cdk::call::Call::bounded_wait(manager, method)
        .with_arg(arg)
        .await
        .map_err(|error| CallError::Pending(format!("NNS {method} call failed: {error:?}")))?
        .candid()
        .map_err(|error| CallError::Invalid(format!("NNS {method} decode failed: {error:?}")))?;
    result.map_err(|error| classify(method, error))
}

fn classify(context: &str, error: NnsError) -> CallError {
    match error {
        NnsError::Busy | NnsError::Pending(_) => {
            CallError::Pending(format!("{context}: {error:?}"))
        }
        NnsError::Paused => CallError::Paused,
        NnsError::BelowMaturityThreshold { .. } => {
            CallError::Waiting(format!("{context}: {error:?}"))
        }
        NnsError::Unauthorized | NnsError::Invalid(_) | NnsError::Stuck(_) => {
            CallError::Invalid(format!("{context}: {error:?}"))
        }
    }
}
