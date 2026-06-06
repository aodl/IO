use candid::Principal;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const RELEASE_PROFILE: &str = "release";
const WASM_TARGET: &str = "wasm32-unknown-unknown";
const MANIFEST_PATH: &str = "release-artifacts/manifest.json";
const KNOWN_TWO_YEAR_NNS_NEURON_ID: u64 = 6_345_890_886_899_317_159;
const KNOWN_CONTROLLER_CANISTER_PRINCIPAL: &str = "oae4c-3iaaa-aaaar-qb5qq-cai";

#[derive(Clone, Copy, Debug)]
struct ReleaseCanister {
    name: &'static str,
    package: &'static str,
    artifact: &'static str,
    value_moving: bool,
}

const RELEASE_CANISTERS: &[ReleaseCanister] = &[
    ReleaseCanister {
        name: "io_stream_manager",
        package: "io-stream-manager",
        artifact: "io_stream_manager",
        value_moving: true,
    },
    ReleaseCanister {
        name: "io_nns_neuron_manager",
        package: "io-nns-neuron-manager",
        artifact: "io_nns_neuron_manager",
        value_moving: true,
    },
    ReleaseCanister {
        name: "io_historian",
        package: "io-historian",
        artifact: "io_historian",
        value_moving: false,
    },
    ReleaseCanister {
        name: "frontend",
        package: "io-frontend",
        artifact: "io_frontend",
        value_moving: false,
    },
];

const STREAM_PRODUCTION_FORBIDDEN_DID: &[&str] = &[
    " get_state :",
    " get_config :",
    " get_redemption_rate :",
    " process_stream_event :",
    " redeem :",
    " tick :",
    " debug_tick :",
    " plan_rebalance :",
    " advance_model_time :",
    "debug_",
    " get_events :",
];

const NNS_PRODUCTION_FORBIDDEN_DID: &[&str] = &[
    " get_state :",
    " get_config :",
    " get_redemption_rate :",
    " process_stream_event :",
    " redeem :",
    " tick :",
    " debug_tick :",
    " plan_rebalance :",
    " advance_model_time :",
    "debug_",
    " get_events :",
];

const PRODUCTION_WASM_FORBIDDEN_METHOD_STRINGS: &[&str] = &[
    "debug_get_state",
    "debug_get_config",
    "debug_get_redemption_rate",
    "debug_process_stream_event",
    "debug_redeem",
    "debug_tick",
    "debug_plan_rebalance",
    "debug_advance_model_time",
    "get_redemption_rate",
    "process_stream_event",
    "get_events",
];

fn run(label: &str, mut cmd: Command) -> bool {
    eprintln!("\n=== {label} ===");
    match cmd.status() {
        Ok(status) if status.success() => {
            eprintln!("✓ {label}");
            true
        }
        Ok(status) => {
            eprintln!("✗ {label}: exited with {status}");
            false
        }
        Err(err) => {
            eprintln!("✗ {label}: {err}");
            false
        }
    }
}

fn cargo_test(args: &[&str]) -> Command {
    let mut c = Command::new("cargo");
    c.arg("test").args(args);
    c
}

fn cargo_check(args: &[&str]) -> Command {
    let mut c = Command::new("cargo");
    c.arg("check").args(args);
    c
}

fn cargo_fmt(args: &[&str]) -> Command {
    let mut c = Command::new("cargo");
    c.arg("fmt").args(args);
    c
}

fn cargo_clippy(args: &[&str]) -> Command {
    let mut c = Command::new("cargo");
    c.arg("clippy").args(args);
    c
}

fn build_canister(package: &str, profile: &str) -> Command {
    let mut c = Command::new("tools/scripts/build-canister");
    c.arg(package).arg(profile);
    c
}

fn icp(args: &[&str]) -> Command {
    let mut c = Command::new("icp");
    c.args(args);
    c
}

fn script(path: &str, args: &[&str]) -> Command {
    let mut c = Command::new(path);
    c.args(args);
    c
}

fn run_subcommand(sub: &str) -> bool {
    let exe = env::current_exe().expect("current exe");
    let mut c = Command::new(exe);
    c.arg(sub);
    run(sub, c)
}

fn read_file(root: &Path, path: &str) -> Result<String, String> {
    fs::read_to_string(root.join(path)).map_err(|err| format!("{path}: {err}"))
}

fn require_absent(path: &str, text: &str, needles: &[&str]) -> Result<(), String> {
    for needle in needles {
        if text.contains(needle) {
            return Err(format!("{path} must not contain {needle:?}"));
        }
    }
    Ok(())
}

fn require_present(path: &str, text: &str, needles: &[&str]) -> Result<(), String> {
    for needle in needles {
        if !text.contains(needle) {
            return Err(format!("{path} must contain {needle:?}"));
        }
    }
    Ok(())
}

fn require_file(root: &Path, path: &str) -> Result<String, String> {
    if !root.join(path).is_file() {
        return Err(format!("{path}: missing required file"));
    }
    read_file(root, path)
}

fn forbidden_did_methods(text: &str, needles: &[&str]) -> Vec<String> {
    needles
        .iter()
        .filter(|needle| text.contains(**needle))
        .map(|needle| (*needle).trim().to_string())
        .collect()
}

fn check_minimal_value_moving_did(path: &str, text: &str) -> Result<(), String> {
    let stripped = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    if !stripped.contains("service : (InitArgs) -> {}") {
        return Err(format!(
            "{path} must keep value-moving production service install-args-only"
        ));
    }
    Ok(())
}

fn check_wasm_forbidden_methods(root: &Path) -> Result<(), String> {
    for canister in RELEASE_CANISTERS
        .iter()
        .filter(|canister| canister.value_moving)
    {
        let path = format!("release-artifacts/{}.wasm", canister.artifact);
        let bytes = fs::read(root.join(&path)).map_err(|err| format!("{path}: {err}"))?;
        let haystack = String::from_utf8_lossy(&bytes);
        for needle in PRODUCTION_WASM_FORBIDDEN_METHOD_STRINGS {
            if haystack.contains(needle) {
                return Err(format!(
                    "{path} production Wasm contains forbidden method string {needle:?}"
                ));
            }
        }
    }
    Ok(())
}

