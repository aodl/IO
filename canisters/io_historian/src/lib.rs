use candid::CandidType;
use serde::Deserialize;

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct ObservedLedgerFlow {
    pub ledger_principal_text: String,
    pub block_index: u64,
    pub amount_e8s: u128,
    pub memo: Option<Vec<u8>>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct ObservedCanisterStatus {
    pub canister_principal_text: String,
    pub module_hash_hex: Option<String>,
    pub cycles_balance: Option<u128>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct ObservedGovernanceSnapshot {
    pub governance_canister_principal_text: String,
    pub observed_neuron_count: u64,
    pub observed_proposal_count: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct PublicStatus {
    pub version: String,
    pub model: String,
    pub observed_ledger_flow_count: u64,
    pub observed_canister_status_count: u64,
    pub observed_governance_snapshot_count: u64,
}

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn get_public_status() -> PublicStatus {
    PublicStatus {
        version: version().to_string(),
        model: "external-observation-placeholder".to_string(),
        observed_ledger_flow_count: 0,
        observed_canister_status_count: 0,
        observed_governance_snapshot_count: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_exists() {
        assert!(!super::version().is_empty());
    }

    #[test]
    fn public_status_is_observation_shaped_and_serializable() {
        let status = get_public_status();
        assert_eq!(status.observed_ledger_flow_count, 0);
        assert!(status.model.contains("external-observation"));
        candid::encode_one(status).unwrap();
    }

    #[test]
    fn placeholder_observation_types_are_serializable() {
        let flow = ObservedLedgerFlow {
            ledger_principal_text: "ryjl3-tyaaa-aaaaa-aaaba-cai".to_string(),
            block_index: 1,
            amount_e8s: 100,
            memo: Some(vec![1, 2, 3]),
        };
        let status = ObservedCanisterStatus {
            canister_principal_text: "oae4c-3iaaa-aaaar-qb5qq-cai".to_string(),
            module_hash_hex: None,
            cycles_balance: Some(1_000),
        };
        let governance = ObservedGovernanceSnapshot {
            governance_canister_principal_text: "rrkah-fqaaa-aaaaa-aaaaq-cai".to_string(),
            observed_neuron_count: 0,
            observed_proposal_count: 0,
        };
        candid::encode_args((flow, status, governance)).unwrap();
    }
}
