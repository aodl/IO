//! Width-subtyped observation DTOs for the pinned public replies. Constructors
//! use the production types in constructors.rs; observations retain raw replies.
use super::*;
use candid::CandidType;

#[derive(CandidType, Deserialize)]
struct Empty {}
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
    root: Option<DappSummary>,
    governance: Option<DappSummary>,
    ledger: Option<DappSummary>,
    index: Option<DappSummary>,
    swap: Option<DappSummary>,
    archives: Vec<DappSummary>,
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
        if chunks[role]["store_canister_id"] != context[role]
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
// Standard upstream Version wire record; no IO version schema or registry.
#[derive(CandidType, Deserialize, serde::Serialize, PartialEq, Eq)]
struct FrameworkVersion {
    root_wasm_hash: Vec<u8>,
    governance_wasm_hash: Vec<u8>,
    ledger_wasm_hash: Vec<u8>,
    index_wasm_hash: Vec<u8>,
    swap_wasm_hash: Vec<u8>,
    archive_wasm_hash: Vec<u8>,
}
#[derive(CandidType, Deserialize)]
struct RunningVersion {
    deployed_version: Option<FrameworkVersion>,
    pending_version: Option<Empty>,
}
#[derive(CandidType, Deserialize)]
struct SnsNeuronId {
    id: Vec<u8>,
}
#[derive(CandidType, Deserialize, PartialEq, Eq)]
struct Permission {
    principal: Option<Principal>,
    permission_type: Vec<i32>,
}
#[derive(CandidType, Deserialize)]
enum SnsDissolveState {
    DissolveDelaySeconds(u64),
    WhenDissolvedTimestampSeconds(u64),
}
#[derive(CandidType, Deserialize)]
struct ObservedNeuron {
    id: Option<SnsNeuronId>,
    cached_neuron_stake_e8s: u64,
    permissions: Vec<Permission>,
    followees: Vec<(u64, Empty)>,
    topic_followees: Option<Empty>,
    dissolve_state: Option<SnsDissolveState>,
    voting_power_percentage_multiplier: u64,
}
#[derive(CandidType, Deserialize)]
struct Neurons {
    neurons: Vec<ObservedNeuron>,
}
#[derive(CandidType, Deserialize)]
struct DefaultFollowees {
    followees: Vec<(u64, Empty)>,
}
#[derive(CandidType, Deserialize)]
struct GenesisParameters {
    default_followees: Option<DefaultFollowees>,
    automatically_advance_target_version: Option<bool>,
    max_number_of_neurons: Option<u64>,
}
#[derive(CandidType, Deserialize, serde::Serialize)]
struct Basket {
    count: u64,
    dissolve_delay_interval_seconds: u64,
}
#[derive(CandidType, Deserialize, serde::Serialize)]
struct FrozenSwapInit {
    sns_token_e8s: Option<u64>,
    should_auto_finalize: Option<bool>,
    swap_start_timestamp_seconds: Option<u64>,
    swap_due_timestamp_seconds: Option<u64>,
    nns_proposal_id: Option<u64>,
    min_participants: Option<u32>,
    min_direct_participation_icp_e8s: Option<u64>,
    max_direct_participation_icp_e8s: Option<u64>,
    min_participant_icp_e8s: Option<u64>,
    max_participant_icp_e8s: Option<u64>,
    neuron_basket_construction_parameters: Option<Basket>,
    neurons_fund_participation: Option<bool>,
}
#[derive(CandidType, Deserialize)]
struct ObservedSwap {
    init: Option<FrozenSwapInit>,
    auto_finalize_swap_response: Option<Empty>,
    already_tried_to_auto_finalize: Option<bool>,
    finalize_swap_in_progress: Option<bool>,
}
#[derive(CandidType, Deserialize)]
struct SwapState {
    swap: Option<ObservedSwap>,
}
#[derive(CandidType, Deserialize)]
struct Lifecycle {
    lifecycle: Option<i32>,
    decentralization_sale_open_timestamp_seconds: Option<u64>,
    decentralization_swap_termination_timestamp_seconds: Option<u64>,
}
#[derive(CandidType, Deserialize)]
struct Derived {
    sns_tokens_per_icp: Option<f64>,
    buyer_total_icp_e8s: Option<u64>,
    direct_participant_count: Option<u64>,
    cf_participant_count: Option<u64>,
    cf_neuron_count: Option<u64>,
    direct_participation_icp_e8s: Option<u64>,
    neurons_fund_participation_icp_e8s: Option<u64>,
}
#[derive(CandidType, Deserialize)]
struct Buyers {
    buyers_total: u64,
}
#[derive(CandidType, Deserialize)]
struct Recipes {
    sns_neuron_recipes: Vec<Empty>,
}
#[derive(CandidType, Deserialize)]
struct Timers {
    requires_periodic_tasks: Option<bool>,
    last_spawned_timestamp_seconds: Option<u64>,
}
#[derive(CandidType, Deserialize)]
struct TimersReply {
    timers: Option<Timers>,
}

