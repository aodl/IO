use candid::{CandidType, Nat, Principal};
use io_ledger_types::{
    map_icrc_transfer_result, Account, BlockIndex, IcrcTransferArg, LedgerBlock,
    LedgerOperationKind, LedgerTransferError, LedgerTransferRequest, LedgerTransferSuccess, Memo,
    Subaccount,
};
use serde::Deserialize;
use std::future::Future;
use std::pin::Pin;

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct LedgerTransaction {
    pub from: String,
    pub to: String,
    pub amount_e8s: u128,
    pub memo: String,
    pub block_index: u64,
    pub timestamp: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct TransferArgs {
    pub from: String,
    pub to: String,
    pub amount_e8s: u128,
    pub memo: String,
}

#[derive(Clone, Copy, Debug)]
pub struct MockLedgerCanisterClient {
    pub canister: Principal,
    pub fee_e8s: u128,
}

impl LedgerTransaction {
    pub fn into_boundary_block(self) -> LedgerBlock {
        LedgerBlock {
            block_index: BlockIndex(self.block_index),
            timestamp_nanos: self.timestamp,
            from: Some(mock_account(&self.from)),
            to: Some(mock_account(&self.to)),
            amount_e8s: self.amount_e8s,
            fee_e8s: None,
            memo: Some(Memo::from(self.memo)),
            operation_kind: LedgerOperationKind::Transfer,
        }
    }
}

pub async fn debug_get_transactions(canister: Principal) -> Result<Vec<LedgerTransaction>, String> {
    let response = ic_cdk::call::Call::bounded_wait(canister, "debug_get_transactions")
        .await
        .map_err(|err| format!("ledger transaction scan failed: {err:?}"))
        .and_then(|response| {
            response
                .candid_tuple::<(Vec<LedgerTransaction>,)>()
                .map_err(|err| format!("ledger transaction decode failed: {err:?}"))
        })?;
    Ok(response.0)
}

pub fn mock_subaccount(label: &str) -> Subaccount {
    let bytes = label.as_bytes();
    let mut subaccount = [0; 32];
    let len = bytes.len().min(31);
    subaccount[0] = len as u8;
    subaccount[1..=len].copy_from_slice(&bytes[..len]);
    Subaccount(subaccount)
}

pub fn mock_account(label: &str) -> Account {
    Account::new(Principal::anonymous(), Some(mock_subaccount(label)))
}

fn mock_label_from_subaccount(subaccount: &Subaccount) -> Option<String> {
    let len = subaccount.0[0] as usize;
    if len == 0 || len > 31 {
        return None;
    }
    std::str::from_utf8(&subaccount.0[1..=len])
        .ok()
        .map(ToString::to_string)
}

pub(crate) fn mock_label_from_account(account: &Account) -> String {
    account
        .subaccount
        .as_ref()
        .and_then(mock_label_from_subaccount)
        .unwrap_or_else(|| account.owner.to_text())
}

fn mock_transfer_args(request: LedgerTransferRequest) -> TransferArgs {
    TransferArgs {
        from: request
            .from_subaccount
            .as_ref()
            .and_then(mock_label_from_subaccount)
            .unwrap_or_else(|| "boundary_from_subaccount".to_string()),
        to: mock_label_from_account(&request.to),
        amount_e8s: request.amount_e8s,
        memo: request
            .memo
            .map(|memo| String::from_utf8_lossy(&memo.0).into_owned())
            .unwrap_or_default(),
    }
}

pub fn map_mock_transfer_result(
    result: Result<candid::Nat, io_ledger_types::IcrcTransferError>,
) -> Result<LedgerTransferSuccess, LedgerTransferError> {
    map_icrc_transfer_result(result)
}

impl io_ledger_types::LedgerTransferClient for MockLedgerCanisterClient {
    fn transfer<'a>(
        &'a self,
        request: LedgerTransferRequest,
    ) -> Pin<Box<dyn Future<Output = Result<LedgerTransferSuccess, LedgerTransferError>> + 'a>>
    {
        Box::pin(async move { transfer(self.canister, mock_transfer_args(request)).await })
    }

    fn fee<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = Result<u128, io_ledger_types::LedgerQueryError>> + 'a>> {
        Box::pin(async move {
            let response = ic_cdk::call::Call::bounded_wait(self.canister, "icrc1_fee")
                .await
                .map_err(
                    |err| io_ledger_types::LedgerQueryError::CanisterCallFailed {
                        method: "icrc1_fee".to_string(),
                        message: format!("{err:?}"),
                    },
                )?;
            let (fee,) = response.candid_tuple::<(Nat,)>().map_err(|err| {
                io_ledger_types::LedgerQueryError::DecodeError {
                    message: format!("{err:?}"),
                }
            })?;
            fee.0.to_str_radix(10).parse::<u128>().map_err(|err| {
                io_ledger_types::LedgerQueryError::DecodeError {
                    message: format!("ledger fee does not fit in u128: {err}"),
                }
            })
        })
    }
}

