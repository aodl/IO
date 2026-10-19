//! Width-subtyped observation DTOs for the pinned public replies. Constructors
//! use the production types in constructors.rs; observations retain raw replies.
use super::*;
use candid::CandidType;

#[derive(CandidType, Deserialize)]
struct Empty {}
#[derive(CandidType, Deserialize)]
struct CallResult<T> {
    possibility: Option<CallPossibility<T>>,
}
#[derive(CandidType, Deserialize)]
enum CallPossibility<T> {
    Ok(T),
    Err(Empty),
}
#[derive(CandidType, Deserialize)]
struct Counts {
    success: u32,
    failure: u32,
    skipped: u32,
    invalid: u32,
    global_failures: u32,
}
#[derive(CandidType, Deserialize)]
struct RootResult {
    failed_updates: Vec<Empty>,
}
#[derive(CandidType, Deserialize)]
struct FundResult {
    neurons_fund_participation_icp_e8s: Option<u64>,
    neurons_fund_neurons_count: Option<u64>,
}
#[derive(CandidType, Deserialize)]
struct Finalization {
    error_message: Option<String>,
    sweep_icp_result: Option<Counts>,
    create_sns_neuron_recipes_result: Option<Counts>,
    sweep_sns_result: Option<Counts>,
    claim_neuron_result: Option<Counts>,
    set_mode_call_result: Option<CallResult<Empty>>,
    set_dapp_controllers_call_result: Option<CallResult<RootResult>>,
    settle_neurons_fund_participation_result: Option<CallResult<FundResult>>,
}
#[derive(CandidType, Deserialize)]
struct Mode {
    mode: Option<i32>,
}
#[derive(CandidType, Deserialize, serde::Serialize)]
struct EffectiveParameters {
    automatically_advance_target_version: Option<bool>,
    max_number_of_neurons: Option<u64>,
    reject_cost_e8s: Option<u64>,
    neuron_minimum_stake_e8s: Option<u64>,
    transaction_fee_e8s: Option<u64>,
    max_dissolve_delay_seconds: Option<u64>,
    neuron_minimum_dissolve_delay_to_vote_seconds: Option<u64>,
    max_dissolve_delay_bonus_percentage: Option<u64>,
    max_age_bonus_percentage: Option<u64>,
    max_neuron_age_for_age_bonus: Option<u64>,
    initial_voting_period_seconds: Option<u64>,
    wait_for_quiet_deadline_increase_seconds: Option<u64>,
    voting_rewards_parameters: Option<EffectiveRewards>,
}
#[derive(CandidType, Deserialize, serde::Serialize)]
struct EffectiveRewards {
    round_duration_seconds: Option<u64>,
    reward_rate_transition_duration_seconds: Option<u64>,
    initial_reward_rate_basis_points: Option<u64>,
    final_reward_rate_basis_points: Option<u64>,
}

pub(super) fn governance(dir: &Path, c: &Value) -> Result<(), String> {
    let actual: EffectiveParameters = decode(dir, "effective-governance-parameters.bin")?;
    if serde_json::to_value(actual).map_err(|e| e.to_string())?
        != c["effective_governance_parameters"]
    {
        return Err("Governance parameter summary differs from Candid observation".into());
    }
    Ok(())
}

