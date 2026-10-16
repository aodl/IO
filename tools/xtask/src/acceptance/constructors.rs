//! Production constructor types checked against the repository Candid records.
//! This avoids accepting a readable JSON draft as an installable constructor.
use super::*;
use candid::{
    types::{Type, TypeInner},
    CandidType,
};
use io_accounts::Account;
use std::path::PathBuf;

use io_nns_neuron_manager::NnsConfig;
use io_stream_manager::StreamConfig;

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
struct InitArgs<T> {
    config: T,
}

/// Check the production types against every field of the checked-in
/// Candid record. Unknown syntax/types and missing/extra fields fail closed.
fn check_record<T: CandidType>(did: &str, name: &str) -> Result<(), String> {
    let marker = format!("type {name} = record {{");
    let body = did
        .split_once(&marker)
        .ok_or_else(|| format!("missing {name} Candid record"))?
        .1
        .split_once('}')
        .ok_or("unterminated Candid record")?
        .0;
    let actual = T::_ty();
    let TypeInner::Record(fields) = actual.as_ref() else {
        return Err("DTO is not a record".into());
    };
    let mut expected = BTreeMap::new();
    for field in body.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        let (name, ty) = field.split_once(':').ok_or("malformed Candid field")?;
        let ty: Type = match ty.trim() {
            "principal" => Principal::_ty(),
            "nat" => u128::_ty(),
            "nat64" => u64::_ty(),
            "blob" => Vec::<u8>::_ty(),
            "opt blob" => Option::<Vec<u8>>::_ty(),
            "Account" => Account::_ty(),
            "vec Account" => Vec::<Account>::_ty(),
            "text" => String::_ty(),
            "CanisterRole" => io_historian::CanisterRole::_ty(),
            "vec NamedAccount" => Vec::<io_historian::NamedAccount>::_ty(),
            "vec ExpectedModule" => Vec::<io_historian::ExpectedModule>::_ty(),
            _ => return Err(format!("unsupported constructor Candid type: {ty}")),
        };
        if expected.insert(candid::idl_hash(name.trim()), ty).is_some() {
            return Err("duplicate Candid field".into());
        }
    }
    if fields.len() != expected.len()
        || fields
            .iter()
            .any(|f| expected.get(&f.id.get_id()) != Some(&f.ty))
    {
        return Err(format!("constructor DTO differs from checked-in {name}"));
    }
    Ok(())
}

fn check_unit_variant<T: CandidType>(did: &str, name: &str) -> Result<(), String> {
    let body = did
        .split_once(&format!("type {name} = variant {{"))
        .ok_or("missing Candid variant")?
        .1
        .split_once('}')
        .ok_or("unterminated Candid variant")?
        .0;
    let mut expected = BTreeSet::new();
    for label in body.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        if label.contains(':') || !expected.insert(candid::idl_hash(label)) {
            return Err("non-unit or duplicate Candid variant".into());
        }
    }
    let actual = T::_ty();
    let TypeInner::Variant(fields) = actual.as_ref() else {
        return Err("production type is not a variant".into());
    };
    if fields.len() != expected.len()
        || fields
            .iter()
            .any(|f| !expected.contains(&f.id.get_id()) || *f.ty.as_ref() != TypeInner::Null)
    {
        return Err(format!("production type differs from checked-in {name}"));
    }
    Ok(())
}

fn roundtrip<T>(config: T) -> Result<Vec<u8>, String>
where
    T: CandidType + for<'de> Deserialize<'de> + PartialEq + std::fmt::Debug,
{
    let expected = InitArgs { config };
    let bytes = candid::encode_one(&expected).map_err(|e| e.to_string())?;
    let decoded: InitArgs<T> = candid::decode_one(&bytes).map_err(|e| e.to_string())?;
    if decoded != expected {
        return Err("constructor decoded values differ from canonical model".into());
    }
    Ok(bytes)
}