pub(super) fn genesis(dir: &Path, context: &Value, b: &Value) -> Result<Value, String> {
    if decode::<Mode>(dir, "governance-mode.bin")?.mode != Some(1) {
        return Err("Governance is not Normal".into());
    }
    let p: GenesisParameters = decode(dir, "genesis-governance-parameters.bin")?;
    if p.automatically_advance_target_version != Some(false)
        || p.max_number_of_neurons != Some(1000)
        || p.default_followees.is_none_or(|d| !d.followees.is_empty())
    {
        return Err("genesis Governance parameters or default following mismatch".into());
    }
    for method in ["name", "symbol"] {
        if decode::<String>(dir, &format!("ledger-{method}.bin"))? != "IO" {
            return Err("Ledger identity must be IO / IO".into());
        }
    }
    let balance = |name: &str| -> Result<u128, String> {
        let n: candid::Nat = decode(dir, &format!("{name}.bin"))?;
        n.0.try_into().map_err(|_| "Ledger amount overflow".into())
    };
    for (name, amount) in [
        ("genesis-total-supply", TOTAL_IO_E8S),
        ("genesis-operator-balance", GOVERNANCE_IO_E8S),
        ("genesis-jupiter-balance", JUPITER_GOVERNANCE_IO_E8S),
        ("genesis-reserve-balance", reserve_stake()?),
        ("genesis-swap-balance", 0),
        ("genesis-treasury-balance", 0),
        ("genesis-minting-balance", 0),
        ("genesis-future-swap-balance", 0),
    ] {
        if balance(name)? != amount {
            return Err(format!("canonical genesis balance mismatch: {name}"));
        }
    }
    for (name, key) in [
        ("b-total-supply", "total_supply_e8s"),
        ("b-stream-reserve", "stream_reserve_e8s"),
        ("b-reserve-neuron", "excluded_reserve_e8s"),
        ("b-jupiter-neuron", "excluded_jupiter_e8s"),
        ("b-operator-neuron", "operator_stake_e8s"),
    ] {
        if balance(name)? != value_u128(&b[key], key)? {
            return Err(format!(
                "B summary differs from raw Ledger observation: {key}"
            ));
        }
    }
    let root = principal(context["root"].as_str().ok_or("Root missing")?, "Root")?;
    let governance = principal(
        context["governance"].as_str().ok_or("Governance missing")?,
        "Governance",
    )?;
    let operator = principal(
        context["operator"].as_str().ok_or("operator missing")?,
        "operator",
    )?;
    let jupiter = principal(
        context["jupiter"].as_str().ok_or("Jupiter missing")?,
        "Jupiter",
    )?;
    let neurons: Neurons = decode(dir, "genesis-neurons.bin")?;
    if neurons.neurons.len() != 3 {
        return Err("exactly three genesis neurons required".into());
    }
    for (owner, memo, stake, delay, rights) in [
        (
            operator,
            GOVERNANCE_MEMO,
            GOVERNANCE_IO_E8S,
            1_296_060,
            (1..=10).collect::<Vec<i32>>(),
        ),
        (
            jupiter,
            JUPITER_GOVERNANCE_MEMO,
            JUPITER_GOVERNANCE_IO_E8S,
            1_296_060,
            vec![4],
        ),
        (
            operator,
            RESERVE_MEMO,
            reserve_stake()?,
            0,
            (1..=10).collect(),
        ),
    ] {
        let id = sns_neuron_subaccount(owner, memo);
        let selected = neurons
            .neurons
            .iter()
            .filter(|n| n.id.as_ref().is_some_and(|n| n.id == id))
            .collect::<Vec<_>>();
        if selected.len() != 1 {
            return Err("missing or duplicate genesis neuron".into());
        }
        let n = selected[0];
        if u128::from(n.cached_neuron_stake_e8s) != stake
            || n.permissions
                != vec![Permission {
                    principal: Some(owner),
                    permission_type: rights,
                }]
            || !n.followees.is_empty()
            || n.topic_followees.is_some()
            || n.voting_power_percentage_multiplier != 100
            || !matches!(n.dissolve_state,Some(SnsDissolveState::DissolveDelaySeconds(d)) if d==delay)
        {
            return Err("genesis neuron stake, permissions or following mismatch".into());
        }
    }
    let running: RunningVersion = decode(dir, "genesis-running-version.bin")?;
    if running.pending_version.is_some() {
        return Err("pending framework upgrade at launch".into());
    }
    let version = running.deployed_version.ok_or("deployed version absent")?;
    let manifest = approved_sns_manifest();
    let hash = |component: &str| -> Result<Vec<u8>, String> {
        hex::decode(
            manifest
                .source_sha256(&format!("sns_{component}"))
                .ok_or("official hash missing")?,
        )
        .map_err(|e| e.to_string())
    };
    let expected = FrameworkVersion {
        governance_wasm_hash: hash("governance")?,
        root_wasm_hash: hash("root")?,
        ledger_wasm_hash: hash("ledger")?,
        index_wasm_hash: hash("index")?,
        swap_wasm_hash: hash("swap")?,
        archive_wasm_hash: hash("archive")?,
    };
    if version != expected {
        return Err("deployed version differs from the reviewed official tuple".into());
    }
    let summary: RootSummary = decode(dir, "genesis-root-summary.bin")?;
    if !summary.archives.is_empty() {
        return Err("unexpected running Archive at genesis".into());
    }
    for (name, record, hash, controller) in [
        ("root", summary.root, version.root_wasm_hash, governance),
        (
            "governance",
            summary.governance,
            version.governance_wasm_hash,
            root,
        ),
        ("ledger", summary.ledger, version.ledger_wasm_hash, root),
        ("index", summary.index, version.index_wasm_hash, root),
        ("swap", summary.swap, version.swap_wasm_hash, root),
    ] {
        let record = record.ok_or("framework Root summary absent")?;
        let status = record.status.ok_or("framework status absent")?;
        if record.canister_id
            != Some(principal(
                context[name].as_str().ok_or("framework ID missing")?,
                name,
            )?)
            || status.module_hash != Some(hash)
            || status.settings.controllers != vec![controller]
        {
            return Err(format!(
                "framework physical version/controller mismatch: {name}"
            ));
        }
    }
    let expected_dapps = ["stream", "nns_manager", "historian", "frontend"]
        .iter()
        .map(|key| principal(context[*key].as_str().ok_or("dapp missing")?, key))
        .collect::<Result<BTreeSet<_>, _>>()?;
    if summary.dapps.len() != 4
        || summary.dapps.iter().any(|d| {
            d.status
                .as_ref()
                .is_none_or(|s| s.module_hash.is_some() || s.settings.controllers != vec![root])
        })
        || summary
            .dapps
            .iter()
            .filter_map(|d| d.canister_id)
            .collect::<BTreeSet<_>>()
            != expected_dapps
    {
        return Err("Root initial registration or sole installation authority mismatch".into());
    }
    parked_swap(dir, &balance)
}

