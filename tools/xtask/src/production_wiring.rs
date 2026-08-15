use super::*;

fn check_production_canister_ids_at(root: &Path) -> Result<(), String> {
    let text = require_file(root, PRODUCTION_CANISTER_IDS_PATH)?;
    require_toml_string(
        PRODUCTION_CANISTER_IDS_PATH,
        &text,
        "environment",
        "name",
        "Production",
    )?;
    require_toml_string(
        PRODUCTION_CANISTER_IDS_PATH,
        &text,
        "environment",
        "network",
        "ic",
    )?;
    require_toml_string(
        PRODUCTION_CANISTER_IDS_PATH,
        &text,
        "environment",
        "subnet_type",
        "fiduciary",
    )?;
    require_toml_string(
        PRODUCTION_CANISTER_IDS_PATH,
        &text,
        "environment",
        "status",
        "ReservedNotLive",
    )?;
    for key in [
        "io_protocol_live",
        "value_moving_logic_installed",
        "io_issuance_live",
        "io_redemption_live",
    ] {
        require_toml_bool(
            PRODUCTION_CANISTER_IDS_PATH,
            &text,
            "environment",
            key,
            false,
        )?;
    }
    for (key, expected) in [
        (
            "io_stream_manager",
            PRODUCTION_IO_STREAM_MANAGER_CANISTER_ID,
        ),
        ("io_historian", PRODUCTION_IO_HISTORIAN_CANISTER_ID),
        ("frontend", PRODUCTION_FRONTEND_CANISTER_ID),
    ] {
        require_toml_string(
            PRODUCTION_CANISTER_IDS_PATH,
            &text,
            "canisters",
            key,
            expected,
        )?;
    }
    require_present(
        PRODUCTION_CANISTER_IDS_PATH,
        &text,
        &[
            "reserved placeholders only",
            "not live protocol deployments",
        ],
    )?;
    require_absent(
        PRODUCTION_CANISTER_IDS_PATH,
        &text,
        &["io_nns_neuron_manager"],
    )
}

fn canonical_reserved_mapping() -> [(&'static str, &'static str); 3] {
    [
        (
            "io_stream_manager",
            PRODUCTION_IO_STREAM_MANAGER_CANISTER_ID,
        ),
        ("io_historian", PRODUCTION_IO_HISTORIAN_CANISTER_ID),
        ("frontend", PRODUCTION_FRONTEND_CANISTER_ID),
    ]
}

fn line_markdown_heading_canister(line: &str) -> Option<&'static str> {
    let heading = line.trim_start();
    if !heading.starts_with('#') {
        return None;
    }
    let title = heading.trim_start_matches('#').trim();
    canonical_reserved_mapping()
        .iter()
        .find_map(|(name, _)| (title == *name).then_some(*name))
}

