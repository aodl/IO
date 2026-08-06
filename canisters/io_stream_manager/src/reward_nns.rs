use candid::{CandidType, Principal};
use serde::Deserialize;
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CallError {
    Pending(String),
    Paused,
    Waiting(String),
    Invalid(String),
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType)]
struct SetTargetArgs {
    target_e8s: u128,
    generation: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType)]
struct PrepareMaturityArgs {
    cohort_generation: u64,
    captured_at_timestamp_seconds: u64,
    closes_at_timestamp_seconds: u64,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
enum MaturityProgress {
    Observed,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
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
    ImplementationIncomplete(String),
}

#[allow(clippy::enum_variant_names)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum TargetStatus {
    UnderTarget,
    AtTarget,
    AtTargetWithinUnwindTolerance,
    OverTarget,
}

pub fn target_fingerprint(generation: u64, target_e8s: u128) -> Vec<u8> {
    fingerprint(
        b"io-target-v1",
        &SetTargetArgs {
            target_e8s,
            generation,
        },
    )
}

pub async fn set_target(
    manager: Principal,
    generation: u64,
    target_e8s: u128,
) -> Result<TargetStatus, CallError> {
    let result: Result<TargetStatus, NnsError> =
        ic_cdk::call::Call::bounded_wait(manager, "set_two_week_target")
            .with_arg(SetTargetArgs {
                target_e8s,
                generation,
            })
            .await
            .map_err(|error| CallError::Pending(format!("NNS target call ambiguous: {error:?}")))?
            .candid()
            .map_err(|error| CallError::Invalid(format!("NNS target decode failed: {error:?}")))?;
    result.map_err(|error| classify("NNS target rejected", error))
}

pub fn maturity_fingerprint(
    generation: u64,
    captured_at_timestamp_seconds: u64,
    closes_at_timestamp_seconds: u64,
) -> Vec<u8> {
    fingerprint(
        b"io-maturity-v1",
        &maturity_args(
            generation,
            captured_at_timestamp_seconds,
            closes_at_timestamp_seconds,
        ),
    )
}

pub async fn prepare_maturity(
    manager: Principal,
    generation: u64,
    captured_at_timestamp_seconds: u64,
    closes_at_timestamp_seconds: u64,
) -> Result<(), CallError> {
    let result: Result<MaturityProgress, NnsError> =
        ic_cdk::call::Call::bounded_wait(manager, "prepare_two_week_maturity")
            .with_arg(maturity_args(
                generation,
                captured_at_timestamp_seconds,
                closes_at_timestamp_seconds,
            ))
            .await
            .map_err(|error| {
                CallError::Pending(format!("two-week maturity call ambiguous: {error:?}"))
            })?
            .candid()
            .map_err(|error| {
                CallError::Invalid(format!("two-week maturity decode failed: {error:?}"))
            })?;
    match result {
        Ok(MaturityProgress::Observed) => Ok(()),
        Err(error) => Err(classify("two-week maturity rejected", error)),
    }
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
        NnsError::Unauthorized
        | NnsError::Invalid(_)
        | NnsError::Stuck(_)
        | NnsError::ImplementationIncomplete(_) => {
            CallError::Invalid(format!("{context}: {error:?}"))
        }
    }
}

fn maturity_args(
    cohort_generation: u64,
    captured_at_timestamp_seconds: u64,
    closes_at_timestamp_seconds: u64,
) -> PrepareMaturityArgs {
    PrepareMaturityArgs {
        cohort_generation,
        captured_at_timestamp_seconds,
        closes_at_timestamp_seconds,
    }
}

fn fingerprint<T: CandidType>(domain: &[u8], args: &T) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update(candid::encode_one(args).expect("typed NNS request must encode"));
    hasher.finalize().to_vec()
}
