#[cfg(target_family = "wasm")]
use candid::Nat;
use candid::{CandidType, Principal};
#[cfg(target_family = "wasm")]
use io_ledger_types::{
    map_icrc_transfer_result, Account, IcrcTransferArg, IcrcTransferError, Subaccount,
};
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
        let transfer = IcrcTransferArg {
            from_subaccount: Some(mock_subaccount(&args.from).0.to_vec()),
            to: mock_account(&args.to).into(),
            amount: Nat::from(args.amount_e8s),
            fee: None,
            memo: Some(args.memo.into_bytes()),
            created_at_time: None,
        };
        let response = ic_cdk::call::Call::bounded_wait(ledger, "icrc1_transfer")
            .with_arg(transfer)
            .await
            .map_err(|err| format!("ledger call failed: {err:?}"))?;
        let result = response
            .candid_tuple::<(Result<Nat, IcrcTransferError>,)>()
            .map_err(|err| format!("ledger decode failed: {err:?}"))?
            .0;
        map_icrc_transfer_result(result)
            .map(|success| success.block_index.0)
            .map_err(|err| format!("{err:?}"))
    }
    #[cfg(not(target_family = "wasm"))]
    {
        let _ = ledger;
        Ok(0)
    }
}

#[cfg(target_family = "wasm")]
fn mock_subaccount(label: &str) -> Subaccount {
    let bytes = label.as_bytes();
    let mut subaccount = [0; 32];
    let len = bytes.len().min(31);
    subaccount[0] = len as u8;
    subaccount[1..=len].copy_from_slice(&bytes[..len]);
    Subaccount(subaccount)
}

#[cfg(target_family = "wasm")]
fn mock_account(label: &str) -> Account {
    Account::new(Principal::anonymous(), Some(mock_subaccount(label)))
}