fn check_did_surface_at(root: &Path, check_wasm: bool) -> Result<(), String> {
    let stream_production_path = "canisters/io_stream_manager/io_stream_manager.did";
    let stream_debug_path = "canisters/io_stream_manager/io_stream_manager_debug.did";
    let nns_production_path = "canisters/io_nns_neuron_manager/io_nns_neuron_manager.did";
    let nns_debug_path = "canisters/io_nns_neuron_manager/io_nns_neuron_manager_debug.did";

    let stream_production = read_file(root, stream_production_path)?;
    let stream_debug = read_file(root, stream_debug_path)?;
    let nns_production = read_file(root, nns_production_path)?;
    let nns_debug = read_file(root, nns_debug_path)?;

    check_minimal_value_moving_did(stream_production_path, &stream_production)?;
    check_minimal_value_moving_did(nns_production_path, &nns_production)?;

    let stream_forbidden =
        forbidden_did_methods(&stream_production, STREAM_PRODUCTION_FORBIDDEN_DID);
    if !stream_forbidden.is_empty() {
        return Err(format!(
            "{stream_production_path} contains forbidden production methods: {}",
            stream_forbidden.join(", ")
        ));
    }
    let nns_forbidden = forbidden_did_methods(&nns_production, NNS_PRODUCTION_FORBIDDEN_DID);
    if !nns_forbidden.is_empty() {
        return Err(format!(
            "{nns_production_path} contains forbidden production methods: {}",
            nns_forbidden.join(", ")
        ));
    }

    require_present(
        stream_debug_path,
        &stream_debug,
        &[
            "debug_get_state",
            "debug_get_redemption_rate",
            "debug_process_stream_event",
            "debug_redeem",
            "debug_tick",
        ],
    )?;
    require_present(
        nns_debug_path,
        &nns_debug,
        &[
            "debug_get_config",
            "debug_get_state",
            "debug_plan_rebalance",
            "debug_advance_model_time",
            "debug_tick",
        ],
    )?;

    require_absent(
        stream_debug_path,
        &stream_debug,
        &[
            "  get_state :",
            "  get_redemption_rate :",
            "  process_stream_event :",
            "  redeem :",
        ],
    )?;
    require_absent(
        nns_debug_path,
        &nns_debug,
        &[
            "  get_config :",
            "  get_state :",
            "  plan_rebalance :",
            "  advance_model_time :",
        ],
    )?;

    if check_wasm && root.join("release-artifacts").is_dir() {
        check_wasm_forbidden_methods(root)?;
    }

    Ok(())
}

fn check_artifacts(root: &Path, paths: &[String]) -> Result<(), String> {
    for path in paths {
        if !root.join(path).is_file() {
            return Err(format!("missing artifact {path}"));
        }
    }
    Ok(())
}

fn expected_release_artifacts() -> Vec<String> {
    RELEASE_CANISTERS
        .iter()
        .flat_map(|canister| {
            [
                format!("release-artifacts/{}.wasm", canister.artifact),
                format!("release-artifacts/{}.wasm.gz", canister.artifact),
                format!("release-artifacts/{}.wasm.sha256", canister.artifact),
                format!("release-artifacts/{}.wasm.gz.sha256", canister.artifact),
            ]
        })
        .chain([MANIFEST_PATH.to_string()])
        .collect()
}

