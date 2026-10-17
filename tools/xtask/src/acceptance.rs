pub(crate) mod checkpoints;
mod constructors;
#[path = "../../acceptance/frontend_assets.rs"]
mod frontend_assets;
mod observations;
use candid::Principal;
use io_core_model::{Backing, EconomicState, ReconcilePlan};
use io_ledger_types::{Account as LedgerAccount, Subaccount};
use io_production_wiring::{
    ICP_INDEX_PRINCIPAL, ICP_LEDGER_PRINCIPAL, ICP_TRANSFER_FEE_E8S, JUPITER_FAUCET_CANISTER_ID,
    NNS_GOVERNANCE_PRINCIPAL, PRODUCTION_FRONTEND_CANISTER_ID, PRODUCTION_IO_HISTORIAN_CANISTER_ID,
    PRODUCTION_IO_NNS_NEURON_MANAGER_CANISTER_ID, PRODUCTION_IO_STREAM_MANAGER_CANISTER_ID,
    PROTECTED_IO_NNS_NEURON_ID,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

const DIR: &str = "deploy/acceptance";
const ACCEPTANCE_NEURON_ID: u64 = 4_021_054_791_166_218;
const TOTAL_IO_E8S: u128 = 100_000_000_000_000_000;
const GOVERNANCE_IO_E8S: u128 = 3_000_000_000;
const PROPOSAL_REJECT_COST_E8S: u128 = 1_000_000_000;
const JUPITER_GOVERNANCE_IO_E8S: u128 = 10_000_000_000_000_000;
const DYNAMIC_ANCHOR_E8S: u128 = 1_000_000_000;
const IO_FEE_E8S: u128 = 10_000;
const PERMANENT_DELAY_SECONDS: u64 = 63_115_200;
const GOVERNANCE_MEMO: u64 = 0x494f_474f_5630_3031; // IOGOV001
const JUPITER_GOVERNANCE_MEMO: u64 = 0x494f_4a55_5030_3031; // IOJUP001
const RESERVE_MEMO: u64 = 0x494f_5245_5330_3031; // IORES001
const SALE_MEMO_START: u64 = 1_000_000;
const SALE_MEMO_END: u64 = 10_000_000;
// The reviewed repository baseline is the approval boundary, not an input from
// an evidence directory. Changing it requires source review and recompilation.
fn approved_sns_baseline() -> &'static Value {
    static BASELINE: std::sync::LazyLock<Value> = std::sync::LazyLock::new(|| {
        serde_json::from_str(include_str!(
            "../../../deploy/acceptance/sns-source-baseline.json"
        ))
        .expect("reviewed SNS baseline JSON")
    });
    &BASELINE
}
fn approved_sns_source() -> &'static str {
    approved_sns_baseline()["commit"]
        .as_str()
        .expect("reviewed SNS commit")
}
const PRODUCTION_JUPITER_SNS_REWARDS_CANISTER_ID: &str = "alk7f-5aaaa-aaaar-qb4ra-cai";
const STREAM_RESERVE_SUBACCOUNT: [u8; 32] =
    hex_literal("781a5fc2d878692d1fa555359c190f96d61f85b392cf11b80f4666169d789027");
const STREAM_LIQUID_SUBACCOUNT: [u8; 32] =
    hex_literal("ccdbb6def992deafdfc3a890cc431818631d7b42f94df6b2b3e1270459be8a78");

const fn hex_literal(text: &str) -> [u8; 32] {
    let bytes = text.as_bytes();
    let mut out = [0_u8; 32];
    let mut index = 0;
    while index < 32 {
        out[index] = (hex_nibble(bytes[index * 2]) << 4) | hex_nibble(bytes[index * 2 + 1]);
        index += 1;
    }
    out
}

const fn hex_nibble(value: u8) -> u8 {
    match value {
        b'0'..=b'9' => value - b'0',
        b'a'..=b'f' => value - b'a' + 10,
        _ => panic!("invalid hex literal"),
    }
}

#[derive(Debug, Deserialize)]
struct AcceptanceManifest {
    reward_share_baseline_commit: String,
    roles: BTreeMap<String, String>,
    system_canisters: BTreeMap<String, String>,
    permanent_nns_neuron: PermanentNeuron,
}

#[derive(Debug, Deserialize)]
struct PermanentNeuron {
    id: u64,
    controller: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Economics {
    schema: String,
    token_name: String,
    token_symbol: String,
    decimals: u8,
    total_supply_e8s: u128,
    transfer_fee_e8s: u128,
    proposal_reject_cost_e8s: u128,
    minimum_neuron_stake_e8s: u128,
    minimum_voting_dissolve_delay_seconds: u64,
    maximum_dissolve_delay_seconds: u64,
    reward_event_duration_seconds: u64,
    maximum_dissolve_delay_bonus_basis_points: u64,
    maximum_neuron_age_seconds: u64,
    maximum_age_bonus_basis_points: u64,
    initial_reward_rate_basis_points: u64,
    final_reward_rate_basis_points: u64,
    reward_rate_transition_seconds: u64,
    maximum_number_of_neurons: u64,
    initial_voting_period_seconds: u64,
    wait_for_quiet_deadline_increase_seconds: u64,
    developer_voting_power_percent: u64,
    sale_allocation_source: String,
    sale_price_constraint: String,
    governance_neuron: GenesisNeuron,
    jupiter_governance_neuron: GenesisNeuron,
    reserve_transport_neuron: GenesisNeuron,
    treasury_e8s: u128,
    future_swap_e8s: u128,
    minimum_participants: u64,
    neurons_fund_participation: bool,
    gross_sale_reference_price: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GenesisNeuron {
    memo_domain: String,
    #[serde(default)]
    io_reward_eligible: Option<bool>,
    #[serde(default)]
    controller_source: Option<String>,
    stake_e8s: Value,
    memo: u64,
    dissolve_delay_seconds: u64,
    vesting_period_seconds: u64,
    claim_bearing: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CapitalizationPlan {
    claims_e8s: u128,
    claim_backing_e8s: u128,
    permanent_top_up_e8s: u128,
    permanent_exact_remainder_thirds: u8,
}

#[derive(Debug, Deserialize)]
struct Overlay {
    environment: String,
    dapp_canisters: BTreeMap<String, String>,
    fallback_controller_principal: String,
    jupiter_sns_rewards_canister_id: String,
}

fn read_json<T: for<'de> Deserialize<'de>>(root: &Path, relative: &str) -> Result<T, String> {
    let path = root.join(relative);
    let bytes = fs::read(&path).map_err(|err| format!("read {}: {err}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|err| format!("parse {}: {err}", path.display()))
}

fn principal(text: &str, field: &str) -> Result<Principal, String> {
    Principal::from_text(text).map_err(|err| format!("invalid {field} principal {text}: {err}"))
}

fn manifest(root: &Path) -> Result<AcceptanceManifest, String> {
    read_json(root, &format!("{DIR}/acceptance-manifest.json"))
}

fn economics(root: &Path) -> Result<Economics, String> {
    read_json(root, &format!("{DIR}/canonical-sns-economics.json"))
}

fn overlay(root: &Path, environment: &str) -> Result<Overlay, String> {
    read_json(root, &format!("{DIR}/{environment}-identity-overlay.json"))
}

fn role<'a>(manifest: &'a AcceptanceManifest, name: &str) -> Result<&'a str, String> {
    manifest
        .roles
        .get(name)
        .map(String::as_str)
        .ok_or_else(|| format!("acceptance role {name} is missing"))
}

fn domain_subaccount(domain: &[u8], controller: Principal, nonce: u64) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update([domain.len() as u8]);
    hasher.update(domain);
    hasher.update(controller.as_slice());
    hasher.update(nonce.to_be_bytes());
    hasher.finalize().into()
}

fn sns_neuron_subaccount(controller: Principal, memo: u64) -> [u8; 32] {
    domain_subaccount(b"neuron-stake", controller, memo)
}

fn sns_distribution_subaccount(governance: Principal, nonce: u64) -> [u8; 32] {
    domain_subaccount(b"token-distribution", governance, nonce)
}

#[allow(
    clippy::too_many_arguments,
    reason = "the explicit manifest fields keep every economic Account reviewable at each call site"
)]
fn account_json(
    name: &str,
    owner: &str,
    subaccount: Option<[u8; 32]>,
    asset: &str,
    role: &str,
    initial_balance_e8s: Value,
    steady_state: &str,
    excluded: bool,
    user_funding_required: bool,
) -> Result<Value, String> {
    let owner_principal = principal(owner, name)?;
    let subaccount_hex = subaccount.map(hex::encode);
    let icp_account_identifier = if asset == "ICP" {
        let account = LedgerAccount::new(owner_principal, subaccount.map(Subaccount));
        Value::String(account.icp_account_identifier_text())
    } else {
        Value::Null
    };
    Ok(json!({
        "name": name,
        "owner": owner,
        "subaccount_hex": subaccount_hex,
        "icrc_account": { "owner": owner, "subaccount_hex": subaccount_hex },
        "icp_account_identifier": icp_account_identifier,
        "role": role,
        "asset": asset,
        "initial_expected_balance_e8s": initial_balance_e8s,
        "steady_state_classification": steady_state,
        "excluded_from_claim_supply": excluded,
        "user_funding_required": user_funding_required
    }))
}

