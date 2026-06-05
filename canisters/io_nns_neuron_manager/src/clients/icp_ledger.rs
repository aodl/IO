use candid::{CandidType, Principal};
use serde::Deserialize;

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct TransferArgs {
    pub from: String,
    pub to: String,
    pub amount_e8s: u128,
    pub memo: String,
}

pub async fn transfer(canister: Principal, args: TransferArgs) -> Result<u64, String> {
    let response = ic_cdk::call::Call::bounded_wait(canister, "icrc1_transfer")
        .with_arg(args)
        .await
        .map_err(|err| format!("ledger transfer call failed: {err:?}"))
        .and_then(|response| {
            response
                .candid_tuple::<(Result<u64, String>,)>()
                .map_err(|err| format!("ledger transfer decode failed: {err:?}"))
        })?;
    response.0
}