fn sha256_hex(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|err| format!("{}: {err}", path.display()))?;
    let digest = Sha256::digest(bytes);
    Ok(digest.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn verify_artifact_hash(root: &Path, sidecar: &str) -> Result<(), String> {
    let sidecar_path = root.join(sidecar);
    let text = fs::read_to_string(&sidecar_path)
        .map_err(|err| format!("{}: {err}", sidecar_path.display()))?;
    let mut parts = text.split_whitespace();
    let expected_hash = parts
        .next()
        .ok_or_else(|| format!("{sidecar}: missing hash"))?;
    let artifact_path = parts
        .next()
        .ok_or_else(|| format!("{sidecar}: missing artifact path"))?;
    if parts.next().is_some() {
        return Err(format!("{sidecar}: expected exactly '<sha256> <path>'"));
    }
    let actual_hash = sha256_hex(&root.join(artifact_path))?;
    if actual_hash != expected_hash {
        return Err(format!(
            "{sidecar}: hash mismatch for {artifact_path}: expected {expected_hash}, got {actual_hash}"
        ));
    }
    Ok(())
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct ArtifactManifest {
    schema_version: u32,
    build_profile: String,
    target: String,
    git_commit: Option<String>,
    artifacts: Vec<ArtifactManifestEntry>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct ArtifactManifestEntry {
    canister: String,
    raw_wasm_path: String,
    raw_wasm_sha256: String,
    raw_wasm_bytes: u64,
    gz_wasm_path: String,
    gz_wasm_sha256: String,
    gz_wasm_bytes: u64,
    build_profile: String,
    target: String,
    git_commit: Option<String>,
}

fn current_git_commit() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn build_manifest(root: &Path) -> Result<ArtifactManifest, String> {
    let git_commit = current_git_commit();
    let mut artifacts = Vec::new();
    for canister in RELEASE_CANISTERS {
        let raw = format!("release-artifacts/{}.wasm", canister.artifact);
        let gz = format!("release-artifacts/{}.wasm.gz", canister.artifact);
        let raw_path = root.join(&raw);
        let gz_path = root.join(&gz);
        let raw_metadata = fs::metadata(&raw_path).map_err(|err| format!("{raw}: {err}"))?;
        let gz_metadata = fs::metadata(&gz_path).map_err(|err| format!("{gz}: {err}"))?;
        artifacts.push(ArtifactManifestEntry {
            canister: canister.name.to_string(),
            raw_wasm_path: raw.clone(),
            raw_wasm_sha256: sha256_hex(&raw_path)?,
            raw_wasm_bytes: raw_metadata.len(),
            gz_wasm_path: gz.clone(),
            gz_wasm_sha256: sha256_hex(&gz_path)?,
            gz_wasm_bytes: gz_metadata.len(),
            build_profile: RELEASE_PROFILE.to_string(),
            target: WASM_TARGET.to_string(),
            git_commit: git_commit.clone(),
        });
    }

    Ok(ArtifactManifest {
        schema_version: 1,
        build_profile: RELEASE_PROFILE.to_string(),
        target: WASM_TARGET.to_string(),
        git_commit,
        artifacts,
    })
}

fn write_manifest(root: &Path) -> Result<(), String> {
    let manifest = build_manifest(root)?;
    let text = serde_json::to_string_pretty(&manifest)
        .map_err(|err| format!("serialize manifest: {err}"))?;
    fs::write(root.join(MANIFEST_PATH), format!("{text}\n"))
        .map_err(|err| format!("{MANIFEST_PATH}: {err}"))?;
    Ok(())
}

fn read_manifest(root: &Path) -> Result<ArtifactManifest, String> {
    let text = read_file(root, MANIFEST_PATH)?;
    serde_json::from_str(&text).map_err(|err| format!("{MANIFEST_PATH}: {err}"))
}

fn verify_manifest(root: &Path) -> Result<(), String> {
    let actual = read_manifest(root)?;
    let expected = build_manifest(root)?;
    if actual.schema_version != 1 {
        return Err(format!(
            "{MANIFEST_PATH}: unsupported schema_version {}",
            actual.schema_version
        ));
    }
    if actual != expected {
        return Err(format!(
            "{MANIFEST_PATH}: manifest does not match current artifacts"
        ));
    }
    Ok(())
}

fn verify_no_stale_release_artifacts(root: &Path) -> Result<(), String> {
    let expected = expected_release_artifacts()
        .into_iter()
        .collect::<BTreeSet<_>>();
    let release_dir = root.join("release-artifacts");
    for entry in fs::read_dir(&release_dir).map_err(|err| format!("release-artifacts: {err}"))? {
        let entry = entry.map_err(|err| format!("release-artifacts: {err}"))?;
        if !entry.file_type().map_err(|err| err.to_string())?.is_file() {
            continue;
        }
        let path = format!("release-artifacts/{}", entry.file_name().to_string_lossy());
        if !expected.contains(&path) {
            return Err(format!("stale or unexpected release artifact {path}"));
        }
    }
    Ok(())
}

fn verify_artifacts_at(root: &Path) -> Result<(), String> {
    let artifacts = expected_release_artifacts();
    check_artifacts(root, &artifacts)?;
    for sha in artifacts.iter().filter(|path| path.ends_with(".sha256")) {
        verify_artifact_hash(root, sha)?;
    }
    verify_manifest(root)?;
    verify_no_stale_release_artifacts(root)?;
    Ok(())
}

fn validate_principal(field: &str, value: &str, mode: InstallArgsMode) -> Result<(), String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(format!("{field}: principal is empty"));
    }
    Principal::from_text(trimmed)
        .map_err(|err| format!("{field}: invalid principal {value:?}: {err}"))?;
    if mode == InstallArgsMode::Mainnet && is_placeholder_principal(trimmed) {
        return Err(format!(
            "{field}: placeholder/mock principal {value:?} is not accepted in mainnet mode"
        ));
    }
    Ok(())
}

fn is_placeholder_principal(value: &str) -> bool {
    let lowered = value.to_ascii_lowercase();
    lowered == "aaaaa-aa"
        || lowered == "2vxsx-fae"
        || lowered.contains("placeholder")
        || lowered.contains("example")
        || lowered.contains("todo")
        || lowered.contains("mock")
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InstallArgsMode {
    Local,
    Mainnet,
    All,
}

impl InstallArgsMode {
    fn parse(value: Option<&str>) -> Result<Self, String> {
        match value.unwrap_or("all") {
            "local" => Ok(Self::Local),
            "mainnet" => Ok(Self::Mainnet),
            "all" => Ok(Self::All),
            other => Err(format!(
                "unknown install-args validation mode {other:?}; expected local, mainnet, or all"
            )),
        }
    }
}

fn parse_required_text_field(text: &str, field: &str) -> Result<String, String> {
    let marker = format!("{field} = ");
    let start = text
        .find(&marker)
        .ok_or_else(|| format!("missing required field {field}"))?
        + marker.len();
    let rest = &text[start..];
    let first_quote = rest
        .find('"')
        .ok_or_else(|| format!("{field}: missing opening quote"))?
        + 1;
    let after_first = &rest[first_quote..];
    let second_quote = after_first
        .find('"')
        .ok_or_else(|| format!("{field}: missing closing quote"))?;
    Ok(after_first[..second_quote].to_string())
}

fn parse_optional_text_field(text: &str, field: &str) -> Result<Option<String>, String> {
    let marker = format!("{field} = ");
    let Some(start) = text.find(&marker).map(|start| start + marker.len()) else {
        return Ok(None);
    };
    let rest = &text[start..];
    let end = rest
        .find(';')
        .ok_or_else(|| format!("{field}: missing semicolon"))?;
    let value = rest[..end].trim();
    if value.starts_with("null") {
        return Ok(None);
    }
    if !value.starts_with("opt ") {
        return Err(format!("{field}: expected null or opt text, got {value:?}"));
    }
    let first_quote = value
        .find('"')
        .ok_or_else(|| format!("{field}: missing opening quote"))?
        + 1;
    let second_quote = value[first_quote..]
        .find('"')
        .ok_or_else(|| format!("{field}: missing closing quote"))?
        + first_quote;
    Ok(Some(value[first_quote..second_quote].to_string()))
}

fn parse_required_u64_field(text: &str, field: &str) -> Result<u64, String> {
    let marker = format!("{field} = ");
    let start = text
        .find(&marker)
        .ok_or_else(|| format!("missing required field {field}"))?
        + marker.len();
    let rest = &text[start..];
    let end = rest
        .find(';')
        .ok_or_else(|| format!("{field}: missing semicolon"))?;
    let digits = rest[..end]
        .chars()
        .take_while(|ch| ch.is_ascii_digit() || *ch == '_')
        .filter(|ch| *ch != '_')
        .collect::<String>();
    digits
        .parse::<u64>()
        .map_err(|err| format!("{field}: invalid nat64: {err}"))
}

fn validate_nns_install_args_text(text: &str, mode: InstallArgsMode) -> Result<(), String> {
    let controller = parse_required_text_field(text, "controller_canister_principal_text")?;
    validate_principal("controller_canister_principal_text", &controller, mode)?;
    if mode == InstallArgsMode::Mainnet && controller != KNOWN_CONTROLLER_CANISTER_PRINCIPAL {
        return Err(format!(
            "controller_canister_principal_text: expected known controller {KNOWN_CONTROLLER_CANISTER_PRINCIPAL}, got {controller}"
        ));
    }
    let neuron_id = parse_required_u64_field(text, "two_year_nns_neuron_id")?;
    if neuron_id == 0 {
        return Err("two_year_nns_neuron_id: missing or zero".to_string());
    }
    if mode == InstallArgsMode::Mainnet && neuron_id != KNOWN_TWO_YEAR_NNS_NEURON_ID {
        return Err(format!(
            "two_year_nns_neuron_id: expected known live id {KNOWN_TWO_YEAR_NNS_NEURON_ID}, got {neuron_id}"
        ));
    }
    let dissolve_seconds = parse_required_u64_field(text, "two_week_dissolve_seconds")?;
    if dissolve_seconds == 0 {
        return Err("two_week_dissolve_seconds: missing or zero".to_string());
    }
    for field in [
        "io_stream_manager_principal_text",
        "nns_governance_principal_text",
        "icp_ledger_principal_text",
        "icp_index_principal_text",
    ] {
        if let Some(value) = parse_optional_text_field(text, field)? {
            validate_principal(field, &value, mode)?;
        }
    }
    Ok(())
}

fn validate_stream_install_args_text(text: &str, mode: InstallArgsMode) -> Result<(), String> {
    for field in [
        "jupiter_faucet_principal_text",
        "io_nns_neuron_manager_principal_text",
        "icp_ledger_principal_text",
        "icp_index_principal_text",
        "io_ledger_principal_text",
        "io_index_principal_text",
        "io_sns_ledger_principal_text",
        "io_sns_index_principal_text",
        "sns_governance_principal_text",
    ] {
        if let Some(value) = parse_optional_text_field(text, field)? {
            validate_principal(field, &value, mode)?;
        }
    }
    Ok(())
}

fn validate_install_args_at(root: &Path, mode: InstallArgsMode) -> Result<(), String> {
    if matches!(mode, InstallArgsMode::Local | InstallArgsMode::All) {
        validate_stream_install_args_text(
            r#"(record {
              jupiter_faucet_principal_text = opt "aaaaa-aa";
              io_nns_neuron_manager_principal_text = opt "oae4c-3iaaa-aaaar-qb5qq-cai";
              icp_ledger_principal_text = null : opt text;
              icp_index_principal_text = null : opt text;
              io_ledger_principal_text = null : opt text;
              io_index_principal_text = null : opt text;
              io_sns_ledger_principal_text = null : opt text;
              io_sns_index_principal_text = null : opt text;
              sns_governance_principal_text = null : opt text;
            })"#,
            InstallArgsMode::Local,
        )?;
        validate_nns_install_args_text(
            r#"(record {
              controller_canister_principal_text = "aaaaa-aa";
              two_year_nns_neuron_id = 42 : nat64;
              two_week_dissolve_seconds = 1_209_600 : nat64;
              io_stream_manager_principal_text = opt "oae4c-3iaaa-aaaar-qb5qq-cai";
              nns_governance_principal_text = null : opt text;
              icp_ledger_principal_text = null : opt text;
              icp_index_principal_text = null : opt text;
            })"#,
            InstallArgsMode::Local,
        )?;
    }

    if matches!(mode, InstallArgsMode::Mainnet | InstallArgsMode::All) {
        let stream_args = read_file(root, "canisters/io_stream_manager/mainnet-install-args.did")?;
        let nns_args = read_file(
            root,
            "canisters/io_nns_neuron_manager/mainnet-install-args.did",
        )?;
        validate_stream_install_args_text(&stream_args, InstallArgsMode::Mainnet)
            .map_err(|err| format!("io_stream_manager mainnet install args: {err}"))?;
        validate_nns_install_args_text(&nns_args, InstallArgsMode::Mainnet)
            .map_err(|err| format!("io_nns_neuron_manager mainnet install args: {err}"))?;
        validate_no_install_args_did(root, "canisters/io_historian/io_historian.did")
            .map_err(|err| format!("io_historian install args: {err}"))?;
        validate_no_install_args_did(root, "canisters/frontend/frontend.did")
            .map_err(|err| format!("frontend install args: {err}"))?;
    }
    Ok(())
}