fn expected_accounts(root: &Path) -> Result<Value, String> {
    let manifest = manifest(root)?;
    let acceptance_overlay = overlay(root, "acceptance")?;
    let inputs: Value = read_json(root, &format!("{DIR}/required-inputs.draft.json"))?;
    let governance = principal(role(&manifest, "sns_governance")?, "SNS Governance")?;
    let stream = role(&manifest, "stream_manager")?;
    let nns = role(&manifest, "nns_manager")?;
    let controller = inputs["genesis_controller_principal"]
        .as_str()
        .ok_or("genesis controller input must be text")?;
    let jupiter = inputs["acceptance_jupiter_source_principal"]
        .as_str()
        .ok_or("Jupiter input must be text")?;
    let sale_allocation = planned_sale_allocation(&inputs)?;
    let reserve_balance = sale_allocation.map(reserve_stake).transpose()?.map_or_else(
        || Value::String("TBD_RESERVE_TRANSPORT_STAKE_E8S".into()),
        |value| json!(value),
    );
    let swap_balance = sale_allocation.map_or_else(
        || Value::String("TBD_ACCEPTANCE_SALE_IO_ALLOCATION_E8S".into()),
        |value| json!(value),
    );

    let mut accounts = Vec::new();
    if !is_placeholder(controller) {
        let controller = principal(controller, "genesis controller")?;
        accounts.push(account_json(
            "sns_governance_neuron_stake",
            role(&manifest, "sns_governance")?,
            Some(sns_neuron_subaccount(controller, GOVERNANCE_MEMO)),
            "IO",
            "initial claim-bearing governance neuron",
            json!(GOVERNANCE_IO_E8S),
            "claim-bearing while staked",
            false,
            false,
        )?);
        accounts.push(account_json(
            "sns_reserve_transport_neuron_stake",
            role(&manifest, "sns_governance")?,
            Some(sns_neuron_subaccount(controller, RESERVE_MEMO)),
            "IO",
            "excluded one-time reserve transport",
            reserve_balance.clone(),
            "permanently excluded; normally zero after migration",
            true,
            false,
        )?);
    } else {
        for (name, memo, balance, excluded, role_text) in [
            (
                "sns_governance_neuron_stake",
                GOVERNANCE_MEMO,
                GOVERNANCE_IO_E8S,
                false,
                "initial claim-bearing governance neuron",
            ),
            (
                "sns_reserve_transport_neuron_stake",
                RESERVE_MEMO,
                0,
                true,
                "excluded one-time reserve transport",
            ),
        ] {
            accounts.push(json!({
                "name": name,
                "owner": role(&manifest, "sns_governance")?,
                "subaccount_hex": format!("TBD_DERIVED_AFTER_GENESIS_CONTROLLER_MEMO_{memo}"),
                "icrc_account": {"owner": role(&manifest, "sns_governance")?, "subaccount_hex": format!("TBD_DERIVED_AFTER_GENESIS_CONTROLLER_MEMO_{memo}")},
                "icp_account_identifier": null,
                "role": role_text,
                "asset": "IO",
                "initial_expected_balance_e8s": if excluded {reserve_balance.clone()} else {json!(balance)},
                "steady_state_classification": if excluded {"permanently excluded; normally zero after migration"} else {"claim-bearing while staked"},
                "excluded_from_claim_supply": excluded,
                "user_funding_required": false
            }));
        }
    }
    let jupiter_governance_controller = acceptance_overlay.jupiter_sns_rewards_canister_id.as_str();
    if is_placeholder(jupiter_governance_controller) {
        accounts.push(json!({
            "name": "sns_jupiter_governance_neuron_stake",
            "owner": role(&manifest, "sns_governance")?,
            "subaccount_hex": format!("TBD_DERIVED_AFTER_JUPITER_SNS_REWARDS_MEMO_{JUPITER_GOVERNANCE_MEMO}"),
            "icrc_account": {
                "owner": role(&manifest, "sns_governance")?,
                "subaccount_hex": format!("TBD_DERIVED_AFTER_JUPITER_SNS_REWARDS_MEMO_{JUPITER_GOVERNANCE_MEMO}")
            },
            "icp_account_identifier": null,
            "role": "excluded Jupiter governance-only neuron",
            "asset": "IO",
            "initial_expected_balance_e8s": JUPITER_GOVERNANCE_IO_E8S,
            "steady_state_classification": "permanently staked and economically excluded",
            "excluded_from_claim_supply": true,
            "user_funding_required": false
        }));
    } else {
        accounts.push(account_json(
            "sns_jupiter_governance_neuron_stake",
            role(&manifest, "sns_governance")?,
            Some(sns_neuron_subaccount(
                principal(
                    jupiter_governance_controller,
                    "acceptance Jupiter SNS Rewards",
                )?,
                JUPITER_GOVERNANCE_MEMO,
            )),
            "IO",
            "excluded Jupiter governance-only neuron",
            json!(JUPITER_GOVERNANCE_IO_E8S),
            "permanently staked and economically excluded",
            true,
            false,
        )?);
    }

    accounts.extend([
        account_json(
            "sns_treasury",
            role(&manifest, "sns_governance")?,
            Some(sns_distribution_subaccount(governance, 0)),
            "IO",
            "zero treasury allocation",
            json!(0),
            "zero",
            false,
            false,
        )?,
        account_json(
            "sns_swap",
            role(&manifest, "sns_swap")?,
            None,
            "IO",
            "small sale allocation pending distribution",
            swap_balance,
            "zero after successful finalization except canonically observed residual dust",
            false,
            false,
        )?,
        account_json(
            "sns_icp_treasury",
            role(&manifest, "sns_governance")?,
            None,
            "ICP",
            "sale proceeds retained by SNS Governance",
            json!(0),
            "sale proceeds; excluded from IO claim backing and permanent capital",
            false,
            false,
        )?,
        account_json(
            "sns_future_swap",
            role(&manifest, "sns_governance")?,
            Some(sns_distribution_subaccount(governance, 1)),
            "IO",
            "zero future swap allocation",
            json!(0),
            "zero",
            false,
            false,
        )?,
        account_json(
            "sns_ledger_minting",
            role(&manifest, "sns_governance")?,
            None,
            "IO",
            "SNS Ledger minting account",
            json!(0),
            "diagnostic authority account",
            false,
            false,
        )?,
        account_json(
            "stream_io_reserve",
            stream,
            Some(STREAM_RESERVE_SUBACCOUNT),
            "IO",
            "canonical protocol reserve",
            json!(0),
            "reserve; excluded by core supply formula",
            true,
            false,
        )?,
        account_json(
            "stream_liquid_icp",
            stream,
            Some(STREAM_LIQUID_SUBACCOUNT),
            "ICP",
            "liquid claim backing",
            json!(0),
            "claim backing",
            false,
            true,
        )?,
        account_json(
            "stream_redemption_staging",
            stream,
            Some(io_accounts::REDEMPTION_STAGING_SUBACCOUNT),
            "IO",
            "redemption staging",
            json!(0),
            "transient value-moving staging",
            false,
            false,
        )?,
        account_json(
            "nns_two_year_maturity_staging",
            nns,
            Some(io_accounts::TWO_YEAR_MATURITY_SUBACCOUNT),
            "ICP",
            "TwoYear maturity staging",
            json!(0),
            "self-funded from gross maturity",
            false,
            false,
        )?,
        account_json(
            "nns_two_week_maturity_staging",
            nns,
            Some(io_accounts::TWO_WEEK_MATURITY_SUBACCOUNT),
            "ICP",
            "TwoWeek maturity staging",
            json!(0),
            "self-funded from gross maturity",
            false,
            false,
        )?,
        account_json(
            "nns_dynamic_parent_stake",
            NNS_GOVERNANCE_PRINCIPAL,
            Some(sns_neuron_subaccount(principal(nns, "NNS Manager")?, 0)),
            "ICP",
            "memo-0 Dynamic parent neuron",
            json!(0),
            "10 ICP anchor plus claim backing",
            false,
            true,
        )?,
        account_json(
            "nns_jupiter_staging",
            nns,
            None,
            "ICP",
            "Jupiter staging",
            json!(0),
            "self-funded from Jupiter receipt",
            false,
            false,
        )?,
    ]);
    if is_placeholder(jupiter) {
        for asset in ["ICP", "IO"] {
            accounts.push(json!({
                "name": format!("jupiter_source_{}", asset.to_ascii_lowercase()),
                "owner": jupiter,
                "subaccount_hex": null,
                "icrc_account": {"owner": jupiter, "subaccount_hex": null},
                "icp_account_identifier": if asset == "ICP" {Value::String("TBD_DERIVED_AFTER_ACCEPTANCE_JUPITER_PRINCIPAL".into())} else {Value::Null},
                "role": "acceptance Jupiter/source default Account",
                "asset": asset,
                "initial_expected_balance_e8s": null,
                "steady_state_classification": "external deterministic source",
                "excluded_from_claim_supply": false,
                "user_funding_required": true
            }));
        }
    } else {
        accounts.push(account_json(
            "jupiter_source_icp",
            jupiter,
            None,
            "ICP",
            "acceptance Jupiter/source default Account",
            Value::Null,
            "external deterministic source",
            false,
            true,
        )?);
        accounts.push(account_json(
            "jupiter_source_io",
            jupiter,
            None,
            "IO",
            "acceptance Jupiter/source default Account",
            Value::Null,
            "external deterministic source",
            false,
            true,
        )?);
    }
    let permanent_subaccount = inputs["permanent_neuron_observation"]["staking_subaccount_hex"]
        .as_str()
        .ok_or("permanent staking subaccount must be hex text")?;
    let permanent_identifier = if is_placeholder(permanent_subaccount) {
        "TBD_DERIVED_AFTER_PERMANENT_NEURON_OBSERVATION".to_string()
    } else {
        let bytes: [u8; 32] = hex::decode(permanent_subaccount)
            .map_err(|err| format!("permanent subaccount hex: {err}"))?
            .try_into()
            .map_err(|_| "permanent subaccount must contain 32 bytes")?;
        LedgerAccount::new(
            principal(NNS_GOVERNANCE_PRINCIPAL, "NNS Governance")?,
            Some(Subaccount(bytes)),
        )
        .icp_account_identifier_text()
    };
    accounts.push(json!({
        "name": "nns_permanent_neuron_stake",
        "owner": NNS_GOVERNANCE_PRINCIPAL,
        "subaccount_hex": inputs["permanent_neuron_observation"]["staking_subaccount_hex"],
        "icrc_account": {"owner": NNS_GOVERNANCE_PRINCIPAL, "subaccount_hex": inputs["permanent_neuron_observation"]["staking_subaccount_hex"]},
        "icp_account_identifier": permanent_identifier,
        "role": "acceptance permanent two-year NNS neuron",
        "asset": "ICP",
        "initial_expected_balance_e8s": inputs["permanent_neuron_observation"]["cached_stake_e8s"],
        "steady_state_classification": "existing principal plus operator-funded delta K; maturity remains separate",
        "excluded_from_claim_supply": false,
        "user_funding_required": false
    }));

    Ok(json!({
        "schema": "io.acceptance.accounts.v1",
        "environment": "acceptance",
        "derivation_authority": "io_accounts + io_ledger_types + SNS/NNS domain-separated SHA-256",
        "accounts": accounts
    }))
}

fn is_placeholder(value: &str) -> bool {
    value.starts_with("TBD_")
}

fn contains_placeholder(value: &Value) -> bool {
    match value {
        Value::String(text) => text.contains("TBD_"),
        Value::Array(values) => values.iter().any(contains_placeholder),
        Value::Object(values) => values.values().any(contains_placeholder),
        _ => false,
    }
}

fn named_account<'a>(manifest: &'a Value, name: &str) -> Result<&'a Value, String> {
    manifest["accounts"]
        .as_array()
        .and_then(|accounts| accounts.iter().find(|account| account["name"] == name))
        .ok_or_else(|| format!("acceptance Account manifest is missing {name}"))
}

fn value_u128(value: &Value, field: &str) -> Result<u128, String> {
    value
        .as_u64()
        .map(u128::from)
        .ok_or_else(|| format!("{field} must be a resolved non-negative integer"))
}

fn optional_observed_u128(value: &Value, field: &str) -> Result<Option<u128>, String> {
    if value.as_str().is_some_and(is_placeholder) {
        Ok(None)
    } else {
        value_u128(value, field).map(Some)
    }
}

fn planned_sale_allocation(inputs: &Value) -> Result<Option<u128>, String> {
    let value = &inputs["sale_plan"]["initial_swap_io_allocation_e8s"];
    if value.as_str().is_some_and(is_placeholder) {
        return Ok(None);
    }
    value_u128(value, "sale_plan.initial_swap_io_allocation_e8s").map(Some)
}

fn reserve_stake(sale_allocation_e8s: u128) -> Result<u128, String> {
    TOTAL_IO_E8S
        .checked_sub(GOVERNANCE_IO_E8S)
        .and_then(|value| value.checked_sub(JUPITER_GOVERNANCE_IO_E8S))
        .and_then(|value| value.checked_sub(sale_allocation_e8s))
        .filter(|value| *value > IO_FEE_E8S)
        .ok_or_else(|| "sale allocation leaves no usable reserve-transport stake".into())
}

fn capitalization_plan(
    total_supply_e8s: u128,
    stream_reserve_e8s: u128,
    excluded_balances_e8s: &[u128],
) -> Result<CapitalizationPlan, String> {
    let claims_e8s =
        io_core_model::claim_supply(total_supply_e8s, stream_reserve_e8s, excluded_balances_e8s)
            .map_err(|error| format!("canonical C0 observation is invalid: {error:?}"))?;
    if claims_e8s == 0 {
        return Err("canonical C0 must be positive after the sale".into());
    }
    let twice_claims = claims_e8s.checked_mul(2).ok_or("2*C0 overflows u128")?;
    let quotient = twice_claims / 3;
    let remainder = (twice_claims % 3) as u8;
    let permanent_top_up_e8s = quotient
        .checked_add(u128::from(remainder != 0))
        .ok_or("rounded permanent top-up overflows u128")?;
    Ok(CapitalizationPlan {
        claims_e8s,
        claim_backing_e8s: claims_e8s,
        permanent_top_up_e8s,
        permanent_exact_remainder_thirds: remainder,
    })
}

