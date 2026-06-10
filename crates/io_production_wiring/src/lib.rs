use candid::{CandidType, Principal};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

pub const ICP_LEDGER_PRINCIPAL: &str = "ryjl3-tyaaa-aaaaa-aaaba-cai";
pub const ICP_INDEX_PRINCIPAL: &str = "qhbym-qaaaa-aaaaa-aaafq-cai";
pub const NNS_GOVERNANCE_PRINCIPAL: &str = "rrkah-fqaaa-aaaaa-aaaaq-cai";
pub const PROTECTED_IO_NEURON_OWNER_CANISTER: &str = "oae4c-3iaaa-aaaar-qb5qq-cai";
pub const PROTECTED_IO_NNS_NEURON_ID: u64 = 6_345_890_886_899_317_159;
pub const PRODUCTION_IO_STREAM_MANAGER_CANISTER_ID: &str = "thset-pqaaa-aaaar-qb7wa-cai";
pub const PRODUCTION_IO_NNS_NEURON_MANAGER_CANISTER_ID: &str = "tatch-ciaaa-aaaar-qb7wq-cai";
pub const PRODUCTION_IO_HISTORIAN_CANISTER_ID: &str = "tjqj3-uaaaa-aaaar-qb7xa-cai";
pub const PRODUCTION_FRONTEND_CANISTER_ID: &str = "torpp-zyaaa-aaaar-qb7xq-cai";
pub const DEV_MAINNET_FRONTEND_CANISTER_ID: &str = concat!("6h2pa-", "qiaaa-aaaao-qp4fa-cai");
pub const DEV_MAINNET_HISTORIAN_CANISTER_ID: &str = concat!("yo47z-", "piaaa-aaaac-qg3xa-cai");
pub const INTERNET_IDENTITY_CANISTER_ID: &str = "rdmx6-jaaaa-aaaaa-aaadq-cai";
pub const NNS_DAPP_CANISTER_ID: &str = "qoctq-giaaa-aaaaa-aaaea-cai";
pub const TEMPLATE_SNS_ROOT_PLACEHOLDER: &str = "qaa6y-5yaaa-aaaaa-aaafa-cai";
pub const TEMPLATE_SNS_GOVERNANCE_PLACEHOLDER: &str = "r7inp-6aaaa-aaaaa-aaabq-cai";
pub const TEMPLATE_SNS_LEDGER_PLACEHOLDER: &str = "qjdve-lqaaa-aaaaa-aaaeq-cai";
pub const TEMPLATE_SNS_INDEX_PLACEHOLDER: &str = "renrk-eyaaa-aaaaa-aaada-cai";
pub const ICP_TRANSFER_FEE_E8S: u128 = 10_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum WiringMode {
    Mock,
    Local,
    DryRun,
    ProductionPlanned,
    ProductionActive,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum IoLedgerRole {
    IoTestNonCanonical,
    FutureCanonicalSnsIo,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct PrincipalWiring {
    pub icp_ledger_principal_text: Option<String>,
    pub icp_index_principal_text: Option<String>,
    pub nns_governance_principal_text: Option<String>,
    pub nns_ledger_principal_text: Option<String>,
    pub nns_index_principal_text: Option<String>,
    pub sns_root_principal_text: Option<String>,
    pub sns_governance_principal_text: Option<String>,
    pub sns_ledger_principal_text: Option<String>,
    pub sns_index_principal_text: Option<String>,
    pub io_ledger_principal_text: Option<String>,
    pub io_index_principal_text: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct FeePolicyWiring {
    pub icp_transfer_fee_e8s: Option<u128>,
    pub io_ledger_transfer_fee_e8s: Option<u128>,
    pub tiny_value_policy_max_fee_e8s: Option<u128>,
    pub allow_zero_fees_for_mock_or_local: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct ProtectedReferences {
    pub neuron_owner_canister_principal_text: Option<String>,
    pub io_nns_neuron_id: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct DeploymentTargets {
    pub io_stream_manager_principal_text: Option<String>,
    pub io_nns_neuron_manager_principal_text: Option<String>,
    pub mutation_target_principal_texts: Vec<String>,
    pub mutation_target_nns_neuron_ids: Vec<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct ProductionWiringConfig {
    pub mode: WiringMode,
    pub io_ledger_role: IoLedgerRole,
    pub fixture_marked: bool,
    pub principals: PrincipalWiring,
    pub fee_policy: FeePolicyWiring,
    pub protected: ProtectedReferences,
    pub deployment_targets: DeploymentTargets,
}

impl ProductionWiringConfig {
    pub fn validate(&self) -> Result<(), WiringValidationError> {
        validate_config(self)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WiringValidationError {
    ProductionActiveUnavailable,
    InvalidPrincipal {
        field: String,
        value: String,
    },
    AnonymousPrincipal {
        field: String,
    },
    ManagementCanisterPrincipal {
        field: String,
    },
    DuplicateIncompatiblePrincipal {
        first_field: String,
        second_field: String,
        principal: String,
    },
    MissingRequiredPrincipal {
        field: String,
    },
    MockOrLocalPrincipalInProduction {
        field: String,
        value: String,
    },
    ProductionPrincipalInMockOrLocal {
        field: String,
        value: String,
    },
    DevMainnetCanisterInProduction {
        field: String,
        value: String,
    },
    ProductionIoCanisterIdMismatch {
        field: String,
        actual: Option<String>,
        expected: String,
    },
    IoTestLabelledCanonical,
    MissingSnsGroup,
    IncompleteSnsGroup {
        field: String,
    },
    IndexWithoutLedger {
        index_field: String,
        ledger_field: String,
    },
    LedgerWithoutIndex {
        ledger_field: String,
        index_field: String,
    },
    MissingFee {
        field: String,
    },
    ZeroProductionFee {
        field: String,
    },
    FeeExceedsTinyValuePolicy {
        fee_field: String,
        fee_e8s: u128,
        max_fee_e8s: u128,
    },
    NnsGovernancePrincipalMismatch {
        actual: String,
    },
    IcpLedgerPrincipalMismatch {
        actual: String,
    },
    IcpIndexPrincipalMismatch {
        actual: String,
    },
    ProtectedCanisterAsTarget {
        field: String,
    },
    ProtectedNeuronAsTarget {
        field: String,
    },
    LivePhase1CanisterAsValueMovingTarget {
        field: String,
        value: String,
    },
    UnrelatedSystemCanisterAsValueMovingTarget {
        field: String,
        value: String,
    },
}

pub fn validate_config(config: &ProductionWiringConfig) -> Result<(), WiringValidationError> {
    if config.mode == WiringMode::ProductionActive {
        return Err(WiringValidationError::ProductionActiveUnavailable);
    }

    let principals = principal_fields(&config.principals);
    for (field, value) in &principals {
        validate_principal_text(field, value)?;
        validate_mode_principal(config.mode, config.fixture_marked, field, value)?;
    }
    validate_no_duplicate_incompatible_principals(&principals)?;
    validate_ledger_index_pairs(config)?;
    validate_role_labels(config)?;
    validate_required_production_principals(config)?;
    validate_known_mainnet_roles(config)?;
    validate_fee_policy(config)?;
    validate_protected_references(config)?;
    validate_io_owned_production_targets(config)?;
    Ok(())
}

fn validate_principal_text(field: &str, value: &str) -> Result<(), WiringValidationError> {
    let principal =
        Principal::from_text(value).map_err(|_| WiringValidationError::InvalidPrincipal {
            field: field.to_string(),
            value: value.to_string(),
        })?;
    if principal == Principal::anonymous() {
        return Err(WiringValidationError::AnonymousPrincipal {
            field: field.to_string(),
        });
    }
    if principal == Principal::management_canister() {
        return Err(WiringValidationError::ManagementCanisterPrincipal {
            field: field.to_string(),
        });
    }
    Ok(())
}

fn validate_mode_principal(
    mode: WiringMode,
    fixture_marked: bool,
    field: &str,
    value: &str,
) -> Result<(), WiringValidationError> {
    if matches!(mode, WiringMode::DryRun | WiringMode::ProductionPlanned)
        && is_known_mock_or_local_principal(value)
    {
        return Err(WiringValidationError::MockOrLocalPrincipalInProduction {
            field: field.to_string(),
            value: value.to_string(),
        });
    }
    if matches!(mode, WiringMode::DryRun | WiringMode::ProductionPlanned)
        && is_dev_mainnet_canister(value)
    {
        return Err(WiringValidationError::DevMainnetCanisterInProduction {
            field: field.to_string(),
            value: value.to_string(),
        });
    }
    if matches!(mode, WiringMode::Mock | WiringMode::Local)
        && !fixture_marked
        && is_known_production_principal(value)
    {
        return Err(WiringValidationError::ProductionPrincipalInMockOrLocal {
            field: field.to_string(),
            value: value.to_string(),
        });
    }
    Ok(())
}

fn validate_no_duplicate_incompatible_principals(
    principals: &[(String, String)],
) -> Result<(), WiringValidationError> {
    let mut seen = BTreeMap::<String, String>::new();
    for (field, principal) in principals {
        if let Some(first_field) = seen.get(principal) {
            if !principal_roles_compatible(first_field, field) {
                return Err(WiringValidationError::DuplicateIncompatiblePrincipal {
                    first_field: first_field.clone(),
                    second_field: field.clone(),
                    principal: principal.clone(),
                });
            }
        } else {
            seen.insert(principal.clone(), field.clone());
        }
    }
    Ok(())
}

fn principal_roles_compatible(first: &str, second: &str) -> bool {
    matches!(
        (first, second),
        ("icp_ledger_principal_text", "nns_ledger_principal_text")
            | ("nns_ledger_principal_text", "icp_ledger_principal_text")
            | ("icp_index_principal_text", "nns_index_principal_text")
            | ("nns_index_principal_text", "icp_index_principal_text")
            | ("sns_ledger_principal_text", "io_ledger_principal_text")
            | ("io_ledger_principal_text", "sns_ledger_principal_text")
            | ("sns_index_principal_text", "io_index_principal_text")
            | ("io_index_principal_text", "sns_index_principal_text")
    )
}

fn validate_ledger_index_pairs(
    config: &ProductionWiringConfig,
) -> Result<(), WiringValidationError> {
    require_pair(
        "icp_ledger_principal_text",
        &config.principals.icp_ledger_principal_text,
        "icp_index_principal_text",
        &config.principals.icp_index_principal_text,
    )?;
    require_pair(
        "io_ledger_principal_text",
        &config.principals.io_ledger_principal_text,
        "io_index_principal_text",
        &config.principals.io_index_principal_text,
    )?;
    require_pair(
        "sns_ledger_principal_text",
        &config.principals.sns_ledger_principal_text,
        "sns_index_principal_text",
        &config.principals.sns_index_principal_text,
    )?;
    require_pair(
        "nns_ledger_principal_text",
        &config.principals.nns_ledger_principal_text,
        "nns_index_principal_text",
        &config.principals.nns_index_principal_text,
    )
}

fn require_pair(
    ledger_field: &str,
    ledger: &Option<String>,
    index_field: &str,
    index: &Option<String>,
) -> Result<(), WiringValidationError> {
    match (ledger, index) {
        (Some(_), None) => Err(WiringValidationError::LedgerWithoutIndex {
            ledger_field: ledger_field.to_string(),
            index_field: index_field.to_string(),
        }),
        (None, Some(_)) => Err(WiringValidationError::IndexWithoutLedger {
            index_field: index_field.to_string(),
            ledger_field: ledger_field.to_string(),
        }),
        _ => Ok(()),
    }
}

fn validate_role_labels(config: &ProductionWiringConfig) -> Result<(), WiringValidationError> {
    if config.io_ledger_role == IoLedgerRole::FutureCanonicalSnsIo
        && (same_optional(
            &config.principals.io_ledger_principal_text,
            &config.principals.sns_ledger_principal_text,
        ) || same_optional(
            &config.principals.io_index_principal_text,
            &config.principals.sns_index_principal_text,
        ))
    {
        return Ok(());
    }
    if config.io_ledger_role == IoLedgerRole::FutureCanonicalSnsIo
        && has_io_test_label(&config.principals)
    {
        return Err(WiringValidationError::IoTestLabelledCanonical);
    }
    if config.io_ledger_role == IoLedgerRole::IoTestNonCanonical
        && matches!(
            config.mode,
            WiringMode::DryRun | WiringMode::ProductionPlanned
        )
    {
        return Err(WiringValidationError::IoTestLabelledCanonical);
    }
    Ok(())
}

fn validate_required_production_principals(
    config: &ProductionWiringConfig,
) -> Result<(), WiringValidationError> {
    if !matches!(
        config.mode,
        WiringMode::DryRun | WiringMode::ProductionPlanned
    ) {
        return Ok(());
    }
    for (field, value) in principal_fields(&config.principals) {
        if field.starts_with("nns_") && field != "nns_governance_principal_text" {
            continue;
        }
        if value.trim().is_empty() {
            return Err(WiringValidationError::MissingRequiredPrincipal { field });
        }
    }
    let sns_fields = [
        (
            "sns_root_principal_text",
            &config.principals.sns_root_principal_text,
        ),
        (
            "sns_governance_principal_text",
            &config.principals.sns_governance_principal_text,
        ),
        (
            "sns_ledger_principal_text",
            &config.principals.sns_ledger_principal_text,
        ),
        (
            "sns_index_principal_text",
            &config.principals.sns_index_principal_text,
        ),
    ];
    let present = sns_fields
        .iter()
        .filter(|(_, value)| value.is_some())
        .count();
    if present == 0 {
        return Err(WiringValidationError::MissingSnsGroup);
    }
    if present != sns_fields.len() {
        let (field, _) = sns_fields
            .iter()
            .find(|(_, value)| value.is_none())
            .expect("missing SNS field");
        return Err(WiringValidationError::IncompleteSnsGroup {
            field: (*field).to_string(),
        });
    }
    Ok(())
}

fn validate_known_mainnet_roles(
    config: &ProductionWiringConfig,
) -> Result<(), WiringValidationError> {
    if !matches!(
        config.mode,
        WiringMode::DryRun | WiringMode::ProductionPlanned
    ) {
        return Ok(());
    }
    require_known(
        &config.principals.icp_ledger_principal_text,
        ICP_LEDGER_PRINCIPAL,
        |actual| WiringValidationError::IcpLedgerPrincipalMismatch { actual },
    )?;
    require_known(
        &config.principals.icp_index_principal_text,
        ICP_INDEX_PRINCIPAL,
        |actual| WiringValidationError::IcpIndexPrincipalMismatch { actual },
    )?;
    require_known(
        &config.principals.nns_governance_principal_text,
        NNS_GOVERNANCE_PRINCIPAL,
        |actual| WiringValidationError::NnsGovernancePrincipalMismatch { actual },
    )
}

fn require_known(
    value: &Option<String>,
    expected: &str,
    err: impl Fn(String) -> WiringValidationError,
) -> Result<(), WiringValidationError> {
    let actual =
        value
            .as_deref()
            .ok_or_else(|| WiringValidationError::MissingRequiredPrincipal {
                field: expected.to_string(),
            })?;
    if actual != expected {
        return Err(err(actual.to_string()));
    }
    Ok(())
}

fn validate_io_owned_production_targets(
    config: &ProductionWiringConfig,
) -> Result<(), WiringValidationError> {
    if !matches!(
        config.mode,
        WiringMode::DryRun | WiringMode::ProductionPlanned
    ) {
        return Ok(());
    }
    require_exact_io_target(
        "deployment_targets.io_stream_manager",
        &config.deployment_targets.io_stream_manager_principal_text,
        PRODUCTION_IO_STREAM_MANAGER_CANISTER_ID,
    )?;
    require_exact_io_target(
        "deployment_targets.io_nns_neuron_manager",
        &config
            .deployment_targets
            .io_nns_neuron_manager_principal_text,
        PRODUCTION_IO_NNS_NEURON_MANAGER_CANISTER_ID,
    )
}

fn require_exact_io_target(
    field: &str,
    actual: &Option<String>,
    expected: &str,
) -> Result<(), WiringValidationError> {
    if actual.as_deref() != Some(expected) {
        return Err(WiringValidationError::ProductionIoCanisterIdMismatch {
            field: field.to_string(),
            actual: actual.clone(),
            expected: expected.to_string(),
        });
    }
    Ok(())
}

fn validate_fee_policy(config: &ProductionWiringConfig) -> Result<(), WiringValidationError> {
    let production = matches!(
        config.mode,
        WiringMode::DryRun | WiringMode::ProductionPlanned
    );
    validate_fee(
        "icp_transfer_fee_e8s",
        config.fee_policy.icp_transfer_fee_e8s,
        production,
        config.fee_policy.allow_zero_fees_for_mock_or_local,
        config.fee_policy.tiny_value_policy_max_fee_e8s,
    )?;
    validate_fee(
        "io_ledger_transfer_fee_e8s",
        config.fee_policy.io_ledger_transfer_fee_e8s,
        production,
        config.fee_policy.allow_zero_fees_for_mock_or_local,
        config.fee_policy.tiny_value_policy_max_fee_e8s,
    )
}

fn validate_fee(
    field: &str,
    value: Option<u128>,
    production: bool,
    allow_zero_local: bool,
    tiny_value_policy_max_fee_e8s: Option<u128>,
) -> Result<(), WiringValidationError> {
    let Some(fee) = value else {
        return if production {
            Err(WiringValidationError::MissingFee {
                field: field.to_string(),
            })
        } else {
            Ok(())
        };
    };
    if fee == 0 && (production || !allow_zero_local) {
        return Err(WiringValidationError::ZeroProductionFee {
            field: field.to_string(),
        });
    }
    if let Some(max_fee_e8s) = tiny_value_policy_max_fee_e8s {
        if fee > max_fee_e8s {
            return Err(WiringValidationError::FeeExceedsTinyValuePolicy {
                fee_field: field.to_string(),
                fee_e8s: fee,
                max_fee_e8s,
            });
        }
    }
    Ok(())
}

fn validate_protected_references(
    config: &ProductionWiringConfig,
) -> Result<(), WiringValidationError> {
    if let Some(owner) = &config.protected.neuron_owner_canister_principal_text {
        validate_principal_text("protected.neuron_owner_canister_principal_text", owner)?;
        if owner != PROTECTED_IO_NEURON_OWNER_CANISTER {
            return Err(WiringValidationError::InvalidPrincipal {
                field: "protected.neuron_owner_canister_principal_text".to_string(),
                value: owner.clone(),
            });
        }
    }
    if config.protected.io_nns_neuron_id != Some(PROTECTED_IO_NNS_NEURON_ID) {
        return Err(WiringValidationError::ProtectedNeuronAsTarget {
            field: "protected.io_nns_neuron_id".to_string(),
        });
    }
    let targets = deployment_target_fields(&config.deployment_targets);
    for (field, value) in targets {
        validate_principal_text(&field, &value)?;
        if value == PROTECTED_IO_NEURON_OWNER_CANISTER {
            return Err(WiringValidationError::ProtectedCanisterAsTarget { field });
        }
        if is_dev_mainnet_canister(&value) {
            return Err(
                WiringValidationError::LivePhase1CanisterAsValueMovingTarget { field, value },
            );
        }
        if is_known_unrelated_system_canister(&value) {
            return Err(
                WiringValidationError::UnrelatedSystemCanisterAsValueMovingTarget { field, value },
            );
        }
    }
    for neuron_id in &config.deployment_targets.mutation_target_nns_neuron_ids {
        if *neuron_id == PROTECTED_IO_NNS_NEURON_ID {
            return Err(WiringValidationError::ProtectedNeuronAsTarget {
                field: "deployment_targets.mutation_target_nns_neuron_ids".to_string(),
            });
        }
    }
    Ok(())
}

fn principal_fields(principals: &PrincipalWiring) -> Vec<(String, String)> {
    [
        (
            "icp_ledger_principal_text",
            &principals.icp_ledger_principal_text,
        ),
        (
            "icp_index_principal_text",
            &principals.icp_index_principal_text,
        ),
        (
            "nns_governance_principal_text",
            &principals.nns_governance_principal_text,
        ),
        (
            "nns_ledger_principal_text",
            &principals.nns_ledger_principal_text,
        ),
        (
            "nns_index_principal_text",
            &principals.nns_index_principal_text,
        ),
        (
            "sns_root_principal_text",
            &principals.sns_root_principal_text,
        ),
        (
            "sns_governance_principal_text",
            &principals.sns_governance_principal_text,
        ),
        (
            "sns_ledger_principal_text",
            &principals.sns_ledger_principal_text,
        ),
        (
            "sns_index_principal_text",
            &principals.sns_index_principal_text,
        ),
        (
            "io_ledger_principal_text",
            &principals.io_ledger_principal_text,
        ),
        (
            "io_index_principal_text",
            &principals.io_index_principal_text,
        ),
    ]
    .into_iter()
    .filter_map(|(field, value)| value.clone().map(|value| (field.to_string(), value)))
    .collect()
}

fn deployment_target_fields(targets: &DeploymentTargets) -> Vec<(String, String)> {
    let mut fields = Vec::new();
    if let Some(value) = &targets.io_stream_manager_principal_text {
        fields.push((
            "deployment_targets.io_stream_manager".to_string(),
            value.clone(),
        ));
    }
    if let Some(value) = &targets.io_nns_neuron_manager_principal_text {
        fields.push((
            "deployment_targets.io_nns_neuron_manager".to_string(),
            value.clone(),
        ));
    }
    for value in &targets.mutation_target_principal_texts {
        fields.push((
            "deployment_targets.mutation_targets".to_string(),
            value.clone(),
        ));
    }
    fields
}

fn same_optional(first: &Option<String>, second: &Option<String>) -> bool {
    first.is_some() && second.is_some() && first == second
}

fn has_io_test_label(principals: &PrincipalWiring) -> bool {
    [
        &principals.io_ledger_principal_text,
        &principals.io_index_principal_text,
    ]
    .into_iter()
    .flatten()
    .any(|value| value.to_ascii_uppercase().contains("IO_TEST"))
}

fn is_known_mock_or_local_principal(value: &str) -> bool {
    matches!(
        value,
        "aaaaa-aa"
            | "2vxsx-fae"
            | "bkyz2-fmaaa-aaaaa-qaaaq-cai"
            | "bd3sg-teaaa-aaaaa-qaaba-cai"
            | "br5f7-7uaaa-aaaaa-qaaca-cai"
            | "be2us-64aaa-aaaaa-qaabq-cai"
            | "bw4dl-smaaa-aaaaa-qaacq-cai"
            | "b77ix-eeaaa-aaaaa-qaada-cai"
            | "by6od-j4aaa-aaaaa-qaadq-cai"
    )
}

fn is_known_production_principal(value: &str) -> bool {
    matches!(
        value,
        ICP_LEDGER_PRINCIPAL
            | ICP_INDEX_PRINCIPAL
            | NNS_GOVERNANCE_PRINCIPAL
            | PROTECTED_IO_NEURON_OWNER_CANISTER
            | PRODUCTION_IO_STREAM_MANAGER_CANISTER_ID
            | PRODUCTION_IO_NNS_NEURON_MANAGER_CANISTER_ID
            | PRODUCTION_IO_HISTORIAN_CANISTER_ID
            | PRODUCTION_FRONTEND_CANISTER_ID
    )
}

fn is_dev_mainnet_canister(value: &str) -> bool {
    matches!(
        value,
        DEV_MAINNET_FRONTEND_CANISTER_ID | DEV_MAINNET_HISTORIAN_CANISTER_ID
    )
}

fn is_known_unrelated_system_canister(value: &str) -> bool {
    matches!(
        value,
        ICP_LEDGER_PRINCIPAL
            | ICP_INDEX_PRINCIPAL
            | NNS_GOVERNANCE_PRINCIPAL
            | INTERNET_IDENTITY_CANISTER_ID
            | NNS_DAPP_CANISTER_ID
            | TEMPLATE_SNS_ROOT_PLACEHOLDER
            | TEMPLATE_SNS_GOVERNANCE_PLACEHOLDER
            | TEMPLATE_SNS_LEDGER_PLACEHOLDER
            | TEMPLATE_SNS_INDEX_PLACEHOLDER
    )
}

#[derive(Clone, Debug, Default)]
pub struct TemplateValidation {
    pub forbid_execution_commands: bool,
}

pub fn parse_template_config(text: &str) -> Result<ProductionWiringConfig, String> {
    let parsed = FlatToml::parse(text)?;
    Ok(ProductionWiringConfig {
        mode: match parsed.required_string("environment", "mode")?.as_str() {
            "Mock" => WiringMode::Mock,
            "Local" => WiringMode::Local,
            "DryRun" => WiringMode::DryRun,
            "ProductionPlanned" => WiringMode::ProductionPlanned,
            "ProductionActive" => WiringMode::ProductionActive,
            other => return Err(format!("environment.mode: unknown mode {other:?}")),
        },
        io_ledger_role: match parsed
            .required_string("environment", "io_ledger_role")?
            .as_str()
        {
            "IoTestNonCanonical" => IoLedgerRole::IoTestNonCanonical,
            "FutureCanonicalSnsIo" => IoLedgerRole::FutureCanonicalSnsIo,
            other => {
                return Err(format!(
                    "environment.io_ledger_role: unknown role {other:?}"
                ))
            }
        },
        fixture_marked: parsed
            .bool("environment", "fixture_marked")?
            .unwrap_or(false),
        principals: PrincipalWiring {
            icp_ledger_principal_text: parsed.string("principals", "icp_ledger")?,
            icp_index_principal_text: parsed.string("principals", "icp_index")?,
            nns_governance_principal_text: parsed.string("principals", "nns_governance")?,
            nns_ledger_principal_text: parsed.string("principals", "nns_ledger")?,
            nns_index_principal_text: parsed.string("principals", "nns_index")?,
            sns_root_principal_text: parsed.string("principals", "sns_root")?,
            sns_governance_principal_text: parsed.string("principals", "sns_governance")?,
            sns_ledger_principal_text: parsed.string("principals", "sns_ledger")?,
            sns_index_principal_text: parsed.string("principals", "sns_index")?,
            io_ledger_principal_text: parsed.string("principals", "io_ledger")?,
            io_index_principal_text: parsed.string("principals", "io_index")?,
        },
        fee_policy: FeePolicyWiring {
            icp_transfer_fee_e8s: parsed.u128("fees", "icp_transfer_fee_e8s")?,
            io_ledger_transfer_fee_e8s: parsed.u128("fees", "io_ledger_transfer_fee_e8s")?,
            tiny_value_policy_max_fee_e8s: parsed.u128("fees", "tiny_value_policy_max_fee_e8s")?,
            allow_zero_fees_for_mock_or_local: parsed
                .bool("fees", "allow_zero_fees_for_mock_or_local")?
                .unwrap_or(false),
        },
        protected: ProtectedReferences {
            neuron_owner_canister_principal_text: parsed
                .string("protected", "neuron_owner_canister")?,
            io_nns_neuron_id: parsed.u64("protected", "io_nns_neuron_id")?,
        },
        deployment_targets: DeploymentTargets {
            io_stream_manager_principal_text: parsed
                .string("deployment_targets", "io_stream_manager")?,
            io_nns_neuron_manager_principal_text: parsed
                .string("deployment_targets", "io_nns_neuron_manager")?,
            mutation_target_principal_texts: parsed
                .string_array("deployment_targets", "mutation_target_principals")?,
            mutation_target_nns_neuron_ids: parsed
                .u64_array("deployment_targets", "mutation_target_nns_neuron_ids")?,
        },
    })
}

pub fn validate_template_text(text: &str) -> Result<ProductionWiringConfig, String> {
    if contains_execution_command(text) {
        return Err("template contains a forbidden mainnet/deployment command".to_string());
    }
    let config = parse_template_config(text)?;
    config
        .validate()
        .map_err(|err| format!("production wiring validation failed: {err:?}"))?;
    Ok(config)
}

fn contains_execution_command(text: &str) -> bool {
    let lowered = text.to_ascii_lowercase();
    [
        "dfx",
        "--network ic",
        "icp canister install",
        "icp canister upgrade",
        "icp canister update-settings",
        "icp canister call",
        "reinstall",
    ]
    .into_iter()
    .any(|needle| lowered.contains(needle))
}

#[derive(Default)]
struct FlatToml {
    values: BTreeMap<(String, String), String>,
}

impl FlatToml {
    fn parse(text: &str) -> Result<Self, String> {
        let mut current_section = String::new();
        let mut values = BTreeMap::new();
        for (index, raw_line) in text.lines().enumerate() {
            let line = raw_line.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            if line.starts_with('[') && line.ends_with(']') {
                current_section = line[1..line.len() - 1].trim().to_string();
                continue;
            }
            let Some((left, right)) = line.split_once('=') else {
                return Err(format!("line {}: expected key = value", index + 1));
            };
            values.insert(
                (current_section.clone(), left.trim().to_string()),
                right.trim().to_string(),
            );
        }
        Ok(Self { values })
    }

    fn required_string(&self, section: &str, key: &str) -> Result<String, String> {
        self.string(section, key)?
            .ok_or_else(|| format!("{section}.{key}: missing required string"))
    }

    fn string(&self, section: &str, key: &str) -> Result<Option<String>, String> {
        let Some(value) = self.values.get(&(section.to_string(), key.to_string())) else {
            return Ok(None);
        };
        if value == "null" {
            return Ok(None);
        }
        parse_quoted(value)
            .map(Some)
            .map_err(|err| format!("{section}.{key}: {err}"))
    }

    fn bool(&self, section: &str, key: &str) -> Result<Option<bool>, String> {
        let Some(value) = self.values.get(&(section.to_string(), key.to_string())) else {
            return Ok(None);
        };
        match value.as_str() {
            "true" => Ok(Some(true)),
            "false" => Ok(Some(false)),
            other => Err(format!("{section}.{key}: expected bool, got {other:?}")),
        }
    }

    fn u128(&self, section: &str, key: &str) -> Result<Option<u128>, String> {
        let Some(value) = self.values.get(&(section.to_string(), key.to_string())) else {
            return Ok(None);
        };
        if value == "null" {
            return Ok(None);
        }
        let digits = value.replace('_', "");
        digits
            .parse::<u128>()
            .map(Some)
            .map_err(|err| format!("{section}.{key}: invalid u128: {err}"))
    }

    fn u64(&self, section: &str, key: &str) -> Result<Option<u64>, String> {
        let Some(value) = self.u128(section, key)? else {
            return Ok(None);
        };
        u64::try_from(value)
            .map(Some)
            .map_err(|_| format!("{section}.{key}: value exceeds u64"))
    }

    fn string_array(&self, section: &str, key: &str) -> Result<Vec<String>, String> {
        let Some(value) = self.values.get(&(section.to_string(), key.to_string())) else {
            return Ok(Vec::new());
        };
        parse_array(value)?
            .into_iter()
            .map(|value| parse_quoted(value.trim()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|err| format!("{section}.{key}: {err}"))
    }

    fn u64_array(&self, section: &str, key: &str) -> Result<Vec<u64>, String> {
        let Some(value) = self.values.get(&(section.to_string(), key.to_string())) else {
            return Ok(Vec::new());
        };
        parse_array(value)?
            .into_iter()
            .map(|value| {
                let digits = value.trim().replace('_', "");
                digits
                    .parse::<u64>()
                    .map_err(|err| format!("{section}.{key}: invalid u64: {err}"))
            })
            .collect()
    }
}

fn parse_quoted(value: &str) -> Result<String, String> {
    if value.starts_with('"') && value.ends_with('"') && value.len() >= 2 {
        Ok(value[1..value.len() - 1].to_string())
    } else {
        Err(format!("expected quoted string, got {value:?}"))
    }
}

fn parse_array(value: &str) -> Result<Vec<&str>, String> {
    let trimmed = value.trim();
    if !(trimmed.starts_with('[') && trimmed.ends_with(']')) {
        return Err(format!("expected array, got {value:?}"));
    }
    let inner = &trimmed[1..trimmed.len() - 1];
    if inner.trim().is_empty() {
        return Ok(Vec::new());
    }
    Ok(inner.split(',').map(str::trim).collect())
}

pub fn template_paths() -> BTreeSet<&'static str> {
    BTreeSet::from([
        "deploy/production-wiring/template.toml",
        "deploy/production-wiring/dry-run.example.toml",
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_production_config() -> ProductionWiringConfig {
        ProductionWiringConfig {
            mode: WiringMode::ProductionPlanned,
            io_ledger_role: IoLedgerRole::FutureCanonicalSnsIo,
            fixture_marked: false,
            principals: PrincipalWiring {
                icp_ledger_principal_text: Some(ICP_LEDGER_PRINCIPAL.to_string()),
                icp_index_principal_text: Some(ICP_INDEX_PRINCIPAL.to_string()),
                nns_governance_principal_text: Some(NNS_GOVERNANCE_PRINCIPAL.to_string()),
                nns_ledger_principal_text: Some(ICP_LEDGER_PRINCIPAL.to_string()),
                nns_index_principal_text: Some(ICP_INDEX_PRINCIPAL.to_string()),
                sns_root_principal_text: Some("qaa6y-5yaaa-aaaaa-aaafa-cai".to_string()),
                sns_governance_principal_text: Some("r7inp-6aaaa-aaaaa-aaabq-cai".to_string()),
                sns_ledger_principal_text: Some("qjdve-lqaaa-aaaaa-aaaeq-cai".to_string()),
                sns_index_principal_text: Some("renrk-eyaaa-aaaaa-aaada-cai".to_string()),
                io_ledger_principal_text: Some("qjdve-lqaaa-aaaaa-aaaeq-cai".to_string()),
                io_index_principal_text: Some("renrk-eyaaa-aaaaa-aaada-cai".to_string()),
            },
            fee_policy: FeePolicyWiring {
                icp_transfer_fee_e8s: Some(ICP_TRANSFER_FEE_E8S),
                io_ledger_transfer_fee_e8s: Some(10_000),
                tiny_value_policy_max_fee_e8s: Some(1_000_000),
                allow_zero_fees_for_mock_or_local: false,
            },
            protected: ProtectedReferences {
                neuron_owner_canister_principal_text: Some(
                    PROTECTED_IO_NEURON_OWNER_CANISTER.to_string(),
                ),
                io_nns_neuron_id: Some(PROTECTED_IO_NNS_NEURON_ID),
            },
            deployment_targets: DeploymentTargets {
                io_stream_manager_principal_text: Some(
                    PRODUCTION_IO_STREAM_MANAGER_CANISTER_ID.to_string(),
                ),
                io_nns_neuron_manager_principal_text: Some(
                    PRODUCTION_IO_NNS_NEURON_MANAGER_CANISTER_ID.to_string(),
                ),
                mutation_target_principal_texts: Vec::new(),
                mutation_target_nns_neuron_ids: Vec::new(),
            },
        }
    }

    fn valid_local_config() -> ProductionWiringConfig {
        let mut config = valid_production_config();
        config.mode = WiringMode::Local;
        config.io_ledger_role = IoLedgerRole::IoTestNonCanonical;
        config.fixture_marked = true;
        config.principals.icp_ledger_principal_text = Some("bkyz2-fmaaa-aaaaa-qaaaq-cai".into());
        config.principals.icp_index_principal_text = Some("bd3sg-teaaa-aaaaa-qaaba-cai".into());
        config.principals.nns_governance_principal_text =
            Some("by6od-j4aaa-aaaaa-qaadq-cai".into());
        config.principals.nns_ledger_principal_text = None;
        config.principals.nns_index_principal_text = None;
        config.principals.sns_root_principal_text = Some("bw4dl-smaaa-aaaaa-qaacq-cai".into());
        config.principals.sns_governance_principal_text =
            Some("b77ix-eeaaa-aaaaa-qaada-cai".into());
        config.principals.sns_ledger_principal_text = Some("br5f7-7uaaa-aaaaa-qaaca-cai".into());
        config.principals.sns_index_principal_text = Some("be2us-64aaa-aaaaa-qaabq-cai".into());
        config.principals.io_ledger_principal_text = Some("br5f7-7uaaa-aaaaa-qaaca-cai".into());
        config.principals.io_index_principal_text = Some("be2us-64aaa-aaaaa-qaabq-cai".into());
        config.fee_policy.allow_zero_fees_for_mock_or_local = true;
        config
    }

    #[test]
    fn valid_mock_local_and_production_configs_pass() {
        valid_local_config().validate().unwrap();
        valid_production_config().validate().unwrap();
    }

    #[test]
    fn checked_in_template_requires_fiduciary_deployment_targets() {
        let text = include_str!("../../../deploy/production-wiring/template.toml");
        let config = validate_template_text(text).unwrap();
        assert_eq!(config.mode, WiringMode::ProductionPlanned);
        assert_eq!(
            config.deployment_targets.io_stream_manager_principal_text,
            Some(PRODUCTION_IO_STREAM_MANAGER_CANISTER_ID.to_string())
        );
        assert_eq!(
            config
                .deployment_targets
                .io_nns_neuron_manager_principal_text,
            Some(PRODUCTION_IO_NNS_NEURON_MANAGER_CANISTER_ID.to_string())
        );
    }

    #[test]
    fn anonymous_and_management_principals_are_rejected() {
        let mut config = valid_local_config();
        config.principals.sns_root_principal_text = Some("2vxsx-fae".into());
        assert!(matches!(
            config.validate().unwrap_err(),
            WiringValidationError::AnonymousPrincipal { .. }
        ));
        let mut config = valid_local_config();
        config.principals.sns_root_principal_text = Some("aaaaa-aa".into());
        assert!(matches!(
            config.validate().unwrap_err(),
            WiringValidationError::ManagementCanisterPrincipal { .. }
        ));
    }

    #[test]
    fn duplicate_incompatible_role_principals_are_rejected() {
        let mut config = valid_production_config();
        config.principals.sns_root_principal_text =
            config.principals.icp_ledger_principal_text.clone();
        assert!(matches!(
            config.validate().unwrap_err(),
            WiringValidationError::DuplicateIncompatiblePrincipal { .. }
        ));
    }

    #[test]
    fn required_production_principals_are_rejected_when_missing() {
        for field in [
            "icp_ledger_principal_text",
            "icp_index_principal_text",
            "nns_governance_principal_text",
        ] {
            let mut config = valid_production_config();
            match field {
                "icp_ledger_principal_text" => config.principals.icp_ledger_principal_text = None,
                "icp_index_principal_text" => config.principals.icp_index_principal_text = None,
                "nns_governance_principal_text" => {
                    config.principals.nns_governance_principal_text = None
                }
                _ => unreachable!(),
            }
            assert!(matches!(
                config.validate().unwrap_err(),
                WiringValidationError::MissingRequiredPrincipal { .. }
                    | WiringValidationError::IndexWithoutLedger { .. }
                    | WiringValidationError::LedgerWithoutIndex { .. }
            ));
        }
    }

    #[test]
    fn incomplete_sns_group_and_ledger_index_pairs_are_rejected() {
        let mut config = valid_production_config();
        config.principals.sns_root_principal_text = None;
        assert!(matches!(
            config.validate().unwrap_err(),
            WiringValidationError::IncompleteSnsGroup { .. }
        ));
        let mut config = valid_production_config();
        config.principals.sns_index_principal_text = None;
        assert!(matches!(
            config.validate().unwrap_err(),
            WiringValidationError::LedgerWithoutIndex { .. }
        ));
        let mut config = valid_production_config();
        config.principals.sns_ledger_principal_text = None;
        assert!(matches!(
            config.validate().unwrap_err(),
            WiringValidationError::IndexWithoutLedger { .. }
        ));
    }

    #[test]
    fn io_test_cannot_be_labelled_canonical() {
        let mut config = valid_local_config();
        config.mode = WiringMode::ProductionPlanned;
        config.io_ledger_role = IoLedgerRole::IoTestNonCanonical;
        assert!(matches!(
            config.validate().unwrap_err(),
            WiringValidationError::IoTestLabelledCanonical
                | WiringValidationError::MockOrLocalPrincipalInProduction { .. }
        ));
    }

    #[test]
    fn invalid_production_fees_are_rejected() {
        let mut config = valid_production_config();
        config.fee_policy.icp_transfer_fee_e8s = None;
        assert!(matches!(
            config.validate().unwrap_err(),
            WiringValidationError::MissingFee { .. }
        ));
        let mut config = valid_production_config();
        config.fee_policy.io_ledger_transfer_fee_e8s = Some(0);
        assert!(matches!(
            config.validate().unwrap_err(),
            WiringValidationError::ZeroProductionFee { .. }
        ));
        let mut config = valid_production_config();
        config.fee_policy.tiny_value_policy_max_fee_e8s = Some(1);
        assert!(matches!(
            config.validate().unwrap_err(),
            WiringValidationError::FeeExceedsTinyValuePolicy { .. }
        ));
    }

    #[test]
    fn known_mainnet_role_mismatches_are_rejected() {
        let mut config = valid_production_config();
        config.principals.nns_governance_principal_text = Some(ICP_LEDGER_PRINCIPAL.into());
        assert!(matches!(
            config.validate().unwrap_err(),
            WiringValidationError::DuplicateIncompatiblePrincipal { .. }
                | WiringValidationError::NnsGovernancePrincipalMismatch { .. }
        ));
        let mut config = valid_production_config();
        config.principals.icp_index_principal_text = Some(NNS_GOVERNANCE_PRINCIPAL.into());
        assert!(matches!(
            config.validate().unwrap_err(),
            WiringValidationError::IcpIndexPrincipalMismatch { .. }
                | WiringValidationError::DuplicateIncompatiblePrincipal { .. }
        ));
    }

    #[test]
    fn protected_canister_and_neuron_cannot_be_mutation_targets() {
        let mut config = valid_production_config();
        config.deployment_targets.io_stream_manager_principal_text =
            Some(PROTECTED_IO_NEURON_OWNER_CANISTER.into());
        assert!(matches!(
            config.validate().unwrap_err(),
            WiringValidationError::ProtectedCanisterAsTarget { .. }
        ));
        let mut config = valid_production_config();
        config
            .deployment_targets
            .mutation_target_nns_neuron_ids
            .push(PROTECTED_IO_NNS_NEURON_ID);
        assert!(matches!(
            config.validate().unwrap_err(),
            WiringValidationError::ProtectedNeuronAsTarget { .. }
        ));
    }

    #[test]
    fn production_io_owned_targets_are_required_and_exact() {
        let mut config = valid_production_config();
        config.deployment_targets.io_stream_manager_principal_text = None;
        assert!(matches!(
            config.validate().unwrap_err(),
            WiringValidationError::ProductionIoCanisterIdMismatch { .. }
        ));

        let mut config = valid_production_config();
        config
            .deployment_targets
            .io_nns_neuron_manager_principal_text = Some("aaaaa-aa".to_string());
        assert!(matches!(
            config.validate().unwrap_err(),
            WiringValidationError::ManagementCanisterPrincipal { .. }
                | WiringValidationError::ProductionIoCanisterIdMismatch { .. }
        ));

        let mut config = valid_production_config();
        config.principals.sns_root_principal_text =
            Some(DEV_MAINNET_HISTORIAN_CANISTER_ID.to_string());
        assert!(matches!(
            config.validate().unwrap_err(),
            WiringValidationError::DevMainnetCanisterInProduction { .. }
        ));
    }

    #[test]
    fn unrelated_system_canisters_cannot_be_value_moving_deployment_targets() {
        let cases = [
            ("stream manager", "rdmx6-jaaaa-aaaaa-aaadq-cai", true),
            ("nns neuron manager", "qoctq-giaaa-aaaaa-aaaea-cai", false),
            ("sns placeholder", "qaa6y-5yaaa-aaaaa-aaafa-cai", true),
        ];
        for (_, principal, stream_manager_field) in cases {
            let mut config = valid_production_config();
            if stream_manager_field {
                config.deployment_targets.io_stream_manager_principal_text =
                    Some(principal.to_string());
            } else {
                config
                    .deployment_targets
                    .io_nns_neuron_manager_principal_text = Some(principal.to_string());
            }
            assert!(matches!(
                config.validate().unwrap_err(),
                WiringValidationError::UnrelatedSystemCanisterAsValueMovingTarget { .. }
            ));
        }
    }

    #[test]
    fn production_active_is_not_available() {
        let mut config = valid_production_config();
        config.mode = WiringMode::ProductionActive;
        assert_eq!(
            config.validate().unwrap_err(),
            WiringValidationError::ProductionActiveUnavailable
        );
    }
}