pub(crate) fn payloads(
    root: &Path,
    context: &Value,
    c: &Value,
    governance_hash: Vec<u8>,
    modules: &Value,
) -> Result<BTreeMap<&'static str, Vec<u8>>, String> {
    let stream_did =
        fs::read_to_string(root.join("canisters/io_stream_manager/io_stream_manager.did"))
            .map_err(|e| e.to_string())?;
    let nns_did =
        fs::read_to_string(root.join("canisters/io_nns_neuron_manager/io_nns_neuron_manager.did"))
            .map_err(|e| e.to_string())?;
    check_record::<Account>(&stream_did, "Account")?;
    check_record::<Account>(&nns_did, "Account")?;
    check_record::<StreamConfig>(&stream_did, "StreamConfig")?;
    check_record::<NnsConfig>(&nns_did, "NnsConfig")?;
    for (did, name) in [(&stream_did, "StreamConfig"), (&nns_did, "NnsConfig")] {
        if !did.contains(&format!("type InitArgs = record {{ config : {name} }};"))
            || !did.contains("service : (InitArgs)")
        {
            return Err("production constructor wrapper changed".into());
        }
    }
    let id = |key: &str| {
        principal(
            context[key]
                .as_str()
                .ok_or_else(|| format!("missing {key}"))?,
            key,
        )
    };
    let stream = id("stream")?;
    let nns = id("nns_manager")?;
    let gov = id("governance")?;
    let jupiter = id("jupiter_source")?;
    let account = |owner, subaccount| Account { owner, subaccount };
    let liquid = account(stream, Some(STREAM_LIQUID_SUBACCOUNT.to_vec()));
    let reserve = account(stream, Some(STREAM_RESERVE_SUBACCOUNT.to_vec()));
    let exclusions = vec![
        account(
            gov,
            Some(sns_neuron_subaccount(id("operator")?, RESERVE_MEMO).to_vec()),
        ),
        account(
            gov,
            Some(sns_neuron_subaccount(id("jupiter")?, JUPITER_GOVERNANCE_MEMO).to_vec()),
        ),
    ];
    let permanent = c["permanent_neuron_id"]
        .as_u64()
        .filter(|n| *n != 0 && *n != PROTECTED_IO_NNS_NEURON_ID)
        .ok_or("invalid permanent neuron ID")?;
    let stream_config = StreamConfig {
        io_ledger: id("ledger")?,
        io_index: id("index")?,
        icp_ledger: principal(ICP_LEDGER_PRINCIPAL, "ICP")?,
        nns_manager: nns,
        jupiter_io_account: account(jupiter, None),
        sns_governance: gov,
        sns_root: id("root")?,
        expected_sns_governance_module_hash: governance_hash.clone(),
        approved_reward_event_duration_seconds: 86_400,
        io_reserve: reserve,
        liquid_icp: liquid.clone(),
        nonredeemable_governance_io_accounts: exclusions,
        minimum_redemption_io_e8s: 20_000,
        expected_io_fee_e8s: IO_FEE_E8S,
        expected_icp_fee_e8s: ICP_TRANSFER_FEE_E8S,
        redemption_poll_interval_seconds: 60,
        retry_delay_nanos: 1_000_000_000,
        ledger_deduplication_window_nanos: 86_400_000_000_000,
    };
    let nns_config = NnsConfig {
        sns_governance: gov,
        stream_manager: stream,
        jupiter,
        icp_ledger: principal(ICP_LEDGER_PRINCIPAL, "ICP")?,
        nns_governance: principal(NNS_GOVERNANCE_PRINCIPAL, "NNS")?,
        two_year_neuron_id: permanent,
        pooled_parent_memo: 0,
        pooled_parent_followee_id: permanent,
        jupiter_account: account(jupiter, None),
        jupiter_staging: account(nns, None),
        stream_liquid_account: liquid,
        expected_io_fee_e8s: IO_FEE_E8S,
        expected_icp_fee_e8s: ICP_TRANSFER_FEE_E8S,
        jupiter_activation_block_floor: value_u128(
            &c["jupiter_activation_block_floor"],
            "activation floor",
        )?,
        audited_permanent_principal_e8s: value_u128(
            &c["permanent_cached_stake_after_refresh_e8s"],
            "audited principal",
        )?,
        transfer_retry_delay_nanos: 1_000_000_000,
        ledger_deduplication_window_nanos: 86_400_000_000_000,
    };
    stream_config.validate(stream)?;
    nns_config.validate(nns)?;
    let stream_bytes = roundtrip(stream_config.clone())?;
    let nns_bytes = roundtrip(nns_config.clone())?;
    // Decode as the actual install entry-point types, not only an equivalent wrapper.
    if candid::decode_one::<io_stream_manager::InitArgs>(&stream_bytes)
        .map_err(|e| e.to_string())?
        .config
        != stream_config
        || candid::decode_one::<io_nns_neuron_manager::InitArgs>(&nns_bytes)
            .map_err(|e| e.to_string())?
            .config
            != nns_config
    {
        return Err("production InitArgs roundtrip mismatch".into());
    }
    let historian_did = fs::read_to_string(root.join("canisters/io_historian/io_historian.did"))
        .map_err(|e| e.to_string())?;
    check_record::<io_ledger_types::Account>(&historian_did, "Account")?;
    check_record::<io_historian::NamedAccount>(&historian_did, "NamedAccount")?;
    check_unit_variant::<io_historian::CanisterRole>(&historian_did, "CanisterRole")?;
    check_record::<io_historian::ExpectedModule>(&historian_did, "ExpectedModule")?;
    check_record::<io_historian::ObservationConfig>(&historian_did, "ObservationConfig")?;
    if !historian_did.contains("service : (opt ObservationConfig)") {
        return Err("historian constructor changed".into());
    }
    let bundle = io_sns_manifest::SnsManifest::read(
        &PathBuf::from(context["bundle_path"].as_str().ok_or("bundle missing")?)
            .join("manifest.toml"),
    )?;
    let mut expected_modules = Vec::new();
    use io_historian::CanisterRole;
    for (name, role, component) in [
        ("stream", CanisterRole::StreamManager, None),
        ("nns_manager", CanisterRole::NnsManager, None),
        ("historian", CanisterRole::Historian, None),
        ("frontend", CanisterRole::Frontend, None),
        (
            "governance",
            CanisterRole::SnsGovernance,
            Some("sns_governance"),
        ),
        ("root", CanisterRole::SnsRoot, Some("sns_root")),
        ("ledger", CanisterRole::SnsLedger, Some("sns_ledger")),
        ("index", CanisterRole::SnsIndex, Some("sns_index")),
        ("swap", CanisterRole::SnsSwap, Some("sns_swap")),
    ] {
        let hash = match component {
            Some(component) => bundle.require_hash(component)?,
            None => modules[name]["raw_wasm_sha256"]
                .as_str()
                .ok_or("IO raw module hash missing")?,
        };
        expected_modules.push(io_historian::ExpectedModule {
            role,
            canister_id: id(name)?,
            wasm_sha256: hex::decode(hash).map_err(|e| e.to_string())?,
        });
    }
    let ledger_account = |account: &Account| -> Result<io_ledger_types::Account, String> {
        let canonical = account.canonical()?;
        Ok(io_ledger_types::Account::new(
            canonical.owner,
            Some(io_ledger_types::Subaccount(canonical.subaccount)),
        ))
    };
    let history = io_historian::ObservationConfig {
        stream_manager: stream,
        nns_manager: nns,
        sns_root: id("root")?,
        sns_governance: gov,
        sns_ledger: id("ledger")?,
        sns_index: id("index")?,
        icp_ledger: stream_config.icp_ledger,
        nns_governance: nns_config.nns_governance,
        two_year_neuron_id: permanent,
        protocol_io_reserve: ledger_account(&stream_config.io_reserve)?,
        liquid_icp_reserve: ledger_account(&stream_config.liquid_icp)?,
        nonredeemable_governance_io_accounts: stream_config
            .nonredeemable_governance_io_accounts
            .iter()
            .zip(["reserve-transport", "Jupiter-governance-only"])
            .map(|(a, name)| {
                Ok(io_historian::NamedAccount {
                    name: name.into(),
                    account: ledger_account(a)?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?,
        history_accounts: vec![],
        expected_modules,
        reward_share_capable_governance_sha256: Some(governance_hash),
        refresh_interval_seconds: 60,
    };
    io_historian::validate_config(&history, Some(id("historian")?))?;
    let historian_bytes = candid::encode_one(Some(&history)).map_err(|e| e.to_string())?;
    if candid::decode_one::<Option<io_historian::ObservationConfig>>(&historian_bytes)
        .map_err(|e| e.to_string())?
        != Some(history)
    {
        return Err("historian constructor roundtrip mismatch".into());
    }
    Ok(BTreeMap::from([
        ("stream", stream_bytes),
        ("nns_manager", nns_bytes),
        ("historian", historian_bytes),
        (
            "frontend",
            candid::encode_args(()).map_err(|e| e.to_string())?,
        ),
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn typed_constructor_records_match_production_interfaces() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let historian =
            fs::read_to_string(root.join("canisters/io_historian/io_historian.did")).unwrap();
        check_unit_variant::<io_historian::CanisterRole>(&historian, "CanisterRole").unwrap();
        assert!(check_unit_variant::<io_historian::CanisterRole>(
            &historian.replace("SnsGovernance;", "WrongGovernance;"),
            "CanisterRole"
        )
        .is_err());
        for (path, name, is_stream) in [
            (
                "canisters/io_stream_manager/io_stream_manager.did",
                "StreamConfig",
                true,
            ),
            (
                "canisters/io_nns_neuron_manager/io_nns_neuron_manager.did",
                "NnsConfig",
                false,
            ),
        ] {
            let did = fs::read_to_string(root.join(path)).unwrap();
            check_record::<Account>(&did, "Account").unwrap();
            if is_stream {
                check_record::<StreamConfig>(&did, name).unwrap();
                assert!(check_record::<StreamConfig>(
                    &did.replace("io_ledger : principal", "io_ledger : nat"),
                    name
                )
                .is_err());
            } else {
                check_record::<NnsConfig>(&did, name).unwrap();
                assert!(check_record::<NnsConfig>(
                    &did.replace("two_year_neuron_id : nat64", "two_year_neuron_id : nat"),
                    name
                )
                .is_err());
            }
        }
    }
}
