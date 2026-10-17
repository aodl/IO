//! Validation boundaries over a local rehearsal evidence directory.
//! A has no dependency on observations that can only exist after the sale.
use super::*;
use std::path::PathBuf;

fn json_at(dir: &Path, name: &str) -> Result<Value, String> {
    serde_json::from_slice(&fs::read(dir.join(name)).map_err(|e| format!("{name}: {e}"))?)
        .map_err(|e| format!("{name}: {e}"))
}
fn text<'a>(v: &'a Value, key: &str) -> Result<&'a str, String> {
    v[key]
        .as_str()
        .filter(|s| !is_placeholder(s))
        .ok_or_else(|| format!("required input {key} unresolved"))
}
fn amount(v: &Value, key: &str) -> Result<u128, String> {
    value_u128(&v[key], key)
}

pub(crate) fn accounts(context: &Value) -> Result<Value, String> {
    let stream = principal(text(context, "stream")?, "Stream")?;
    let governance = principal(text(context, "governance")?, "Governance")?;
    let operator = principal(text(context, "operator")?, "operator")?;
    let jupiter = principal(text(context, "jupiter")?, "Jupiter")?;
    let account = |owner: Principal, sub: [u8; 32]| json!({"owner": owner.to_text(), "subaccount_hex": hex::encode(sub)});
    Ok(json!({
        "io_reserve": account(stream, STREAM_RESERVE_SUBACCOUNT),
        "liquid_icp": account(stream, STREAM_LIQUID_SUBACCOUNT),
        "excluded_staking_accounts": [
            account(governance, sns_neuron_subaccount(operator, RESERVE_MEMO)),
            account(governance, sns_neuron_subaccount(jupiter, JUPITER_GOVERNANCE_MEMO))
        ],
        "excluded_from_claims": true,
        "jupiter_excluded_from_rewards": true,
        "dynamic_anchor_e8s": DYNAMIC_ANCHOR_E8S,
        "sale_treasury": {"owner": governance.to_text(), "subaccount_hex": hex::encode([0u8;32])}
    }))
}

fn validate_context(root: &Path, context: &Value) -> Result<(), String> {
    if context.get("root_install").is_some_and(|v| !v.is_boolean()) {
        return Err("root_install must be a boolean".into());
    }
    let mut principals = BTreeSet::new();
    let forbidden = [
        PRODUCTION_IO_STREAM_MANAGER_CANISTER_ID,
        PRODUCTION_IO_NNS_NEURON_MANAGER_CANISTER_ID,
        PRODUCTION_IO_HISTORIAN_CANISTER_ID,
        PRODUCTION_FRONTEND_CANISTER_ID,
        PRODUCTION_JUPITER_SNS_REWARDS_CANISTER_ID,
        JUPITER_FAUCET_CANISTER_ID,
    ];
    for name in [
        "operator",
        "jupiter",
        "jupiter_source",
        "stream",
        "nns_manager",
        "historian",
        "frontend",
        "governance",
        "root",
        "ledger",
        "index",
        "swap",
    ] {
        let value = text(context, name)?;
        let p = principal(value, name)?;
        if p == Principal::anonymous()
            || p == Principal::management_canister()
            || forbidden.contains(&value)
            || !principals.insert(p)
        {
            return Err(format!(
                "invalid, production, or colliding local role: {name}"
            ));
        }
    }
    match context["local_fixture"].as_bool() {
        Some(true) => {}
        Some(false) => {
            let manifest: AcceptanceManifest =
                read_json(root, &format!("{DIR}/acceptance-manifest.json"))?;
            validate_manifest(&manifest)?;
            for (name, frozen) in [
                ("governance", "sns_governance"),
                ("root", "sns_root"),
                ("ledger", "sns_ledger"),
                ("index", "sns_index"),
                ("swap", "sns_swap"),
                ("stream", "stream_manager"),
                ("nns_manager", "nns_manager"),
                ("historian", "historian"),
                ("frontend", "frontend"),
            ] {
                if context[name] != role(&manifest, frozen)? {
                    return Err(format!("acceptance role mismatch: {name}"));
                }
            }
            let inputs: Value = read_json(root, &format!("{DIR}/required-inputs.draft.json"))?;
            validate_sale_plan(&economics(root)?, &inputs, true)?;
            for (name, input) in [
                ("operator", "genesis_controller_principal"),
                ("jupiter", "acceptance_jupiter_sns_rewards_canister_id"),
                ("jupiter_source", "acceptance_jupiter_source_principal"),
            ] {
                if context[name] != text(&inputs, input)? {
                    return Err(format!(
                        "acceptance identity differs from reviewed inputs: {name}"
                    ));
                }
            }
            let overlay = overlay(root, "acceptance")?;
            if overlay.jupiter_sns_rewards_canister_id != text(context, "jupiter")?
                || overlay.fallback_controller_principal != text(context, "operator")?
            {
                return Err(
                    "acceptance overlay authority differs from checkpoint identities".into(),
                );
            }
            let dapps = overlay
                .dapp_canisters
                .values()
                .cloned()
                .collect::<BTreeSet<_>>();
            let required = ["stream", "nns_manager", "historian", "frontend"]
                .iter()
                .map(|key| text(context, key).map(str::to_string))
                .collect::<Result<BTreeSet<_>, _>>()?;
            if dapps != required {
                return Err("acceptance dapp registration mismatch".into());
            }
            for field in [
                "final_token_name",
                "final_token_symbol",
                "proposal_url",
                "project_url",
                "sns_logo_path",
            ] {
                let value = text(&inputs, field)?;
                if value == "IOLT"
                    || value.to_ascii_lowercase().contains("fixture")
                    || value.contains("example.invalid")
                {
                    return Err("local test metadata cannot be deployment input".into());
                }
            }
            if context["root_install"] != true || context.get("fixture_environment").is_some() {
                return Err(
                    "acceptance requires the proven initial-registration route and real metadata"
                        .into(),
                );
            }
        }
        None => return Err("explicit local_fixture boolean is required".into()),
    }
    Ok(())
}