fn check_production_mapping_text(path: &str, text: &str) -> Result<(), String> {
    let mapping = canonical_reserved_mapping();
    let mut required = Vec::with_capacity(mapping.len() * 2);
    for (name, id) in mapping {
        required.push(name);
        required.push(id);
    }
    require_present(path, text, &required)?;

    let mut markdown_section: Option<&'static str> = None;
    for (line_index, line) in text.lines().enumerate() {
        let line_no = line_index + 1;
        if let Some(name) = line_markdown_heading_canister(line) {
            markdown_section = Some(name);
        } else if line.trim_start().starts_with('#') {
            markdown_section = None;
        }

        if let Some(name) = markdown_section {
            let expected_id = mapping
                .iter()
                .find_map(|(candidate, id)| (*candidate == name).then_some(*id))
                .expect("known canister section");
            for (_, id) in mapping {
                if id != expected_id && line.contains(id) {
                    return Err(format!(
                        "{path}:{line_no}: section {name} must map to {expected_id}, not {id}"
                    ));
                }
            }
        }

        for (name, expected_id) in mapping {
            for (_, id) in mapping {
                if id == expected_id {
                    continue;
                }
                for pattern in [
                    format!("`{name}` `{id}`"),
                    format!("`{id}` (`{name}`)"),
                    format!("| `{name}` | `{id}` |"),
                    format!("{name} = \"{id}\""),
                    format!("{name} {id}"),
                ] {
                    if line.contains(&pattern) {
                        return Err(format!(
                            "{path}:{line_no}: {name} must map to {expected_id}, not {id}"
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}

fn check_production_mapping_docs_at(root: &Path) -> Result<(), String> {
    for path in PRODUCTION_MAPPING_PATHS {
        let text = require_file(root, path)?;
        check_production_mapping_text(path, &text)?;
        if *path != PRODUCTION_CANISTER_IDS_PATH {
            require_present(
                path,
                &text,
                &[
                    "io_nns_neuron_manager",
                    PRODUCTION_IO_NNS_NEURON_MANAGER_CANISTER_ID,
                ],
            )?;
        }
    }
    Ok(())
}

pub(super) fn check_production_wiring_at(root: &Path) -> Result<(), String> {
    for path in template_paths() {
        let text = require_file(root, path)?;
        validate_template_text(&text).map_err(|err| format!("{path}: {err}"))?;
        require_toml_string(path, &text, "environment", "status", "ReservedNotLive")?;
        for key in [
            "io_protocol_live",
            "value_moving_logic_installed",
            "io_issuance_live",
            "io_redemption_live",
        ] {
            require_toml_bool(path, &text, "environment", key, false)?;
        }
        require_toml_string(
            path,
            &text,
            "deployment_targets",
            "io_stream_manager",
            PRODUCTION_IO_STREAM_MANAGER_CANISTER_ID,
        )?;
        require_toml_string(
            path,
            &text,
            "deployment_targets",
            "io_nns_neuron_manager",
            PRODUCTION_IO_NNS_NEURON_MANAGER_CANISTER_ID,
        )?;
        require_absent(
            path,
            &text,
            &[
                "dfx",
                "--network ic",
                "icp canister install",
                "icp canister upgrade",
                "icp canister update-settings",
                "icp canister call",
            ],
        )?;
    }
    check_production_canister_ids_at(root)?;
    check_production_mapping_docs_at(root)?;

    let readme = require_file(root, "deploy/production-wiring/README.md")?;
    let operations = require_file(root, "docs/operations/production-wiring.md")?;
    let prelaunch = require_file(root, "docs/operations/prelaunch-config-validation.md")?;
    let combined = format!("{readme}\n{operations}\n{prelaunch}\n");
    require_present(
        "production wiring docs",
        &combined,
        &[
            "dry-run/config validation only",
            "No production execution is active",
            "IO protocol remains not live",
            "SNS IO ledger is not launched",
            "production activation is a later audited milestone",
            PROTECTED_IO_NEURON_OWNER_CANISTER,
            "10292412127977304661",
            "use `icp-cli` convention",
            "required workflows do not use `dfx`",
            "IO_TEST ledger is non-canonical",
            "planned wiring placeholders only",
            "ReservedNotLive",
            "reserved",
            "empty/inert",
            "not live",
            "no value-moving Wasm installed",
            "no production activation has happened",
            "no IO issuance/redemption is enabled",
            "Production Wiring Checklist",
            PRODUCTION_IO_STREAM_MANAGER_CANISTER_ID,
            PRODUCTION_IO_NNS_NEURON_MANAGER_CANISTER_ID,
            PRODUCTION_IO_HISTORIAN_CANISTER_ID,
            PRODUCTION_FRONTEND_CANISTER_ID,
        ],
    )?;
    require_absent(
        "production wiring docs",
        &combined,
        &["--network ic", "dfx canister", "dfx deploy"],
    )?;

    check_did_surface_at(root, false)?;
    check_required_executable_scripts_at(root)?;
    Ok(())
}