fn require_successful_finalization(
    observed: bool,
    swap_lifecycle: &str,
    governance_mode: &str,
) -> Result<(), String> {
    if observed && swap_lifecycle == "Committed" && governance_mode == "Normal" {
        Ok(())
    } else {
        Err("successful sale finalization must be canonically observed as Committed/Normal".into())
    }
}

fn validate_manifest(manifest: &AcceptanceManifest) -> Result<(), String> {
    let expected = BTreeMap::from([
        ("sns_governance", "h7y7s-kyaaa-aaaas-amxrq-cai"),
        ("sns_root", "hk7o7-lqaaa-aaaas-amxsa-cai"),
        ("sns_ledger", "hn6il-giaaa-aaaas-amxsq-cai"),
        ("sns_index", "he5dx-qaaaa-aaaas-amxta-cai"),
        ("sns_swap", "hd4fd-5yaaa-aaaas-amxtq-cai"),
        ("stream_manager", "gjthz-7iaaa-aaaas-amxuq-cai"),
        ("nns_manager", "gosbn-sqaaa-aaaas-amxua-cai"),
        ("historian", "gaqmf-jaaaa-aaaas-amxva-cai"),
        ("frontend", "ghrkr-eyaaa-aaaas-amxvq-cai"),
    ]);
    if manifest.reward_share_baseline_commit != approved_sns_source() {
        return Err("acceptance reward-share baseline commit changed".into());
    }
    if manifest.roles.len() != expected.len() {
        return Err("acceptance role allowlist must contain exactly nine roles".into());
    }
    let mut principals = BTreeSet::new();
    for (name, expected_principal) in expected {
        let actual = role(manifest, name)?;
        if actual != expected_principal {
            return Err(format!("acceptance {name} changed: {actual}"));
        }
        principal(actual, name)?;
        if !principals.insert(actual) {
            return Err(format!("duplicate acceptance role principal: {actual}"));
        }
    }
    if manifest
        .system_canisters
        .get("icp_ledger")
        .map(String::as_str)
        != Some(ICP_LEDGER_PRINCIPAL)
        || manifest
            .system_canisters
            .get("icp_index")
            .map(String::as_str)
            != Some(ICP_INDEX_PRINCIPAL)
        || manifest
            .system_canisters
            .get("nns_governance")
            .map(String::as_str)
            != Some(NNS_GOVERNANCE_PRINCIPAL)
    {
        return Err("canonical system-canister allowlist changed".into());
    }
    if manifest.permanent_nns_neuron.id != ACCEPTANCE_NEURON_ID
        || manifest.permanent_nns_neuron.controller != role(manifest, "nns_manager")?
    {
        return Err("acceptance permanent neuron identity/controller mismatch".into());
    }
    let forbidden = [
        PRODUCTION_IO_STREAM_MANAGER_CANISTER_ID,
        PRODUCTION_IO_NNS_NEURON_MANAGER_CANISTER_ID,
        PRODUCTION_IO_HISTORIAN_CANISTER_ID,
        PRODUCTION_FRONTEND_CANISTER_ID,
        JUPITER_FAUCET_CANISTER_ID,
    ];
    for value in manifest.roles.values() {
        if forbidden.contains(&value.as_str()) {
            return Err(format!(
                "production principal contaminated acceptance: {value}"
            ));
        }
    }
    if manifest.permanent_nns_neuron.id == PROTECTED_IO_NNS_NEURON_ID {
        return Err("production permanent neuron contaminated acceptance".into());
    }
    Ok(())
}

fn validate_economics(e: &Economics) -> Result<(), String> {
    if e.schema != "io.sns.economics.v1"
        || e.developer_voting_power_percent != 100
        || e.sale_allocation_source
            != "required-inputs.draft.json#sale_plan.initial_swap_io_allocation_e8s"
        || e.sale_price_constraint
            != "accepted_direct_participation_icp_e8s == initial_swap_io_allocation_e8s"
        || e.governance_neuron.memo_domain != "IOGOV001"
        || e.jupiter_governance_neuron.memo_domain != "IOJUP001"
        || e.reserve_transport_neuron.memo_domain != "IORES001"
        || e.jupiter_governance_neuron.io_reward_eligible != Some(false)
        || e.jupiter_governance_neuron.controller_source.as_deref()
            != Some("environment-overlay#jupiter_sns_rewards_canister_id")
        || e.governance_neuron.io_reward_eligible.is_some()
        || e.reserve_transport_neuron.io_reward_eligible.is_some()
        || e.governance_neuron.controller_source.is_some()
        || e.reserve_transport_neuron.controller_source.is_some()
        || e.governance_neuron.vesting_period_seconds != 0
    {
        return Err("canonical economic classification or source mismatch".into());
    }
    if e.token_name != "IO" || e.token_symbol != "IO" || e.decimals != 8 {
        return Err("token identity/decimals changed".into());
    }
    if e.total_supply_e8s != TOTAL_IO_E8S
        || e.governance_neuron.stake_e8s != json!(GOVERNANCE_IO_E8S)
        || e.jupiter_governance_neuron.stake_e8s != json!(JUPITER_GOVERNANCE_IO_E8S)
        || e.reserve_transport_neuron.stake_e8s
            != "TOTAL_SUPPLY_E8S - GOVERNANCE_STAKE_E8S - JUPITER_GOVERNANCE_STAKE_E8S - SALE_IO_ALLOCATION_E8S"
    {
        return Err("parameterized genesis 1B arithmetic mismatch".into());
    }
    if e.governance_neuron.memo != GOVERNANCE_MEMO
        || e.jupiter_governance_neuron.memo != JUPITER_GOVERNANCE_MEMO
        || e.reserve_transport_neuron.memo != RESERVE_MEMO
        || e.governance_neuron.memo == e.reserve_transport_neuron.memo
        || e.governance_neuron.memo == e.jupiter_governance_neuron.memo
        || e.jupiter_governance_neuron.memo == e.reserve_transport_neuron.memo
        || (SALE_MEMO_START..=SALE_MEMO_END).contains(&e.governance_neuron.memo)
        || (SALE_MEMO_START..=SALE_MEMO_END).contains(&e.jupiter_governance_neuron.memo)
        || (SALE_MEMO_START..=SALE_MEMO_END).contains(&e.reserve_transport_neuron.memo)
    {
        return Err("genesis neuron memo policy violated".into());
    }
    if !e.governance_neuron.claim_bearing
        || e.jupiter_governance_neuron.claim_bearing
        || e.reserve_transport_neuron.claim_bearing
        || e.jupiter_governance_neuron.dissolve_delay_seconds
            != io_core_model::SNS_USER_DISSOLVE_DELAY_SECONDS
        || e.jupiter_governance_neuron.vesting_period_seconds != 0
        || e.reserve_transport_neuron.dissolve_delay_seconds != 0
        || e.reserve_transport_neuron.vesting_period_seconds != 0
        || e.governance_neuron.dissolve_delay_seconds
            != io_core_model::SNS_USER_DISSOLVE_DELAY_SECONDS
    {
        return Err("genesis neuron economic classification mismatch".into());
    }
    if e.transfer_fee_e8s != IO_FEE_E8S
        || e.treasury_e8s != 0
        || e.future_swap_e8s != 0
        || e.minimum_participants != 1
        || e.neurons_fund_participation
        || e.gross_sale_reference_price != "1 ICP per IO"
    {
        return Err("sale economics mismatch".into());
    }
    if e.maximum_dissolve_delay_bonus_basis_points != 0
        || e.maximum_neuron_age_seconds != 0
        || e.maximum_age_bonus_basis_points != 0
        || e.initial_reward_rate_basis_points != 0
        || e.final_reward_rate_basis_points != 0
        || e.reward_rate_transition_seconds != 0
        || e.maximum_number_of_neurons != 1_000
        || e.initial_voting_period_seconds != 86_400
        || e.wait_for_quiet_deadline_increase_seconds != 43_200
        || e.reward_event_duration_seconds != 86_400
        || e.minimum_voting_dissolve_delay_seconds != 1_209_599
        || e.maximum_dissolve_delay_seconds != io_core_model::SNS_USER_DISSOLVE_DELAY_SECONDS
        || e.minimum_neuron_stake_e8s != 100_000_000
        || e.proposal_reject_cost_e8s != PROPOSAL_REJECT_COST_E8S
    {
        return Err("locked SNS governance/reward economics mismatch".into());
    }
    Ok(())
}

fn scalar(value: &Value, fallback: &str) -> String {
    value
        .as_u64()
        .map(|value| value.to_string())
        .or_else(|| value.as_str().map(str::to_string))
        .unwrap_or_else(|| fallback.to_string())
}

// These are early deployment checks, not a substitute for the pinned SNS parser.
fn validate_sale_plan(e: &Economics, inputs: &Value, sealed: bool) -> Result<(), String> {
    let plan = &inputs["sale_plan"];
    let Some(sale) = planned_sale_allocation(inputs)? else {
        return if sealed {
            Err("unresolved sale allocation".into())
        } else {
            Ok(())
        };
    };
    let count = value_u128(&plan["basket_count"], "basket_count")?;
    let interval = value_u128(&plan["basket_interval_seconds"], "basket_interval_seconds")?;
    let minimum = value_u128(
        &plan["minimum_participant_icp_e8s"],
        "minimum_participant_icp_e8s",
    )?;
    let required = count
        .checked_mul(e.minimum_neuron_stake_e8s + e.transfer_fee_e8s)
        .ok_or("basket minimum overflow")?;
    if !(2..=10).contains(&count)
        || interval == 0
        || (count - 1)
            .checked_mul(interval)
            .is_none_or(|delay| delay > u128::from(e.maximum_dissolve_delay_seconds))
        || minimum < required
        || minimum > sale
    {
        return Err(
            "sale basket cannot fund the minimum stakes and per-neuron fees at 1 ICP/IO".into(),
        );
    }
    if sealed {
        if plan["approved_for_deployment"] != true {
            return Err("sale candidate has not been approved for deployment".into());
        }
        for (field, minimum) in [("final_token_name", 4), ("final_token_symbol", 3)] {
            let value = inputs[field]
                .as_str()
                .ok_or_else(|| format!("missing {field}"))?;
            if is_placeholder(value) || value.len() < minimum {
                return Err(format!(
                    "{field} is unresolved or below the SNS minimum length {minimum}"
                ));
            }
        }
    }
    Ok(())
}