fn validate_raw_wasm(path: &Path, did: Option<&Path>, hidden: Option<&Path>) -> Result<(), String> {
    let mut command = std::process::Command::new("ic-wasm");
    command.arg(path);
    if let Some(did) = did {
        command.arg("check-endpoints").arg("--candid").arg(did);
        if let Some(hidden) = hidden {
            command.arg("--hidden").arg(hidden);
        }
    } else {
        command.arg("info");
    }
    let output = command
        .output()
        .map_err(|e| format!("run existing ic-wasm validator: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "invalid Wasm/interface {}: {}",
            path.display(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

fn validate_sns_source(manifest: &io_sns_manifest::SnsManifest) -> Result<(), String> {
    if manifest.value("variant", "scope") != Some("same-source")
        || manifest.value("variant", "ic_commit") != Some(approved_sns_source())
        || manifest.bool_value("variant", "source_tree_clean") != Some(true)
        || manifest.bool_value("capabilities", "latest_reward_event_participation") != Some(true)
    {
        return Err("A requires the clean approved same-source reward-capable bundle".into());
    }
    for component in ["governance", "root", "ledger", "index", "swap", "archive"] {
        if manifest.artifact(&format!("sns_{component}"), "source_commit")
            != Some(approved_sns_source())
        {
            return Err(format!("{component} source mismatch"));
        }
        if manifest
            .value("contract", &format!("{component}_did_sha256"))
            .is_none()
        {
            return Err(format!("required {component} DID binding missing"));
        }
    }
    Ok(())
}

fn validate_parser_binding(binding: &Value, adapter: &[u8], binary: &[u8]) -> Result<(), String> {
    if binding["ic_commit"] != approved_sns_source()
        || binding["ic_dirty"] != false
        || binding["friendly_parser_sha256"] != approved_sns_baseline()["friendly_parser_sha256"]
        || binding["adapter_sha256"] != hex::encode(Sha256::digest(adapter))
        || binding["binary_sha256"] != hex::encode(Sha256::digest(binary))
    {
        return Err("pinned parser byte/source binding mismatch".into());
    }
    Ok(())
}

pub(crate) fn checkpoint_a(root: &Path, dir: &Path) -> Result<Value, String> {
    let context = json_at(dir, "context.json")?;
    validate_context(root, &context)?;
    let io_artifacts = validate_io_artifacts(root, dir)?;
    let expected_accounts = accounts(&context)?;
    if json_at(dir, "accounts.json")? != expected_accounts {
        return Err("complete Account identity or economic exclusion mismatch".into());
    }
    let baseline = json_at(dir, "baseline-canisters.json")?;
    for role in [
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
        principal(text(&baseline[role], "subnet_id")?, "baseline subnet")?;
        if context["root_install"] == true
            && baseline[role]["subnet_id"] != baseline["governance"]["subnet_id"]
        {
            return Err("Governance chunk store and IO install targets must share a subnet".into());
        }
        if baseline[role]["canister_id"] != context[role]
            || baseline[role]["controllers"] != json!([context["operator"]])
            || baseline[role].get("module_hash").is_none()
            || !baseline[role]["module_hash"].is_null()
        {
            return Err(format!(
                "pre-install authority/empty-canister baseline mismatch: {role}"
            ));
        }
    }
    let bundle = PathBuf::from(text(&context, "bundle_path")?);
    crate::sns_framework::validate_bundle(&bundle)?;
    let manifest = io_sns_manifest::SnsManifest::read(&bundle.join("manifest.toml"))?;
    validate_sns_source(&manifest)?;
    for component in ["governance", "root", "ledger", "index", "swap", "archive"] {
        let did = fs::read(bundle.join(format!("{component}.did"))).map_err(|e| e.to_string())?;
        if manifest.value("contract", &format!("{component}_did_sha256"))
            != Some(hex::encode(Sha256::digest(&did)).as_str())
        {
            return Err(format!("{component} DID hash mismatch"));
        }
        if component == "governance" {
            crate::sns_framework::verify_candidate_did(
                std::str::from_utf8(&did).map_err(|e| e.to_string())?,
            )?;
        }
    }
    let mut hashes = BTreeMap::new();
    for component in [
        "sns_governance",
        "sns_root",
        "sns_ledger",
        "sns_index",
        "sns_swap",
        "sns_archive",
    ] {
        let bytes = fs::read(
            bundle
                .join("wasms")
                .join(manifest.artifact_name(component)?),
        )
        .map_err(|e| e.to_string())?;
        validate_raw_wasm(
            &bundle
                .join("wasms")
                .join(manifest.artifact_name(component)?),
            None,
            None,
        )?;
        if !bytes.starts_with(b"\0asm\x01\0\0\0") {
            return Err(format!("malformed {component} Wasm"));
        }
        hashes.insert(component, json!({"raw_wasm_sha256": manifest.require_hash(component)?, "gzip_sha256": manifest.source_sha256(component).ok_or("missing gzip hash")?}));
    }
    if context["local_fixture"] == false {
        let inputs: Value = read_json(root, &format!("{DIR}/required-inputs.draft.json"))?;
        if inputs["final_sns_sale_commit"] != approved_sns_source() {
            return Err("deployment sale source differs from verified pinned source".into());
        }
        for component in ["governance", "root", "ledger", "index", "swap", "archive"] {
            if text(
                &inputs["final_sns_sale_artifacts"],
                &format!("{component}_wasm_sha256"),
            )? != manifest.require_hash(&format!("sns_{component}"))?
                || text(
                    &inputs["final_sns_sale_artifacts"],
                    &format!("{component}_did_sha256"),
                )? != manifest
                    .value("contract", &format!("{component}_did_sha256"))
                    .ok_or("pinned interface hash missing")?
            {
                return Err(format!(
                    "deployment artifact/interface hash differs from verified bytes: {component}"
                ));
            }
        }
        for role in ["stream", "nns_manager", "historian", "frontend"] {
            let key = if role == "nns_manager" {
                "nns_manager"
            } else {
                role
            };
            if inputs["current_source_io_artifacts"][format!("{key}_wasm_sha256")]
                != io_artifacts[role]["raw_wasm_sha256"]
            {
                return Err(format!("deployment IO artifact hash mismatch: {role}"));
            }
        }
        let before = super::observations::existing_permanent_baseline(dir, &context)?;
        let declared = &inputs["permanent_neuron_observation"];
        if declared["neuron_id"] != ACCEPTANCE_NEURON_ID
            || declared["controller"] != context["nns_manager"]
            || declared["staking_subaccount_hex"] != hex::encode(&before.account)
            || amount(declared, "cached_stake_e8s")? != u128::from(before.cached_neuron_stake_e8s)
            || amount(declared, "dissolve_delay_seconds")? != u128::from(PERMANENT_DELAY_SECONDS)
            || declared["auto_stake_maturity"] != false
            || declared["dissolve_state"] != "NotDissolving"
            || declared["maturity_disbursement_in_progress"] != false
            || declared["follow_policy"] != json!(before.followees.iter().map(|(topic, group)| json!({"topic":topic,"followees":group.followees.iter().map(|id|id.id).collect::<Vec<_>>()})).collect::<Vec<_>>())
        {
            return Err(
                "declared permanent baseline differs from canonical pre-install observation".into(),
            );
        }
    }
    let authority = json_at(dir, "authority-plan.json")?;
    let expected = json!({
        "initial_framework_installer": context["operator"],
        "initial_io_installer": if context["root_install"] == true {&context["root"]} else {&context["operator"]},
        "route": if context["root_install"] == true {"root-install-after-sale"} else {"local-deferred-registration-candidate"},
        "initial_registered_dapps": if context["root_install"] == true {vec![context["frontend"].clone(),context["historian"].clone(),context["nns_manager"].clone(),context["stream"].clone()]} else {vec![]},
        "io_install_before_controller_handoff": context["root_install"] != true,
        "registration_caller": if context["root_install"] == true {&context["operator"]} else {&context["governance"]},
        "registration_method":if context["root_install"] == true {"Root init payload"} else {"Governance/RegisterDappCanisters"},
        "chunk_store":if context["root_install"] == true {Some(&context["governance"])} else {None},
        "final_io_controller": context["root"],
        "deployment_parameters_approved": context["local_fixture"] == false
    });
    if authority != expected {
        return Err("installation-authority plan mismatch".into());
    }
    let parser = PathBuf::from(text(&context, "parser_tool")?);
    let binding = json_at(
        parser.parent().ok_or("parser has no parent")?,
        "build-inputs.json",
    )?;
    validate_parser_binding(
        &binding,
        &fs::read(root.join("tools/acceptance/sns_payloads.rs")).map_err(|e| e.to_string())?,
        &fs::read(&parser).map_err(|e| e.to_string())?,
    )?;
    let temporary = dir.join(format!("verify-a-{}", std::process::id()));
    fs::create_dir(&temporary).map_err(|e| e.to_string())?;
    let result = (|| {
        let generated = temporary.join("sns.yaml");
        if context["local_fixture"] == true {
            super::local_config(root, &dir.join("context.json"), &generated)?;
        } else {
            fs::write(&generated, super::render(root, "acceptance")?).map_err(|e| e.to_string())?;
        }
        if fs::read(&generated).map_err(|e| e.to_string())?
            != fs::read(dir.join("sns.yaml")).map_err(|e| e.to_string())?
        {
            return Err("stale generated SNS configuration".into());
        }
        let outputs = temporary.join("payloads");
        let status = std::process::Command::new(&parser)
            .arg(&generated)
            .arg(root.join(DIR).join("canonical-sns-economics.json"))
            .arg(dir.join("context.json"))
            .arg(&outputs)
            .status()
            .map_err(|e| e.to_string())?;
        if !status.success() {
            return Err(
                "pinned parser/pre-execution validator/payload builder rejected configuration"
                    .into(),
            );
        }
        for name in [
            "governance.bin",
            "root.bin",
            "ledger.bin",
            "index.bin",
            "swap.bin",
            "required-governance-parameters.bin",
            "parsed-init.json",
            "parsed-economics.json",
            "effective-parameters.txt",
            "effective-governance-parameters.bin",
            "requirements.json",
        ] {
            if fs::read(outputs.join(name)).map_err(|e| e.to_string())?
                != fs::read(dir.join("payloads").join(name)).map_err(|e| e.to_string())?
            {
                return Err(format!("stale or mismatched generated payload: {name}"));
            }
        }
        Ok(
            json!({"checkpoint":"A", "local_fixture":context["local_fixture"], "source_commit":approved_sns_source(), "artifacts":hashes, "io_artifacts":io_artifacts,
            "post_sale_observations_required":false, "deployment_parameters_approved":context["local_fixture"] == false}),
        )
    })();
    // Only this invocation's temporary verification outputs are removed.
    fs::remove_dir_all(&temporary).map_err(|e| e.to_string())?;
    result
}

pub(crate) fn checkpoint_b(root: &Path, dir: &Path) -> Result<Value, String> {
    checkpoint_a(root, dir)?;
    let b = json_at(dir, "observation-b.json")?;
    if b["schema"] != "io.acceptance.observation-b.v1" {
        return Err("B observation schema mismatch".into());
    }
    super::observations::finalization(dir, &b)?;
    let inputs: Value = read_json(root, &format!("{DIR}/required-inputs.draft.json"))?;
    let basket_count = amount(&inputs["sale_plan"], "basket_count")?;
    let buyers = amount(&b["finalization"]["sweep_icp"], "success")?
        .checked_add(amount(&b["finalization"]["sweep_icp"], "skipped")?)
        .filter(|n| *n > 0)
        .ok_or("no finalized participants")?;
    let neuron_count = buyers
        .checked_mul(basket_count)
        .ok_or("basket count overflow")?;
    for step in ["sweep_icp", "create_recipes", "sweep_sns", "claim_neurons"] {
        let result = &b["finalization"][step];
        if amount(result, "failure")? != 0
            || amount(result, "invalid")? != 0
            || amount(result, "global_failures")? != 0
            || amount(result, "success")?
                .checked_add(amount(result, "skipped")?)
                .ok_or("step count overflow")?
                != if step == "sweep_icp" {
                    buyers
                } else {
                    neuron_count
                }
        {
            return Err(format!("sale finalization {step} incomplete"));
        }
    }
    if b["finalization"]["governance_normal"] != true
        || b["finalization"]["root_controller_changes_successful"] != true
        || b["finalization"]["fund_settlement_external_call_required"] != false
    {
        return Err("finalization authority/mode results missing".into());
    }
    let sale = planned_sale_allocation(&inputs)?.ok_or("sale allocation missing")?;
    let accepted = amount(&b, "accepted_direct_icp_e8s")?;
    if amount(&inputs["sale_plan"], "minimum_participant_icp_e8s")?
        .checked_mul(buyers)
        .ok_or("participation bound overflow")?
        > accepted
    {
        return Err("finalized participants violate the configured minimum contribution".into());
    }
    let participant_stake = amount(&b, "participant_stake_e8s")?;
    if amount(&b, "operator_stake_e8s")? != GOVERNANCE_IO_E8S {
        return Err("actual operator stake mismatch".into());
    }
    let context = json_at(dir, "context.json")?;
    let participants = json_at(dir, "participant-accounts.json")?;
    let participants = participants
        .as_array()
        .ok_or("participant Accounts missing")?;
    let mut unique = BTreeSet::new();
    let mut actual = 0u128;
    for participant in participants {
        let sub = text(participant, "subaccount_hex")?;
        if participant["owner"] != context["governance"]
            || hex::decode(sub).map_err(|e| e.to_string())?.len() != 32
            || !unique.insert(sub)
        {
            return Err("participant Account identity mismatch".into());
        }
        actual = actual
            .checked_add(amount(participant, "balance_e8s")?)
            .ok_or("participant balance overflow")?;
    }
    if participants.len() as u128 != neuron_count || actual != participant_stake {
        return Err("participant Ledger balances differ from distributed stake".into());
    }

    let distribution_fees = amount(&b, "distribution_fees_e8s")?;
    let residual = amount(&b, "swap_residual_e8s")?;
    if accepted != sale
        || amount(&b, "accepted_fund_icp_e8s")? != 0
        || participant_stake
            .checked_add(distribution_fees)
            .and_then(|n| n.checked_add(residual))
            != Some(sale)
        || amount(&b, "treasury_icp_e8s")?.checked_add(amount(&b, "icp_sweep_fees_e8s")?)
            != Some(accepted)
    {
        return Err("sale price, distribution or retained treasury conservation mismatch".into());
    }
    let supply = amount(&b, "total_supply_e8s")?;
    let reserve = amount(&b, "stream_reserve_e8s")?;
    let excluded_reserve = amount(&b, "excluded_reserve_e8s")?;
    let excluded_jupiter = amount(&b, "excluded_jupiter_e8s")?;
    let reserve_fee = amount(&b, "reserve_disbursement_fee_e8s")?;
    if reserve_fee != IO_FEE_E8S
        || excluded_jupiter != JUPITER_GOVERNANCE_IO_E8S
        || excluded_reserve != 0
        || supply
            .checked_add(distribution_fees)
            .and_then(|n| n.checked_add(reserve_fee))
            != Some(TOTAL_IO_E8S)
        || reserve.checked_add(reserve_fee) != Some(reserve_stake(sale)?)
    {
        return Err("reserve migration, exclusion or total supply conservation mismatch".into());
    }
    let plan = capitalization_plan(supply, reserve, &[excluded_reserve, excluded_jupiter])?;
    if plan.claims_e8s
        != GOVERNANCE_IO_E8S
            .checked_add(participant_stake)
            .and_then(|n| n.checked_add(residual))
            .ok_or("claim sum overflow")?
    {
        return Err("claim supply does not match actual participant/operator holdings".into());
    }
    Ok(
        json!({"checkpoint":"B", "io_total_supply_e8s":supply, "c0_e8s":plan.claims_e8s, "b0_e8s":plan.claim_backing_e8s,
        "permanent_top_up_e8s":plan.permanent_top_up_e8s,
        "permanent_exact_remainder_thirds":plan.permanent_exact_remainder_thirds,
        "dynamic_anchor_e8s":DYNAMIC_ANCHOR_E8S, "sale_treasury_retained_icp_e8s": amount(&b,"treasury_icp_e8s")?}),
    )
}

fn validate_io_artifacts(root: &Path, dir: &Path) -> Result<Value, String> {
    let modules = json_at(dir, "io-artifacts.json")?;
    for role in ["stream", "nns_manager", "historian", "frontend"] {
        let artifact = &modules[role];
        let path = PathBuf::from(text(artifact, "path")?);
        let bytes = fs::read(&path).map_err(|e| e.to_string())?;
        if !bytes.starts_with(b"\0asm\x01\0\0\0")
            || text(artifact, "raw_wasm_sha256")? != hex::encode(Sha256::digest(&bytes))
        {
            return Err(format!("malformed or mismatched raw IO Wasm: {role}"));
        }
        if role == "frontend" {
            frontend_assets::validate(
                Path::new(text(artifact, "browser_assets")?),
                &bytes,
                &frontend_assets::acceptance_config(),
            )?;
        }
        let did = match role {
            "stream" => "canisters/io_stream_manager/io_stream_manager.did",
            "nns_manager" => "canisters/io_nns_neuron_manager/io_nns_neuron_manager.did",
            "historian" => "canisters/io_historian/io_historian.did",
            "frontend" => "canisters/frontend/frontend.did",
            _ => unreachable!(),
        };
        validate_raw_wasm(
            &path,
            Some(&root.join(did)),
            Some(&root.join(format!("tools/acceptance/system-exports/{role}.txt"))),
        )?;
        // Reuse the artifact verifier as well as binding the declaration to bytes.
        crate::verify_artifact_hash(
            path.parent().ok_or("artifact parent missing")?,
            text(artifact, "sha256_sidecar")?,
        )?;
    }
    Ok(modules)
}

/// C consumes post-capitalization observations and binds the actual install
/// payloads to the same canonical model. Purely informational balances are absent.
pub(crate) fn checkpoint_c(root: &Path, dir: &Path) -> Result<Value, String> {
    let b = checkpoint_b(root, dir)?;
    let context = json_at(dir, "context.json")?;
    let c = json_at(dir, "observation-c.json")?;
    validate_capitalization(root, &context, &b, &c)?;
    super::observations::permanent(dir, &context, &c)?;
    super::observations::governance(dir, &c)?;
    super::observations::installation(dir, &context)?;
    let generated = generated_constructors(root, dir)?;
    for (role, bytes) in &generated {
        if fs::read(dir.join("constructors").join(format!("{role}.bin")))
            .map_err(|e| e.to_string())?
            != *bytes
        {
            return Err(format!("stale or mismatched typed constructor: {role}"));
        }
    }
    Ok(
        json!({"checkpoint":"C", "constructors":generated.iter().map(|(role,bytes)|
        (*role,hex::encode(Sha256::digest(bytes)))).collect::<BTreeMap<_,_>>(),
        "monetary_install_authority":if context["root_install"] == true { &context["root"] } else { &context["operator"] }, "local_fixture":context["local_fixture"]}),
    )
}

fn validate_capitalization(
    root: &Path,
    context: &Value,
    b: &Value,
    c: &Value,
) -> Result<(), String> {
    if context["local_fixture"] == false {
        let inputs: Value = read_json(root, &format!("{DIR}/required-inputs.draft.json"))?;
        if amount(&inputs, "jupiter_activation_block_floor")?
            != amount(c, "jupiter_activation_block_floor")?
            || amount(&inputs, "audited_permanent_principal_after_top_up_e8s")?
                != amount(c, "permanent_cached_stake_after_refresh_e8s")?
        {
            return Err(
                "deployment constructor inputs differ from actual capitalization observations"
                    .into(),
            );
        }
    }
    if c["schema"] != "io.acceptance.observation-c.v1" {
        return Err("C observation schema mismatch".into());
    }
    if amount(c, "io_total_supply_e8s")? != amount(b, "io_total_supply_e8s")?
        || amount(c, "liquid_backing_e8s")? != amount(b, "b0_e8s")?
        || amount(c, "io_total_supply_e8s")? != amount(c, "io_total_supply_before_funding_e8s")?
        || amount(c, "sale_treasury_icp_e8s")? != amount(b, "sale_treasury_retained_icp_e8s")?
        // This is the canonical initial funding observation, before bootstrap
        // or reconciliation consumes fees. Only 10 ICP becomes anchor capacity;
        // runtime classifies all additional physical principal as surplus.
        || amount(c, "dynamic_anchor_seed_e8s")? < DYNAMIC_ANCHOR_E8S
    {
        return Err("capitalization, anchor segregation, treasury or no-issuance mismatch".into());
    }
    let actual = amount(c, "permanent_cached_stake_after_refresh_e8s")?;
    if amount(c, "permanent_principal_before_top_up_e8s")?
        .checked_add(amount(b, "permanent_top_up_e8s")?)
        != Some(actual)
        || amount(c, "permanent_ledger_stake_e8s")? != actual
        || c["permanent_controller"] != context["nns_manager"]
        || c["permanent_dissolve_delay_seconds"] != PERMANENT_DELAY_SECONDS
        || c["permanent_auto_stake_maturity"] != false
        || c["refresh_caller"] != context["operator"]
        || c["refresh_succeeded"] != true
    {
        return Err("permanent principal, delay, controller or refresh mismatch".into());
    }
    let installer = if context["root_install"] == true {
        &context["root"]
    } else {
        &context["operator"]
    };
    if c["remaining_install_controllers"]
        != json!({"stream":[installer],"nns_manager":[installer],
        "historian":[installer],"frontend":[installer]})
    {
        return Err("remaining IO installation authority mismatch".into());
    }
    let p = &c["effective_governance_parameters"];
    for (field, expected) in [
        ("max_number_of_neurons", 1000),
        ("reject_cost_e8s", 1_000_000_000),
        ("neuron_minimum_stake_e8s", 100_000_000),
        ("transaction_fee_e8s", 10_000),
        ("max_dissolve_delay_seconds", 1_296_060),
        ("neuron_minimum_dissolve_delay_to_vote_seconds", 1_209_599),
        ("max_dissolve_delay_bonus_percentage", 0),
        ("max_age_bonus_percentage", 0),
        ("max_neuron_age_for_age_bonus", 0),
        ("initial_voting_period_seconds", 86_400),
        ("wait_for_quiet_deadline_increase_seconds", 43_200),
    ] {
        if amount(p, field)? != expected {
            return Err(format!("effective Governance parameter mismatch: {field}"));
        }
    }
    for (field, expected) in [
        ("round_duration_seconds", 86_400),
        ("reward_rate_transition_duration_seconds", 0),
        ("initial_reward_rate_basis_points", 0),
        ("final_reward_rate_basis_points", 0),
    ] {
        if amount(&p["voting_rewards_parameters"], field)? != expected {
            return Err(format!("effective reward parameter mismatch: {field}"));
        }
    }
    Ok(())
}

fn generated_constructors(
    root: &Path,
    dir: &Path,
) -> Result<BTreeMap<&'static str, Vec<u8>>, String> {
    let context = json_at(dir, "context.json")?;
    let bundle = PathBuf::from(text(&context, "bundle_path")?);
    let manifest = io_sns_manifest::SnsManifest::read(&bundle.join("manifest.toml"))?;
    super::constructors::payloads(
        root,
        &context,
        &json_at(dir, "observation-c.json")?,
        hex::decode(manifest.require_hash("sns_governance")?).map_err(|e| e.to_string())?,
        &json_at(dir, "io-artifacts.json")?,
    )
}

pub(crate) fn generate_constructors(root: &Path, dir: &Path) -> Result<(), String> {
    let b = checkpoint_b(root, dir)?;
    validate_capitalization(
        root,
        &json_at(dir, "context.json")?,
        &b,
        &json_at(dir, "observation-c.json")?,
    )?;
    super::observations::permanent(
        dir,
        &json_at(dir, "context.json")?,
        &json_at(dir, "observation-c.json")?,
    )?;
    super::observations::governance(dir, &json_at(dir, "observation-c.json")?)?;
    super::observations::installation(dir, &json_at(dir, "context.json")?)?;
    let generated = generated_constructors(root, dir)?;
    let destination = dir.join("constructors");
    fs::create_dir(&destination).map_err(|e| e.to_string())?;
    for (role, bytes) in generated {
        fs::write(destination.join(format!("{role}.bin")), bytes).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn acceptance_source_and_parser_require_the_reviewed_baseline() {
        let mut source = io_sns_manifest::SnsManifest::parse(&format!(
            "[variant]\nscope='same-source'\nic_commit='{}'\nsource_tree_clean=true\n[capabilities]\nlatest_reward_event_participation=true\n", approved_sns_source()
        )).unwrap();
        for component in ["governance", "root", "ledger", "index", "swap", "archive"] {
            source
                .set_artifact(
                    &format!("sns_{component}"),
                    "source_commit",
                    approved_sns_source(),
                )
                .unwrap();
            source
                .set_value(
                    "contract",
                    &format!("{component}_did_sha256"),
                    "a".repeat(64),
                )
                .unwrap();
        }
        validate_sns_source(&source).unwrap();
        for (section, key, value) in [
            ("variant", "ic_commit", "0".repeat(40)),
            ("variant", "source_tree_clean", "false".into()),
            (
                "capabilities",
                "latest_reward_event_participation",
                "false".into(),
            ),
        ] {
            let mut bad = source.clone();
            bad.set_value(section, key, value).unwrap();
            assert!(validate_sns_source(&bad).is_err());
        }
        for component in ["governance", "root", "ledger", "index", "swap", "archive"] {
            let mut bad = source.clone();
            bad.set_artifact(&format!("sns_{component}"), "source_commit", "0".repeat(40))
                .unwrap();
            assert!(
                validate_sns_source(&bad).is_err(),
                "mixed {component} source accepted"
            );
        }
        let adapter = b"component adapter";
        let binary = b"component binary";
        let binding = json!({"ic_commit":approved_sns_source(),"ic_dirty":false,
            "friendly_parser_sha256":approved_sns_baseline()["friendly_parser_sha256"],
            "adapter_sha256":hex::encode(Sha256::digest(adapter)),"binary_sha256":hex::encode(Sha256::digest(binary))});
        validate_parser_binding(&binding, adapter, binary).unwrap();
        for key in [
            "ic_commit",
            "friendly_parser_sha256",
            "adapter_sha256",
            "binary_sha256",
        ] {
            let mut bad = binding.clone();
            bad[key] = json!("0".repeat(64));
            assert!(
                validate_parser_binding(&bad, adapter, binary).is_err(),
                "accepted {key}"
            );
        }
        let mut bad = binding.clone();
        bad["ic_dirty"] = json!(true);
        assert!(validate_parser_binding(&bad, adapter, binary).is_err());
        // Matching untrusted declarations do not promote a revision to approval.
        let mut bad_source = source.clone();
        bad_source
            .set_value("variant", "ic_commit", "0".repeat(40))
            .unwrap();
        for component in ["governance", "root", "ledger", "index", "swap", "archive"] {
            bad_source
                .set_artifact(&format!("sns_{component}"), "source_commit", "0".repeat(40))
                .unwrap();
        }
        bad["ic_commit"] = json!("0".repeat(40));
        bad["ic_dirty"] = json!(false);
        assert!(validate_sns_source(&bad_source).is_err());
        assert!(validate_parser_binding(&bad, adapter, binary).is_err());
    }

    #[test]
    fn acceptance_context_component_requires_frozen_roles_and_explicit_real_metadata() {
        // Synthetic component inputs only; this is not deployment approval or A evidence.
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let root = source.join("target").join(format!(
            "acceptance-context-component-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join(DIR)).unwrap();
        for name in ["acceptance-manifest.json", "canonical-sns-economics.json"] {
            fs::copy(source.join(DIR).join(name), root.join(DIR).join(name)).unwrap();
        }
        let mut inputs: Value =
            read_json(&source, &format!("{DIR}/required-inputs.draft.json")).unwrap();
        let operator = Principal::from_slice(&[51; 29]).to_text();
        let jupiter = Principal::from_slice(&[52; 29]).to_text();
        let jupiter_source = Principal::from_slice(&[53; 29]).to_text();
        inputs["genesis_controller_principal"] = json!(operator);
        inputs["acceptance_jupiter_sns_rewards_canister_id"] = json!(jupiter);
        inputs["acceptance_jupiter_source_principal"] = json!(jupiter_source);
        inputs["sale_plan"]["approved_for_deployment"] = json!(true);
        for (key, value) in [
            ("final_token_name", "Component Example"),
            ("final_token_symbol", "CMPT"),
            ("proposal_url", "https://forum.dfinity.org/"),
            ("project_url", "https://example.org/component"),
            ("sns_logo_path", "component-logo.png"),
        ] {
            inputs[key] = json!(value);
        }
        let manifest: AcceptanceManifest =
            read_json(&source, &format!("{DIR}/acceptance-manifest.json")).unwrap();
        let mut context = json!({"local_fixture":false,"root_install":true,"operator":operator,"jupiter":jupiter,"jupiter_source":jupiter_source});
        for (name, frozen) in [
            ("governance", "sns_governance"),
            ("root", "sns_root"),
            ("ledger", "sns_ledger"),
            ("index", "sns_index"),
            ("swap", "sns_swap"),
            ("stream", "stream_manager"),
            ("nns_manager", "nns_manager"),
            ("historian", "historian"),
            ("frontend", "frontend"),
        ] {
            context[name] = json!(role(&manifest, frozen).unwrap());
        }
        let mut overlay: Value =
            read_json(&source, &format!("{DIR}/acceptance-identity-overlay.json")).unwrap();
        overlay["fallback_controller_principal"] = json!(operator);
        overlay["jupiter_sns_rewards_canister_id"] = json!(jupiter);
        fs::write(
            root.join(DIR).join("acceptance-identity-overlay.json"),
            serde_json::to_vec(&overlay).unwrap(),
        )
        .unwrap();
        let input_path = root.join(DIR).join("required-inputs.draft.json");
        fs::write(&input_path, serde_json::to_vec(&inputs).unwrap()).unwrap();
        validate_context(&root, &context).unwrap();
        for (field, value) in [
            ("stream", json!(Principal::from_slice(&[54; 29]).to_text())),
            ("jupiter", json!(PRODUCTION_JUPITER_SNS_REWARDS_CANISTER_ID)),
            ("root_install", json!(false)),
        ] {
            let mut bad = context.clone();
            bad[field] = value;
            assert!(validate_context(&root, &bad).is_err());
        }
        for (field, value) in [
            ("final_token_symbol", json!("IOLT")),
            (
                "project_url",
                json!("https://example.invalid/io-local-fixture"),
            ),
            ("proposal_url", json!("TBD_URL")),
        ] {
            let mut bad = inputs.clone();
            bad[field] = value;
            fs::write(&input_path, serde_json::to_vec(&bad).unwrap()).unwrap();
            assert!(validate_context(&root, &context).is_err());
        }
        inputs["sale_plan"]["approved_for_deployment"] = json!(false);
        fs::write(&input_path, serde_json::to_vec(&inputs).unwrap()).unwrap();
        assert!(validate_context(&root, &context).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    // Component fixture: this tests one validation function, not checkpoint C
    // or real capitalization. The ignored launch case supplies live observations.
    #[test]
    fn capitalization_component_checks_evidence_and_parameters() {
        let operator = Principal::from_slice(&[41; 29]).to_text();
        let manager = Principal::from_slice(&[42; 29]).to_text();
        let context = json!({"operator":operator,"nns_manager":manager});
        let b = json!({"io_total_supply_e8s":99_999_999_999_970_000u64,"b0_e8s":3_299_980_000u64,"permanent_top_up_e8s":2_199_986_667u64,"sale_treasury_retained_icp_e8s":299_990_000});
        let c = json!({"schema":"io.acceptance.observation-c.v1","liquid_backing_e8s":3_299_980_000u64,
            "io_total_supply_e8s":99_999_999_999_970_000u64,"io_total_supply_before_funding_e8s":99_999_999_999_970_000u64,
            "sale_treasury_icp_e8s":299_990_000,"dynamic_anchor_seed_e8s":1_000_000_000,
            "permanent_principal_before_top_up_e8s":100_000_000,"permanent_cached_stake_after_refresh_e8s":2_299_986_667u64,
            "permanent_ledger_stake_e8s":2_299_986_667u64,"permanent_controller":manager,
            "permanent_dissolve_delay_seconds":63_115_200,"permanent_auto_stake_maturity":false,
            "refresh_caller":operator,"refresh_succeeded":true,
            "remaining_install_controllers":{"stream":[operator],"nns_manager":[operator],"historian":[operator],"frontend":[operator]},
            "effective_governance_parameters":{"max_number_of_neurons":1000,"reject_cost_e8s":1_000_000_000,"neuron_minimum_stake_e8s":100_000_000,
                "transaction_fee_e8s":10_000,"max_dissolve_delay_seconds":1_296_060,"neuron_minimum_dissolve_delay_to_vote_seconds":1_209_599,
                "max_dissolve_delay_bonus_percentage":0,"max_age_bonus_percentage":0,"max_neuron_age_for_age_bonus":0,
                "initial_voting_period_seconds":86_400,"wait_for_quiet_deadline_increase_seconds":43_200,"voting_rewards_parameters":{
                    "round_duration_seconds":86_400,"reward_rate_transition_duration_seconds":0,"initial_reward_rate_basis_points":0,"final_reward_rate_basis_points":0}}});
        validate_capitalization(Path::new("."), &context, &b, &c).unwrap();
        for seed in [999_999_999_u64, 1_000_000_000, 1_000_000_001, 6_000_000_000] {
            let mut funded = c.clone();
            funded["dynamic_anchor_seed_e8s"] = json!(seed);
            assert_eq!(
                validate_capitalization(Path::new("."), &context, &b, &funded).is_ok(),
                seed >= 1_000_000_000,
                "initial seed {seed}"
            );
        }
        for (pointer,replacement) in [("/permanent_controller",json!(operator)),("/permanent_dissolve_delay_seconds",json!(63_115_199)),
            ("/permanent_cached_stake_after_refresh_e8s",json!(2_299_986_666u64)),("/refresh_succeeded",json!(false)),
            ("/dynamic_anchor_seed_e8s",json!(0)),("/liquid_backing_e8s",json!(0)),
            ("/effective_governance_parameters/reject_cost_e8s",json!(999_999_999)),
            ("/effective_governance_parameters/voting_rewards_parameters/round_duration_seconds",json!(43_200)),
            ("/effective_governance_parameters/voting_rewards_parameters/initial_reward_rate_basis_points",json!(1)),
            ("/effective_governance_parameters/max_number_of_neurons",json!(200_000))] {
            let mut bad=c.clone();*bad.pointer_mut(pointer).unwrap()=replacement;
            assert!(validate_capitalization(Path::new("."),&context,&b,&bad).is_err(),"accepted mutation {pointer}");
        }
    }
}