fn check_required_executable_scripts_at(root: &Path) -> Result<(), String> {
    for dir in ["tools/scripts", "tools/sns"] {
        let path = root.join(dir);
        if !path.exists() {
            continue;
        }
        let entries = fs::read_dir(&path).map_err(|err| format!("{dir}: {err}"))?;
        for entry in entries {
            let entry = entry.map_err(|err| format!("{dir}: {err}"))?;
            let file_type = entry
                .file_type()
                .map_err(|err| format!("{}: {err}", entry.path().display()))?;
            if !file_type.is_file() {
                continue;
            }
            let path = entry.path();
            let text = fs::read_to_string(&path).unwrap_or_default();
            if !text.starts_with("#!") {
                continue;
            }
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .to_string();
            require_absent(&rel, &text, &["dfx", "--network ic"])?;
        }
    }
    Ok(())
}

fn check_sns_harness_at(root: &Path) -> Result<(), String> {
    let local_sns_doc = require_file(root, "docs/operations/local-sns-testing.md")?;
    require_present(
        "docs/operations/local-sns-testing.md",
        &local_sns_doc,
        &[
            "Pure model tests remain the main accounting guardrail",
            "Mock and PocketIC tests remain the main journal, retry, and upgrade guardrail",
            "Official SNS Testing Flow",
            "optional, local-only, and not part of `test_ci` or `verify_release`",
            "IO-Owned PocketIC SNS Harness",
            "must not call mainnet",
            "must not use `--network ic`",
            "not production launch configuration",
        ],
    )?;

    let sns_readme = require_file(root, "tools/sns/README.md")?;
    require_present(
        "tools/sns/README.md",
        &sns_readme,
        &[
            "not production launch configuration",
            "must not depend on `dfx`",
            "must not use `--network ic`",
            "placeholder principals",
        ],
    )?;

    let sns_init = require_file(root, "tools/sns/sns_init.io.local.yaml")?;
    require_present(
        "tools/sns/sns_init.io.local.yaml",
        &sns_init,
        &[
            "name: \"IO\"",
            "symbol: \"IO\"",
            "transaction_fee_e8s",
            "proposal_rejection_fee_e8s: 10_000_000_000",
            "initial_reward_rate_basis_points: 0",
            "final_reward_rate_basis_points: 0",
            "age_bonus_percentage: 0",
            "jupiter_faucet_governance_neuron",
            "jupiter_faucet_non_dissolvable_neuron",
            "ordinary_user_neurons",
            "fallback_controllers",
            "io_stream_manager",
            "io_nns_neuron_manager",
            "io_historian",
            "frontend",
            "icp_ledger_principal_text",
            "icp_index_principal_text",
            "io_ledger_principal_text",
            "io_index_principal_text",
            "io_sns_ledger_principal_text",
            "io_sns_index_principal_text",
            "sns_governance_principal_text",
            "nns_governance_principal_text",
            "not production-ready",
            "placeholder",
        ],
    )?;
    require_absent(
        "tools/sns/sns_init.io.local.yaml",
        &sns_init,
        &["--network ic", "ryjl3-tyaaa-aaaaa-aaaba-cai"],
    )?;

    let official_notes = require_file(root, "tools/sns/official-sns-testing-notes.md")?;
    require_present(
        "tools/sns/official-sns-testing-notes.md",
        &official_notes,
        &[
            "optional",
            "local-only",
            "not part of `test_ci`",
            "not used by `verify_release`",
            "must not call mainnet",
            "dfx sns",
            "Do not use --network ic",
        ],
    )?;

    check_required_executable_scripts_at(root)?;
    Ok(())
}