fn parked_swap(
    dir: &Path,
    balance: &impl Fn(&str) -> Result<u128, String>,
) -> Result<Value, String> {
    let mut observed = serde_json::Map::new();
    for phase in ["before", "after"] {
        let state: SwapState = decode(dir, &format!("swap-state-{phase}.bin"))?;
        let state = state.swap.ok_or("Swap state absent")?;
        let init = state.init.ok_or("Swap Init absent")?;
        if serde_json::to_value(init).map_err(|e| e.to_string())?
            != parked_swap_contract()["parked_swap"]
            || state.auto_finalize_swap_response.is_some()
            || state.already_tried_to_auto_finalize != Some(false)
            || state.finalize_swap_in_progress != Some(false)
        {
            return Err("parked Swap constructor/finalization invariant changed".into());
        }
        let lifecycle: Lifecycle = decode(dir, &format!("swap-lifecycle-{phase}.bin"))?;
        if lifecycle.lifecycle != Some(5)
            || lifecycle.decentralization_sale_open_timestamp_seconds != Some(u64::MAX)
            || lifecycle
                .decentralization_swap_termination_timestamp_seconds
                .is_some()
        {
            return Err("parked Swap transitioned or opening time changed".into());
        }
        let derived: Derived = decode(dir, &format!("swap-derived-{phase}.bin"))?;
        if derived.sns_tokens_per_icp != Some(0.0)
            || [
                derived.buyer_total_icp_e8s,
                derived.direct_participant_count,
                derived.cf_participant_count,
                derived.cf_neuron_count,
                derived.direct_participation_icp_e8s,
                derived.neurons_fund_participation_icp_e8s,
            ]
            .iter()
            .any(|v| *v != Some(0))
        {
            return Err("unexpected Swap participation or price".into());
        }
        // get_state deliberately elides buyers and recipes; use their real endpoints.
        if decode::<Buyers>(dir, &format!("swap-buyers-{phase}.bin"))?.buyers_total != 0
            || !decode::<Recipes>(dir, &format!("swap-recipes-{phase}.bin"))?
                .sns_neuron_recipes
                .is_empty()
        {
            return Err("unexpected buyer or neuron recipe".into());
        }
        let timers: TimersReply = decode(dir, &format!("swap-timers-{phase}.bin"))?;
        let timers = timers.timers.ok_or("Swap timers absent")?;
        if timers.requires_periodic_tasks != Some(true)
            || (phase == "after" && timers.last_spawned_timestamp_seconds.is_none())
        {
            return Err("Swap periodic task observation missing".into());
        }
        // These default-Account observations do not inventory Swap subaccounts.
        // ICP is quarantined, never funding/backing. IO remains in total supply
        // and is not an exclusion from claim supply, regardless of its owner.
        let io = balance(&format!("swap-io-{phase}"))?;
        let icp = decode::<io_ledger_types::IcpTokens>(dir, &format!("swap-icp-{phase}.bin"))?.e8s;
        observed.insert(
            phase.into(),
            json!({"quarantined_icp_e8s":icp,"claim_bearing_io_e8s":io}),
        );
    }
    Ok(Value::Object(observed))
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

fn full_neuron(
    dir: &Path,
    context: &Value,
    phase: &str,
) -> Result<io_governance_types::NnsNeuronRecord, String> {
    match context["permanent_observation_route"].as_str() {
        Some("public-list-neurons") => {
            let request: io_governance_types::NnsProductionListNeuronsRequest =
                decode(dir, &format!("permanent-list-neurons-{phase}-request.bin"))?;
            if request.neuron_ids != vec![ACCEPTANCE_NEURON_ID]
                || request.include_public_neurons_in_full_neurons != Some(true)
                || request.include_neurons_readable_by_caller
            {
                return Err("public route requires the exact requested ID and include_public_neurons_in_full_neurons=Some(true)".into());
            }
            let denied = decode::<
                Result<
                    io_governance_types::NnsNeuronRecord,
                    io_governance_types::NnsGovernanceErrorRecord,
                >,
            >(
                dir,
                &format!("permanent-get-full-neuron-{phase}-unauthorized.bin"),
            )?;
            if !matches!(denied,Err(error) if error.error_type==3) {
                return Err("public observer must remain unauthorized for get_full_neuron".into());
            }
            let response: io_governance_types::NnsProductionListNeuronsResponse =
                decode(dir, &format!("permanent-list-neurons-{phase}.bin"))?;
            let mut selected = response
                .full_neurons
                .into_iter()
                .filter(|neuron| neuron.id.as_ref().map(|n| n.id) == Some(ACCEPTANCE_NEURON_ID));
            let neuron = selected
                .next()
                .ok_or("requested Public permanent neuron missing")?;
            if selected.next().is_some() || neuron.visibility != Some(2) {
                return Err("expected exactly one matching Public full-neuron record".into());
            }
            let observer = principal(
                context["permanent_observer"]
                    .as_str()
                    .ok_or("public observer principal missing")?,
                "public observer",
            )?;
            if neuron.controller == Some(observer) || neuron.hot_keys.contains(&observer) {
                return Err("public observer has controller or hotkey privileges".into());
            }
            Ok(neuron)
        }
        Some("authorized-get-full-neuron") => {
            decode::<
                Result<
                    io_governance_types::NnsNeuronRecord,
                    io_governance_types::NnsGovernanceErrorRecord,
                >,
            >(dir, &format!("permanent-full-neuron-{phase}.bin"))?
            .map_err(|e| format!("authorized full-neuron observation failed: {e:?}"))
        }
        _ => {
            Err("explicit existing public/authorized full-neuron observation route required".into())
        }
    }
}

pub(super) fn permanent_baseline(dir: &Path, context: &Value) -> Result<(), String> {
    if context["local_fixture"] != true {
        existing_permanent_baseline(dir, context)?;
        return Ok(());
    }
    let request: CreatePermanent = decode(dir, "permanent-creation-request.bin")?;
    let Some(NnsCommand::DisburseToNeuron(created)) =
        decode::<NnsResponse>(dir, "permanent-creation-response.bin")?.command
    else {
        return Err("fresh local permanent creation failed".into());
    };
    let id = created
        .created_neuron_id
        .ok_or("created permanent ID absent")?
        .id;
    let info: NeuronInfo = decode(dir, "permanent-info-before.bin")?;
    if request.new_controller
        != Some(principal(
            context["nns_manager"].as_str().ok_or("manager missing")?,
            "manager",
        )?)
        || request.dissolve_delay_seconds != PERMANENT_DELAY_SECONDS
        || info.dissolve_delay_seconds != PERMANENT_DELAY_SECONDS
        || info.state != 1
        || info.stake_e8s == 0
        || request.amount_e8s.checked_sub(ICP_TRANSFER_FEE_E8S as u64) != Some(info.stake_e8s)
        || context["local_permanent_neuron_id"] != id
    {
        return Err(
            "fresh local permanent baseline differs from real NNS construction and public info"
                .into(),
        );
    }
    Ok(())
}

pub(super) fn existing_permanent_baseline(
    dir: &Path,
    context: &Value,
) -> Result<io_governance_types::NnsNeuronRecord, String> {
    let neuron = full_neuron(dir, context, "before")?;
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
    let after = full_neuron(dir, context, "after")?;
    validate_existing_neuron(&after, before.controller.ok_or("controller missing")?)?;
    // Both authentic records retain informational maturity. Ordinary NNS reward
    // accrual is not a principal top-up and cannot enter audited capital.
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
        for mutate in &variants {
            let mut bad = baseline.clone();
            mutate(&mut bad);
            assert!(validate_existing_neuron(&bad, manager).is_err());
        }
        // Component-only width-subtyping and selection coverage. Real public
        // NNS responses are retained by real_nns_existing_id_noncontroller_refresh.
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target")
            .join(format!("io-public-neuron-decoder-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let observer = Principal::from_slice(&[91; 29]);
        let context = json!({"permanent_observation_route":"public-list-neurons","permanent_observer":observer.to_text(),"nns_manager":manager.to_text()});
        let request = io_governance_types::NnsProductionListNeuronsRequest {
            neuron_ids: vec![ACCEPTANCE_NEURON_ID],
            include_neurons_readable_by_caller: false,
            include_empty_neurons_readable_by_caller: Some(false),
            include_public_neurons_in_full_neurons: Some(true),
            page_number: None,
            page_size: None,
            neuron_subaccounts: None,
        };
        fs::write(
            dir.join("permanent-list-neurons-before-request.bin"),
            candid::encode_one(&request).unwrap(),
        )
        .unwrap();
        let denied: Result<NnsNeuronRecord, io_governance_types::NnsGovernanceErrorRecord> =
            Err(io_governance_types::NnsGovernanceErrorRecord {
                error_type: 3,
                error_message: "component-only unrelated observer".into(),
            });
        fs::write(
            dir.join("permanent-get-full-neuron-before-unauthorized.bin"),
            candid::encode_one(denied).unwrap(),
        )
        .unwrap();
        let mut public = baseline.clone();
        public.visibility = Some(2);
        let response = io_governance_types::NnsProductionListNeuronsResponse {
            neuron_infos: vec![],
            full_neurons: vec![public.clone()],
            total_pages_available: Some(1),
        };
        let path = dir.join("permanent-list-neurons-before.bin");
        fs::write(&path, candid::encode_one(&response).unwrap()).unwrap();
        assert_eq!(existing_permanent_baseline(&dir, &context).unwrap(), public);
        for records in [
            vec![],
            vec![public.clone(), public.clone()],
            vec![baseline.clone()],
        ] {
            let mut bad = response.clone();
            bad.full_neurons = records;
            fs::write(&path, candid::encode_one(&bad).unwrap()).unwrap();
            assert!(existing_permanent_baseline(&dir, &context).is_err());
        }
        for mutate in &variants {
            let mut bad = response.clone();
            mutate(&mut bad.full_neurons[0]);
            fs::write(&path, candid::encode_one(&bad).unwrap()).unwrap();
            assert!(existing_permanent_baseline(&dir, &context).is_err());
        }
        let mut bad = response.clone();
        bad.full_neurons[0].hot_keys.push(observer);
        fs::write(&path, candid::encode_one(bad).unwrap()).unwrap();
        assert!(existing_permanent_baseline(&dir, &context).is_err());
        fs::write(&path, candid::encode_one(response).unwrap()).unwrap();
        let mut bad_request = request;
        bad_request.include_public_neurons_in_full_neurons = None;
        fs::write(
            dir.join("permanent-list-neurons-before-request.bin"),
            candid::encode_one(bad_request).unwrap(),
        )
        .unwrap();
        assert!(existing_permanent_baseline(&dir, &context).is_err());
        fs::remove_dir_all(dir).unwrap();
        // Informational maturity balances are neither required capital nor backing.
        let mut maturity = baseline;
        maturity.maturity_e8s_equivalent = 123;
        maturity.staked_maturity_e8s_equivalent = Some(456);
        validate_existing_neuron(&maturity, manager).unwrap();
    }
}