fn render_sns_yaml(e: &Economics, overlay: &Overlay, inputs: &Value) -> Result<String, String> {
    validate_economics(e)?;
    let dapps = overlay
        .dapp_canisters
        .values()
        .map(|principal| format!("  - \"{principal}\""))
        .collect::<Vec<_>>()
        .join("\n");
    let sale_allocation = planned_sale_allocation(inputs)?;
    let sale = sale_allocation.map_or_else(
        || "TBD_ACCEPTANCE_SALE_IO_ALLOCATION_E8S".to_string(),
        |value| value.to_string(),
    );
    let reserve_stake = match sale_allocation {
        Some(value) => reserve_stake(value)?.to_string(),
        None => "TBD_RESERVE_TRANSPORT_STAKE_E8S".to_string(),
    };
    let minimum_participant = scalar(
        &inputs["sale_plan"]["minimum_participant_icp_e8s"],
        "TBD_ACCEPTANCE_MINIMUM_PARTICIPANT_ICP_E8S",
    );
    let controller = scalar(
        &inputs["genesis_controller_principal"],
        "TBD_GENESIS_CONTROLLER_PRINCIPAL",
    );
    let jupiter_governance_controller = &overlay.jupiter_sns_rewards_canister_id;
    // JSON strings are also YAML quoted scalars; paths/metadata cannot inject YAML.
    let quoted = |key: &str, fallback: &str| json!(scalar(&inputs[key], fallback)).to_string();
    let logo = quoted("sns_logo_path", "TBD_SNS_LOGO_PATH");
    let name = quoted("final_token_name", "TBD_FINAL_TOKEN_NAME");
    let symbol = quoted("final_token_symbol", "TBD_FINAL_TOKEN_SYMBOL");
    let proposal_url = quoted("proposal_url", "TBD_PROPOSAL_URL");
    let project_url = quoted("project_url", "TBD_PROJECT_URL");
    let basket_count = scalar(&inputs["sale_plan"]["basket_count"], "TBD_BASKET_COUNT");
    let basket_interval = scalar(
        &inputs["sale_plan"]["basket_interval_seconds"],
        "TBD_BASKET_INTERVAL_SECONDS",
    );
    let start_time = scalar(
        &inputs["sale_plan"]["start_time"],
        "TBD_ACCEPTANCE_SALE_START_TIME",
    );
    let duration = scalar(
        &inputs["sale_plan"]["duration_seconds"],
        "TBD_ACCEPTANCE_SALE_DURATION_SECONDS",
    );
    Ok(format!(
        "# Generated from canonical-sns-economics.json + {environment}-identity-overlay.json.\n\
name: {name}\n\
description: IO protocol ({environment}) small-sale launch\n\
NnsProposal:\n  title: IO small-sale {environment} launch\n  url: {proposal_url}\n  summary: IO small-sale governance and protocol {environment} launch.\n\
logo: {logo}\n\
url: {project_url}\n\
fallback_controller_principals:\n  - {fallback}\n\
dapp_canisters:\n{dapps}\n\
Token:\n  name: {name}\n  symbol: {symbol}\n  transaction_fee: {fee} e8s\n  logo: {logo}\n\
Proposals:\n  rejection_fee: {reject} e8s\n  initial_voting_period: {initial_voting_period} seconds\n  maximum_wait_for_quiet_deadline_extension: {wait_for_quiet} seconds\n\
Neurons:\n  minimum_creation_stake: {minimum_stake} e8s\n\
Voting:\n  minimum_dissolve_delay: {minimum_delay} seconds\n  MaximumVotingPowerBonuses:\n    DissolveDelay:\n      duration: {maximum_delay} seconds\n      bonus: {dissolve_bonus}%\n    Age:\n      duration: {maximum_age} seconds\n      bonus: {age_bonus}%\n  RewardRate:\n    initial: {initial_reward}%\n    final: {final_reward}%\n    transition_duration: {reward_transition} seconds\n\
Distribution:\n  Neurons:\n    - principal: {controller}\n      dissolve_delay: {governance_delay} seconds\n      memo: {governance_memo}\n      stake: {governance_stake} e8s\n      vesting_period: 0 seconds\n    - principal: {jupiter_governance_controller}\n      dissolve_delay: {jupiter_governance_delay} seconds\n      memo: {jupiter_governance_memo}\n      stake: {jupiter_governance_stake} e8s\n      vesting_period: 0 seconds\n    - principal: {controller}\n      dissolve_delay: 0 seconds\n      memo: {reserve_memo}\n      stake: {reserve_stake} e8s\n      vesting_period: 0 seconds\n  InitialBalances:\n    treasury: 0 e8s\n    swap: {sale} e8s\n  total: {total} e8s\n\
Swap:\n  minimum_participants: 1\n  minimum_direct_participation_icp: {sale} e8s\n  maximum_direct_participation_icp: {sale} e8s\n  minimum_participant_icp: {minimum_participant} e8s\n  maximum_participant_icp: {sale} e8s\n  start_time: {start_time}\n  duration: {duration} seconds\n  neurons_fund_participation: false\n  VestingSchedule:\n    events: {basket_count}\n    interval: {basket_interval} seconds\n  confirmation_text: IO {environment} sale at a gross reference price of 1 ICP per IO.\n",
        initial_voting_period = e.initial_voting_period_seconds,
        wait_for_quiet = e.wait_for_quiet_deadline_increase_seconds,
        dissolve_bonus = e.maximum_dissolve_delay_bonus_basis_points / 100,
        maximum_age = e.maximum_neuron_age_seconds,
        age_bonus = e.maximum_age_bonus_basis_points / 100,
        initial_reward = e.initial_reward_rate_basis_points / 100,
        final_reward = e.final_reward_rate_basis_points / 100,
        reward_transition = e.reward_rate_transition_seconds,
        environment = overlay.environment,
        name = name,
        symbol = symbol,
        fallback = overlay.fallback_controller_principal,
        fee = e.transfer_fee_e8s,
        reject = e.proposal_reject_cost_e8s,
        minimum_stake = e.minimum_neuron_stake_e8s,
        minimum_delay = e.minimum_voting_dissolve_delay_seconds,
        maximum_delay = e.maximum_dissolve_delay_seconds,
        governance_delay = e.governance_neuron.dissolve_delay_seconds,
        governance_memo = e.governance_neuron.memo,
        governance_stake = e.governance_neuron.stake_e8s,
        jupiter_governance_controller = jupiter_governance_controller,
        jupiter_governance_delay = e.jupiter_governance_neuron.dissolve_delay_seconds,
        jupiter_governance_memo = e.jupiter_governance_neuron.memo,
        jupiter_governance_stake = e.jupiter_governance_neuron.stake_e8s,
        reserve_memo = e.reserve_transport_neuron.memo,
        reserve_stake = reserve_stake,
        sale = sale,
        total = e.total_supply_e8s,
        minimum_participant = minimum_participant,
        controller = controller,
        logo = logo,
        start_time = start_time,
        duration = duration,
    ))
}

fn validate_rendered_yaml(root: &Path, e: &Economics) -> Result<(), String> {
    let acceptance = overlay(root, "acceptance")?;
    let production = overlay(root, "production")?;
    let manifest = manifest(root)?;
    let inputs: Value = read_json(root, &format!("{DIR}/required-inputs.draft.json"))?;
    if !is_placeholder(&acceptance.jupiter_sns_rewards_canister_id) {
        principal(
            &acceptance.jupiter_sns_rewards_canister_id,
            "acceptance Jupiter SNS Rewards",
        )?;
    }
    if acceptance.environment != "acceptance"
        || acceptance.dapp_canisters.len() != 4
        || (acceptance.jupiter_sns_rewards_canister_id
            != "TBD_ACCEPTANCE_JUPITER_SNS_REWARDS_CANISTER_ID"
            && acceptance.jupiter_sns_rewards_canister_id
                == PRODUCTION_JUPITER_SNS_REWARDS_CANISTER_ID)
        || acceptance
            .dapp_canisters
            .get("stream_manager")
            .map(String::as_str)
            != Some(role(&manifest, "stream_manager")?)
        || acceptance
            .dapp_canisters
            .get("nns_manager")
            .map(String::as_str)
            != Some(role(&manifest, "nns_manager")?)
        || acceptance
            .dapp_canisters
            .get("historian")
            .map(String::as_str)
            != Some(role(&manifest, "historian")?)
        || acceptance
            .dapp_canisters
            .get("frontend")
            .map(String::as_str)
            != Some(role(&manifest, "frontend")?)
    {
        return Err("acceptance identity overlay is not the frozen dapp allowlist".into());
    }
    let expected_production = BTreeSet::from([
        PRODUCTION_IO_STREAM_MANAGER_CANISTER_ID,
        PRODUCTION_IO_NNS_NEURON_MANAGER_CANISTER_ID,
        PRODUCTION_IO_HISTORIAN_CANISTER_ID,
        PRODUCTION_FRONTEND_CANISTER_ID,
    ]);
    if production.environment != "production"
        || production.jupiter_sns_rewards_canister_id != PRODUCTION_JUPITER_SNS_REWARDS_CANISTER_ID
        || production
            .dapp_canisters
            .values()
            .map(String::as_str)
            .collect::<BTreeSet<_>>()
            != expected_production
    {
        return Err("production overlay diverges from the production principal registry".into());
    }
    let acceptance_rendered = render_sns_yaml(e, &acceptance, &inputs)?;
    let production_rendered = render_sns_yaml(e, &production, &inputs)?;
    for (environment, expected) in [
        ("acceptance", acceptance_rendered),
        ("production", production_rendered),
    ] {
        let path = root.join(format!("{DIR}/{environment}-sns-init.draft.yaml"));
        let actual =
            fs::read_to_string(&path).map_err(|err| format!("read {}: {err}", path.display()))?;
        if actual != expected {
            return Err(format!(
                "{} is stale; run `cargo run -p xtask -- acceptance_render {environment}`",
                path.display()
            ));
        }
    }
    Ok(())
}

