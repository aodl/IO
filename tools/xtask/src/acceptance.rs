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
use serde::{Deserialize, Serialize};
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
const APPROVED_SNS_LOCK_TEXT: &str =
    include_str!("../../../tests/e2e_real_canisters/wasms.example.toml");
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
    approved_sns_baseline()["parser"]["commit"]
        .as_str()
        .expect("reviewed SNS parser commit")
}
fn approved_sns_framework() -> &'static str {
    approved_sns_baseline()["framework_id"]
        .as_str()
        .expect("reviewed SNS framework ID")
}
fn approved_sns_manifest() -> &'static io_sns_manifest::SnsManifest {
    static MANIFEST: std::sync::LazyLock<io_sns_manifest::SnsManifest> =
        std::sync::LazyLock::new(|| {
            io_sns_manifest::SnsManifest::parse(APPROVED_SNS_LOCK_TEXT)
                .expect("reviewed official SNS lock")
        });
    &MANIFEST
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

#[derive(Debug, Deserialize, Serialize)]
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
    governance_neuron: GenesisNeuron,
    jupiter_governance_neuron: GenesisNeuron,
    reserve_transport_neuron: GenesisNeuron,
    treasury_e8s: u128,
    future_swap_e8s: u128,
    swap_e8s: u128,
    initial_redemption_backing: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct GenesisNeuron {
    memo_domain: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    io_reward_eligible: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
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
    governance_voting_principal: String,
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
    let reserve_balance = json!(reserve_stake()?);
    let swap_balance = json!(0);

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
    let jupiter_governance_controller = acceptance_overlay.governance_voting_principal.as_str();
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
                principal(jupiter_governance_controller, "acceptance governance voter")?,
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
            "parked Swap; zero genesis allocation",
            swap_balance,
            "later IO transfers remain claim-bearing; never an exclusion",
            false,
            false,
        )?,
        account_json(
            "sns_icp_treasury",
            role(&manifest, "sns_governance")?,
            None,
            "ICP",
            "unused Governance default ICP Account",
            json!(0),
            "unexpected balances are never IO backing or permanent capital",
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

fn reserve_stake() -> Result<u128, String> {
    TOTAL_IO_E8S
        .checked_sub(GOVERNANCE_IO_E8S)
        .and_then(|value| value.checked_sub(JUPITER_GOVERNANCE_IO_E8S))
        .ok_or_else(|| "genesis allocation overflow".into())
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
        return Err("canonical C0 must be positive after direct genesis".into());
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

fn require_direct_genesis(observed: &Value) -> Result<(), String> {
    if observed["observed"] != true
        || observed["governance_mode"] != "Normal"
        || observed["total_supply_e8s"] != json!(TOTAL_IO_E8S)
        || observed["operator_stake_e8s"] != json!(GOVERNANCE_IO_E8S)
        || observed["jupiter_stake_e8s"] != json!(JUPITER_GOVERNANCE_IO_E8S)
        || observed["reserve_stake_e8s"] != json!(reserve_stake()?)
        || observed["swap_io_e8s"] != 0
        || observed["swap_icp_e8s"].as_u64().is_none()
        || observed["accepted_icp_e8s"] != 0
        || observed["swap_lifecycle"] != "Adopted"
    {
        return Err("canonical sale-free genesis observation mismatch".into());
    }
    Ok(())
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
    if Some(manifest.reward_share_baseline_commit.as_str())
        != approved_sns_manifest().artifact("sns_governance", "upstream_rev")
    {
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
        || e.governance_neuron.memo_domain != "IOGOV001"
        || e.jupiter_governance_neuron.memo_domain != "IOJUP001"
        || e.reserve_transport_neuron.memo_domain != "IORES001"
        || e.jupiter_governance_neuron.io_reward_eligible != Some(false)
        || e.jupiter_governance_neuron.controller_source.as_deref()
            != Some("environment-overlay#governance_voting_principal")
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
        || e.reserve_transport_neuron.stake_e8s != json!(reserve_stake()?)
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
        || e.swap_e8s != 0
        || e.initial_redemption_backing != "1 ICP per claim-bearing IO"
    {
        return Err("zero genesis allocations or backing invariant changed".into());
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

fn validate_genesis_plan(inputs: &Value) -> Result<(), String> {
    if inputs.get("sale_plan").is_some()
        || inputs.get("proposal_url").is_some()
        || inputs["final_token_name"] != "IO"
        || inputs["final_token_symbol"] != "IO"
        || inputs["genesis_controller_principal"]
            != "zkkkd-i34qc-367ln-e2u7o-ezznu-dkfqh-gtfvz-cviph-6qa4v-evtfs-wqe"
        || inputs["project_url"] != "https://ghrkr-eyaaa-aaaas-amxvq-cai.icp0.io/"
    {
        return Err("direct-genesis identity, branding or obsolete sale input mismatch".into());
    }
    if inputs["governance_voting_principal"] != inputs["genesis_controller_principal"] {
        return Err("acceptance governance voting principal must be the reviewed operator".into());
    }
    Ok(())
}

fn parked_swap_contract() -> Value {
    serde_json::from_str(include_str!(
        "../../../deploy/acceptance/genesis-contract.json"
    ))
    .expect("reviewed genesis contract")
}

fn render_sns_yaml(e: &Economics, overlay: &Overlay, inputs: &Value) -> Result<String, String> {
    validate_economics(e)?;
    let manifest = approved_sns_manifest();
    let mut version = serde_json::Map::new();
    for component in ["governance", "root", "ledger", "index", "swap", "archive"] {
        let hash = manifest
            .source_sha256(&format!("sns_{component}"))
            .ok_or("official module hash missing")?;
        version.insert(
            format!("{component}_wasm_hash"),
            json!(hex::decode(hash).map_err(|e| e.to_string())?),
        );
    }
    // JSON is a strict YAML subset. Values are typed e8s/seconds, with no sale parser.
    serde_json::to_string_pretty(&json!({
        "schema":"io.acceptance.direct-sns-genesis.v1", "environment":overlay.environment,
        "token_name":"IO", "token_symbol":"IO", "display_name":"IO Protocol",
        "description":format!("IO protocol ({}) direct sale-free genesis",overlay.environment),
        "project_url":inputs["project_url"], "logo_path":inputs["sns_logo_path"],
        "operator":inputs["genesis_controller_principal"], "governance_voting_principal":overlay.governance_voting_principal,
        "dapp_canisters":overlay.dapp_canisters.values().collect::<Vec<_>>(),
        "economics":e, "framework_version":version,
        "parked_swap":parked_swap_contract()["parked_swap"]
    })).map(|text| text+"\n").map_err(|error|error.to_string())
}

fn validate_rendered_yaml(root: &Path, e: &Economics) -> Result<(), String> {
    let acceptance = overlay(root, "acceptance")?;
    let production = overlay(root, "production")?;
    let manifest = manifest(root)?;
    let inputs: Value = read_json(root, &format!("{DIR}/required-inputs.draft.json"))?;
    if !is_placeholder(&acceptance.governance_voting_principal) {
        principal(
            &acceptance.governance_voting_principal,
            "acceptance governance voter",
        )?;
    }
    if acceptance.environment != "acceptance"
        || acceptance.dapp_canisters.len() != 4
        || acceptance.governance_voting_principal != inputs["genesis_controller_principal"]
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
        || production.governance_voting_principal != PRODUCTION_JUPITER_SNS_REWARDS_CANISTER_ID
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

fn validate_static_artifacts(root: &Path) -> Result<(), String> {
    let required = [
        "acceptance-manifest.json",
        "sns-source-baseline.json",
        "canonical-sns-economics.json",
        "acceptance-identity-overlay.json",
        "production-identity-overlay.json",
        "required-inputs.draft.json",
        "genesis-contract.json",
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
    validate_genesis_plan(&inputs)?;
    let acceptance_overlay = overlay(root, "acceptance")?;
    if inputs["governance_voting_principal"] != acceptance_overlay.governance_voting_principal {
        return Err(
            "acceptance voting principal input and identity overlay must match exactly".into(),
        );
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
        || genesis["neurons"][1]["controller"] != acceptance_overlay.governance_voting_principal
        || genesis["neurons"][1]["ledger_account_owner"]
            != role(&frozen_manifest, "sns_governance")?
        || genesis["neurons"][1]["memo"] != json!(JUPITER_GOVERNANCE_MEMO)
        || genesis["neurons"][1]["stake_e8s"] != json!(JUPITER_GOVERNANCE_IO_E8S)
        || genesis["neurons"][1]["stream_excluded"] != true
        || genesis["neurons"][1]["permissions"] != json!(["Vote"])
        || genesis["neurons"][1]["subaccount_hex"] != jupiter_account["subaccount_hex"]
        || genesis["jupiter_staking_account_derivations"]["acceptance"]["controller"]
            != acceptance_overlay.governance_voting_principal
        || genesis["jupiter_staking_account_derivations"]["acceptance"]["subaccount_hex"]
            != jupiter_account["subaccount_hex"]
        || genesis["neurons"][2]["stake_e8s"] != json!(reserve_stake()?)
        || genesis["neurons"][2]["dissolve_delay_seconds"] != 0
        || genesis["swap_e8s"] != 0
        || genesis["operator_controller"] != inputs["genesis_controller_principal"]
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
    let contract: Value = read_json(root, &format!("{DIR}/genesis-contract.json"))?;
    let parked = &contract["parked_swap"];
    if contract != parked_swap_contract()
        || contract["schema"] != "io.acceptance.genesis-contract.v1"
        || contract["reviewed_sns_framework_id"] != approved_sns_framework()
        || parked["sns_token_e8s"] != 0
        || parked["should_auto_finalize"] != false
        || parked["swap_start_timestamp_seconds"] != json!(u64::MAX)
        || parked["swap_due_timestamp_seconds"] != json!(u64::MAX)
        || parked["nns_proposal_id"] != 0
        || parked["min_participants"] != 1
        || parked["min_direct_participation_icp_e8s"] != 100_000_000
        || parked["max_direct_participation_icp_e8s"] != 100_000_000
        || parked["min_participant_icp_e8s"] != 100_000_000
        || parked["max_participant_icp_e8s"] != 100_000_000
        || parked["neurons_fund_participation"] != false
        || parked["neuron_basket_construction_parameters"]
            != json!({"count":2,"dissolve_delay_interval_seconds":86_400})
    {
        return Err("frozen parked-Swap contract changed; sale activation is prohibited".into());
    }
    let baseline: Value = read_json(root, &format!("{DIR}/sns-source-baseline.json"))?;
    if baseline["schema"] != "io.acceptance.sns-source-baseline.v2"
        || baseline["framework_id"] != approved_sns_framework()
        || baseline["parser"]["commit"] != approved_sns_source()
        || baseline["official_lock"]["path"] != "tests/e2e_real_canisters/wasms.example.toml"
        || baseline["official_lock"]["sha256"]
            != hex::encode(Sha256::digest(APPROVED_SNS_LOCK_TEXT.as_bytes()))
        || approved_sns_manifest().value("metadata", "version") != Some(approved_sns_framework())
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
        "acceptance_preflight" => matches!(args, []) || (args.len() == 1 && args[0] == "draft"),
        "acceptance_render" => {
            args.is_empty()
                || (args.len() == 1
                    && matches!(args[0].as_str(), "acceptance" | "production" | "accounts"))
        }
        "acceptance_capitalization"
        | "acceptance_preframework"
        | "acceptance_preframework_pocketic"
        | "acceptance_genesis_real" => args.is_empty(),
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

pub(crate) fn validate(root: &Path) -> Result<(), String> {
    let manifest = manifest(root)?;
    let economics = economics(root)?;
    validate_manifest(&manifest)?;
    validate_economics(&economics)?;
    let inputs: Value = read_json(root, &format!("{DIR}/required-inputs.draft.json"))?;
    validate_genesis_plan(&inputs)?;
    validate_static_artifacts(root)?;
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
        "governance_voting_principal",
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
        dapp_canisters: ["frontend", "historian", "nns_manager", "stream"]
            .iter()
            .map(|role| {
                (
                    (*role).to_string(),
                    context[*role].as_str().unwrap().to_string(),
                )
            })
            .collect(),
        fallback_controller_principal: context["operator"].as_str().unwrap().into(),
        governance_voting_principal: context["governance_voting_principal"]
            .as_str()
            .unwrap()
            .into(),
    };
    let rendered = render_sns_yaml(&economics(root)?, &local, &inputs)?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .map_err(|e| e.to_string())?;
    std::io::Write::write_all(&mut file, rendered.as_bytes()).map_err(|e| e.to_string())
}

pub(crate) fn capitalization(root: &Path) -> Result<String, String> {
    validate(root)?;
    let observation: Value = read_json(root, &format!("{DIR}/launch-observation.draft.json"))?;
    require_direct_genesis(&observation["direct_genesis"])?;
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
    let expected_reserve = reserve_stake()?
        .checked_sub(IO_FEE_E8S)
        .ok_or("reserve migration amount does not cover its IO fee")?;
    let expected_supply = TOTAL_IO_E8S
        .checked_sub(IO_FEE_E8S)
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
    validate(root)?;
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
    let reserve = reserve_stake()?;
    // Only the reserve disbursement burns a genesis transport fee.
    let post_total = TOTAL_IO_E8S - IO_FEE_E8S;
    let post_reserve = reserve - IO_FEE_E8S;
    let plan = capitalization_plan(post_total, post_reserve, &[JUPITER_GOVERNANCE_IO_E8S])?;
    let expected_claims = GOVERNANCE_IO_E8S;
    if plan.claims_e8s != expected_claims || plan.claim_backing_e8s != expected_claims {
        return Err("canonical genesis claims did not determine B0 exactly".into());
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
            ));
        }
    }
    let redemption = io_core_model::redemption_quote(state, 100_000_000, ICP_TRANSFER_FEE_E8S)
        .map_err(|error| format!("launch redemption quote: {error:?}"))?;
    if redemption.gross_icp != 100_000_000 || redemption.net_icp != 99_990_000 {
        return Err("1:1 gross redemption or existing payout fee changed".into());
    }
    eprintln!("✓ IO-side sale-free genesis capitalization rehearsal");
    eprintln!("  initial redemption backing: 1 ICP per claim-bearing IO; no sale price");
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
            "--offline",
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
            "--offline",
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
            ("acceptance_genesis_real", vec!["extra"]),
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
        validate_cli("acceptance_preflight", &["draft".into()]).unwrap();
        assert!(validate_cli("acceptance_preflight", &["sealed".into()]).is_err());
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
            "governance_voting_principal": Principal::from_slice(&[1; 29]).to_text(),
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
        let config: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(config["token_name"], "IO");
        assert_eq!(config["token_symbol"], "IO");
        assert_eq!(config["dapp_canisters"].as_array().unwrap().len(), 4);
        assert_eq!(config["parked_swap"]["sns_token_e8s"], 0);
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
        let config: Value = serde_json::from_str(&rendered).unwrap();
        assert_eq!(config["environment"], "production");
        assert!(!config["description"]
            .as_str()
            .unwrap()
            .contains("acceptance"));
        assert_eq!(config["token_symbol"], "IO");
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
    fn parked_opening_is_outside_the_entire_pinned_clock_range() {
        // Pinned Swap's now_fn returns u64 nanoseconds, then divides by 1e9.
        let maximum_clock_seconds = u64::MAX / 1_000_000_000;
        let contract = parked_swap_contract();
        for field in ["swap_start_timestamp_seconds", "swap_due_timestamp_seconds"] {
            assert_eq!(contract["parked_swap"][field], u64::MAX);
            assert!(contract["parked_swap"][field].as_u64().unwrap() > maximum_clock_seconds);
        }
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
    fn direct_genesis_rejects_branding_authority_and_sale_inputs() {
        let root = repository_root();
        let inputs: Value = read_json(&root, &format!("{DIR}/required-inputs.draft.json")).unwrap();
        validate_genesis_plan(&inputs).unwrap();
        for (field, value) in [
            ("final_token_name", json!("IO Local Fixture")),
            ("final_token_symbol", json!("IOLT")),
            (
                "governance_voting_principal",
                json!(PRODUCTION_JUPITER_SNS_REWARDS_CANISTER_ID),
            ),
            (
                "genesis_controller_principal",
                json!(Principal::anonymous().to_text()),
            ),
            (
                "sale_plan",
                json!({"initial_swap_io_allocation_e8s":300_000_000}),
            ),
            ("proposal_url", json!("https://forum.dfinity.org/")),
        ] {
            let mut bad = inputs.clone();
            bad[field] = value;
            assert!(validate_genesis_plan(&bad).is_err(), "{field}");
        }
        for (field, value) in [
            ("proposal_reject_cost_e8s", 1u64),
            ("minimum_neuron_stake_e8s", 1),
            ("swap_e8s", 1),
        ] {
            let mut bad: Value =
                read_json(&root, &format!("{DIR}/canonical-sns-economics.json")).unwrap();
            bad[field] = json!(value);
            assert!(validate_economics(&serde_json::from_value(bad).unwrap()).is_err());
        }
    }

    #[test]
    fn observed_genesis_claims_drive_exact_capitalization_without_issuance() {
        let supply = TOTAL_IO_E8S - IO_FEE_E8S;
        let plan = capitalization_plan(
            supply,
            reserve_stake().unwrap() - IO_FEE_E8S,
            &[0, JUPITER_GOVERNANCE_IO_E8S],
        )
        .unwrap();
        assert_eq!(plan.claims_e8s, 3_000_000_000);
        assert_eq!(plan.claim_backing_e8s, 3_000_000_000);
        assert_eq!(plan.permanent_top_up_e8s, 2_000_000_000);
        assert_eq!(plan.permanent_exact_remainder_thirds, 0);
        assert_eq!(
            io_core_model::split_40_60(plan.claim_backing_e8s + plan.permanent_top_up_e8s),
            Ok(io_core_model::Split {
                permanent: plan.permanent_top_up_e8s,
                claim: plan.claim_backing_e8s
            })
        );
    }

    #[test]
    fn io_transferred_to_swap_remains_claim_bearing_after_fee_burn() {
        let supply = TOTAL_IO_E8S - IO_FEE_E8S;
        let reserve = reserve_stake().unwrap() - IO_FEE_E8S;
        let exclusions = [0, JUPITER_GOVERNANCE_IO_E8S];
        let before = capitalization_plan(supply, reserve, &exclusions).unwrap();
        // A later transfer of existing liquid claims to Swap burns one fee.
        // The destination balance is not subtracted from canonical claims.
        let transferred = 100_000_000;
        let remaining = before.claims_e8s - transferred - IO_FEE_E8S;
        let after = capitalization_plan(supply - IO_FEE_E8S, reserve, &exclusions).unwrap();
        assert_eq!(after.claims_e8s, remaining + transferred);
        assert_eq!(
            after.claim_backing_e8s,
            before.claim_backing_e8s - IO_FEE_E8S
        );
        let accounts = expected_accounts(&repository_root()).unwrap();
        assert_eq!(
            named_account(&accounts, "sns_swap").unwrap()["excluded_from_claim_supply"],
            false
        );
    }

    #[test]
    fn moving_jupiter_stake_from_excluded_reserve_preserves_launch_economics() {
        let old_reserve = TOTAL_IO_E8S - GOVERNANCE_IO_E8S;
        let new_reserve = reserve_stake().unwrap();
        assert_eq!(new_reserve + JUPITER_GOVERNANCE_IO_E8S, old_reserve);
        let post_total = TOTAL_IO_E8S - IO_FEE_E8S;
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
        let acceptance = principal(
            "zkkkd-i34qc-367ln-e2u7o-ezznu-dkfqh-gtfvz-cviph-6qa4v-evtfs-wqe",
            "acceptance voter",
        )
        .unwrap();
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
            TOTAL_IO_E8S - GOVERNANCE_IO_E8S - reserve_stake().unwrap()
        );
    }

    #[test]
    fn launch_reconciliation_and_redemption_use_the_canonical_one_to_one_rate() {
        let claims = GOVERNANCE_IO_E8S;
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
    fn direct_genesis_requires_observed_normal_and_parked_swap() {
        let good = json!({"observed":true,"governance_mode":"Normal","total_supply_e8s":TOTAL_IO_E8S,"operator_stake_e8s":GOVERNANCE_IO_E8S,"jupiter_stake_e8s":JUPITER_GOVERNANCE_IO_E8S,"reserve_stake_e8s":reserve_stake().unwrap(),"swap_io_e8s":0,"swap_icp_e8s":0,"accepted_icp_e8s":0,"swap_lifecycle":"Adopted"});
        require_direct_genesis(&good).unwrap();
        let mut dusted = good.clone();
        dusted["swap_icp_e8s"] = json!(100_000_000);
        require_direct_genesis(&dusted).unwrap();
        for (key, value) in [
            ("observed", json!(false)),
            ("governance_mode", json!("PreInitializationSwap")),
            ("swap_lifecycle", json!("Committed")),
            ("swap_icp_e8s", Value::Null),
            ("accepted_icp_e8s", json!(1)),
            ("swap_io_e8s", json!(1)),
        ] {
            let mut bad = good.clone();
            bad[key] = value;
            assert!(require_direct_genesis(&bad).is_err());
        }
    }

    #[test]
    fn checked_draft_package_is_self_consistent_and_monolithic_sealing_is_rejected() {
        let root = repository_root();
        let mut manifest = manifest(&root).unwrap();
        manifest.reward_share_baseline_commit = approved_sns_manifest()
            .artifact("sns_governance", "upstream_rev")
            .unwrap()
            .to_string();
        validate_manifest(&manifest).unwrap();
        manifest.reward_share_baseline_commit = "0".repeat(40);
        assert!(validate_manifest(&manifest)
            .unwrap_err()
            .contains("reward-share baseline"));
        validate(&root).unwrap();
        assert!(validate_cli("acceptance_preflight", &["sealed".into()]).is_err());
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
        validate(&root).unwrap();
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
            let result = validate(&root);
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
            "/governance_voting_principal",
            json!(PRODUCTION_JUPITER_SNS_REWARDS_CANISTER_ID),
        );
        for (field, value) in [
            ("sns_token_e8s", json!(1)),
            ("should_auto_finalize", json!(true)),
            (
                "swap_start_timestamp_seconds",
                json!(u64::MAX / 1_000_000_000),
            ),
            (
                "swap_due_timestamp_seconds",
                json!(u64::MAX / 1_000_000_000),
            ),
            ("min_participants", json!(0)),
            ("nns_proposal_id", json!(144202)),
            ("min_direct_participation_icp_e8s", json!(0)),
            ("max_direct_participation_icp_e8s", json!(200_000_000)),
            ("min_participant_icp_e8s", json!(0)),
            ("max_participant_icp_e8s", json!(200_000_000)),
            ("neurons_fund_participation", json!(true)),
        ] {
            mutate(
                "genesis-contract.json",
                &format!("/parked_swap/{field}"),
                value,
            );
        }
        mutate(
            "genesis-neurons.draft.json",
            "/neurons/1/permissions",
            json!(["Vote", "ManagePrincipals"]),
        );
        // Exercise the allowlist itself as well as the full-package consistency gate.
        let overlay_path = root.join(DIR).join("acceptance-identity-overlay.json");
        let mut value: Value = serde_json::from_slice(&fs::read(&overlay_path).unwrap()).unwrap();
        value["governance_voting_principal"] = json!(PRODUCTION_JUPITER_SNS_REWARDS_CANISTER_ID);
        fs::write(&overlay_path, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(validate_rendered_yaml(&root, &economics(&root).unwrap())
            .unwrap_err()
            .contains("frozen dapp allowlist"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn same_voter_keeps_claim_and_governance_only_accounts_distinct() {
        let operator = principal(
            "zkkkd-i34qc-367ln-e2u7o-ezznu-dkfqh-gtfvz-cviph-6qa4v-evtfs-wqe",
            "operator",
        )
        .unwrap();
        let governance = principal("h7y7s-kyaaa-aaaas-amxrq-cai", "Governance").unwrap();
        let proposer = sns_neuron_subaccount(operator, GOVERNANCE_MEMO);
        let voting = sns_neuron_subaccount(operator, JUPITER_GOVERNANCE_MEMO);
        let reserve = sns_neuron_subaccount(operator, RESERVE_MEMO);
        assert_eq!(
            hex::encode(proposer),
            "39d4b187d2057bdc49dc3078c0a8e97efb4dc09c93ce6d35c699665f35b1e053"
        );
        assert_eq!(
            hex::encode(voting),
            "8d56e3b80021a892f55e161e8725092d866a5f9580badb2d408ebe09dbae5eef"
        );
        assert_eq!(
            hex::encode(reserve),
            "d7a5e01a747275983dbccdf7b596d55c3816844ac7b3731fa8d6c90e9922fc40"
        );
        assert_ne!(proposer, voting);
        let context = json!({"operator":operator.to_text(), "governance_voting_principal":operator.to_text(), "governance":governance.to_text(), "stream":"gjthz-7iaaa-aaaas-amxuq-cai"});
        let accounts = checkpoints::accounts(&context).unwrap();
        let excluded = accounts["excluded_staking_accounts"].as_array().unwrap();
        assert_eq!(excluded.len(), 2);
        assert!(excluded
            .iter()
            .any(|a| a["owner"] == governance.to_text()
                && a["subaccount_hex"] == hex::encode(voting)));
        assert!(!excluded
            .iter()
            .any(|a| a["subaccount_hex"] == hex::encode(proposer)));
        let plan = capitalization_plan(
            TOTAL_IO_E8S - 10_000,
            reserve_stake().unwrap() - 10_000,
            &[JUPITER_GOVERNANCE_IO_E8S],
        )
        .unwrap();
        assert_eq!(plan.claims_e8s, GOVERNANCE_IO_E8S);
        assert_eq!(plan.permanent_top_up_e8s, 2_000_000_000);
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
