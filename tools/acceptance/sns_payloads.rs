// Offline direct-genesis adapter, linked to the unmodified pinned upstream types.
use base64::{Engine, engine::general_purpose::STANDARD};
use candid::{CandidType, Principal, decode_one, encode_one};
use ic_base_types::PrincipalId;
use ic_icrc1_index_ng::{IndexArg, InitArg};
use ic_icrc1_ledger::{InitArgsBuilder, LedgerArgument};
use ic_ledger_canister_core::archive::ArchiveOptions;
use ic_nervous_system_common::ledger::compute_neuron_staking_subaccount_bytes;
use ic_sns_governance::{
    init::GovernanceCanisterInitPayloadBuilder,
    pb::v1::{
        Governance, NervousSystemParameters, Neuron, NeuronId, NeuronPermission,
        governance::{Mode, SnsMetadata, Version},
        neuron::DissolveState,
    },
};
use ic_sns_root::pb::v1::SnsRootCanister;
use ic_sns_swap::pb::v1::Init as SwapInit;
use icrc_ledger_types::{icrc::generic_metadata_value::MetadataValue, icrc1::account::Account};
use serde_json::{Value, json};
use std::{fs, path::Path};

fn write_payload<T: CandidType + for<'de> serde::Deserialize<'de>>(
    out: &Path,
    name: &str,
    value: &T,
) -> Result<(), Box<dyn std::error::Error>> {
    let bytes = encode_one(value)?;
    let decoded: T = decode_one(&bytes)?;
    if encode_one(decoded)? != bytes {
        return Err("typed constructor roundtrip mismatch".into());
    }
    fs::write(out.join(format!("{name}.bin")), bytes)?;
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 4 {
        return Err(
            "usage: sns-payloads CONFIG.yaml ECONOMICS.json CONTEXT.json OUTPUT_DIRECTORY".into(),
        );
    }
    let config: Value = serde_json::from_slice(&fs::read(&args[0])?)?;
    let economics: Value = serde_json::from_slice(&fs::read(&args[1])?)?;
    let canonical: Value = serde_json::from_str(include_str!(
        "../../deploy/acceptance/canonical-sns-economics.json"
    ))?;
    let contract: Value = serde_json::from_str(include_str!(
        "../../deploy/acceptance/genesis-contract.json"
    ))?;
    let context: Value = serde_json::from_slice(&fs::read(&args[2])?)?;
    let out = Path::new(&args[3]);
    if out.exists() {
        return Err("output directory already exists".into());
    }
    if economics != canonical
        || config["economics"] != canonical
        || config["schema"] != "io.acceptance.direct-sns-genesis.v1"
        || config["token_name"] != "IO"
        || config["token_symbol"] != "IO"
        || config["display_name"] != "IO Protocol"
        || config["parked_swap"] != contract["parked_swap"]
        || config["project_url"] != "https://ghrkr-eyaaa-aaaas-amxvq-cai.icp0.io/"
        || context.get("swap_start_timestamp_seconds").is_some()
        || context.get("swap_due_timestamp_seconds").is_some()
        || context.get("nns_proposal_id").is_some()
    {
        return Err("canonical direct genesis or frozen parked Swap changed".into());
    }
    let id = |key: &str| -> Result<PrincipalId, Box<dyn std::error::Error>> {
        Ok(context[key].as_str().ok_or("identity missing")?.parse()?)
    };
    let operator = id("operator")?;
    let jupiter = id("governance_voting_principal")?;
    if config["operator"] != context["operator"]
        || config["governance_voting_principal"] != context["governance_voting_principal"]
        || !matches!(
            config["environment"].as_str(),
            Some("acceptance" | "production" | "local-fixture-not-for-deployment")
        )
        || if config["environment"] == "production" {
            jupiter != "alk7f-5aaaa-aaaar-qb4ra-cai".parse::<PrincipalId>()? || operator == jupiter
        } else {
            operator != jupiter
        }
    {
        return Err("genesis authority differs from context".into());
    }
    let mut identities = std::collections::BTreeSet::new();
    for role in [
        "operator",
        "jupiter_source",
        "governance",
        "root",
        "ledger",
        "index",
        "swap",
        "stream",
        "nns_manager",
        "historian",
        "frontend",
    ] {
        let value = id(role)?.to_string();
        if value == "aaaaa-aa" || value == "2vxsx-fae" || !identities.insert(value) {
            return Err("invalid or colliding constructor identity".into());
        }
    }
    if operator != jupiter && !identities.insert(jupiter.to_string()) {
        return Err("governance voter collides with another role".into());
    }
    let mut dapps = config["dapp_canisters"]
        .as_array()
        .ok_or("dapp list missing")?
        .iter()
        .map(|v| v.as_str().ok_or("dapp invalid"))
        .collect::<Result<Vec<_>, _>>()?;
    let mut expected = ["stream", "nns_manager", "historian", "frontend"]
        .iter()
        .map(|k| context[*k].as_str().ok_or("dapp context missing"))
        .collect::<Result<Vec<_>, _>>()?;
    dapps.sort();
    expected.sort();
    if dapps != expected {
        return Err("exact four Root dapps required".into());
    }
    let gov_id = id("governance")?;
    let root_id = id("root")?;
    let ledger_id = id("ledger")?;
    let index_id = id("index")?;
    let swap_id = id("swap")?;
    let number = |key: &str| -> Result<u64, Box<dyn std::error::Error>> {
        Ok(canonical[key].as_u64().ok_or("canonical integer missing")?)
    };
    let logo = fs::read(config["logo_path"].as_str().ok_or("logo missing")?)?;
    if !logo.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err("PNG logo required".into());
    }
    let logo = format!("data:image/png;base64,{}", STANDARD.encode(logo));
    let mut p = NervousSystemParameters::with_default_values();
    p.automatically_advance_target_version = Some(false);
    p.max_number_of_neurons = Some(number("maximum_number_of_neurons")?);
    p.reject_cost_e8s = Some(number("proposal_reject_cost_e8s")?);
    p.neuron_minimum_stake_e8s = Some(number("minimum_neuron_stake_e8s")?);
    p.transaction_fee_e8s = Some(number("transfer_fee_e8s")?);
    p.neuron_minimum_dissolve_delay_to_vote_seconds =
        Some(number("minimum_voting_dissolve_delay_seconds")?);
    p.max_dissolve_delay_seconds = Some(number("maximum_dissolve_delay_seconds")?);
    p.max_dissolve_delay_bonus_percentage =
        Some(number("maximum_dissolve_delay_bonus_basis_points")? / 100);
    p.max_neuron_age_for_age_bonus = Some(number("maximum_neuron_age_seconds")?);
    p.max_age_bonus_percentage = Some(number("maximum_age_bonus_basis_points")? / 100);
    p.initial_voting_period_seconds = Some(number("initial_voting_period_seconds")?);
    p.wait_for_quiet_deadline_increase_seconds =
        Some(number("wait_for_quiet_deadline_increase_seconds")?);
    let r = p
        .voting_rewards_parameters
        .as_mut()
        .ok_or("rewards missing")?;
    r.initial_reward_rate_basis_points = Some(number("initial_reward_rate_basis_points")?);
    r.final_reward_rate_basis_points = Some(number("final_reward_rate_basis_points")?);
    r.reward_rate_transition_duration_seconds = Some(number("reward_rate_transition_seconds")?);
    r.round_duration_seconds = Some(number("reward_event_duration_seconds")?);
    let version: Version = serde_json::from_value(config["framework_version"].clone())?;
    for h in [
        &version.root_wasm_hash,
        &version.governance_wasm_hash,
        &version.ledger_wasm_hash,
        &version.index_wasm_hash,
        &version.swap_wasm_hash,
        &version.archive_wasm_hash,
    ] {
        if h.len() != 32 {
            return Err("six exact official module/template hashes required".into());
        }
    }
    let timestamp = context["genesis_timestamp_seconds"]
        .as_u64()
        .ok_or("genesis observation time missing")?;
    let mut governance = GovernanceCanisterInitPayloadBuilder::new()
        .with_mode(Mode::Normal)
        .with_ledger_canister_id(ledger_id)
        .with_root_canister_id(root_id)
        .with_swap_canister_id(swap_id)
        .with_parameters(p.clone())
        .with_genesis_timestamp_seconds(timestamp)
        .with_sns_metadata(SnsMetadata {
            name: Some("IO Protocol".into()),
            description: Some(
                config["description"]
                    .as_str()
                    .ok_or("description missing")?
                    .into(),
            ),
            url: Some(config["project_url"].as_str().unwrap().into()),
            logo: Some(logo.clone()),
        })
        .build();
    governance.deployed_version = Some(version);
    governance.sns_initialization_parameters = serde_json::to_string(&config)?;
    let mut ledger = InitArgsBuilder::with_symbol_and_name("IO", "IO")
        .with_minting_account(Principal::from(gov_id))
        .with_transfer_fee(number("transfer_fee_e8s")?)
        .with_decimals(8)
        .with_index_principal(Principal::from(index_id))
        .with_metadata_entry("icrc1:logo", MetadataValue::Text(logo))
        .with_archive_options(ArchiveOptions {
            trigger_threshold: 2000,
            num_blocks_to_archive: 1000,
            node_max_memory_size_bytes: Some(1_073_741_824),
            max_message_size_bytes: Some(131_072),
            controller_id: root_id,
            more_controller_ids: None,
            cycles_for_archive_creation: Some(10_000_000_000_000),
            max_transactions_per_response: None,
        });
    let mut total = 0u64;
    for (key, controller, permissions) in [
        (
            "governance_neuron",
            operator,
            (1..=10).collect::<Vec<i32>>(),
        ),
        ("jupiter_governance_neuron", jupiter, vec![4]),
        ("reserve_transport_neuron", operator, (1..=10).collect()),
    ] {
        let n = &canonical[key];
        let memo = n["memo"].as_u64().ok_or("memo missing")?;
        let stake = n["stake_e8s"].as_u64().ok_or("stake missing")?;
        let sub = compute_neuron_staking_subaccount_bytes(controller, memo);
        let neuron = Neuron {
            id: Some(NeuronId { id: sub.to_vec() }),
            permissions: vec![NeuronPermission {
                principal: Some(controller),
                permission_type: permissions,
            }],
            cached_neuron_stake_e8s: stake,
            created_timestamp_seconds: timestamp,
            aging_since_timestamp_seconds: timestamp,
            dissolve_state: Some(DissolveState::DissolveDelaySeconds(
                n["dissolve_delay_seconds"]
                    .as_u64()
                    .ok_or("delay missing")?,
            )),
            voting_power_percentage_multiplier: 100,
            auto_stake_maturity: Some(false),
            vesting_period_seconds: Some(0),
            ..Default::default()
        };
        if governance
            .neurons
            .insert(hex::encode(sub), neuron)
            .is_some()
        {
            return Err("genesis staking subaccounts must be distinct".into());
        }
        ledger = ledger.with_initial_balance(
            Account {
                owner: Principal::from(gov_id),
                subaccount: Some(sub),
            },
            stake,
        );
        total = total.checked_add(stake).ok_or("supply overflow")?;
    }
    if total != number("total_supply_e8s")? {
        return Err("genesis supply mismatch".into());
    }
    let root = SnsRootCanister {
        governance_canister_id: Some(gov_id),
        ledger_canister_id: Some(ledger_id),
        swap_canister_id: Some(swap_id),
        index_canister_id: Some(index_id),
        dapp_canister_ids: dapps.iter().map(|s| s.parse()).collect::<Result<_, _>>()?,
        archive_canister_ids: vec![],
        extensions: None,
        testflight: false,
        timers: None,
    };
    let mut swap_json = contract["parked_swap"].clone();
    for (key, value) in [
        ("sns_root_canister_id", root_id.to_string()),
        ("sns_governance_canister_id", gov_id.to_string()),
        ("sns_ledger_canister_id", ledger_id.to_string()),
        (
            "nns_governance_canister_id",
            ic_nns_constants::GOVERNANCE_CANISTER_ID.to_string(),
        ),
        (
            "icp_ledger_canister_id",
            ic_nns_constants::LEDGER_CANISTER_ID.to_string(),
        ),
    ] {
        swap_json[key] = json!(value);
    }
    swap_json["fallback_controller_principal_ids"] = json!([operator.to_string()]);
    swap_json["transaction_fee_e8s"] = json!(number("transfer_fee_e8s")?);
    swap_json["neuron_minimum_stake_e8s"] = json!(number("minimum_neuron_stake_e8s")?);
    let swap: SwapInit = serde_json::from_value(swap_json)?;
    swap.validate()?;
    let index = Some(IndexArg::Init(InitArg {
        ledger_id: Principal::from(ledger_id),
        retrieve_blocks_from_ledger_interval_seconds: None,
        min_retrieve_blocks_from_ledger_interval_seconds: None,
        max_retrieve_blocks_from_ledger_interval_seconds: None,
    }));
    fs::create_dir(out)?;
    write_payload(out, "governance", &governance)?;
    write_payload(out, "ledger", &LedgerArgument::Init(ledger.build()))?;
    write_payload(out, "root", &root)?;
    write_payload(out, "swap", &swap)?;
    write_payload(out, "index", &index)?;
    write_payload(out, "effective-governance-parameters", &p)?;
    let decoded: Governance = decode_one(&fs::read(out.join("governance.bin"))?)?;
    if decoded != governance {
        return Err("Governance roundtrip mismatch".into());
    }
    fs::write(
        out.join("parsed-economics.json"),
        format!("{}\n", serde_json::to_string_pretty(&canonical)?),
    )?;
    fs::write(
        out.join("parsed-init.json"),
        format!("{}\n", serde_json::to_string_pretty(&config)?),
    )?;
    fs::write(out.join("effective-parameters.txt"), format!("{p:#?}\n"))?;
    fs::write(
        out.join("requirements.json"),
        format!(
            "{}\n",
            json!({"genesis_max_number_of_neurons":p.max_number_of_neurons,"genesis_automatically_advance_target_version":p.automatically_advance_target_version,"route":"direct-Normal-no-sale","testflight":false})
        ),
    )?;
    Ok(())
}