fn validate_static_artifacts(root: &Path, sealed: bool) -> Result<(), String> {
    let required = [
        "acceptance-manifest.json",
        "sns-source-baseline.json",
        "canonical-sns-economics.json",
        "acceptance-identity-overlay.json",
        "production-identity-overlay.json",
        "required-inputs.draft.json",
        "sale-contract.json",
        "launch-observation.draft.json",
        "genesis-neurons.draft.json",
        "stream-exclusions.draft.json",
        "funding.draft.json",
        "io-init-configs.draft.json",
        "controller-topology.draft.json",
        "blockers.json",
        "README.md",
        "runbook.md",
    ];
    for name in required {
        let path = root.join(DIR).join(name);
        if !path.is_file() {
            return Err(format!(
                "required acceptance artifact missing: {}",
                path.display()
            ));
        }
    }
    let inputs: Value = read_json(root, &format!("{DIR}/required-inputs.draft.json"))?;
    if let Some(sale_allocation) = planned_sale_allocation(&inputs)? {
        let reserve = reserve_stake(sale_allocation)?;
        if GOVERNANCE_IO_E8S
            .checked_add(reserve)
            .and_then(|value| value.checked_add(JUPITER_GOVERNANCE_IO_E8S))
            .and_then(|value| value.checked_add(sale_allocation))
            != Some(TOTAL_IO_E8S)
        {
            return Err(
                "resolved governance + Jupiter + reserve + sale distribution is not 1B IO".into(),
            );
        }
        let minimum_participant = value_u128(
            &inputs["sale_plan"]["minimum_participant_icp_e8s"],
            "sale_plan.minimum_participant_icp_e8s",
        )?;
        if minimum_participant == 0 || minimum_participant > sale_allocation {
            return Err("sale minimum participant amount is outside the fixed sale".into());
        }
    }
    let acceptance_overlay = overlay(root, "acceptance")?;
    if inputs["acceptance_jupiter_sns_rewards_canister_id"]
        != acceptance_overlay.jupiter_sns_rewards_canister_id
    {
        return Err(
            "acceptance Jupiter SNS Rewards input and identity overlay must match exactly".into(),
        );
    }
    if sealed && is_placeholder(&acceptance_overlay.jupiter_sns_rewards_canister_id) {
        return Err(
            "sealed acceptance package requires ACCEPTANCE_JUPITER_SNS_REWARDS_CANISTER_ID".into(),
        );
    }
    if sealed && contains_placeholder(&inputs) {
        return Err("sealed acceptance package contains unresolved TBD inputs".into());
    }
    if sealed {
        let manifest = manifest(root)?;
        let controller = inputs["genesis_controller_principal"]
            .as_str()
            .ok_or("sealed genesis controller must be text")?;
        let jupiter = inputs["acceptance_jupiter_source_principal"]
            .as_str()
            .ok_or("sealed Jupiter principal must be text")?;
        let jupiter_governance = principal(
            &acceptance_overlay.jupiter_sns_rewards_canister_id,
            "sealed acceptance Jupiter SNS Rewards",
        )?;
        let controller = principal(controller, "sealed genesis controller")?;
        let jupiter = principal(jupiter, "sealed Jupiter")?;
        let forbidden = BTreeSet::from([
            principal(
                PRODUCTION_IO_STREAM_MANAGER_CANISTER_ID,
                "production Stream",
            )?,
            principal(
                PRODUCTION_IO_NNS_NEURON_MANAGER_CANISTER_ID,
                "production NNS manager",
            )?,
            principal(PRODUCTION_IO_HISTORIAN_CANISTER_ID, "production Historian")?,
            principal(PRODUCTION_FRONTEND_CANISTER_ID, "production Frontend")?,
            principal(JUPITER_FAUCET_CANISTER_ID, "production Jupiter")?,
            principal(
                PRODUCTION_JUPITER_SNS_REWARDS_CANISTER_ID,
                "production Jupiter SNS Rewards",
            )?,
        ]);
        if forbidden.contains(&controller)
            || forbidden.contains(&jupiter)
            || forbidden.contains(&jupiter_governance)
        {
            return Err("sealed acceptance input contains a production IO principal".into());
        }
        let role_principals = manifest
            .roles
            .values()
            .map(|value| principal(value, "acceptance role"))
            .collect::<Result<BTreeSet<_>, _>>()?;
        if role_principals.contains(&controller)
            || role_principals.contains(&jupiter)
            || role_principals.contains(&jupiter_governance)
            || jupiter_governance == controller
            || jupiter_governance == jupiter
            || manifest
                .system_canisters
                .values()
                .map(|value| principal(value, "system canister"))
                .collect::<Result<BTreeSet<_>, _>>()?
                .contains(&jupiter)
            || manifest
                .system_canisters
                .values()
                .map(|value| principal(value, "system canister"))
                .collect::<Result<BTreeSet<_>, _>>()?
                .contains(&jupiter_governance)
        {
            return Err(
                "sealed operator/Jupiter principals collide with distinct canister roles".into(),
            );
        }
        let observation = &inputs["permanent_neuron_observation"];
        if observation["neuron_id"] != json!(ACCEPTANCE_NEURON_ID)
            || observation["controller"] != role(&manifest, "nns_manager")?
            || observation["auto_stake_maturity"] != false
            || observation["maturity_disbursement_in_progress"] != false
            || observation["dissolve_state"] != "NotDissolving"
            || observation["dissolve_delay_seconds"] != json!(PERMANENT_DELAY_SECONDS)
            || observation["cached_stake_e8s"]
                .as_u64()
                .is_none_or(|value| value == 0)
        {
            return Err("sealed permanent-neuron observation violates launch invariants".into());
        }
        for name in [
            "acceptance-identity-overlay.json",
            "acceptance-accounts.draft.json",
            "genesis-neurons.draft.json",
            "stream-exclusions.draft.json",
            "funding.draft.json",
            "io-init-configs.draft.json",
            "controller-topology.draft.json",
            "sale-contract.json",
        ] {
            let value: Value = read_json(root, &format!("{DIR}/{name}"))?;
            if contains_placeholder(&value) {
                return Err(format!(
                    "sealed acceptance artifact still contains TBD: {name}"
                ));
            }
        }
        let yaml = fs::read_to_string(root.join(format!("{DIR}/acceptance-sns-init.draft.yaml")))
            .map_err(|err| format!("read sealed acceptance YAML: {err}"))?;
        if yaml.contains("TBD_") {
            return Err("sealed acceptance SNS YAML still contains TBD".into());
        }
    }
    let exclusions: Value = read_json(root, &format!("{DIR}/stream-exclusions.draft.json"))?;
    let account_manifest: Value =
        read_json(root, &format!("{DIR}/acceptance-accounts.draft.json"))?;
    let frozen_manifest = manifest(root)?;
    let reserve_account = named_account(&account_manifest, "sns_reserve_transport_neuron_stake")?;
    let jupiter_account = named_account(&account_manifest, "sns_jupiter_governance_neuron_stake")?;
    if exclusions["excluded_accounts"].as_array().map(Vec::len) != Some(2)
        || exclusions["excluded_accounts"][0]["source"] != "reserve-transport genesis neuron"
        || exclusions["excluded_accounts"][1]["source"] != "Jupiter governance-only genesis neuron"
        || exclusions["excluded_accounts"][0]["owner"] != reserve_account["owner"]
        || exclusions["excluded_accounts"][0]["subaccount_hex"] != reserve_account["subaccount_hex"]
        || exclusions["excluded_accounts"][1]["owner"] != jupiter_account["owner"]
        || exclusions["excluded_accounts"][1]["subaccount_hex"] != jupiter_account["subaccount_hex"]
        || exclusions["governance_neuron_is_excluded"] != false
    {
        return Err("Stream exclusion manifest is not the locked two-account policy".into());
    }
    let genesis: Value = read_json(root, &format!("{DIR}/genesis-neurons.draft.json"))?;
    if genesis["neurons"].as_array().map(Vec::len) != Some(3)
        || genesis["neurons"][0]["stake_e8s"] != json!(GOVERNANCE_IO_E8S)
        || genesis["neurons"][0]["stream_excluded"] != false
        || genesis["neurons"][1]["controller"] != acceptance_overlay.jupiter_sns_rewards_canister_id
        || genesis["neurons"][1]["ledger_account_owner"]
            != role(&frozen_manifest, "sns_governance")?
        || genesis["neurons"][1]["memo"] != json!(JUPITER_GOVERNANCE_MEMO)
        || genesis["neurons"][1]["stake_e8s"] != json!(JUPITER_GOVERNANCE_IO_E8S)
        || genesis["neurons"][1]["stream_excluded"] != true
        || genesis["neurons"][2]["memo"] != json!(RESERVE_MEMO)
        || genesis["neurons"][2]["stream_excluded"] != true
        || genesis["jupiter_staking_account_derivations"]["production"]["controller"]
            != PRODUCTION_JUPITER_SNS_REWARDS_CANISTER_ID
        || genesis["jupiter_staking_account_derivations"]["production"]["subaccount_hex"]
            != "7ad7034691f63c7f831a919cde4a7960de689e842d4aa256f1b0c6623c8da0a7"
    {
        return Err("genesis neuron manifest diverges from the locked three-neuron model".into());
    }
    let funding: Value = read_json(root, &format!("{DIR}/funding.draft.json"))?;
    if funding["stream_claim_backing_target_e8s"] != "C0"
        || funding["additional_permanent_capital_transfer_e8s"] != "ceil(2 * C0 / 3)"
        || funding["dynamic_anchor_e8s"] != json!(DYNAMIC_ANCHOR_E8S)
        || funding["standalone_semantic_fee_float_e8s"] != json!(0)
        || funding["jupiter_governance_allocation_e8s"] != json!(JUPITER_GOVERNANCE_IO_E8S)
    {
        return Err("acceptance funding manifest mismatch".into());
    }
    let init: Value = read_json(root, &format!("{DIR}/io-init-configs.draft.json"))?;
    let manifest = manifest(root)?;
    if init["stream_manager"]["io_ledger"] != role(&manifest, "sns_ledger")?
        || init["stream_manager"]["io_index"] != role(&manifest, "sns_index")?
        || init["stream_manager"]["nns_manager"] != role(&manifest, "nns_manager")?
        || init["stream_manager"]["sns_governance"] != role(&manifest, "sns_governance")?
        || init["stream_manager"]["sns_root"] != role(&manifest, "sns_root")?
        || init["stream_manager"]["io_reserve"]["subaccount_hex"]
            != hex::encode(STREAM_RESERVE_SUBACCOUNT)
        || init["stream_manager"]["liquid_icp"]["subaccount_hex"]
            != hex::encode(STREAM_LIQUID_SUBACCOUNT)
        || init["nns_manager"]["two_year_neuron_id"] != json!(ACCEPTANCE_NEURON_ID)
        || init["nns_manager"]["pooled_parent_memo"] != json!(0)
        || init["nns_manager"]["pooled_parent_followee_id"] != json!(ACCEPTANCE_NEURON_ID)
        || init["nns_manager"]["stream_manager"] != role(&manifest, "stream_manager")?
        || init["stream_manager"]["nonredeemable_governance_io_accounts"]
            .as_array()
            .map(Vec::len)
            != Some(2)
        || init["stream_manager"]["nonredeemable_governance_io_accounts"][0]["owner"]
            != reserve_account["owner"]
        || init["stream_manager"]["nonredeemable_governance_io_accounts"][0]["subaccount_hex"]
            != reserve_account["subaccount_hex"]
        || init["stream_manager"]["nonredeemable_governance_io_accounts"][1]["owner"]
            != jupiter_account["owner"]
        || init["stream_manager"]["nonredeemable_governance_io_accounts"][1]["subaccount_hex"]
            != jupiter_account["subaccount_hex"]
    {
        return Err(
            "IO acceptance init configuration diverges from frozen identities/Accounts".into(),
        );
    }
    let contract: Value = read_json(root, &format!("{DIR}/sale-contract.json"))?;
    for key in [
        "finalization_observed",
        "swap_committed_terminal_state_observed",
        "governance_normal_observed",
        "accepted_direct_participation_equals_sale_allocation",
        "sale_icp_proceeds_retained_in_sns_governance_default_account",
        "sale_proceeds_excluded_from_io_claim_backing",
        "developer_voting_power_multiplier_percent",
    ] {
        let expected = if key == "developer_voting_power_multiplier_percent" {
            json!(100)
        } else {
            json!(true)
        };
        if contract["required_observed_postconditions"][key] != expected {
            return Err(format!("sale semantic contract missing {key}"));
        }
    }
    let baseline: Value = read_json(root, &format!("{DIR}/sns-source-baseline.json"))?;
    if baseline["commit"] != approved_sns_source()
        || baseline["reward_event_participation_candid"]["reward_event_end_timestamp_seconds"]
            != "opt nat64"
        || baseline["reward_event_participation_candid"]["reward_shares"] != "opt nat"
        || baseline["ledger_fee_semantics"] != "burned"
    {
        return Err("reviewed SNS source baseline facts changed".into());
    }
    Ok(())
}

/// Validate the complete argument vector before any acceptance command runs.
pub(crate) fn validate_cli(command: &str, args: &[String]) -> Result<(), String> {
    let valid = match command {
        "acceptance_local_config" | "acceptance_local_accounts" => args.len() == 2,
        "acceptance_constructors" => args.len() == 1,
        "acceptance_checkpoint" => args.len() == 2 && matches!(args[0].as_str(), "A" | "B" | "C"),
        "acceptance_preflight" => {
            matches!(args, [])
                || (args.len() == 1 && matches!(args[0].as_str(), "draft" | "sealed"))
        }
        "acceptance_render" => {
            args.is_empty()
                || (args.len() == 1
                    && matches!(args[0].as_str(), "acceptance" | "production" | "accounts"))
        }
        "acceptance_capitalization"
        | "acceptance_preframework"
        | "acceptance_preframework_pocketic"
        | "acceptance_sale_real" => args.is_empty(),
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(format!(
            "unknown mode or extra arguments for {command}: {args:?}"
        ))
    }
}

pub(crate) fn validate(root: &Path, sealed: bool) -> Result<(), String> {
    let manifest = manifest(root)?;
    let economics = economics(root)?;
    validate_manifest(&manifest)?;
    validate_economics(&economics)?;
    let inputs: Value = read_json(root, &format!("{DIR}/required-inputs.draft.json"))?;
    validate_sale_plan(&economics, &inputs, sealed)?;
    validate_static_artifacts(root, sealed)?;
    validate_rendered_yaml(root, &economics)?;

    let accounts = expected_accounts(root)?;
    let checked: Value = read_json(root, &format!("{DIR}/acceptance-accounts.draft.json"))?;
    if checked != accounts {
        return Err(format!(
            "{DIR}/acceptance-accounts.draft.json is stale; run `cargo run -p xtask -- acceptance_render accounts`"
        ));
    }

    Ok(())
}

pub(crate) fn render(root: &Path, target: &str) -> Result<String, String> {
    let e = economics(root)?;
    let inputs: Value = read_json(root, &format!("{DIR}/required-inputs.draft.json"))?;
    match target {
        "acceptance" | "production" => render_sns_yaml(&e, &overlay(root, target)?, &inputs),
        "accounts" => serde_json::to_string_pretty(&expected_accounts(root)?)
            .map(|text| text + "\n")
            .map_err(|err| err.to_string()),
        _ => Err("acceptance_render expects acceptance, production, or accounts".into()),
    }
}