#[derive(CandidType, Deserialize)]
struct RootSummary {
    dapps: Vec<DappSummary>,
}
#[derive(CandidType, Deserialize)]
struct DappSummary {
    canister_id: Option<Principal>,
    status: Option<DappStatus>,
}
#[derive(CandidType, Deserialize)]
struct DappStatus {
    module_hash: Option<Vec<u8>>,
    settings: DappSettings,
}
#[derive(CandidType, Deserialize)]
struct DappSettings {
    controllers: Vec<Principal>,
}
#[derive(CandidType, Deserialize)]
struct LedgerTip {
    chain_length: u64,
}
pub(super) fn installation(dir: &Path, context: &Value) -> Result<(), String> {
    let tip: LedgerTip = decode(dir, "activation-ledger-tip.bin")?;
    let c: Value = serde_json::from_slice(
        &fs::read(dir.join("observation-c.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    if tip.chain_length == 0 || c["jupiter_activation_block_floor"] != tip.chain_length {
        return Err("Jupiter activation floor differs from observed ICP ledger tip".into());
    }
    if context["root_install"] != true {
        return Ok(());
    }
    let summary: RootSummary = decode(dir, "remaining-authority-root-summary.bin")?;
    let root = principal(context["root"].as_str().ok_or("Root missing")?, "Root")?;
    if summary.dapps.len() != 4 {
        return Err("Root registration count mismatch".into());
    }
    let chunks: Value = serde_json::from_slice(
        &fs::read(dir.join("preuploaded-chunks.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let modules: Value = serde_json::from_slice(
        &fs::read(dir.join("io-artifacts.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    for role in ["stream", "nns_manager", "historian", "frontend"] {
        let id = principal(context[role].as_str().ok_or("IO role missing")?, role)?;
        let status = summary
            .dapps
            .iter()
            .find(|d| d.canister_id == Some(id))
            .and_then(|d| d.status.as_ref())
            .ok_or("registered IO status absent")?;
        if status.module_hash.is_some() || status.settings.controllers != vec![root] {
            return Err("Root installation authority or empty-module observation mismatch".into());
        }
        let bytes = fs::read(
            modules[role]["path"]
                .as_str()
                .ok_or("IO artifact missing")?,
        )
        .map_err(|e| e.to_string())?;
        let hashes: Vec<String> = bytes
            .chunks(1_000_000)
            .map(|chunk| hex::encode(Sha256::digest(chunk)))
            .collect();
        if chunks[role]["store_canister_id"] != context["governance"]
            || chunks[role]["raw_wasm_sha256"] != modules[role]["raw_wasm_sha256"]
            || chunks[role]["chunk_hashes"] != json!(hashes)
        {
            return Err(
                "preuploaded chunks differ from the declared install bytes or authority plan"
                    .into(),
            );
        }
    }
    Ok(())
}
fn decode<T: CandidType + for<'de> Deserialize<'de>>(dir: &Path, name: &str) -> Result<T, String> {
    candid::decode_one(&fs::read(dir.join(name)).map_err(|e| format!("{name}: {e}"))?)
        .map_err(|e| format!("{name}: {e}"))
}
pub(super) fn finalization(dir: &Path, b: &Value) -> Result<(), String> {
    let raw: Finalization = decode(dir, "finalization.bin")?;
    if raw.error_message.is_some() {
        return Err("raw finalization returned an error".into());
    }
    for (name, result) in [
        ("sweep_icp", raw.sweep_icp_result),
        ("create_recipes", raw.create_sns_neuron_recipes_result),
        ("sweep_sns", raw.sweep_sns_result),
        ("claim_neurons", raw.claim_neuron_result),
    ] {
        let r = result.ok_or_else(|| format!("raw finalization missing {name}"))?;
        if b["finalization"][name]
            != json!({"success":r.success,"failure":r.failure,"skipped":r.skipped,"invalid":r.invalid,"global_failures":r.global_failures})
        {
            return Err(format!(
                "{name} summary differs from raw finalization reply"
            ));
        }
    }
    if !matches!(
        raw.set_mode_call_result.and_then(|r| r.possibility),
        Some(CallPossibility::Ok(_))
    ) || decode::<Mode>(dir, "governance-mode.bin")?.mode != Some(1)
    {
        return Err("raw Governance mode transition failed".into());
    }
    let Some(CallPossibility::Ok(root)) = raw
        .set_dapp_controllers_call_result
        .and_then(|r| r.possibility)
    else {
        return Err("raw Root finalization failed".into());
    };
    if !root.failed_updates.is_empty() {
        return Err("raw Root controller updates failed".into());
    }
    let Some(CallPossibility::Ok(fund)) = raw
        .settle_neurons_fund_participation_result
        .and_then(|r| r.possibility)
    else {
        return Err("raw Fund-disabled settlement failed".into());
    };
    if fund.neurons_fund_participation_icp_e8s != Some(0)
        || fund.neurons_fund_neurons_count != Some(0)
    {
        return Err("unexpected Fund participation".into());
    }
    Ok(())
}
#[derive(CandidType, Deserialize)]
struct NeuronId {
    id: u64,
}
#[derive(CandidType, Deserialize)]
struct CreatePermanent {
    new_controller: Option<Principal>,
    dissolve_delay_seconds: u64,
    amount_e8s: u64,
    nonce: u64,
}
#[derive(CandidType, Deserialize)]
struct NnsResponse {
    command: Option<NnsCommand>,
}
#[derive(CandidType, Deserialize)]
enum NnsCommand {
    DisburseToNeuron(Created),
    ClaimOrRefresh(Refreshed),
    Error(Empty),
}
#[derive(CandidType, Deserialize)]
struct Created {
    created_neuron_id: Option<NeuronId>,
}
#[derive(CandidType, Deserialize)]
struct Refreshed {
    refreshed_neuron_id: Option<NeuronId>,
}
#[derive(CandidType, Deserialize)]
struct NeuronInfo {
    stake_e8s: u64,
    dissolve_delay_seconds: u64,
    state: i32,
}

fn validate_existing_neuron(
    neuron: &io_governance_types::NnsNeuronRecord,
    manager: Principal,
) -> Result<(), String> {
    use io_governance_types::NnsDissolveStateRecord;
    if neuron.id.as_ref().map(|id| id.id) != Some(ACCEPTANCE_NEURON_ID)
        || neuron.controller != Some(manager)
        || neuron.account.len() != 32
        || neuron.cached_neuron_stake_e8s == 0
        || neuron.neuron_fees_e8s != 0
        || neuron.auto_stake_maturity.unwrap_or(false)
        || neuron.dissolve_state
            != Some(NnsDissolveStateRecord::DissolveDelaySeconds(
                PERMANENT_DELAY_SECONDS,
            ))
        || neuron.spawn_at_timestamp_seconds.is_some()
        || neuron.maturity_disbursements_in_progress.as_deref() != Some(&[])
    {
        return Err(
            "existing permanent identity, principal, policy or pending disbursement mismatch"
                .into(),
        );
    }
    Ok(())
}

pub(super) fn existing_permanent_baseline(
    dir: &Path,
    context: &Value,
) -> Result<io_governance_types::NnsNeuronRecord, String> {
    let neuron = decode::<
        Result<io_governance_types::NnsNeuronRecord, io_governance_types::NnsGovernanceErrorRecord>,
    >(dir, "permanent-full-neuron-before.bin")?
    .map_err(|e| format!("permanent baseline observation failed: {e:?}"))?;
    validate_existing_neuron(
        &neuron,
        principal(
            context["nns_manager"].as_str().ok_or("manager missing")?,
            "manager",
        )?,
    )?;
    Ok(neuron)
}

fn existing_permanent(dir: &Path, context: &Value, c: &Value) -> Result<(), String> {
    let before = existing_permanent_baseline(dir, context)?;
    let after = decode::<
        Result<io_governance_types::NnsNeuronRecord, io_governance_types::NnsGovernanceErrorRecord>,
    >(dir, "permanent-full-neuron-after.bin")?
    .map_err(|e| format!("permanent refreshed observation failed: {e:?}"))?;
    validate_existing_neuron(&after, before.controller.ok_or("controller missing")?)?;
    if before.account != after.account
        || before.controller != after.controller
        || before.dissolve_state != after.dissolve_state
        || before.hot_keys != after.hot_keys
        || before.followees != after.followees
        || before.auto_stake_maturity != after.auto_stake_maturity
        || c["permanent_neuron_id"] != ACCEPTANCE_NEURON_ID
        || c["permanent_staking_owner"] != NNS_GOVERNANCE_PRINCIPAL
        || c["permanent_staking_subaccount_hex"] != hex::encode(&before.account)
        || c["permanent_principal_before_top_up_e8s"] != before.cached_neuron_stake_e8s
        || c["permanent_cached_stake_after_refresh_e8s"] != after.cached_neuron_stake_e8s
    {
        return Err("existing permanent refresh changed identity/configuration or disagrees with canonical stake".into());
    }
    let Some(NnsCommand::ClaimOrRefresh(refreshed)) =
        decode::<NnsResponse>(dir, "permanent-refresh-response.bin")?.command
    else {
        return Err("existing permanent refresh did not succeed".into());
    };
    if refreshed.refreshed_neuron_id.map(|id| id.id) != Some(ACCEPTANCE_NEURON_ID) {
        return Err("existing permanent refresh returned wrong neuron ID".into());
    }
    Ok(())
}

pub(super) fn permanent(dir: &Path, context: &Value, c: &Value) -> Result<(), String> {
    if context["local_fixture"] == false {
        return existing_permanent(dir, context, c);
    }
    let request: CreatePermanent = decode(dir, "permanent-creation-request.bin")?;
    let manager = principal(
        context["nns_manager"].as_str().ok_or("manager missing")?,
        "manager",
    )?;
    if request.new_controller != Some(manager)
        || request.dissolve_delay_seconds != PERMANENT_DELAY_SECONDS
    {
        return Err("permanent creation authority or delay mismatch".into());
    }
    let mut hash = Sha256::new();
    hash.update([12]);
    hash.update(b"neuron-split");
    hash.update(manager.as_slice());
    hash.update(request.nonce.to_be_bytes());
    if c["permanent_staking_owner"] != NNS_GOVERNANCE_PRINCIPAL
        || c["permanent_staking_subaccount_hex"] != hex::encode(hash.finalize())
    {
        return Err("permanent staking Account differs from actual creation request".into());
    }
    let Some(NnsCommand::DisburseToNeuron(created)) =
        decode::<NnsResponse>(dir, "permanent-creation-response.bin")?.command
    else {
        return Err("permanent creation did not succeed".into());
    };
    let id = created
        .created_neuron_id
        .ok_or("created neuron ID missing")?
        .id;
    if c["permanent_neuron_id"] != id {
        return Err("permanent ID differs from actual creation reply".into());
    }
    let Some(NnsCommand::ClaimOrRefresh(refreshed)) =
        decode::<NnsResponse>(dir, "permanent-refresh-response.bin")?.command
    else {
        return Err("permanent refresh did not succeed".into());
    };
    if refreshed
        .refreshed_neuron_id
        .ok_or("refreshed neuron ID missing")?
        .id
        != id
    {
        return Err("refreshed wrong permanent neuron".into());
    }
    let before: NeuronInfo = decode(dir, "permanent-info-before.bin")?;
    let after: NeuronInfo = decode(dir, "permanent-info-after.bin")?;
    if before.state != 1
        || after.state != 1
        || before.dissolve_delay_seconds != PERMANENT_DELAY_SECONDS
        || after.dissolve_delay_seconds != PERMANENT_DELAY_SECONDS
        || request.amount_e8s.checked_sub(ICP_TRANSFER_FEE_E8S as u64) != Some(before.stake_e8s)
        || c["permanent_principal_before_top_up_e8s"] != before.stake_e8s
        || c["permanent_cached_stake_after_refresh_e8s"] != after.stake_e8s
    {
        return Err("permanent summaries differ from actual public neuron observations".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use io_governance_types::{NnsDissolveStateRecord, NnsNeuronIdRecord, NnsNeuronRecord};

    #[test]
    fn existing_permanent_component_rejects_identity_and_policy_mutations() {
        // Typed arithmetic/component fixture, not a real NNS observation.
        let manager = Principal::from_text("gosbn-sqaaa-aaaas-amxua-cai").unwrap();
        let baseline = NnsNeuronRecord {
            id: Some(NnsNeuronIdRecord {
                id: ACCEPTANCE_NEURON_ID,
            }),
            controller: Some(manager),
            account: vec![7; 32],
            cached_neuron_stake_e8s: 100_000_000,
            dissolve_state: Some(NnsDissolveStateRecord::DissolveDelaySeconds(
                PERMANENT_DELAY_SECONDS,
            )),
            auto_stake_maturity: Some(false),
            neuron_fees_e8s: 0,
            staked_maturity_e8s_equivalent: None,
            recent_ballots: vec![],
            kyc_verified: true,
            neuron_type: None,
            not_for_profit: false,
            maturity_e8s_equivalent: 0,
            created_timestamp_seconds: 1,
            aging_since_timestamp_seconds: 1,
            hot_keys: vec![],
            joined_community_fund_timestamp_seconds: None,
            followees: vec![],
            visibility: None,
            transfer: None,
            known_neuron_data: None,
            spawn_at_timestamp_seconds: None,
            voting_power_refreshed_timestamp_seconds: None,
            deciding_voting_power: None,
            potential_voting_power: None,
            eight_year_gang_bonus_base_e8s: None,
            maturity_disbursements_in_progress: Some(vec![]),
        };
        validate_existing_neuron(&baseline, manager).unwrap();
        let variants: Vec<fn(&mut NnsNeuronRecord)> = vec![
            |n| n.controller = Some(Principal::anonymous()),
            |n| {
                n.id = Some(NnsNeuronIdRecord {
                    id: PROTECTED_IO_NNS_NEURON_ID,
                })
            },
            |n| {
                n.account.pop();
            },
            |n| n.cached_neuron_stake_e8s = 0,
            |n| n.neuron_fees_e8s = 1,
            |n| n.auto_stake_maturity = Some(true),
            |n| {
                n.dissolve_state = Some(NnsDissolveStateRecord::DissolveDelaySeconds(
                    PERMANENT_DELAY_SECONDS - 1,
                ))
            },
            |n| n.maturity_disbursements_in_progress = None,
        ];
        for mutate in variants {
            let mut bad = baseline.clone();
            mutate(&mut bad);
            assert!(validate_existing_neuron(&bad, manager).is_err());
        }
        // Informational maturity balances are neither required capital nor backing.
        let mut maturity = baseline;
        maturity.maturity_e8s_equivalent = 123;
        maturity.staked_maturity_e8s_equivalent = Some(456);
        validate_existing_neuron(&maturity, manager).unwrap();
    }
}
