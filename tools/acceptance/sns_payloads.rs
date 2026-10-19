// Compiled against the pinned IC build, with its unmodified friendly parser
// included by build-sns-payload-tool. This is an offline payload adapter.
use candid::{Decode, Encode};
use ic_base_types::PrincipalId;
use ic_sns_init::{SnsCanisterIds, pb::v1::SnsInitPayload};
use serde_json::{Value, json};
use std::{fs, path::Path};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 4 {
        return Err(
            "usage: sns-payloads CONFIG.yaml ECONOMICS.json CONTEXT.json OUTPUT_DIRECTORY".into(),
        );
    }
    let config = Path::new(&args[0]);
    let e: Value = serde_json::from_slice(&fs::read(&args[1])?)?;
    let context: Value = serde_json::from_slice(&fs::read(&args[2])?)?;
    let output = Path::new(&args[3]);
    if output.exists() {
        return Err("output directory already exists".into());
    }
    let friendly: friendly::SnsConfigurationFile = serde_yaml::from_slice(&fs::read(config)?)?;
    // Also validate proposal metadata: conversion of the action alone ignores it.
    friendly.try_convert_to_nns_proposal(config.parent().ok_or("config has no parent")?)?;
    let proposal =
        friendly.try_convert_to_create_service_nervous_system(config.parent().unwrap())?;
    let declared_swap = proposal
        .swap_parameters
        .clone()
        .ok_or("swap parameters missing")?;
    let mut init = SnsInitPayload::try_from(proposal)?;
    init.validate_pre_execution()?;
    let parsed = serde_json::to_value(&init)?;
    let plan: Value = serde_json::from_slice(&fs::read(
        Path::new(&args[1])
            .parent()
            .ok_or("economics parent missing")?
            .join("required-inputs.draft.json"),
    )?)?;
    let duration = plan["sale_plan"]["duration_seconds"]
        .as_u64()
        .ok_or("sale duration missing")?;
    let start_time = ic_nervous_system_humanize::parse_time_of_day(
        plan["sale_plan"]["start_time"]
            .as_str()
            .ok_or("start time missing")?,
    )?;
    if declared_swap.duration.as_ref().and_then(|d| d.seconds) != Some(duration)
        || declared_swap.start_time != Some(start_time)
        || context["swap_due_timestamp_seconds"]
            .as_u64()
            .and_then(|due| {
                context["swap_start_timestamp_seconds"]
                    .as_u64()
                    .and_then(|start| due.checked_sub(start))
            })
            != Some(duration)
    {
        return Err("parsed sale schedule or execution duration mismatch".into());
    }
    let sale = plan["sale_plan"]["initial_swap_io_allocation_e8s"]
        .as_u64()
        .ok_or("sale allocation missing")?;
    if init.min_direct_participation_icp_e8s != Some(sale)
        || init.max_direct_participation_icp_e8s != Some(sale)
        || init.min_participant_icp_e8s != plan["sale_plan"]["minimum_participant_icp_e8s"].as_u64()
        || init.max_participant_icp_e8s != Some(sale)
        || init.min_participants != e["minimum_participants"].as_u64()
        || init.neurons_fund_participation != Some(false)
    {
        return Err("parsed sale price or participant limits differ from canonical draft".into());
    }
    let basket = init
        .neuron_basket_construction_parameters
        .as_ref()
        .ok_or("basket missing")?;
    if Some(basket.count) != plan["sale_plan"]["basket_count"].as_u64()
        || Some(basket.dissolve_delay_interval_seconds)
            != plan["sale_plan"]["basket_interval_seconds"].as_u64()
    {
        return Err("parsed sale basket mismatch".into());
    }
    let Some(ic_sns_init::pb::v1::sns_init_payload::InitialTokenDistribution::FractionalDeveloperVotingPower(distribution)) = init.initial_token_distribution.as_ref() else { return Err("initial distribution missing".into()) };
    let swap = distribution
        .swap_distribution
        .as_ref()
        .ok_or("swap distribution missing")?;
    if swap.total_e8s != sale
        || swap.initial_swap_amount_e8s != sale
        || distribution
            .treasury_distribution
            .as_ref()
            .ok_or("treasury distribution missing")?
            .total_e8s
            != 0
    {
        return Err("initial swap multiplier, future-swap or treasury mismatch".into());
    }
    let neurons = &distribution
        .developer_distribution
        .as_ref()
        .ok_or("genesis missing")?
        .developer_neurons;
    if neurons.len() != 3 {
        return Err("exactly three genesis neurons required".into());
    }
    let mut total = sale;
    for (key, controller_key) in [
        ("governance_neuron", "operator"),
        ("jupiter_governance_neuron", "jupiter"),
        ("reserve_transport_neuron", "operator"),
    ] {
        let memo = e[key]["memo"].as_u64().ok_or("genesis memo missing")?;
        let neuron = neurons
            .iter()
            .find(|n| n.memo == memo)
            .ok_or("genesis memo mismatch")?;
        let expected_stake = if key == "reserve_transport_neuron" {
            e["total_supply_e8s"]
                .as_u64()
                .ok_or("supply missing")?
                .checked_sub(
                    e["governance_neuron"]["stake_e8s"]
                        .as_u64()
                        .ok_or("operator stake missing")?,
                )
                .ok_or("stake underflow")?
                .checked_sub(
                    e["jupiter_governance_neuron"]["stake_e8s"]
                        .as_u64()
                        .ok_or("Jupiter stake missing")?,
                )
                .ok_or("stake underflow")?
                .checked_sub(sale)
                .ok_or("sale underflow")?
        } else {
            e[key]["stake_e8s"]
                .as_u64()
                .ok_or("genesis stake missing")?
        };
        let controller: PrincipalId = context[controller_key]
            .as_str()
            .ok_or("controller missing")?
            .parse()?;
        if neuron.controller != Some(controller)
            || neuron.stake_e8s != expected_stake
            || Some(neuron.dissolve_delay_seconds) != e[key]["dissolve_delay_seconds"].as_u64()
            || neuron.vesting_period_seconds != e[key]["vesting_period_seconds"].as_u64()
        {
            return Err(format!("parsed genesis mismatch: {key}").into());
        }
        total = total
            .checked_add(neuron.stake_e8s)
            .ok_or("supply overflow")?;
    }
    if Some(total) != e["total_supply_e8s"].as_u64() {
        return Err("parsed total genesis supply mismatch".into());
    }

    for (actual, canonical) in [
        ("transaction_fee_e8s", "transfer_fee_e8s"),
        ("proposal_reject_cost_e8s", "proposal_reject_cost_e8s"),
        ("neuron_minimum_stake_e8s", "minimum_neuron_stake_e8s"),
        (
            "neuron_minimum_dissolve_delay_to_vote_seconds",
            "minimum_voting_dissolve_delay_seconds",
        ),
        (
            "max_dissolve_delay_seconds",
            "maximum_dissolve_delay_seconds",
        ),
        (
            "max_neuron_age_seconds_for_age_bonus",
            "maximum_neuron_age_seconds",
        ),
        (
            "initial_reward_rate_basis_points",
            "initial_reward_rate_basis_points",
        ),
        (
            "final_reward_rate_basis_points",
            "final_reward_rate_basis_points",
        ),
        (
            "reward_rate_transition_duration_seconds",
            "reward_rate_transition_seconds",
        ),
        (
            "initial_voting_period_seconds",
            "initial_voting_period_seconds",
        ),
        (
            "wait_for_quiet_deadline_increase_seconds",
            "wait_for_quiet_deadline_increase_seconds",
        ),
    ] {
        if parsed[actual].is_null() || parsed[actual] != e[canonical] {
            return Err(format!(
                "parsed economic mismatch: {actual}: {:?} != {:?}",
                parsed[actual], e[canonical]
            )
            .into());
        }
    }
    for (actual, canonical) in [
        (
            "max_dissolve_delay_bonus_percentage",
            "maximum_dissolve_delay_bonus_basis_points",
        ),
        ("max_age_bonus_percentage", "maximum_age_bonus_basis_points"),
    ] {
        if parsed[actual].as_u64().and_then(|n| n.checked_mul(100)) != e[canonical].as_u64() {
            return Err(format!("parsed bonus mismatch: {actual}").into());
        }
    }
    // These fields are execution context, never fabricated sale observations.
    init.nns_proposal_id = Some(
        context["nns_proposal_id"]
            .as_u64()
            .ok_or("proposal context missing")?,
    );
    init.swap_start_timestamp_seconds = Some(
        context["swap_start_timestamp_seconds"]
            .as_u64()
            .ok_or("start missing")?,
    );
    init.swap_due_timestamp_seconds = Some(
        context["swap_due_timestamp_seconds"]
            .as_u64()
            .ok_or("due missing")?,
    );
    let id = |key: &str| -> Result<PrincipalId, Box<dyn std::error::Error>> {
        Ok(context[key]
            .as_str()
            .ok_or("canister ID missing")?
            .parse()?)
    };
    let ids = SnsCanisterIds {
        governance: id("governance")?,
        ledger: id("ledger")?,
        root: id("root")?,
        swap: id("swap")?,
        index: id("index")?,
    };
    let mut payloads = init.build_canister_payloads(&ids, None, false)?;
    // This SNS is installed manually from a reviewed, frozen official tuple.
    // The friendly init format does not expose this standard Governance
    // parameter, so bind it explicitly before encoding the source-defined
    // constructor payload. Otherwise Governance's periodic task can discover
    // its running version through Root and advance to a later SNS-W target.
    payloads
        .governance
        .parameters
        .as_mut()
        .ok_or("Governance parameters missing")?
        .automatically_advance_target_version = Some(false);
    let parameters = payloads
        .governance
        .parameters
        .as_ref()
        .ok_or("Governance parameters missing")?;
    let rewards = parameters
        .voting_rewards_parameters
        .as_ref()
        .ok_or("reward parameters missing")?;
    if rewards.round_duration_seconds != e["reward_event_duration_seconds"].as_u64() {
        return Err("effective reward duration differs from canonical model".into());
    }
    // The friendly parser cannot set the capacity bound. Do not silently patch
    // that economic limit at genesis: emit the standard post-sale Governance
    // change required before C, and repeat the frozen-version setting in the
    // same proposal.
    let mut required_parameters = ic_sns_governance::pb::v1::NervousSystemParameters::default();
    required_parameters.max_number_of_neurons = Some(
        e["maximum_number_of_neurons"]
            .as_u64()
            .ok_or("neuron bound missing")?,
    );
    required_parameters.automatically_advance_target_version = Some(false);
    fs::create_dir(output)?;
    macro_rules! write_payload {
        ($name:literal, $payload:expr) => {{
            let bytes = Encode!(&$payload)?;
            fn verify<T: candid::CandidType + for<'de> serde::Deserialize<'de>>(
                value: &T,
                bytes: &[u8],
            ) -> Result<(), Box<dyn std::error::Error>> {
                let decoded: T = candid::decode_one(bytes)?;
                if candid::encode_one(&decoded)? != candid::encode_one(value)? {
                    return Err("typed payload roundtrip mismatch".into());
                }
                Ok(())
            }
            verify(&$payload, &bytes)?;
            fs::write(output.join(concat!($name, ".bin")), &bytes)?;
        }};
    }
    write_payload!("governance", payloads.governance);
    write_payload!("ledger", payloads.ledger);
    write_payload!("root", payloads.root);
    write_payload!("swap", payloads.swap);
    write_payload!("index", payloads.index_ng);
    write_payload!("required-governance-parameters", required_parameters);
    write_payload!("effective-governance-parameters", *parameters);
    let decoded = Decode!(
        &fs::read(output.join("governance.bin"))?,
        ic_sns_governance::pb::v1::Governance
    )?;
    if decoded != payloads.governance {
        return Err("Governance payload roundtrip mismatch".into());
    }
    // Project parsed economics, allowing only named environment/branding fields
    // and controller identities to differ. New parser fields remain compared.
    let mut economic_projection = parsed.clone();
    for field in [
        "token_name",
        "token_symbol",
        "token_logo",
        "logo",
        "name",
        "description",
        "url",
        "confirmation_text",
        "fallback_controller_principal_ids",
        "dapp_canisters",
        "sns_initialization_parameters",
    ] {
        economic_projection
            .as_object_mut()
            .ok_or("parsed payload not object")?
            .remove(field);
    }
    if let Some(neurons) = economic_projection.pointer_mut("/initial_token_distribution/FractionalDeveloperVotingPower/developer_distribution/developer_neurons").and_then(Value::as_array_mut) {
        for neuron in neurons {neuron.as_object_mut().ok_or("neuron not object")?.remove("controller");}
    } else {return Err("parsed distribution shape changed".into());}
    fs::write(
        output.join("parsed-economics.json"),
        format!("{}\n", serde_json::to_string_pretty(&economic_projection)?),
    )?;
    fs::write(
        output.join("parsed-init.json"),
        format!("{}\n", serde_json::to_string_pretty(&parsed)?),
    )?;
    fs::write(
        output.join("effective-parameters.txt"),
        format!("{parameters:#?}\n"),
    )?;
    fs::write(
        output.join("requirements.json"),
        format!(
            "{}\n",
            json!({
                "genesis_max_number_of_neurons": parameters.max_number_of_neurons,
                "genesis_automatically_advance_target_version": parameters.automatically_advance_target_version,
                "required_max_number_of_neurons": required_parameters.max_number_of_neurons,
                "required_automatically_advance_target_version": required_parameters.automatically_advance_target_version,
                "route": "standard ManageNervousSystemParameters proposal after successful sale, before IO installation",
                "testflight": false
            })
        ),
    )?;
    Ok(())
}