/// Render only into an explicit local fixture, without approving deployment inputs.
pub(crate) fn local_config(root: &Path, context_path: &Path, output: &Path) -> Result<(), String> {
    let context: Value =
        serde_json::from_slice(&fs::read(context_path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    if context["local_fixture"] != true {
        return Err("local metadata requires explicit local_fixture=true".into());
    }
    let mut inputs: Value = read_json(root, &format!("{DIR}/required-inputs.draft.json"))?;
    for field in [
        "operator",
        "jupiter",
        "stream",
        "nns_manager",
        "historian",
        "frontend",
    ] {
        principal(
            context[field]
                .as_str()
                .ok_or_else(|| format!("missing local {field}"))?,
            field,
        )?;
    }
    inputs["genesis_controller_principal"] = context["operator"].clone();
    inputs["final_token_name"] = json!("IO Local Fixture");
    inputs["final_token_symbol"] = json!("IOLT");
    // The pinned NNS proposal validator requires forum.dfinity.org. The forum
    // homepage is local test metadata, not a claimed deployment discussion.
    inputs["proposal_url"] = json!("https://forum.dfinity.org/");
    inputs["project_url"] = json!("https://example.invalid/io-local-fixture");
    inputs["sns_logo_path"] = context["logo_path"].clone();
    if inputs["sns_logo_path"].as_str().is_none() {
        return Err("missing local logo path".into());
    }
    let environment = match context["fixture_environment"].as_str() {
        None => "local-fixture-not-for-deployment",
        Some(value @ ("acceptance" | "production")) if context["local_fixture"] == true => value,
        _ => return Err("invalid explicitly local equivalence environment".into()),
    };
    let local = Overlay {
        environment: environment.into(),
        // Candidate deferred registration route: uninstalled IO monetary canisters
        // must retain operator installation authority until checkpoint C.
        dapp_canisters: if context["root_install"] == true {
            if context["local_fixture"] != true {
                return Err("Root installation route requires explicit local fixture".into());
            }
            ["frontend", "historian", "nns_manager", "stream"]
                .iter()
                .map(|role| {
                    (
                        (*role).to_string(),
                        context[*role].as_str().unwrap().to_string(),
                    )
                })
                .collect()
        } else {
            BTreeMap::new()
        },
        fallback_controller_principal: context["operator"].as_str().unwrap().into(),
        jupiter_sns_rewards_canister_id: context["jupiter"].as_str().unwrap().into(),
    };
    let mut rendered = render_sns_yaml(&economics(root)?, &local, &inputs)?;
    rendered = rendered.replace("dapp_canisters:\n\n", "dapp_canisters: []\n");
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .map_err(|e| e.to_string())?;
    std::io::Write::write_all(&mut file, rendered.as_bytes()).map_err(|e| e.to_string())
}

pub(crate) fn capitalization(root: &Path) -> Result<String, String> {
    validate(root, false)?;
    let inputs: Value = read_json(root, &format!("{DIR}/required-inputs.draft.json"))?;
    let sale_allocation = planned_sale_allocation(&inputs)?
        .ok_or("sale allocation is unresolved; capitalization cannot be derived")?;
    let observation: Value = read_json(root, &format!("{DIR}/launch-observation.draft.json"))?;
    let sale = &observation["sale_finalization"];
    require_successful_finalization(
        sale["observed"] == true,
        sale["swap_lifecycle"].as_str().unwrap_or_default(),
        sale["governance_mode"].as_str().unwrap_or_default(),
    )?;
    let accepted = value_u128(
        &sale["accepted_direct_participation_icp_e8s"],
        "accepted direct participation",
    )?;
    let neurons_fund = value_u128(
        &sale["accepted_neurons_fund_participation_icp_e8s"],
        "accepted Neurons' Fund participation",
    )?;
    let distributed = value_u128(
        &sale["distributed_sale_neuron_stake_e8s"],
        "distributed sale-neuron stake",
    )?;
    let io_fees = value_u128(&sale["sns_io_transfer_fees_burned_e8s"], "sale IO fees")?;
    let swap_residual = value_u128(&sale["swap_io_residual_e8s"], "Swap IO residual")?;
    let treasury = value_u128(
        &sale["sns_icp_treasury_balance_e8s"],
        "SNS ICP treasury balance",
    )?;
    let icp_fees = value_u128(&sale["icp_sweep_fees_burned_e8s"], "sale ICP sweep fees")?;
    if accepted != sale_allocation || neurons_fund != 0 {
        return Err("observed gross sale price is not exactly 1 ICP per IO".into());
    }
    if distributed
        .checked_add(io_fees)
        .and_then(|value| value.checked_add(swap_residual))
        != Some(sale_allocation)
    {
        return Err(
            "sale IO allocation does not reconcile across stakes, fees and residual".into(),
        );
    }
    if treasury.checked_add(icp_fees) != Some(accepted) {
        return Err(
            "accepted ICP does not reconcile to retained SNS treasury proceeds and fees".into(),
        );
    }

    let checkpoint = &observation["post_reserve_migration"];
    let total_supply = value_u128(
        &checkpoint["ledger_total_supply_e8s"],
        "post-migration total supply",
    )?;
    let stream_reserve = value_u128(
        &checkpoint["stream_reserve_balance_e8s"],
        "post-migration Stream reserve",
    )?;
    let exclusions = checkpoint["explicit_excluded_balances_e8s"]
        .as_array()
        .ok_or("explicit excluded balances must be an array")?
        .iter()
        .enumerate()
        .map(|(index, value)| value_u128(value, &format!("excluded balance {index}")))
        .collect::<Result<Vec<_>, _>>()?;
    if exclusions.as_slice() != [0, JUPITER_GOVERNANCE_IO_E8S] {
        return Err(
            "post-migration exclusions must be zero reserve-neuron balance plus exact 100M Jupiter stake"
                .into(),
        );
    }
    let expected_reserve = reserve_stake(sale_allocation)?
        .checked_sub(IO_FEE_E8S)
        .ok_or("reserve migration amount does not cover its IO fee")?;
    let expected_supply = TOTAL_IO_E8S
        .checked_sub(io_fees)
        .and_then(|value| value.checked_sub(IO_FEE_E8S))
        .ok_or("post-migration supply underflow")?;
    if stream_reserve != expected_reserve || total_supply != expected_supply {
        return Err("post-reserve-migration balances do not match observed fee burns".into());
    }
    let plan = capitalization_plan(total_supply, stream_reserve, &exclusions)?;
    let operator = &observation["operator_capitalization"];
    let backing_before = value_u128(
        &operator["stream_claim_backing_before_e8s"],
        "Stream claim backing before capitalization",
    )?;
    let stream_transfer = plan
        .claim_backing_e8s
        .checked_sub(backing_before)
        .ok_or("pre-existing claim backing exceeds the 1:1 launch target")?;
    if let Some(backing_after) = optional_observed_u128(
        &operator["stream_claim_backing_after_e8s"],
        "Stream claim backing after capitalization",
    )? {
        if backing_after != plan.claim_backing_e8s {
            return Err("operator-funded B0 does not equal canonical C0".into());
        }
        let supply_before = value_u128(
            &operator["io_total_supply_before_e8s"],
            "IO supply before operator capitalization",
        )?;
        let supply_after = value_u128(
            &operator["io_total_supply_after_e8s"],
            "IO supply after operator capitalization",
        )?;
        if supply_before != supply_after || supply_after != total_supply {
            return Err("manual capitalization changed IO supply".into());
        }
        let permanent_before = value_u128(
            &operator["permanent_cached_stake_before_e8s"],
            "permanent stake before top-up",
        )?;
        let permanent_after = value_u128(
            &operator["permanent_cached_stake_after_refresh_e8s"],
            "permanent stake after refresh",
        )?;
        if permanent_before.checked_add(plan.permanent_top_up_e8s) != Some(permanent_after) {
            return Err(
                "permanent-neuron credit does not match the calculated 40/60 top-up".into(),
            );
        }
    }
    let rounding_excess_thirds = match plan.permanent_exact_remainder_thirds {
        0 => 0,
        remainder => 3 - remainder,
    };
    serde_json::to_string_pretty(&json!({
        "schema": "io.acceptance.capitalization-plan.v1",
        "gross_sale_reference": {
            "accepted_icp_e8s": accepted,
            "sale_io_allocation_e8s": sale_allocation,
            "price": "1 ICP per IO"
        },
        "sns_icp_treasury_proceeds_e8s": treasury,
        "sns_icp_treasury_is_claim_backing": false,
        "canonical_claim_supply_c0_e8s": plan.claims_e8s,
        "operator_stream_backing_b0_e8s": plan.claim_backing_e8s,
        "stream_backing_before_e8s": backing_before,
        "operator_stream_transfer_e8s": stream_transfer,
        "operator_permanent_top_up_e8s": plan.permanent_top_up_e8s,
        "permanent_exact_target": format!("{} + {}/3 e8s", (plan.claims_e8s * 2) / 3, plan.permanent_exact_remainder_thirds),
        "permanent_rounding_excess_thirds_of_e8": rounding_excess_thirds,
        "dynamic_anchor_e8s": DYNAMIC_ANCHOR_E8S,
        "additional_io_issued_by_capitalization_e8s": 0
    }))
    .map(|text| text + "\n")
    .map_err(|error| error.to_string())
}

pub(crate) fn preframework(root: &Path) -> Result<(), String> {
    validate(root, false)?;
    for relative in [
        "canisters/io_stream_manager/io_stream_manager.did",
        "canisters/io_nns_neuron_manager/io_nns_neuron_manager.did",
    ] {
        let did = fs::read_to_string(root.join(relative))
            .map_err(|err| format!("read {relative}: {err}"))?;
        for forbidden in [
            "set_paused",
            "validate_set_paused",
            "start_maturity",
            "validate_start_maturity",
        ] {
            if did.contains(forbidden) {
                return Err(format!(
                    "{relative} retains recurring lifecycle/maturity proposal method {forbidden}"
                ));
            }
        }
    }
    const SAMPLE_SALE_E8S: u128 = 300_000_000;
    let reserve = reserve_stake(SAMPLE_SALE_E8S)?;
    // Assumed two-basket fixture: two SNS distribution fees plus reserve disbursement.
    // The real sale must observe its own fees/residuals instead of borrowing this example.
    let post_total = TOTAL_IO_E8S - 3 * IO_FEE_E8S;
    let post_reserve = reserve - IO_FEE_E8S;
    let plan = capitalization_plan(post_total, post_reserve, &[JUPITER_GOVERNANCE_IO_E8S])?;
    let expected_claims = GOVERNANCE_IO_E8S + SAMPLE_SALE_E8S - 2 * IO_FEE_E8S;
    if plan.claims_e8s != expected_claims || plan.claim_backing_e8s != expected_claims {
        return Err("canonical sale-token claims did not determine B0 exactly".into());
    }
    // Manual capitalization rounds K upward; it is not a Jupiter paired receipt.
    // Reapplying split_40_60 could move the rounded e8s into B and issue extra IO.
    if plan.permanent_top_up_e8s != (2 * expected_claims).div_ceil(3) {
        return Err("sample permanent capitalization is not ceil(2*C0/3)".into());
    }
    let state = EconomicState {
        backing: Backing {
            liquid: plan.claim_backing_e8s,
            ..Backing::default()
        },
        claims: plan.claims_e8s,
        active_backing: plan.claims_e8s,
        active_reward: plan.claims_e8s,
    };
    if io_core_model::claim_rate(state)
        != Ok(io_core_model::ClaimRate::Ratio {
            backing: plan.claim_backing_e8s,
            claims: plan.claims_e8s,
        })
    {
        return Err("launch claim rate is not exactly 1 ICP/IO".into());
    }
    match io_core_model::reconcile(state, ICP_TRANSFER_FEE_E8S, DYNAMIC_ANCHOR_E8S, 1) {
        Ok(ReconcilePlan::TopUp {
            target,
            transfer,
            claim_credit,
        }) if target == plan.claim_backing_e8s
            && transfer == plan.claim_backing_e8s - ICP_TRANSFER_FEE_E8S
            && claim_credit == plan.claim_backing_e8s => {}
        other => {
            return Err(format!(
                "unexpected launch Dynamic reconciliation: {other:?}"
            ))
        }
    }
    let redemption = io_core_model::redemption_quote(state, 100_000_000, ICP_TRANSFER_FEE_E8S)
        .map_err(|error| format!("launch redemption quote: {error:?}"))?;
    if redemption.gross_icp != 100_000_000 || redemption.net_icp != 99_990_000 {
        return Err("1:1 gross redemption or existing payout fee changed".into());
    }
    eprintln!("✓ IO-side rehearsal under observed successful-sale assumptions");
    eprintln!("  fixture gross sale price: 1 ICP/IO; sale proceeds remain outside B");
    eprintln!("  excluded 100000000 IO Jupiter voting stake adds zero to C0 and B0");
    eprintln!("  canonical C0=B0={} e8s", plan.claims_e8s);
    eprintln!(
        "  operator permanent delta K={} e8s; Dynamic anchor remains 1000000000 e8s",
        plan.permanent_top_up_e8s
    );
    eprintln!(
        "  exact delta K remainder: {}/3 e8s before upward rounding",
        plan.permanent_exact_remainder_thirds
    );
    eprintln!("  10000 e8s transfer fee is charged against the 10 ICP anchor");
    eprintln!("  1 IO redemption: gross 100000000, net 99990000 e8s ICP");
    Ok(())
}

pub(crate) fn real_framework_hook(root: &Path) -> Result<(), String> {
    const TEST: &str = "acceptance_sale::real_launch_checkpoints_a_b_c";
    let listed = std::process::Command::new("cargo")
        .current_dir(root)
        .args([
            "test",
            "--locked",
            "-p",
            "e2e-real-canisters",
            TEST,
            "--",
            "--ignored",
            "--exact",
            "--list",
        ])
        .output()
        .map_err(|error| format!("discover real launch test: {error}"))?;
    if !listed.status.success()
        || !String::from_utf8_lossy(&listed.stdout)
            .lines()
            .any(|line| line == format!("{TEST}: test"))
    {
        return Err(format!(
            "required real launch test did not compile or was not found: {}",
            String::from_utf8_lossy(&listed.stderr)
        ));
    }
    let status = std::process::Command::new("cargo")
        .current_dir(root)
        .args([
            "test",
            "--locked",
            "-p",
            "e2e-real-canisters",
            TEST,
            "--",
            "--ignored",
            "--exact",
            "--test-threads=1",
            "--nocapture",
        ])
        .status()
        .map_err(|error| format!("start real launch: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("real self-deployed launch failed: {status}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repository_root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    #[test]
    fn acceptance_cli_rejects_typos_and_extra_arguments() {
        for (command, args) in [
            ("acceptance_preflight", vec!["seald"]),
            ("acceptance_preflight", vec!["sealed", "extra"]),
            ("acceptance_render", vec!["production", "extra"]),
            ("acceptance_sale_real", vec!["extra"]),
            ("acceptance_unknown", vec![]),
            ("acceptance_checkpoint", vec!["D", "fixture"]),
            ("acceptance_checkpoint", vec!["A", "fixture", "extra"]),
            ("acceptance_constructors", vec!["fixture", "extra"]),
        ] {
            assert!(validate_cli(
                command,
                &args.into_iter().map(str::to_string).collect::<Vec<_>>()
            )
            .is_err());
        }
        for mode in ["draft", "sealed"] {
            validate_cli("acceptance_preflight", &[mode.into()]).unwrap();
        }
    }

    #[test]
    fn all_canonical_reward_and_voting_fields_are_checked() {
        let original: Value = read_json(
            &repository_root(),
            &format!("{DIR}/canonical-sns-economics.json"),
        )
        .unwrap();
        for field in [
            "maximum_dissolve_delay_bonus_basis_points",
            "maximum_neuron_age_seconds",
            "maximum_age_bonus_basis_points",
            "initial_reward_rate_basis_points",
            "final_reward_rate_basis_points",
            "reward_rate_transition_seconds",
            "maximum_number_of_neurons",
            "initial_voting_period_seconds",
            "wait_for_quiet_deadline_increase_seconds",
            "reward_event_duration_seconds",
        ] {
            let mut invalid = original.clone();
            invalid[field] = json!(original[field].as_u64().unwrap() + 1);
            assert!(
                validate_economics(&serde_json::from_value(invalid).unwrap()).is_err(),
                "{field}"
            );
        }
        let mut invalid = original.clone();
        invalid["ignored_economic_field"] = json!(1);
        assert!(serde_json::from_value::<Economics>(invalid).is_err());
        let mut invalid = original;
        invalid["jupiter_governance_neuron"]["io_reward_eligible"] = json!(true);
        assert!(validate_economics(&serde_json::from_value(invalid).unwrap()).is_err());
    }

    #[test]
    fn local_metadata_is_explicit_and_never_approves_the_draft() {
        let root = repository_root();
        let before = fs::read(root.join(format!("{DIR}/required-inputs.draft.json"))).unwrap();
        let dir = root.join("target").join(format!(
            "acceptance-local-config-test-{}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        let mut context = json!({
            "local_fixture": true,
            "operator": Principal::from_slice(&[1; 29]).to_text(),
            "jupiter": Principal::from_slice(&[2; 29]).to_text(),
            "stream": Principal::from_slice(&[3; 29]).to_text(),
            "nns_manager": Principal::from_slice(&[4; 29]).to_text(),
            "historian": Principal::from_slice(&[5; 29]).to_text(),
            "frontend": Principal::from_slice(&[6; 29]).to_text(),
            "logo_path": "local-fixture.png"
        });
        let context_path = dir.join("context.json");
        fs::write(&context_path, serde_json::to_vec(&context).unwrap()).unwrap();
        let output = dir.join("local.yaml");
        if output.exists() {
            fs::remove_file(&output).unwrap();
        }
        local_config(&root, &context_path, &output).unwrap();
        let text = fs::read_to_string(&output).unwrap();
        assert!(text.contains("local-fixture-not-for-deployment"));
        assert!(text.contains("name: \"IO Local Fixture\""));
        assert!(text.contains("dapp_canisters: []"));
        assert!(text.contains("minimum_direct_participation_icp: 300000000 e8s"));
        assert!(text.contains("maximum_direct_participation_icp: 300000000 e8s"));
        assert!(!text.contains("TBD_"));
        assert!(local_config(&root, &context_path, &output).is_err());
        assert_eq!(
            fs::read(root.join(format!("{DIR}/required-inputs.draft.json"))).unwrap(),
            before
        );
        context["local_fixture"] = json!(false);
        fs::write(&context_path, serde_json::to_vec(&context).unwrap()).unwrap();
        assert!(
            local_config(&root, &context_path, &dir.join("forbidden.yaml"))
                .unwrap_err()
                .contains("explicit local_fixture")
        );
        fs::remove_file(output).unwrap();
        fs::remove_file(context_path).unwrap();
        fs::remove_dir(dir).unwrap();
    }

    #[test]
    fn production_proposal_text_does_not_claim_acceptance() {
        let root = repository_root();
        let rendered = render(&root, "production").unwrap();
        for line in rendered.lines().filter(|line| !line.starts_with('#')) {
            assert!(!line.contains("acceptance"), "{line}");
        }
    }

    #[test]
    fn fixed_genesis_memos_are_unique_and_outside_sale_range() {
        assert_ne!(GOVERNANCE_MEMO, RESERVE_MEMO);
        assert_ne!(GOVERNANCE_MEMO, JUPITER_GOVERNANCE_MEMO);
        assert_ne!(JUPITER_GOVERNANCE_MEMO, RESERVE_MEMO);
        assert!(!(SALE_MEMO_START..=SALE_MEMO_END).contains(&GOVERNANCE_MEMO));
        assert!(!(SALE_MEMO_START..=SALE_MEMO_END).contains(&JUPITER_GOVERNANCE_MEMO));
        assert!(!(SALE_MEMO_START..=SALE_MEMO_END).contains(&RESERVE_MEMO));
    }

    #[test]
    fn permanent_delay_matches_runtime_source() {
        let source = include_str!("../../../canisters/io_nns_neuron_manager/src/execution.rs");
        let value = source
            .split("pub const APPROVED_PERMANENT_DISSOLVE_DELAY_SECONDS: u64 = ")
            .nth(1)
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .replace('_', "");
        assert_eq!(value.parse::<u64>().unwrap(), PERMANENT_DELAY_SECONDS);
    }

    #[test]
    fn sale_candidate_and_locked_proposer_economics_reject_mutations() {
        let root = repository_root();
        let e = economics(&root).unwrap();
        let mut inputs: Value =
            read_json(&root, &format!("{DIR}/required-inputs.draft.json")).unwrap();
        validate_sale_plan(&e, &inputs, false).unwrap();
        assert!(validate_sale_plan(&e, &inputs, true).is_err());
        inputs["sale_plan"]["approved_for_deployment"] = json!(true);
        inputs["final_token_name"] = json!("IO local test token");
        inputs["final_token_symbol"] = json!("IOLT");
        validate_sale_plan(&e, &inputs, true).unwrap();
        for (field, value) in [
            ("basket_count", 1),
            ("basket_count", 11),
            ("basket_interval_seconds", 0),
            ("minimum_participant_icp_e8s", 200_000_000),
        ] {
            let mut invalid = inputs.clone();
            invalid["sale_plan"][field] = json!(value);
            assert!(validate_sale_plan(&e, &invalid, true).is_err(), "{field}");
        }
        for (field, value) in [
            ("final_token_name", "IO"),
            ("final_token_symbol", "IO"),
            ("final_token_symbol", "TBD_FINAL_TOKEN_SYMBOL"),
        ] {
            let mut invalid = inputs.clone();
            invalid[field] = json!(value);
            assert!(validate_sale_plan(&e, &invalid, true).is_err());
        }
        for (field, value) in [
            ("proposal_reject_cost_e8s", 10_000_000_000u64),
            ("minimum_neuron_stake_e8s", 1),
        ] {
            let mut invalid: Value =
                read_json(&root, &format!("{DIR}/canonical-sns-economics.json")).unwrap();
            invalid[field] = json!(value);
            assert!(validate_economics(&serde_json::from_value(invalid).unwrap()).is_err());
        }
        let mut invalid = economics(&root).unwrap();
        invalid.governance_neuron.stake_e8s = json!(150_000_000);
        assert!(validate_economics(&invalid).is_err());
    }

    #[test]
    fn observed_sale_claims_drive_operator_capitalization_without_new_io() {
        let sale_allocation = 1_000_000_000;
        let sale_io_fee = IO_FEE_E8S;
        let reserve = reserve_stake(sale_allocation).unwrap();
        let supply_before_capitalization = TOTAL_IO_E8S - sale_io_fee - IO_FEE_E8S;
        let plan = capitalization_plan(
            supply_before_capitalization,
            reserve - IO_FEE_E8S,
            &[JUPITER_GOVERNANCE_IO_E8S],
        )
        .unwrap();
        assert_eq!(
            plan.claims_e8s,
            GOVERNANCE_IO_E8S + sale_allocation - sale_io_fee
        );
        assert_eq!(plan.claim_backing_e8s, plan.claims_e8s);
        assert_eq!(plan.permanent_top_up_e8s, 2_666_660_000);
        assert_eq!(plan.permanent_exact_remainder_thirds, 0);
        assert_eq!(supply_before_capitalization, TOTAL_IO_E8S - 20_000);
        let treasury_proceeds_e8s = sale_allocation - ICP_TRANSFER_FEE_E8S;
        let state = EconomicState {
            backing: Backing {
                liquid: plan.claim_backing_e8s,
                ..Backing::default()
            },
            claims: plan.claims_e8s,
            active_backing: 0,
            active_reward: 0,
        };
        assert_eq!(
            io_core_model::claim_backing(state.backing),
            Ok(plan.claims_e8s)
        );
        assert_ne!(treasury_proceeds_e8s, plan.claim_backing_e8s);
        assert_eq!(
            io_core_model::split_40_60(plan.claim_backing_e8s + plan.permanent_top_up_e8s),
            Ok(io_core_model::Split {
                permanent: plan.permanent_top_up_e8s,
                claim: plan.claim_backing_e8s,
            })
        );
    }

    #[test]
    fn moving_jupiter_stake_from_excluded_reserve_preserves_launch_economics() {
        let sale = 1_000_000_000;
        let old_reserve = TOTAL_IO_E8S - GOVERNANCE_IO_E8S - sale;
        let new_reserve = reserve_stake(sale).unwrap();
        assert_eq!(new_reserve + JUPITER_GOVERNANCE_IO_E8S, old_reserve);
        let post_total = TOTAL_IO_E8S - 2 * IO_FEE_E8S;
        let old = capitalization_plan(post_total, old_reserve - IO_FEE_E8S, &[0]).unwrap();
        let new = capitalization_plan(
            post_total,
            new_reserve - IO_FEE_E8S,
            &[JUPITER_GOVERNANCE_IO_E8S],
        )
        .unwrap();
        assert_eq!(new, old);

        let old_state = EconomicState {
            backing: Backing {
                liquid: old.claim_backing_e8s,
                ..Backing::default()
            },
            claims: old.claims_e8s,
            active_backing: old.claims_e8s,
            active_reward: old.claims_e8s,
        };
        let new_state = EconomicState { ..old_state };
        assert_eq!(
            io_core_model::claim_rate(new_state),
            io_core_model::claim_rate(old_state)
        );
        assert_eq!(
            io_core_model::target(
                new_state.active_backing,
                io_core_model::claim_backing(new_state.backing).unwrap(),
                new_state.claims,
            ),
            io_core_model::target(
                old_state.active_backing,
                io_core_model::claim_backing(old_state.backing).unwrap(),
                old_state.claims,
            )
        );
    }

    #[test]
    fn environment_specific_jupiter_controllers_bind_distinct_neuron_accounts() {
        let governance = principal("h7y7s-kyaaa-aaaas-amxrq-cai", "test governance").unwrap();
        let acceptance = principal("aaaaa-aa", "test acceptance Jupiter").unwrap();
        let production = principal(
            PRODUCTION_JUPITER_SNS_REWARDS_CANISTER_ID,
            "test production Jupiter",
        )
        .unwrap();
        let acceptance_account = LedgerAccount::new(
            governance,
            Some(Subaccount(sns_neuron_subaccount(
                acceptance,
                JUPITER_GOVERNANCE_MEMO,
            ))),
        );
        let production_account = LedgerAccount::new(
            governance,
            Some(Subaccount(sns_neuron_subaccount(
                production,
                JUPITER_GOVERNANCE_MEMO,
            ))),
        );
        assert_eq!(
            hex::encode(production_account.subaccount.as_ref().unwrap().0),
            "7ad7034691f63c7f831a919cde4a7960de689e842d4aa256f1b0c6623c8da0a7"
        );
        assert_ne!(acceptance_account, production_account);
        let wrong_owner = LedgerAccount::new(
            production,
            Some(Subaccount(sns_neuron_subaccount(
                acceptance,
                JUPITER_GOVERNANCE_MEMO,
            ))),
        );
        let wrong_memo = LedgerAccount::new(
            governance,
            Some(Subaccount(sns_neuron_subaccount(
                acceptance,
                JUPITER_GOVERNANCE_MEMO + 1,
            ))),
        );
        assert_ne!(acceptance_account, wrong_owner);
        assert_ne!(acceptance_account, wrong_memo);
        assert_eq!(
            JUPITER_GOVERNANCE_IO_E8S,
            TOTAL_IO_E8S
                - GOVERNANCE_IO_E8S
                - reserve_stake(1_000_000_000).unwrap()
                - 1_000_000_000
        );
    }

    #[test]
    fn full_initial_swap_allocation_gives_developers_full_voting_multiplier() {
        let initial_swap_amount = 1_000_000_000u128;
        let total_swap_amount = initial_swap_amount;
        let multiplier_percent = initial_swap_amount * 100 / total_swap_amount;
        assert_eq!(multiplier_percent, 100);
        let developer_voting_power = GOVERNANCE_IO_E8S + JUPITER_GOVERNANCE_IO_E8S;
        assert!(GOVERNANCE_IO_E8S * 2 <= developer_voting_power);
        assert!(JUPITER_GOVERNANCE_IO_E8S * 2 > developer_voting_power);
    }

    #[test]
    fn launch_reconciliation_and_redemption_use_the_canonical_one_to_one_rate() {
        let claims = GOVERNANCE_IO_E8S + 300_000_000 - 2 * IO_FEE_E8S;
        let state = EconomicState {
            backing: Backing {
                liquid: claims,
                ..Backing::default()
            },
            claims,
            active_backing: claims,
            active_reward: claims,
        };
        assert_eq!(
            io_core_model::reconcile(state, ICP_TRANSFER_FEE_E8S, DYNAMIC_ANCHOR_E8S, 1),
            Ok(ReconcilePlan::TopUp {
                target: claims,
                transfer: claims - ICP_TRANSFER_FEE_E8S,
                claim_credit: claims,
            })
        );
        assert_eq!(
            io_core_model::redemption_quote(state, 100_000_000, ICP_TRANSFER_FEE_E8S),
            Ok(io_core_model::RedemptionQuote {
                gross_icp: 100_000_000,
                net_icp: 99_990_000,
            })
        );
    }

    #[test]
    fn permanent_top_up_rounds_up_and_reports_sub_e8_excess() {
        let one_third = capitalization_plan(10, 9, &[]).unwrap();
        assert_eq!(one_third.claims_e8s, 1);
        assert_eq!(one_third.permanent_top_up_e8s, 1);
        assert_eq!(one_third.permanent_exact_remainder_thirds, 2);

        let two_thirds = capitalization_plan(10, 8, &[]).unwrap();
        assert_eq!(two_thirds.claims_e8s, 2);
        assert_eq!(two_thirds.permanent_top_up_e8s, 2);
        assert_eq!(two_thirds.permanent_exact_remainder_thirds, 1);
    }

    #[test]
    fn sale_finalization_requires_observation_not_only_the_contribution_cap() {
        assert!(require_successful_finalization(false, "Committed", "Normal").is_err());
        assert!(require_successful_finalization(true, "Open", "Normal").is_err());
        assert!(
            require_successful_finalization(true, "Committed", "PreInitializationSwap").is_err()
        );
        assert_eq!(
            require_successful_finalization(true, "Committed", "Normal"),
            Ok(())
        );
    }

    #[test]
    fn checked_draft_package_is_self_consistent_and_sealed_mode_fails_closed() {
        let root = repository_root();
        validate(&root, false).unwrap();
        let error = validate(&root, true).unwrap_err();
        assert!(
            error.contains("sale candidate")
                || error.contains("ACCEPTANCE_JUPITER_SNS_REWARDS_CANISTER_ID")
        );
    }

    #[test]
    fn package_validator_rejects_jupiter_identity_account_and_memo_mutations() {
        let source = repository_root();
        let root = source.join("target").join(format!(
            "acceptance-jupiter-mutations-{}",
            std::process::id()
        ));
        fs::create_dir(&root).unwrap();
        fs::create_dir_all(root.join(DIR)).unwrap();
        for entry in fs::read_dir(source.join(DIR)).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_file() {
                fs::copy(entry.path(), root.join(DIR).join(entry.file_name())).unwrap();
            }
        }
        validate(&root, false).unwrap();
        let inputs_path = root.join(DIR).join("required-inputs.draft.json");
        let original_inputs = fs::read(&inputs_path).unwrap();
        let mut inputs: Value = serde_json::from_slice(&original_inputs).unwrap();
        let subaccount = [42u8; 32];
        inputs["permanent_neuron_observation"]["staking_subaccount_hex"] =
            json!(hex::encode(subaccount));
        fs::write(&inputs_path, serde_json::to_vec(&inputs).unwrap()).unwrap();
        let accounts = expected_accounts(&root).unwrap();
        let observed = named_account(&accounts, "nns_permanent_neuron_stake").unwrap();
        let expected = LedgerAccount::new(
            principal(NNS_GOVERNANCE_PRINCIPAL, "NNS").unwrap(),
            Some(Subaccount(subaccount)),
        );
        assert_eq!(
            observed["icp_account_identifier"],
            expected.icp_account_identifier_text()
        );
        assert_eq!(observed["subaccount_hex"], hex::encode(subaccount));
        inputs["permanent_neuron_observation"]["staking_subaccount_hex"] = json!("00");
        fs::write(&inputs_path, serde_json::to_vec(&inputs).unwrap()).unwrap();
        assert!(expected_accounts(&root).is_err());
        fs::write(&inputs_path, original_inputs).unwrap();
        let mutate = |name: &str, pointer: &str, replacement: Value| {
            let path = root.join(DIR).join(name);
            let original = fs::read(&path).unwrap();
            let mut value: Value = serde_json::from_slice(&original).unwrap();
            *value.pointer_mut(pointer).unwrap() = replacement;
            fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
            let result = validate(&root, false);
            fs::write(&path, original).unwrap();
            assert!(result.is_err(), "accepted mutation: {name}{pointer}");
        };
        mutate(
            "genesis-neurons.draft.json",
            "/neurons/1/memo",
            json!(JUPITER_GOVERNANCE_MEMO + 1),
        );
        mutate(
            "acceptance-accounts.draft.json",
            "/accounts/2/owner",
            json!("gosbn-sqaaa-aaaas-amxua-cai"),
        );
        mutate(
            "stream-exclusions.draft.json",
            "/excluded_accounts/1/subaccount_hex",
            json!("00".repeat(32)),
        );
        mutate(
            "acceptance-identity-overlay.json",
            "/jupiter_sns_rewards_canister_id",
            json!(PRODUCTION_JUPITER_SNS_REWARDS_CANISTER_ID),
        );
        // Exercise the allowlist itself as well as the full-package consistency gate.
        let overlay_path = root.join(DIR).join("acceptance-identity-overlay.json");
        let mut value: Value = serde_json::from_slice(&fs::read(&overlay_path).unwrap()).unwrap();
        value["jupiter_sns_rewards_canister_id"] =
            json!(PRODUCTION_JUPITER_SNS_REWARDS_CANISTER_ID);
        fs::write(&overlay_path, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(validate_rendered_yaml(&root, &economics(&root).unwrap())
            .unwrap_err()
            .contains("frozen dapp allowlist"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn dynamic_account_derivation_matches_independent_known_vectors() {
        let controller = principal("gosbn-sqaaa-aaaas-amxua-cai", "test controller").unwrap();
        let subaccount = sns_neuron_subaccount(controller, 0);
        assert_eq!(
            hex::encode(subaccount),
            "07734f006ddcefc43f9ec1ebcdf0e053811ffd03bbf0b06df8df4a874e3e51d0"
        );
        let governance = principal(NNS_GOVERNANCE_PRINCIPAL, "NNS Governance").unwrap();
        let account = LedgerAccount::new(governance, Some(Subaccount(subaccount)));
        assert_eq!(
            account.icp_account_identifier_text(),
            "23f97e98045b2376759598bdd5b49e538f1c19cfd96280437f7c72a2cb1b8362"
        );
    }
}
