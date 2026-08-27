use candid::{CandidType, Principal};
#[cfg(not(target_family = "wasm"))]
use io_governance_types::NnsProductionManageNeuronResponse;
#[cfg(target_family = "wasm")]
use io_governance_types::{
    EmptyRecord, NnsClaimOrRefresh, NnsClaimOrRefreshBy, NnsManageNeuronCommandRequest,
    NnsNeuronIdOrSubaccount, NnsNeuronIdRecord, NnsProductionManageNeuronRequest,
    NnsProductionManageNeuronResponse,
};
#[cfg(target_family = "wasm")]
use io_ledger_types::{IcpTokens, IcpTransferArgs, IcpTransferError};
use serde::Deserialize;

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct TransferIcpArgs {
    pub ledger: Principal,
    pub from_subaccount: Option<Vec<u8>>,
    pub to: Vec<u8>,
    pub amount_e8s: u64,
    pub fee_e8s: u64,
    pub memo: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct RefreshNeuronArgs {
    pub governance: Principal,
    pub neuron_id: u64,
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub async fn transfer_icp(args: TransferIcpArgs) -> Result<u64, String> {
    #[cfg(target_family = "wasm")]
    {
        if let Some(subaccount) = &args.from_subaccount {
            if subaccount.len() != 32 {
                return Err(format!(
                    "test fixture subaccount must contain 32 bytes, got {}",
                    subaccount.len()
                ));
            }
        }
        let response = ic_cdk::call::Call::bounded_wait(args.ledger, "transfer")
            .with_arg(IcpTransferArgs {
                memo: args.memo,
                amount: IcpTokens {
                    e8s: args.amount_e8s,
                },
                fee: IcpTokens { e8s: args.fee_e8s },
                from_subaccount: args.from_subaccount,
                to: args.to,
                created_at_time: None,
            })
            .await
            .map_err(|error| format!("ICP ledger call failed: {error:?}"))?;
        let result = response
            .candid_tuple::<(Result<u64, IcpTransferError>,)>()
            .map_err(|error| format!("ICP ledger response decode failed: {error:?}"))?
            .0;
        result.map_err(|error| format!("ICP transfer rejected: {error:?}"))
    }
    #[cfg(not(target_family = "wasm"))]
    {
        let _ = args;
        Ok(0)
    }
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub async fn refresh_neuron(
    args: RefreshNeuronArgs,
) -> Result<NnsProductionManageNeuronResponse, String> {
    #[cfg(target_family = "wasm")]
    {
        let request = NnsProductionManageNeuronRequest {
            neuron_id_or_subaccount: Some(NnsNeuronIdOrSubaccount::NeuronId(NnsNeuronIdRecord {
                id: args.neuron_id,
            })),
            command: Some(NnsManageNeuronCommandRequest::ClaimOrRefresh(
                NnsClaimOrRefresh {
                    by: Some(NnsClaimOrRefreshBy::NeuronIdOrSubaccount(EmptyRecord {})),
                },
            )),
            id: None,
        };
        let response = ic_cdk::call::Call::bounded_wait(args.governance, "manage_neuron")
            .with_arg(request)
            .await
            .map_err(|error| format!("NNS Governance call failed: {error:?}"))?;
        response
            .candid::<NnsProductionManageNeuronResponse>()
            .map_err(|error| format!("NNS Governance response decode failed: {error:?}"))
    }
    #[cfg(not(target_family = "wasm"))]
    {
        let _ = args;
        Ok(NnsProductionManageNeuronResponse { command: None })
    }
}
