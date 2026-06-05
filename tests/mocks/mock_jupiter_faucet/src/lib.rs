use candid::{CandidType, Principal};
use serde::Deserialize;

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct TransferArgs {
    pub from: String,
    pub to: String,
    pub amount_e8s: u128,
    pub memo: String,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct SendArgs {
    pub ledger_principal_text: String,
    pub from: String,
    pub to: String,
    pub amount_e8s: u128,
    pub memo: String,
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub async fn debug_send_icp(args: SendArgs) -> Result<u64, String> {
    let ledger = Principal::from_text(&args.ledger_principal_text)
        .map_err(|err| format!("invalid ledger principal: {err}"))?;
    #[cfg(target_family = "wasm")]
    {
        let transfer = TransferArgs {
            from: args.from,
            to: args.to,
            amount_e8s: args.amount_e8s,
            memo: args.memo,
        };
        let response = ic_cdk::call::Call::bounded_wait(ledger, "icrc1_transfer")
            .with_arg(transfer)
            .await
            .map_err(|err| format!("ledger call failed: {err:?}"))?;
        response
            .candid_tuple::<(Result<u64, String>,)>()
            .map_err(|err| format!("ledger decode failed: {err:?}"))?
            .0
    }
    #[cfg(not(target_family = "wasm"))]
    {
        let _ = ledger;
        Ok(0)
    }
}