fn validate_no_install_args_did(root: &Path, path: &str) -> Result<(), String> {
    let text = read_file(root, path)?;
    if text.contains("service : (") {
        return Err(format!(
            "{path}: unexpected init/install args in service declaration"
        ));
    }
    require_present(path, &text, &["service : {"])?;
    Ok(())
}

fn run_security_scan(required: bool) -> bool {
    let mode = if required { "required" } else { "permissive" };
    run(
        &format!("security scan: {mode}"),
        script("tools/scripts/security-scan", &[mode]),
    )
}

fn print_known_commands() {
    eprintln!("known: test_all, test_ci, verify_release, security_scan, security_scan_required, validate_install_args, sns_harness_check, sns_governance_read_tests, sns_governance_read_required, sns_ledger_index_tests, sns_ledger_index_required, sns_pocketic_smoke, sns_pocketic_required, test_pocketic_required, preflight, check, fmt_check, did_surface, build_canisters, verify_artifacts, build_debug_canisters, test_unit, test_pocketic_integration, test_local_integration, test_e2e, stream_manager_unit, nns_neuron_manager_unit, stream_manager_pocketic_integration, nns_neuron_manager_pocketic_integration");
}

fn main() -> ExitCode {
    let mut args = env::args().skip(1).collect::<Vec<_>>();
    let cmd = if args.is_empty() {
        "test_all".to_string()
    } else {
        args.remove(0)
    };
    let root = PathBuf::from(".");
    let mut ok = true;
    match cmd.as_str() {
        "check" => {
            ok &= run(
                "check: workspace all targets",
                cargo_check(&["--workspace", "--all-targets"]),
            );
        }
        "fmt_check" => {
            ok &= run("fmt: workspace", cargo_fmt(&["--all", "--", "--check"]));
        }
        "did_surface" => match check_did_surface_at(&root, true) {
            Ok(()) => eprintln!("✓ did_surface"),
            Err(err) => {
                eprintln!("✗ did_surface: {err}");
                ok = false;
            }
        },
        "build_canisters" => {
            for canister in RELEASE_CANISTERS {
                ok &= run(
                    &format!("build canister: {}", canister.package),
                    build_canister(canister.package, RELEASE_PROFILE),
                );
            }
            if ok {
                match write_manifest(&root) {
                    Ok(()) => eprintln!("✓ build_canisters manifest"),
                    Err(err) => {
                        eprintln!("✗ build_canisters manifest: {err}");
                        ok = false;
                    }
                }
            }
            match verify_artifacts_at(&root) {
                Ok(()) => eprintln!("✓ build_canisters artifacts"),
                Err(err) => {
                    eprintln!("✗ build_canisters artifacts: {err}");
                    ok = false;
                }
            }
        }
        "verify_artifacts" => match verify_artifacts_at(&root) {
            Ok(()) => eprintln!("✓ verify_artifacts"),
            Err(err) => {
                eprintln!("✗ verify_artifacts: {err}");
                ok = false;
            }
        },
        "validate_install_args" => {
            let mode = match InstallArgsMode::parse(args.first().map(String::as_str)) {
                Ok(mode) => mode,
                Err(err) => {
                    eprintln!("✗ validate_install_args: {err}");
                    return ExitCode::from(2);
                }
            };
            match validate_install_args_at(&root, mode) {
                Ok(()) => eprintln!("✓ validate_install_args"),
                Err(err) => {
                    eprintln!("✗ validate_install_args: {err}");
                    ok = false;
                }
            }
        }
        "sns_harness_check" => match check_sns_harness_at(&root) {
            Ok(()) => eprintln!("✓ sns_harness_check"),
            Err(err) => {
                eprintln!("✗ sns_harness_check: {err}");
                ok = false;
            }
        },
        "sns_governance_read_tests" => {
            ok &= run(
                "unit: mock-sns-governance",
                cargo_test(&["-p", "mock-sns-governance"]),
            );
            ok &= run(
                "unit: io-stream-manager governance snapshot",
                cargo_test(&["-p", "io-stream-manager", "--lib", "governance_snapshot"]),
            );
        }
        "sns_governance_read_required" => {
            if env::var_os("POCKET_IC_BIN").is_none() {
                eprintln!("✗ sns_governance_read_required: POCKET_IC_BIN is not set");
                ok = false;
            } else {
                ok &= run_subcommand("build_debug_canisters");
                ok &= run(
                    "pocketic: io-sns-governance-read",
                    cargo_test(&[
                        "-p",
                        "io-stream-manager",
                        "--test",
                        "io_sns_governance_read_pocketic",
                    ]),
                );
            }
        }
        "sns_ledger_index_tests" => {
            ok &= run(
                "unit: io-ledger-types",
                cargo_test(&["-p", "io-ledger-types"]),
            );
            ok &= run(
                "unit: stream-manager scheduler boundary",
                cargo_test(&["-p", "io-stream-manager", "--lib", "scheduler"]),
            );
            ok &= run(
                "unit: mock SNS-shaped ledger/index",
                cargo_test(&[
                    "-p",
                    "mock-icp-ledger",
                    "-p",
                    "mock-io-ledger",
                    "-p",
                    "mock-icp-index",
                    "-p",
                    "mock-io-index",
                ]),
            );
        }
        "sns_ledger_index_required" => {
            if env::var_os("POCKET_IC_BIN").is_none() {
                eprintln!("✗ sns_ledger_index_required: POCKET_IC_BIN is not set");
                ok = false;
            } else {
                ok &= run_subcommand("sns_ledger_index_tests");
                ok &= run_subcommand("build_debug_canisters");
                ok &= run(
                    "pocketic: io-stream-manager ledger/index value flows",
                    cargo_test(&[
                        "-p",
                        "io-stream-manager",
                        "--test",
                        "io_stream_manager_pocketic",
                    ]),
                );
            }
        }
        "security_scan" => {
            ok &= run_security_scan(false);
        }
        "security_scan_required" => {
            ok &= run_security_scan(true);
        }
        "verify_release" => {
            for sub in [
                "did_surface",
                "build_canisters",
                "verify_artifacts",
                "validate_install_args",
                "sns_harness_check",
                "sns_governance_read_tests",
                "sns_ledger_index_tests",
                "security_scan_required",
            ] {
                ok &= run_subcommand(sub);
            }
        }
        "build_debug_canisters" => {
            for package in [
                "io-stream-manager",
                "io-nns-neuron-manager",
                "io-historian",
                "io-frontend",
                "mock-icp-ledger",
                "mock-io-ledger",
                "mock-icp-index",
                "mock-io-index",
                "mock-nns-governance",
                "mock-sns-governance",
                "mock-jupiter-faucet",
            ] {
                ok &= run(
                    &format!("build debug canister: {package}"),
                    build_canister(package, "debug"),
                );
            }
        }
        "preflight" => {
            ok &= run_subcommand("check");
            ok &= run_subcommand("did_surface");
            ok &= run_subcommand("validate_install_args");
        }
        "test_unit" => {
            ok &= run("unit: xtask guardrails", cargo_test(&["-p", "xtask"]));
            ok &= run("unit: io-core-model", cargo_test(&["-p", "io-core-model"]));
            ok &= run(
                "unit: io-reward-policy",
                cargo_test(&["-p", "io-reward-policy"]),
            );
            ok &= run(
                "unit: io-stream-manager",
                cargo_test(&["-p", "io-stream-manager", "--lib"]),
            );
            ok &= run(
                "unit: io-nns-neuron-manager",
                cargo_test(&["-p", "io-nns-neuron-manager", "--lib"]),
            );
            ok &= run(
                "unit: placeholders",
                cargo_test(&["-p", "io-historian", "-p", "io-frontend"]),
            );
        }
        "test_pocketic_integration" => {
            ok &= run_subcommand("build_debug_canisters");
            ok &= run(
                "pocketic: io-stream-manager",
                cargo_test(&[
                    "-p",
                    "io-stream-manager",
                    "--test",
                    "io_stream_manager_pocketic",
                ]),
            );
            ok &= run(
                "pocketic: io-nns-neuron-manager",
                cargo_test(&[
                    "-p",
                    "io-nns-neuron-manager",
                    "--test",
                    "io_nns_neuron_manager_pocketic",
                ]),
            );
        }
        "test_pocketic_required" => {
            if env::var_os("POCKET_IC_BIN").is_none() {
                eprintln!("✗ test_pocketic_required: POCKET_IC_BIN is not set");
                ok = false;
            } else {
                ok &= run_subcommand("test_pocketic_integration");
            }
        }
        "sns_pocketic_smoke" => {
            ok &= run_subcommand("sns_harness_check");
            if env::var_os("POCKET_IC_BIN").is_none() {
                eprintln!("skipping sns_pocketic_smoke: POCKET_IC_BIN is not set");
            } else {
                ok &= run_subcommand("sns_pocketic_required");
            }
        }
        "sns_pocketic_required" => {
            if env::var_os("POCKET_IC_BIN").is_none() {
                eprintln!("✗ sns_pocketic_required: POCKET_IC_BIN is not set");
                ok = false;
            } else {
                ok &= run_subcommand("build_debug_canisters");
                ok &= run(
                    "pocketic: io-sns-topology",
                    cargo_test(&[
                        "-p",
                        "io-stream-manager",
                        "--test",
                        "io_sns_topology_pocketic",
                    ]),
                );
                ok &= run(
                    "pocketic: io-sns-governance-read",
                    cargo_test(&[
                        "-p",
                        "io-stream-manager",
                        "--test",
                        "io_sns_governance_read_pocketic",
                    ]),
                );
            }
        }
        "test_local_integration" => {
            ok &= run_subcommand("build_canisters");
            ok &= run_subcommand("did_surface");
            ok &= run_subcommand("validate_install_args");
            ok &= run("local-cli: icp project show", icp(&["project", "show"]));
            ok &= run("local-cli: icp build", icp(&["build"]));
            ok &= run(
                "local-cli: io-stream-manager",
                cargo_test(&["-p", "io-stream-manager", "--test", "io_stream_manager_cli"]),
            );
            ok &= run(
                "local-cli: io-nns-neuron-manager",
                cargo_test(&[
                    "-p",
                    "io-nns-neuron-manager",
                    "--test",
                    "io_nns_neuron_manager_cli",
                ]),
            );
        }
        "test_e2e" => {
            ok &= run(
                "e2e: io suite model",
                cargo_test(&["-p", "io-stream-manager", "--test", "io_e2e"]),
            );
        }
        "stream_manager_unit" => {
            ok &= run(
                "unit: io-stream-manager",
                cargo_test(&["-p", "io-stream-manager", "--lib"]),
            )
        }
        "nns_neuron_manager_unit" => {
            ok &= run(
                "unit: io-nns-neuron-manager",
                cargo_test(&["-p", "io-nns-neuron-manager", "--lib"]),
            )
        }
        "stream_manager_pocketic_integration" => {
            ok &= run(
                "pocketic: io-stream-manager",
                cargo_test(&[
                    "-p",
                    "io-stream-manager",
                    "--test",
                    "io_stream_manager_pocketic",
                ]),
            )
        }
        "nns_neuron_manager_pocketic_integration" => {
            ok &= run(
                "pocketic: io-nns-neuron-manager",
                cargo_test(&[
                    "-p",
                    "io-nns-neuron-manager",
                    "--test",
                    "io_nns_neuron_manager_pocketic",
                ]),
            )
        }
        "test_all" => {
            for sub in [
                "preflight",
                "test_unit",
                "test_pocketic_integration",
                "test_local_integration",
                "test_e2e",
                "security_scan",
            ] {
                ok &= run_subcommand(sub);
            }
        }
        "test_ci" => {
            for sub in [
                "fmt_check",
                "check",
                "did_surface",
                "build_canisters",
                "verify_artifacts",
                "validate_install_args",
                "security_scan_required",
                "test_unit",
                "test_pocketic_required",
                "test_local_integration",
                "test_e2e",
            ] {
                ok &= run_subcommand(sub);
            }
            ok &= run(
                "clippy: workspace all targets",
                cargo_clippy(&["--workspace", "--all-targets", "--", "-D", "warnings"]),
            );
        }
        other => {
            eprintln!("unknown xtask command: {other}");
            print_known_commands();
            return ExitCode::from(2);
        }
    }
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        let root = env::temp_dir().join(format!(
            "io-xtask-{name}-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("release-artifacts")).unwrap();
        root
    }

    fn write(root: &Path, path: &str, text: &str) {
        let path = root.join(path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, text).unwrap();
    }

    fn write_artifact_set(root: &Path) {
        for canister in RELEASE_CANISTERS {
            let raw = format!("release-artifacts/{}.wasm", canister.artifact);
            let gz = format!("release-artifacts/{}.wasm.gz", canister.artifact);
            write(root, &raw, &format!("{} raw", canister.name));
            write(root, &gz, &format!("{} gz", canister.name));
            let raw_sha = sha256_hex(&root.join(&raw)).unwrap();
            let gz_sha = sha256_hex(&root.join(&gz)).unwrap();
            write(
                root,
                &format!("{raw}.sha256"),
                &format!("{raw_sha}  {raw}\n"),
            );
            write(root, &format!("{gz}.sha256"), &format!("{gz_sha}  {gz}\n"));
        }
        write_manifest(root).unwrap();
    }

    fn write_sns_harness_fixture(root: &Path) {
        write(
            root,
            "docs/operations/local-sns-testing.md",
            r#"# Local SNS Testing
Pure model tests remain the main accounting guardrail.
Mock and PocketIC tests remain the main journal, retry, and upgrade guardrail.
## Official SNS Testing Flow
dfx-based SNS testing for IO is optional, local-only, and not part of `test_ci` or `verify_release`.
## IO-Owned PocketIC SNS Harness
This must not call mainnet, must not use `--network ic`, and is not production launch configuration.
"#,
        );
        write(
            root,
            "tools/sns/README.md",
            "not production launch configuration\nmust not depend on `dfx`\nmust not use `--network ic`\nplaceholder principals\n",
        );
        write(
            root,
            "tools/sns/sns_init.io.local.yaml",
            r#"# not production-ready placeholder
name: "IO"
symbol: "IO"
ledger:
  transaction_fee_e8s: 10_000
governance:
  proposal_rejection_fee_e8s: 10_000_000_000
  initial_reward_rate_basis_points: 0
  final_reward_rate_basis_points: 0
  age_bonus_percentage: 0
neurons:
  jupiter_faucet_governance_neuron: {}
  jupiter_faucet_non_dissolvable_neuron: {}
  ordinary_user_neurons: {}
fallback_controllers: []
dapp_canisters:
  io_stream_manager: "TODO"
  io_nns_neuron_manager: "TODO"
  io_historian: "TODO"
  frontend: "TODO"
io_constructor_arg_mapping:
  io_stream_manager:
    icp_ledger_principal_text: "TODO"
    icp_index_principal_text: "TODO"
    io_ledger_principal_text: "TODO"
    io_index_principal_text: "TODO"
    io_sns_ledger_principal_text: "TODO"
    io_sns_index_principal_text: "TODO"
    sns_governance_principal_text: "TODO"
  io_nns_neuron_manager:
    nns_governance_principal_text: "TODO"
    icp_ledger_principal_text: "TODO"
    icp_index_principal_text: "TODO"
"#,
        );
        write(
            root,
            "tools/sns/official-sns-testing-notes.md",
            "optional local-only not part of `test_ci` not used by `verify_release` must not call mainnet dfx sns Do not use --network ic\n",
        );
        write(
            root,
            "tools/scripts/required-check",
            "#!/usr/bin/env bash\ncargo test\n",
        );
    }

    #[test]
    fn artifact_manifest_validation_accepts_good_manifest() {
        let root = temp_root("manifest-good");
        write_artifact_set(&root);
        verify_artifacts_at(&root).unwrap();
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn artifact_manifest_validation_rejects_wrong_hash() {
        let root = temp_root("manifest-wrong-hash");
        write_artifact_set(&root);
        write(
            &root,
            "release-artifacts/io_stream_manager.wasm.sha256",
            "0000000000000000000000000000000000000000000000000000000000000000  release-artifacts/io_stream_manager.wasm\n",
        );
        assert!(verify_artifacts_at(&root)
            .unwrap_err()
            .contains("hash mismatch"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn artifact_manifest_validation_rejects_missing_artifact() {
        let root = temp_root("manifest-missing-artifact");
        write_artifact_set(&root);
        fs::remove_file(root.join("release-artifacts/io_stream_manager.wasm")).unwrap();
        assert!(verify_artifacts_at(&root)
            .unwrap_err()
            .contains("missing artifact"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn install_args_validation_accepts_valid_local_args() {
        validate_stream_install_args_text(
            r#"(record {
              jupiter_faucet_principal_text = opt "aaaaa-aa";
              io_nns_neuron_manager_principal_text = null : opt text;
              icp_ledger_principal_text = null : opt text;
              icp_index_principal_text = null : opt text;
              io_ledger_principal_text = null : opt text;
              io_index_principal_text = null : opt text;
              io_sns_ledger_principal_text = null : opt text;
              io_sns_index_principal_text = null : opt text;
              sns_governance_principal_text = null : opt text;
            })"#,
            InstallArgsMode::Local,
        )
        .unwrap();
    }

    #[test]
    fn install_args_validation_accepts_local_sns_shaped_args() {
        validate_stream_install_args_text(
            r#"(record {
              jupiter_faucet_principal_text = opt "aaaaa-aa";
              io_nns_neuron_manager_principal_text = opt "oae4c-3iaaa-aaaar-qb5qq-cai";
              icp_ledger_principal_text = opt "bkyz2-fmaaa-aaaaa-qaaaq-cai";
              icp_index_principal_text = opt "bd3sg-teaaa-aaaaa-qaaba-cai";
              io_ledger_principal_text = opt "br5f7-7uaaa-aaaaa-qaaca-cai";
              io_index_principal_text = opt "be2us-64aaa-aaaaa-qaabq-cai";
              io_sns_ledger_principal_text = opt "bw4dl-smaaa-aaaaa-qaacq-cai";
              io_sns_index_principal_text = opt "b77ix-eeaaa-aaaaa-qaada-cai";
              sns_governance_principal_text = opt "by6od-j4aaa-aaaaa-qaadq-cai";
            })"#,
            InstallArgsMode::Local,
        )
        .unwrap();
        validate_nns_install_args_text(
            r#"(record {
              controller_canister_principal_text = "aaaaa-aa";
              two_year_nns_neuron_id = 42 : nat64;
              two_week_dissolve_seconds = 1_209_600 : nat64;
              io_stream_manager_principal_text = opt "oae4c-3iaaa-aaaar-qb5qq-cai";
              nns_governance_principal_text = opt "rrkah-fqaaa-aaaaa-aaaaq-cai";
              icp_ledger_principal_text = opt "ryjl3-tyaaa-aaaaa-aaaba-cai";
              icp_index_principal_text = opt "qhbym-qaaaa-aaaaa-aaafq-cai";
            })"#,
            InstallArgsMode::Local,
        )
        .unwrap();
    }

    #[test]
    fn install_args_validation_accepts_known_live_shaped_args() {
        validate_nns_install_args_text(
            r#"(record {
              controller_canister_principal_text = "oae4c-3iaaa-aaaar-qb5qq-cai";
              two_year_nns_neuron_id = 6_345_890_886_899_317_159 : nat64;
              two_week_dissolve_seconds = 1_209_600 : nat64;
              io_stream_manager_principal_text = null : opt text;
              nns_governance_principal_text = null : opt text;
              icp_ledger_principal_text = null : opt text;
            })"#,
            InstallArgsMode::Mainnet,
        )
        .unwrap();
    }

    #[test]
    fn install_args_validation_rejects_malformed_principal() {
        let err = validate_stream_install_args_text(
            r#"(record {
              jupiter_faucet_principal_text = opt "not-a-principal";
            })"#,
            InstallArgsMode::Local,
        )
        .unwrap_err();
        assert!(err.contains("invalid principal"));
    }

    #[test]
    fn install_args_validation_rejects_malformed_sns_principals() {
        let err = validate_stream_install_args_text(
            r#"(record {
              sns_governance_principal_text = opt "not-sns-governance";
            })"#,
            InstallArgsMode::Local,
        )
        .unwrap_err();
        assert!(err.contains("sns_governance_principal_text"));

        let err = validate_stream_install_args_text(
            r#"(record {
              io_sns_ledger_principal_text = opt "not-sns-ledger";
            })"#,
            InstallArgsMode::Local,
        )
        .unwrap_err();
        assert!(err.contains("io_sns_ledger_principal_text"));

        let err = validate_stream_install_args_text(
            r#"(record {
              io_sns_index_principal_text = opt "not-sns-index";
            })"#,
            InstallArgsMode::Local,
        )
        .unwrap_err();
        assert!(err.contains("io_sns_index_principal_text"));
    }

    #[test]
    fn install_args_validation_rejects_placeholder_in_mainnet_mode() {
        let err = validate_stream_install_args_text(
            r#"(record {
              jupiter_faucet_principal_text = opt "aaaaa-aa";
            })"#,
            InstallArgsMode::Mainnet,
        )
        .unwrap_err();
        assert!(err.contains("placeholder"));
    }

    #[test]
    fn sns_harness_check_accepts_fixture() {
        let root = temp_root("sns-harness-good");
        write_sns_harness_fixture(&root);
        check_sns_harness_at(&root).unwrap();
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn sns_harness_check_rejects_missing_fixture() {
        let root = temp_root("sns-harness-missing");
        write_sns_harness_fixture(&root);
        fs::remove_file(root.join("tools/sns/sns_init.io.local.yaml")).unwrap();
        assert!(check_sns_harness_at(&root)
            .unwrap_err()
            .contains("missing required file"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn sns_harness_check_rejects_network_ic_in_required_script() {
        let root = temp_root("sns-harness-network-ic");
        write_sns_harness_fixture(&root);
        write(
            &root,
            "tools/scripts/bad-required",
            "#!/usr/bin/env bash\ncargo run -- --network ic\n",
        );
        assert!(check_sns_harness_at(&root)
            .unwrap_err()
            .contains("--network ic"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn sns_harness_check_rejects_dfx_in_required_script() {
        let root = temp_root("sns-harness-dfx");
        write_sns_harness_fixture(&root);
        write(
            &root,
            "tools/scripts/bad-required",
            "#!/usr/bin/env bash\ndfx deploy\n",
        );
        assert!(check_sns_harness_at(&root).unwrap_err().contains("dfx"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn install_args_validation_rejects_missing_required_value() {
        let err = validate_nns_install_args_text(
            r#"(record {
              controller_canister_principal_text = "oae4c-3iaaa-aaaar-qb5qq-cai";
              two_week_dissolve_seconds = 1_209_600 : nat64;
            })"#,
            InstallArgsMode::Mainnet,
        )
        .unwrap_err();
        assert!(err.contains("missing required field two_year_nns_neuron_id"));
    }

    #[test]
    fn install_args_validation_rejects_unknown_mode() {
        assert!(InstallArgsMode::parse(Some("staging")).is_err());
    }

    #[test]
    fn did_surface_forbidden_method_list_catches_bad_did_text() {
        let bad = "service : (InitArgs) -> { debug_get_state : () -> (text) query; }";
        let forbidden = forbidden_did_methods(bad, STREAM_PRODUCTION_FORBIDDEN_DID);
        assert!(forbidden.iter().any(|item| item == "debug_"));
    }
}