pub async fn transfer(
    canister: Principal,
    args: TransferArgs,
) -> Result<LedgerTransferSuccess, LedgerTransferError> {
    let response = ic_cdk::call::Call::bounded_wait(canister, "icrc1_transfer")
        .with_arg(IcrcTransferArg {
            from_subaccount: Some(mock_subaccount(&args.from).0.to_vec()),
            to: mock_account(&args.to).into(),
            amount: candid::Nat::from(args.amount_e8s),
            fee: None,
            memo: Some(args.memo.into_bytes()),
            created_at_time: None,
        })
        .await
        .map_err(|err| LedgerTransferError::CanisterCallFailed {
            method: "icrc1_transfer".to_string(),
            message: format!("{err:?}"),
        })
        .and_then(|response| {
            response
                .candid_tuple::<(Result<candid::Nat, io_ledger_types::IcrcTransferError>,)>()
                .map_err(|err| LedgerTransferError::DecodeError {
                    message: format!("{err:?}"),
                })
        })?;
    map_mock_transfer_result(response.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_transfer_errors_map_to_boundary_errors() {
        assert!(matches!(
            map_mock_transfer_result(Err(io_ledger_types::IcrcTransferError::InsufficientFunds {
                balance: candid::Nat::from(0_u64)
            })),
            Err(LedgerTransferError::InsufficientFunds { .. })
        ));
        assert!(matches!(
            map_mock_transfer_result(Err(io_ledger_types::IcrcTransferError::Duplicate {
                duplicate_of: candid::Nat::from(0_u64)
            })),
            Err(LedgerTransferError::Duplicate { .. })
        ));
        assert!(matches!(
            map_mock_transfer_result(Err(
                io_ledger_types::IcrcTransferError::TemporarilyUnavailable
            )),
            Err(LedgerTransferError::TemporarilyUnavailable)
        ));
    }

    #[test]
    fn mock_boundary_request_decodes_debug_account_labels() {
        let args = mock_transfer_args(LedgerTransferRequest {
            from_subaccount: Some(mock_subaccount("protocol_reserve")),
            to: mock_account("sns_neuron_10"),
            amount_e8s: 42,
            fee_e8s: None,
            memo: Some(Memo::from("reward")),
            created_at_time: None,
        });
        assert_eq!(args.from, "protocol_reserve");
        assert_eq!(args.to, "sns_neuron_10");
        assert_eq!(args.amount_e8s, 42);
        assert_eq!(args.memo, "reward");
    }

    #[test]
    fn configured_fee_remains_available_for_host_fixture_clients() {
        let client = MockLedgerCanisterClient {
            canister: Principal::anonymous(),
            fee_e8s: 10_000,
        };
        assert_eq!(client.fee_e8s, 10_000);
    }
}
