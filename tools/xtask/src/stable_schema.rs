use super::*;

pub(super) fn check_stable_storage_at(root: &Path) -> Result<(), String> {
    check_did_surface_at(root, false)?;
    check_production_wiring_at(root)?;
    check_historian_freshness_at(root)?;
    check_exact_two_week_policy_at(root)?;

    if STABLE_SCHEMA_REGISTRY.len() != 3 {
        return Err(
            "stable schema registry must contain exactly the three IO canisters".to_string(),
        );
    }
    for required in ["io_stream_manager", "io_nns_neuron_manager", "io_historian"] {
        let entry = STABLE_SCHEMA_REGISTRY
            .iter()
            .find(|entry| entry.canister_name == required)
            .ok_or_else(|| format!("stable schema registry missing {required}"))?;
        if entry.current_version == 0 {
            return Err(format!(
                "{required}: current stable schema version must be nonzero"
            ));
        }
        if !accepts_schema_version(entry, entry.current_version) {
            return Err(format!(
                "{required}: current stable schema version must be accepted"
            ));
        }
        if accepts_schema_version(entry, entry.current_version + 1) {
            return Err(format!(
                "{required}: future stable schema version must reject"
            ));
        }
        if entry.fixture_files.is_empty() {
            return Err(format!("{required}: fixture list must be nonempty"));
        }
        for fixture in entry.fixture_files {
            let text = require_file(root, fixture)?;
            if !fixture.ends_with("corrupt.fixture") {
                require_present(
                    fixture,
                    &text,
                    &["canister=", "schema_version=", "live_snapshot=false"],
                )?;
                let schema_version = fixture_schema_version(fixture, &text)?;
                if fixture.ends_with("current.fixture")
                    || fixture.ends_with("bounded-history-near-limit.fixture")
                    || fixture.ends_with("launch-v1.fixture")
                {
                    if schema_version != entry.current_version {
                        return Err(format!(
                            "{fixture}: schema_version {schema_version} must match registry current {}",
                            entry.current_version
                        ));
                    }
                } else if fixture.ends_with("future-version.fixture")
                    && schema_version <= entry.current_version
                {
                    return Err(format!(
                        "{fixture}: future fixture schema_version {schema_version} must be greater than registry current {}",
                        entry.current_version
                    ));
                }
            }
        }
        if entry.current_version != 1 {
            return Err(format!(
                "{} must register launch V1 only",
                entry.canister_name
            ));
        }
        if matches!(
            entry.canister_name,
            "io_stream_manager" | "io_nns_neuron_manager"
        ) && (entry.fixture_files.len() != 1
            || !entry.fixture_files[0].ends_with("launch-v1.fixture"))
        {
            return Err(format!(
                "{} must have one launch V1 fixture",
                entry.canister_name
            ));
        }
        if entry.canister_name == "io_historian" && entry.fixture_files.len() != 4 {
            return Err(
                "io_historian must retain current, bounded, corrupt and future fixtures".into(),
            );
        }
    }

    let stable_storage_doc = require_file(root, "docs/architecture/stable-storage.md")?;
    require_present(
        "docs/architecture/stable-storage.md",
        &stable_storage_doc,
        &[
            "io_stream_manager",
            "io_nns_neuron_manager",
            "only `V1",
            "strict launch V1",
            "No production canister",
        ],
    )?;
    let stream_source = require_file(root, "canisters/io_stream_manager/src/state.rs")?;
    let nns_source = require_file(root, "canisters/io_nns_neuron_manager/src/state.rs")?;
    require_present(
        "launch V1 stable envelopes",
        &format!("{stream_source}\n{nns_source}"),
        &[
            "enum StableStreamState",
            "enum StableNnsState",
            "invalid stable stream V1 state",
            "invalid stable NNS V1 state",
        ],
    )?;
    Ok(())
}
