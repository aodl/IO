use candid::Principal;
use io_production_wiring::{
    template_paths, validate_template_text, DEV_MAINNET_FRONTEND_CANISTER_ID,
    DEV_MAINNET_HISTORIAN_CANISTER_ID, PRODUCTION_FRONTEND_CANISTER_ID,
    PRODUCTION_IO_HISTORIAN_CANISTER_ID, PRODUCTION_IO_NNS_NEURON_MANAGER_CANISTER_ID,
    PRODUCTION_IO_STREAM_MANAGER_CANISTER_ID, PROTECTED_IO_NEURON_OWNER_CANISTER,
    PROTECTED_IO_NNS_NEURON_ID,
};
use io_stable_schema::{accepts_schema_version, STABLE_SCHEMA_REGISTRY};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const RELEASE_PROFILE: &str = "release";
const WASM_TARGET: &str = "wasm32-unknown-unknown";
const MANIFEST_PATH: &str = "release-artifacts/manifest.json";
const KNOWN_TWO_YEAR_NNS_NEURON_ID: u64 = PROTECTED_IO_NNS_NEURON_ID;
const KNOWN_CONTROLLER_CANISTER_PRINCIPAL: &str = PROTECTED_IO_NEURON_OWNER_CANISTER;
const DEV_MAINNET_MODE: &str = "LegacyPhase1DevPublicShell";
const DEV_MAINNET_CONFIG_PATH: &str = "deploy/mainnet-dev/legacy-phase1/canister-ids.toml";
const DEV_MAINNET_README_PATH: &str = "deploy/mainnet-dev/legacy-phase1/README.md";
const DEV_MAINNET_STATUS_PATH: &str = "deploy/mainnet-dev/legacy-phase1/status.md";
const PRODUCTION_CANISTER_IDS_PATH: &str = "deploy/production-wiring/canister-ids.toml";
const PRODUCTION_MAPPING_PATHS: &[&str] = &[
    PRODUCTION_CANISTER_IDS_PATH,
    "deploy/production-wiring/README.md",
    "docs/operations/production-wiring.md",
    "docs/operations/mainnet-readiness.md",
    "docs/architecture/canister-roles.md",
    "README.md",
];

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

const HISTORIAN_PRODUCTION_FORBIDDEN_DID: &[&str] = &[
    "debug_",
    " get_all",
    " tick :",
    " process_stream_event :",
    " redeem :",
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

fn npm(args: &[&str]) -> Command {
    let mut c = Command::new("npm");
    c.args(args);
    c
}

fn dfx(args: &[&str]) -> Command {
    let mut c = Command::new("dfx");
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

fn parse_toml_string(text: &str, section: &str, key: &str) -> Result<String, String> {
    let mut current_section = "";
    for raw_line in text.lines() {
        let line = raw_line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            current_section = line[1..line.len() - 1].trim();
            continue;
        }
        if current_section != section {
            continue;
        }
        let Some((left, right)) = line.split_once('=') else {
            continue;
        };
        if left.trim() != key {
            continue;
        }
        let value = right.trim();
        if !(value.starts_with('"') && value.ends_with('"') && value.len() >= 2) {
            return Err(format!("{section}.{key}: expected quoted string"));
        }
        return Ok(value[1..value.len() - 1].to_string());
    }
    Err(format!("missing required field {section}.{key}"))
}

fn parse_toml_bool(text: &str, section: &str, key: &str) -> Result<bool, String> {
    let mut current_section = "";
    for raw_line in text.lines() {
        let line = raw_line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            current_section = line[1..line.len() - 1].trim();
            continue;
        }
        if current_section != section {
            continue;
        }
        let Some((left, right)) = line.split_once('=') else {
            continue;
        };
        if left.trim() != key {
            continue;
        }
        return match right.trim() {
            "true" => Ok(true),
            "false" => Ok(false),
            other => Err(format!("{section}.{key}: expected boolean, got {other:?}")),
        };
    }
    Err(format!("missing required field {section}.{key}"))
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum SimpleTomlValue {
    String(String),
    Bool(bool),
    Integer(u128),
}

type SimpleTomlDocument = BTreeMap<String, BTreeMap<String, SimpleTomlValue>>;

fn parse_simple_toml_document(path: &str, text: &str) -> Result<SimpleTomlDocument, String> {
    let mut doc = SimpleTomlDocument::new();
    let mut current_section: Option<String> = None;
    for (line_no, raw_line) in text.lines().enumerate() {
        let line_no = line_no + 1;
        let line = raw_line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            let section = line[1..line.len() - 1].trim();
            if section.is_empty() || section.contains('.') {
                return Err(format!(
                    "{path}:{line_no}: unsupported section name {section:?}"
                ));
            }
            current_section = Some(section.to_string());
            doc.entry(section.to_string()).or_default();
            continue;
        }
        let section = current_section
            .as_ref()
            .ok_or_else(|| format!("{path}:{line_no}: key outside a section"))?;
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| format!("{path}:{line_no}: expected key = value"))?;
        let key = key.trim();
        if key.is_empty() || key.contains('.') {
            return Err(format!("{path}:{line_no}: unsupported key name {key:?}"));
        }
        let value = parse_simple_toml_value(path, line_no, value.trim())?;
        let values = doc.entry(section.clone()).or_default();
        if values.insert(key.to_string(), value).is_some() {
            return Err(format!("{path}:{line_no}: duplicate key {section}.{key}"));
        }
    }
    Ok(doc)
}

fn parse_simple_toml_value(
    path: &str,
    line_no: usize,
    value: &str,
) -> Result<SimpleTomlValue, String> {
    if value.starts_with('"') {
        if !(value.ends_with('"') && value.len() >= 2) {
            return Err(format!("{path}:{line_no}: unterminated string"));
        }
        return Ok(SimpleTomlValue::String(
            value[1..value.len() - 1].to_string(),
        ));
    }
    match value {
        "true" => return Ok(SimpleTomlValue::Bool(true)),
        "false" => return Ok(SimpleTomlValue::Bool(false)),
        _ => {}
    }
    let digits = value.replace('_', "");
    if !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Ok(SimpleTomlValue::Integer(digits.parse::<u128>().map_err(
            |err| format!("{path}:{line_no}: integer does not fit u128: {err}"),
        )?));
    }
    Err(format!(
        "{path}:{line_no}: unsupported TOML value {value:?}"
    ))
}

fn require_simple_section<'a>(
    path: &str,
    doc: &'a SimpleTomlDocument,
    section: &str,
) -> Result<&'a BTreeMap<String, SimpleTomlValue>, String> {
    doc.get(section)
        .ok_or_else(|| format!("{path}: missing section [{section}]"))
}

fn require_simple_value<'a>(
    path: &str,
    doc: &'a SimpleTomlDocument,
    section: &str,
    key: &str,
) -> Result<&'a SimpleTomlValue, String> {
    require_simple_section(path, doc, section)?
        .get(key)
        .ok_or_else(|| format!("{path}: missing required field {section}.{key}"))
}

fn require_simple_string(
    path: &str,
    doc: &SimpleTomlDocument,
    section: &str,
    key: &str,
) -> Result<String, String> {
    match require_simple_value(path, doc, section, key)? {
        SimpleTomlValue::String(value) => Ok(value.clone()),
        other => Err(format!(
            "{path}: expected {section}.{key} to be string, got {other:?}"
        )),
    }
}

fn require_simple_bool(
    path: &str,
    doc: &SimpleTomlDocument,
    section: &str,
    key: &str,
) -> Result<bool, String> {
    match require_simple_value(path, doc, section, key)? {
        SimpleTomlValue::Bool(value) => Ok(*value),
        other => Err(format!(
            "{path}: expected {section}.{key} to be bool, got {other:?}"
        )),
    }
}

fn require_simple_u128(
    path: &str,
    doc: &SimpleTomlDocument,
    section: &str,
    key: &str,
) -> Result<u128, String> {
    match require_simple_value(path, doc, section, key)? {
        SimpleTomlValue::Integer(value) => Ok(*value),
        other => Err(format!(
            "{path}: expected {section}.{key} to be integer, got {other:?}"
        )),
    }
}

fn require_simple_u64(
    path: &str,
    doc: &SimpleTomlDocument,
    section: &str,
    key: &str,
) -> Result<u64, String> {
    match require_simple_value(path, doc, section, key)? {
        SimpleTomlValue::Integer(value) => (*value)
            .try_into()
            .map_err(|_| format!("{path}: {section}.{key} does not fit u64")),
        SimpleTomlValue::String(value) => value
            .replace('_', "")
            .parse::<u64>()
            .map_err(|err| format!("{path}: {section}.{key} is not a u64: {err}")),
        other => Err(format!(
            "{path}: expected {section}.{key} to be integer or numeric string, got {other:?}"
        )),
    }
}

fn require_toml_string(
    path: &str,
    text: &str,
    section: &str,
    key: &str,
    expected: &str,
) -> Result<(), String> {
    let actual = parse_toml_string(text, section, key)?;
    if actual != expected {
        return Err(format!(
            "{path}: expected {section}.{key} = {expected:?}, got {actual:?}"
        ));
    }
    Ok(())
}

fn require_toml_bool(
    path: &str,
    text: &str,
    section: &str,
    key: &str,
    expected: bool,
) -> Result<(), String> {
    let actual = parse_toml_bool(text, section, key)?;
    if actual != expected {
        return Err(format!(
            "{path}: expected {section}.{key} = {expected}, got {actual}"
        ));
    }
    Ok(())
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
    let historian_production_path = "canisters/io_historian/io_historian.did";
    let historian_debug_path = "canisters/io_historian/io_historian_debug.did";

    let stream_production = read_file(root, stream_production_path)?;
    let stream_debug = read_file(root, stream_debug_path)?;
    let nns_production = read_file(root, nns_production_path)?;
    let nns_debug = read_file(root, nns_debug_path)?;
    let historian_production = read_file(root, historian_production_path)?;
    let historian_debug = read_file(root, historian_debug_path)?;

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
    let historian_forbidden =
        forbidden_did_methods(&historian_production, HISTORIAN_PRODUCTION_FORBIDDEN_DID);
    if !historian_forbidden.is_empty() {
        return Err(format!(
            "{historian_production_path} contains forbidden production methods: {}",
            historian_forbidden.join(", ")
        ));
    }

    require_present(
        historian_production_path,
        &historian_production,
        &[
            "get_dashboard_state",
            "get_protocol_snapshot",
            "get_redemption_rate",
            "list_streams",
            "list_redemptions",
            "list_rewards",
            "list_nns_lifecycle_events",
            "get_index_health",
            "get_governance_summary",
            "get_release_artifacts",
            "get_canister_status_summary",
        ],
    )?;

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
    require_present(
        historian_debug_path,
        &historian_debug,
        &[
            "debug_clear",
            "debug_ingest_ledger_flow",
            "debug_ingest_stream_record",
            "debug_ingest_redemption_record",
            "debug_ingest_reward_record",
            "debug_ingest_index_health",
            "debug_ingest_governance_snapshot",
            "debug_ingest_canister_artifact_status",
        ],
    )?;
    check_historian_js_declaration_at(root)?;

    if check_wasm && root.join("release-artifacts").is_dir() {
        check_wasm_forbidden_methods(root)?;
    }

    Ok(())
}

fn parse_did_service_methods(text: &str) -> BTreeSet<String> {
    let service = text.split("service").last().unwrap_or(text);
    service
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            let (name, _) = trimmed.split_once(':')?;
            let name = name.trim().trim_matches('"');
            (!name.is_empty()
                && name
                    .chars()
                    .all(|ch| ch == '_' || ch.is_ascii_alphanumeric()))
            .then(|| name.to_string())
        })
        .collect()
}

fn parse_js_service_methods(text: &str) -> BTreeSet<String> {
    let service = text.split("IDL.Service({").last().unwrap_or(text);
    service
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if !trimmed.contains(": IDL.Func") {
                return None;
            }
            let (name, _) = trimmed.split_once(": IDL.Func")?;
            let name = name.trim().trim_matches('"').trim_matches('\'');
            (!name.is_empty()).then(|| name.to_string())
        })
        .collect()
}

fn check_historian_js_declaration_text(
    did_path: &str,
    did_text: &str,
    js_path: &str,
    js_text: &str,
    index_path: &str,
    index_text: &str,
) -> Result<(), String> {
    for (path, text) in [(js_path, js_text), (index_path, index_text)] {
        require_absent(
            path,
            text,
            &["debug_", "io_historian_debug", ".dfx", "src/declarations"],
        )?;
    }

    let did_methods = parse_did_service_methods(did_text);
    let js_methods = parse_js_service_methods(js_text);
    let missing = did_methods
        .difference(&js_methods)
        .cloned()
        .collect::<Vec<_>>();
    let extra = js_methods
        .difference(&did_methods)
        .cloned()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(format!(
            "{js_path} is missing historian production methods from {did_path}: {}",
            missing.join(", ")
        ));
    }
    if !extra.is_empty() {
        return Err(format!(
            "{js_path} contains methods absent from {did_path}: {}",
            extra.join(", ")
        ));
    }
    Ok(())
}

fn check_historian_js_declaration_at(root: &Path) -> Result<(), String> {
    let did_path = "canisters/io_historian/io_historian.did";
    let js_path = "canisters/frontend/web/declarations/io_historian/io_historian.did.js";
    let index_path = "canisters/frontend/web/declarations/io_historian/index.js";
    let did_text = read_file(root, did_path)?;
    let js_text = read_file(root, js_path)?;
    let index_text = read_file(root, index_path)?;
    check_historian_js_declaration_text(
        did_path,
        &did_text,
        js_path,
        &js_text,
        index_path,
        &index_text,
    )
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

fn check_sns_config_at(root: &Path) -> Result<(), String> {
    let readme = require_file(root, "tools/sns/README.md")?;
    require_present(
        "tools/sns/README.md",
        &readme,
        &[
            "official SNS compatibility package",
            "Layer 1",
            "Layer 2",
            "Layer 3",
            "Layer 4",
            "must not depend on `dfx`",
            "IO_TEST ledger is non-canonical",
        ],
    )?;

    for path in [
        "tools/sns/sns_init.io.template.yaml",
        "tools/sns/sns_init.io.local.yaml",
        "tools/sns/sns_init.io.testflight.template.yaml",
        "tools/sns/testflight/sns_init.testflight.template.yaml",
    ] {
        let text = require_file(root, path)?;
        require_present(
            path,
            &text,
            &[
                "name: \"IO\"",
                "symbol: \"IO\"",
                "transaction_fee_e8s",
                "proposal_rejection_fee_e8s",
                "fallback_controller_principals",
                "dapp_canisters",
                "io_stream_manager",
                "io_nns_neuron_manager",
                "io_historian",
                "frontend",
                "TODO",
                "placeholder",
            ],
        )?;
        require_absent(path, &text, &["--network ic"])?;
    }

    let local = require_file(root, "tools/sns/sns_init.io.local.yaml")?;
    require_present(
        "tools/sns/sns_init.io.local.yaml",
        &local,
        &[
            "TODO_LOCAL_IO_STREAM_MANAGER_CANISTER_PLACEHOLDER",
            "TODO_LOCAL_IO_NNS_NEURON_MANAGER_CANISTER_PLACEHOLDER",
            "TODO_LOCAL_FALLBACK_CONTROLLER_PRINCIPAL_PLACEHOLDER",
            "TODO_LOCAL_SNS_LEDGER_PLACEHOLDER",
            "TODO_LOCAL_SNS_INDEX_PLACEHOLDER",
            "TODO_LOCAL_SNS_GOVERNANCE_PLACEHOLDER",
            "IO_TEST ledger is non-canonical",
        ],
    )?;
    require_absent(
        "tools/sns/sns_init.io.local.yaml",
        &local,
        &["ryjl3-tyaaa-aaaaa-aaaba-cai", "rrkah-fqaaa-aaaaa-aaaaq-cai"],
    )?;

    let testflight = require_file(root, "tools/sns/sns_init.io.testflight.template.yaml")?;
    require_present(
        "tools/sns/sns_init.io.testflight.template.yaml",
        &testflight,
        &[
            "TODO_TESTFLIGHT_FALLBACK_CONTROLLER_PRINCIPAL_PLACEHOLDER",
            "TODO_TESTFLIGHT_IO_STREAM_MANAGER_CANISTER_PLACEHOLDER",
            "TODO_FINAL_TOKENOMICS",
            "TODO_FINAL_SWAP_PARAMETERS",
            "TODO_FINAL_DEVELOPER_NEURONS",
            "TODO_FINAL_TREASURY_DISTRIBUTION",
            "TODO_FINAL_LOGO_URL_SUMMARY",
            "TODO_FINAL_SNS_PROPOSAL_FORUM_URL",
        ],
    )?;

    check_required_executable_scripts_at(root)?;
    Ok(())
}

fn check_sns_official_testing_at(root: &Path) -> Result<(), String> {
    let doc = require_file(root, "docs/operations/official-sns-testing.md")?;
    require_present(
        "docs/operations/official-sns-testing.md",
        &doc,
        &[
            "We currently run SNS-shaped mock/PocketIC tests.",
            "We do not currently run the official SNS launch locally in required CI.",
            "Official SNS testing is optional and heavier.",
            "current official ICP/DFINITY SNS testing documentation is the source of truth",
            "historical standalone `dfinity/sns-testing` repository is deprecated",
            "The official SNS launch path may require `dfx sns`",
            "not part of required IO workflows",
            "SNS testflight is a future manual/mainnet rehearsal.",
            "IO's canonical IO ledger should be the SNS ledger; any IO_TEST ledger is non-canonical.",
            "The existing canister that owns IO NNS neuron 6345890886899317159 is not touched by these tests.",
            "Layer 1",
            "Layer 2",
            "Layer 3",
            "Layer 4",
        ],
    )?;

    let local_doc = require_file(root, "docs/operations/local-sns-testing.md")?;
    require_present(
        "docs/operations/local-sns-testing.md",
        &local_doc,
        &[
            "We currently run SNS-shaped mock/PocketIC tests.",
            "not official SNS launch tests",
            "not SNS-W",
            "not decentralization swap",
            "not mainnet testflight",
        ],
    )?;

    let scripts = [
        "tools/sns-testing/check-prereqs.sh",
        "tools/sns-testing/deploy-io-dapp-local.sh",
        "tools/sns-testing/run-local-sns-testing.sh",
        "tools/sns-testing/validate-local-sns-config.sh",
    ];
    for path in scripts {
        let text = require_file(root, path)?;
        require_present(path, &text, &["optional", "local"])?;
        require_absent(path, &text, &["--network ic"])?;
    }
    let deploy_script = require_file(root, "tools/sns-testing/deploy-io-dapp-local.sh")?;
    require_absent(
        "tools/sns-testing/deploy-io-dapp-local.sh",
        &deploy_script,
        &["dfx start", "dfx replica"],
    )?;

    let testflight = require_file(root, "tools/sns/testflight/README.md")?;
    require_present(
        "tools/sns/testflight/README.md",
        &testflight,
        &[
            "manual",
            "mainnet",
            "not CI",
            "not a real launch",
            "no real swap",
        ],
    )?;
    Ok(())
}

fn check_sns_launch_readiness_at(root: &Path, strict: bool) -> Result<usize, String> {
    let checklist = require_file(root, "tools/sns/launch-readiness.toml")?;
    require_present(
        "tools/sns/launch-readiness.toml",
        &checklist,
        &[
            "[source_open]",
            "[reproducible_builds]",
            "[security_review]",
            "[sns_config_validated]",
            "[local_sns_testing_rehearsal]",
            "[mainnet_testflight]",
            "[app_canisters_stable_on_mainnet]",
            "[nns_root_co_controller_step_planned]",
            "[fallback_controllers_defined]",
            "[dapp_canisters_listed]",
            "[all_upgrades_tested_via_sns_proposal]",
            "[frontend_sns_integration_tested]",
            "[cycles_management_strategy]",
            "[custom_domain_frontend_plan]",
            "[audit_package]",
        ],
    )?;

    let incomplete = checklist
        .lines()
        .filter(|line| line.trim() == "status = \"incomplete\"")
        .count();
    if incomplete == 0 {
        return Err("tools/sns/launch-readiness.toml must mark incomplete items explicitly".into());
    }
    if strict && incomplete > 0 {
        return Err(format!(
            "SNS launch readiness has {incomplete} incomplete item(s)"
        ));
    }
    Ok(incomplete)
}

fn check_local_sns_rehearsal_at(root: &Path) -> Result<(), String> {
    let readme = require_file(root, "deploy/local-sns-rehearsal/README.md")?;
    require_present(
        "deploy/local-sns-rehearsal/README.md",
        &readme,
        &[
            "local-only",
            "real SNS-created IO ledger/index/governance/root stack",
            "not final tokenomics",
            "not a mainnet SNS proposal",
            "not required CI",
            "Do not use `--network ic`",
            "protocol reserve",
            "reserve-to-user transfer",
            "user-to-reserve transfer",
            "validate_local_sns_rehearsal",
            "validate_local_sns_ledger",
            "validate_local_sns_scripts",
            "Human-readable local evidence-derived wiring",
            "Not accepted by production wiring validators",
            "Do not use as install args",
        ],
    )?;

    let sns_init = require_file(
        root,
        "deploy/local-sns-rehearsal/sns_init.local.template.yaml",
    )?;
    require_present(
        "deploy/local-sns-rehearsal/sns_init.local.template.yaml",
        &sns_init,
        &[
            "Local-only",
            "Not final tokenomics",
            "Not a mainnet SNS proposal",
            "fallback_controller_principals",
            "dapp_canisters",
            "Token:",
            "symbol: \"IO\"",
            "transaction_fee",
            "Distribution:",
            "protocol_reserve",
            "Swap:",
            "archive_options",
            "issuance_model: \"protocol reserve transfer\"",
            "redemption_model: \"user transfer back to protocol reserve\"",
            "io_test_ledger_role: \"non-canonical staging only\"",
            "{{",
        ],
    )?;
    require_absent(
        "deploy/local-sns-rehearsal/sns_init.local.template.yaml",
        &sns_init,
        &[
            "--network ic",
            DEV_MAINNET_FRONTEND_CANISTER_ID,
            DEV_MAINNET_HISTORIAN_CANISTER_ID,
            PROTECTED_IO_NEURON_OWNER_CANISTER,
        ],
    )?;

    let evidence_template = require_file(
        root,
        "deploy/local-sns-rehearsal/canister-ids.local.example.toml",
    )?;
    require_present(
        "deploy/local-sns-rehearsal/canister-ids.local.example.toml",
        &evidence_template,
        &[
            "network = \"local\"",
            "source = \"official-local-sns-rehearsal\"",
            "[sns_canisters]",
            "root",
            "governance",
            "ledger",
            "index",
            "swap",
            "archive",
            "[expected_local_sns_config]",
            "transaction_fee_e8s",
            "total_supply_e8s",
            "[ledger_evidence]",
            "transaction_fee_e8s",
            "total_supply_e8s",
            "protocol_reserve_balance_e8s",
            "reserve_transfer_amount_e8s",
            "redemption_return_amount_e8s",
            "bad_fee_error_observed = true",
            "insufficient_funds_error_observed = true",
            "duplicate_transfer_observed = true",
            "duplicate_block_verified = true",
            "index_account_history_observed = true",
            "[issuance_model]",
            "resolved_as = \"protocol_reserve_transfer\"",
            "minting_assumed = false",
            "total_supply_constant_across_issuance_redemption = true",
        ],
    )?;
    require_absent(
        "deploy/local-sns-rehearsal/canister-ids.local.example.toml",
        &evidence_template,
        &["--network ic"],
    )?;

    for path in [
        "deploy/local-sns-rehearsal/runbook.sh",
        "deploy/local-sns-rehearsal/scripts/00-check-prereqs.sh",
        "deploy/local-sns-rehearsal/scripts/01-render-sns-init.sh",
        "deploy/local-sns-rehearsal/scripts/02-record-canister-ids.sh",
        "deploy/local-sns-rehearsal/scripts/03-capture-ledger-evidence.sh",
        "deploy/local-sns-rehearsal/scripts/04-render-local-wiring.sh",
        "deploy/local-sns-rehearsal/scripts/05-validate-evidence.sh",
    ] {
        let text = require_file(root, path)?;
        require_present(
            path,
            &text,
            &[
                "IO_LOCAL_SNS_REHEARSAL_ACK",
                "local-only",
                "require_local_script_guard",
            ],
        )?;
        require_absent(path, &text, &["dfx start"])?;
    }

    let commands = require_file(root, "deploy/local-sns-rehearsal/commands.local.example.md")?;
    require_present(
        "deploy/local-sns-rehearsal/commands.local.example.md",
        &commands,
        &[
            "Local-only",
            "icrc1_symbol",
            "icrc1_fee",
            "icrc1_total_supply",
            "icrc1_balance_of",
            "icrc1_transfer",
            "get_account_transactions",
            "governance",
            "root",
            "IO_LOCAL_SNS_REHEARSAL_ACK=local-only",
        ],
    )?;
    require_absent(
        "deploy/local-sns-rehearsal/commands.local.example.md",
        &commands,
        &[
            DEV_MAINNET_FRONTEND_CANISTER_ID,
            DEV_MAINNET_HISTORIAN_CANISTER_ID,
        ],
    )?;

    for path in [
        "docs/operations/sns-testing-layers.md",
        "docs/operations/official-local-sns-rehearsal.md",
        "docs/operations/mainnet-readiness.md",
    ] {
        let text = require_file(root, path)?;
        require_present(
            path,
            &text,
            &[
                "real SNS-created",
                "SNS-W",
                "IO_TEST",
                "non-canonical",
                "protocol reserve",
                "not launched on mainnet",
            ],
        )?;
    }

    Ok(())
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), String> {
    fs::create_dir_all(dst).map_err(|err| format!("{}: {err}", dst.display()))?;
    for entry in fs::read_dir(src).map_err(|err| format!("{}: {err}", src.display()))? {
        let entry = entry.map_err(|err| format!("{}: {err}", src.display()))?;
        let file_type = entry
            .file_type()
            .map_err(|err| format!("{}: {err}", entry.path().display()))?;
        let target = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_recursive(&entry.path(), &target)?;
        } else if file_type.is_file() {
            fs::copy(entry.path(), &target)
                .map_err(|err| format!("{}: {err}", target.display()))?;
            let permissions = entry
                .metadata()
                .map_err(|err| format!("{}: {err}", entry.path().display()))?
                .permissions();
            fs::set_permissions(&target, permissions)
                .map_err(|err| format!("{}: {err}", target.display()))?;
        }
    }
    Ok(())
}

fn temp_root_for_command(name: &str) -> Result<PathBuf, String> {
    let root = env::temp_dir().join(format!("io-xtask-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).map_err(|err| format!("{}: {err}", root.display()))?;
    Ok(root)
}

fn write_text(path: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| format!("{}: {err}", parent.display()))?;
    }
    fs::write(path, text).map_err(|err| format!("{}: {err}", path.display()))
}

fn run_rehearsal_script(
    runbook: &Path,
    args: &[&str],
    xtask: &Path,
    expect_success: bool,
) -> Result<String, String> {
    let output = Command::new(runbook)
        .args(args)
        .env("IO_LOCAL_SNS_REHEARSAL_ACK", "local-only")
        .env("IO_LOCAL_SNS_REHEARSAL_XTASK", xtask)
        .output()
        .map_err(|err| format!("{} {:?}: {err}", runbook.display(), args))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");
    if output.status.success() != expect_success {
        return Err(format!(
            "{} {:?}: expected success={expect_success}, got status {:?}\n{}",
            runbook.display(),
            args,
            output.status.code(),
            combined
        ));
    }
    Ok(combined)
}

fn run_rehearsal_script_without_ack(runbook: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new(runbook)
        .args(args)
        .env_remove("IO_LOCAL_SNS_REHEARSAL_ACK")
        .output()
        .map_err(|err| format!("{} {:?}: {err}", runbook.display(), args))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");
    if output.status.success() {
        return Err(format!(
            "{} {:?}: missing ACK unexpectedly succeeded\n{}",
            runbook.display(),
            args,
            combined
        ));
    }
    Ok(combined)
}

fn fixture_local_vars(
    io_stream_manager: &str,
    io_nns_neuron_manager: &str,
    io_historian: &str,
    frontend: &str,
) -> String {
    format!(
        r#"[local]
fallback_controller_principal = "a3shf-5eaaa-aaaaa-qaafa-cai"
io_stream_manager_canister = "{io_stream_manager}"
io_nns_neuron_manager_canister = "{io_nns_neuron_manager}"
io_historian_canister = "{io_historian}"
frontend_canister = "{frontend}"
developer_neuron_principal = "bkyz2-fmaaa-aaaaa-qaaaq-cai"
protocol_reserve_principal = "bd3sg-teaaa-aaaaa-qaaba-cai"
archive_controller_principal = "br5f7-7uaaa-aaaaa-qaaca-cai"
logo_url = "http://127.0.0.1:4943/local-io-logo.png"
token_logo_url = "http://127.0.0.1:4943/local-io-token-logo.png"

[expected_local_sns_config]
token_symbol = "IO"
transaction_fee_e8s = 10_000
total_supply_e8s = 100_000_000_000_000
protocol_reserve_initial_balance_e8s = 60_000_000_000_000
"#
    )
}

fn completed_local_sns_evidence() -> String {
    r#"[mode]
network = "local"
source = "official-local-sns-rehearsal"
dfx_sns = "manual-local-only"
io_protocol_live = false
sns_io_ledger_mainnet_launched = false

[expected_local_sns_config]
token_symbol = "IO"
transaction_fee_e8s = 10000
total_supply_e8s = 100000000000000

[sns_canisters]
root = "bkyz2-fmaaa-aaaaa-qaaaq-cai"
governance = "bd3sg-teaaa-aaaaa-qaaba-cai"
ledger = "br5f7-7uaaa-aaaaa-qaaca-cai"
index = "be2us-64aaa-aaaaa-qaabq-cai"
swap = "bw4dl-smaaa-aaaaa-qaacq-cai"
archive = "by6od-j4aaa-aaaaa-qaadq-cai"

[io_dapp_canisters]
io_stream_manager = "avqkn-guaaa-aaaaa-qaaea-cai"
io_nns_neuron_manager = "aax3a-h4aaa-aaaaa-qaahq-cai"
io_historian = "ajuq4-ruaaa-aaaaa-qaaga-cai"
frontend = "b77ix-eeaaa-aaaaa-qaada-cai"

[ledger_evidence]
token_symbol = "IO"
transaction_fee_e8s = 10000
total_supply_e8s = 100000000000000
protocol_reserve_account_owner = "a3shf-5eaaa-aaaaa-qaafa-cai"
protocol_reserve_subaccount_hex = "none"
protocol_reserve_balance_e8s = 60000000000000
reserve_transfer_block_index = 1
redemption_return_block_index = 2
reserve_transfer_amount_e8s = 100000000
redemption_return_amount_e8s = 100000000
bad_fee_error_observed = true
insufficient_funds_error_observed = true
duplicate_transfer_observed = true
duplicate_block_verified = true
duplicate_of_block_index = 1
index_account_history_observed = true
index_history_order = "descending"
index_lag_or_archive_required_observed = "not-observed"

[governance_evidence]
governance_available = true
root_available = true
swap_available = true
dapp_controller_state_checked = true
governance_upgrade_proposal_tested = false
governance_upgrade_gap = "local tooling did not support upgrade proposal in this run"

[issuance_model]
resolved_as = "protocol_reserve_transfer"
minting_assumed = false
treasury_transfer_assumed = false
total_supply_constant_across_issuance_redemption = true

[protected]
must_not_touch_neuron_owner_canister = "oae4c-3iaaa-aaaar-qb5qq-cai"
must_not_touch_io_nns_neuron_id = "6345890886899317159"
"#
    .to_string()
}

fn validate_local_sns_scripts_at(root: &Path) -> Result<(), String> {
    let temp = temp_root_for_command("local-sns-scripts")?;
    let temp_rehearsal = temp.join("deploy/local-sns-rehearsal");
    copy_dir_recursive(&root.join("deploy/local-sns-rehearsal"), &temp_rehearsal)?;

    let runbook = temp_rehearsal.join("runbook.sh");
    let xtask = env::current_exe().map_err(|err| format!("current exe: {err}"))?;
    let local_vars = temp_rehearsal.join("local-vars.toml");
    write_text(
        &local_vars,
        &fixture_local_vars(
            "avqkn-guaaa-aaaaa-qaaea-cai",
            "aax3a-h4aaa-aaaaa-qaahq-cai",
            "ajuq4-ruaaa-aaaaa-qaaga-cai",
            "b77ix-eeaaa-aaaaa-qaada-cai",
        ),
    )?;

    run_rehearsal_script(&runbook, &["render-sns-init"], &xtask, true)?;
    let rendered_sns_path = temp_rehearsal.join("generated/sns_init.local.yaml");
    let rendered_sns = fs::read_to_string(&rendered_sns_path)
        .map_err(|err| format!("{}: {err}", rendered_sns_path.display()))?;
    require_absent(
        &rendered_sns_path.display().to_string(),
        &rendered_sns,
        &[
            "TODO_LOCAL",
            "{{",
            "}}",
            "--network ic",
            PROTECTED_IO_NEURON_OWNER_CANISTER,
            &PROTECTED_IO_NNS_NEURON_ID.to_string(),
            DEV_MAINNET_FRONTEND_CANISTER_ID,
            DEV_MAINNET_HISTORIAN_CANISTER_ID,
            "ryjl3-tyaaa-aaaaa-aaaba-cai",
            "qhbym-qaaaa-aaaaa-aaafq-cai",
            "rrkah-fqaaa-aaaaa-aaaaq-cai",
        ],
    )?;

    run_rehearsal_script(&runbook, &["record-ids"], &xtask, true)?;
    let evidence_path = temp_rehearsal.join("canister-ids.local.toml");
    write_text(&evidence_path, &completed_local_sns_evidence())?;

    let capture_output = run_rehearsal_script(&runbook, &["capture-evidence"], &xtask, true)?;
    require_present(
        "capture-evidence output",
        &capture_output,
        &["--network local"],
    )?;
    run_rehearsal_script(&runbook, &["render-wiring"], &xtask, true)?;
    run_rehearsal_script(&runbook, &["validate"], &xtask, true)?;

    let wiring_path = temp_rehearsal.join("generated/local-production-wiring.toml");
    let wiring = fs::read_to_string(&wiring_path)
        .map_err(|err| format!("{}: {err}", wiring_path.display()))?;
    require_present(
        &wiring_path.display().to_string(),
        &wiring,
        &[
            "Human-readable local evidence-derived wiring",
            "Not accepted by production_wiring validators",
            "Do not use as install args",
            "io_ledger = \"br5f7-7uaaa-aaaaa-qaaca-cai\"",
            "io_index = \"be2us-64aaa-aaaaa-qaabq-cai\"",
            "production_active = false",
        ],
    )?;
    require_absent(
        &wiring_path.display().to_string(),
        &wiring,
        &[
            "IO_TEST",
            PROTECTED_IO_NEURON_OWNER_CANISTER,
            &PROTECTED_IO_NNS_NEURON_ID.to_string(),
            DEV_MAINNET_FRONTEND_CANISTER_ID,
            DEV_MAINNET_HISTORIAN_CANISTER_ID,
            "ryjl3-tyaaa-aaaaa-aaaba-cai",
            "qhbym-qaaaa-aaaaa-aaafq-cai",
            "rrkah-fqaaa-aaaaa-aaaaq-cai",
            "production_active = true",
        ],
    )?;

    let err = run_rehearsal_script_without_ack(&runbook, &["render-sns-init"])?;
    require_present("missing ACK error", &err, &["IO_LOCAL_SNS_REHEARSAL_ACK"])?;
    let err = run_rehearsal_script(
        &runbook,
        &["render-sns-init", "--network", "ic"],
        &xtask,
        false,
    )?;
    require_present(
        "mainnet argument error",
        &err,
        &["refusing mainnet-like argument"],
    )?;

    for (name, text, needle) in [
        (
            "protected-canister",
            fixture_local_vars(
                PROTECTED_IO_NEURON_OWNER_CANISTER,
                "aax3a-h4aaa-aaaaa-qaahq-cai",
                "ajuq4-ruaaa-aaaaa-qaaga-cai",
                "b77ix-eeaaa-aaaaa-qaada-cai",
            ),
            "protected value",
        ),
        (
            "protected-neuron",
            fixture_local_vars(
                &PROTECTED_IO_NNS_NEURON_ID.to_string(),
                "aax3a-h4aaa-aaaaa-qaahq-cai",
                "ajuq4-ruaaa-aaaaa-qaaga-cai",
                "b77ix-eeaaa-aaaaa-qaada-cai",
            ),
            "protected value",
        ),
        (
            "mainnet-icp-ledger",
            fixture_local_vars(
                "ryjl3-tyaaa-aaaaa-aaaba-cai",
                "aax3a-h4aaa-aaaaa-qaahq-cai",
                "ajuq4-ruaaa-aaaaa-qaaga-cai",
                "b77ix-eeaaa-aaaaa-qaada-cai",
            ),
            "mainnet/prior canister",
        ),
        (
            "placeholder",
            fixture_local_vars(
                "TODO_LOCAL_IO_STREAM_MANAGER_CANISTER",
                "aax3a-h4aaa-aaaaa-qaahq-cai",
                "ajuq4-ruaaa-aaaaa-qaaga-cai",
                "b77ix-eeaaa-aaaaa-qaada-cai",
            ),
            "placeholder local variable",
        ),
    ] {
        write_text(&local_vars, &text)?;
        let err = run_rehearsal_script(&runbook, &["render-sns-init"], &xtask, false)?;
        require_present(&format!("{name} error"), &err, &[needle])?;
    }

    let _ = fs::remove_dir_all(&temp);
    Ok(())
}

#[derive(Clone, Debug)]
struct LocalSnsEvidence {
    mode: LocalSnsModeEvidence,
    expected: LocalSnsExpectedConfig,
    sns_canisters: LocalSnsCanisters,
    io_dapp_canisters: LocalSnsIoDappCanisters,
    ledger: LocalSnsLedgerEvidence,
    governance: LocalSnsGovernanceEvidence,
    issuance: LocalSnsIssuanceModel,
}

#[derive(Clone, Debug)]
struct LocalSnsModeEvidence {
    network: String,
    source: String,
    dfx_sns: String,
    io_protocol_live: bool,
    sns_io_ledger_mainnet_launched: bool,
}

#[derive(Clone, Debug)]
struct LocalSnsExpectedConfig {
    token_symbol: String,
    transaction_fee_e8s: u128,
    total_supply_e8s: u128,
}

#[derive(Clone, Debug)]
struct LocalSnsCanisters {
    root: Principal,
    governance: Principal,
    ledger: Principal,
    index: Principal,
    swap: Principal,
    archive: Option<Principal>,
}

#[derive(Clone, Debug)]
struct LocalSnsIoDappCanisters {
    io_stream_manager: Principal,
    io_nns_neuron_manager: Principal,
    io_historian: Principal,
    frontend: Principal,
}

#[derive(Clone, Debug)]
struct LocalSnsLedgerEvidence {
    token_symbol: String,
    transaction_fee_e8s: u128,
    total_supply_e8s: u128,
    protocol_reserve_account_owner: Principal,
    protocol_reserve_subaccount_hex: Option<String>,
    protocol_reserve_balance_e8s: u128,
    reserve_transfer_block_index: u64,
    redemption_return_block_index: u64,
    reserve_transfer_amount_e8s: u128,
    redemption_return_amount_e8s: u128,
    bad_fee_error_observed: bool,
    insufficient_funds_error_observed: bool,
    duplicate_transfer_observed: bool,
    duplicate_block_verified: bool,
    duplicate_of_block_index: Option<u64>,
    index_account_history_observed: bool,
    index_history_order: String,
    index_lag_or_archive_required_observed: String,
}

#[derive(Clone, Debug)]
struct LocalSnsGovernanceEvidence {
    governance_available: bool,
    root_available: bool,
    swap_available: bool,
    dapp_controller_state_checked: bool,
    governance_upgrade_proposal_tested: bool,
    governance_upgrade_gap: String,
}

#[derive(Clone, Debug)]
struct LocalSnsIssuanceModel {
    resolved_as: String,
    minting_assumed: bool,
    treasury_transfer_assumed: bool,
    total_supply_constant_across_issuance_redemption: bool,
}

const LOCAL_SNS_MAINNET_CANISTER_IDS: &[&str] = &[
    DEV_MAINNET_FRONTEND_CANISTER_ID,
    DEV_MAINNET_HISTORIAN_CANISTER_ID,
    "ryjl3-tyaaa-aaaaa-aaaba-cai",
    "qhbym-qaaaa-aaaaa-aaafq-cai",
    "rrkah-fqaaa-aaaaa-aaaaq-cai",
    "r7inp-6aaaa-aaaaa-aaabq-cai",
    "qaa6y-5yaaa-aaaaa-aaafa-cai",
    "qjdve-lqaaa-aaaaa-aaaeq-cai",
    "renrk-eyaaa-aaaaa-aaada-cai",
];

fn parse_local_sns_evidence(path: &str, text: &str) -> Result<LocalSnsEvidence, String> {
    require_absent(
        path,
        text,
        &["TODO_", "{{", "}}", "--network ic", "-n ic", "IO_TEST"],
    )?;
    let doc = parse_simple_toml_document(path, text)?;
    for section in doc.keys() {
        match section.as_str() {
            "mode"
            | "expected_local_sns_config"
            | "sns_canisters"
            | "io_dapp_canisters"
            | "ledger_evidence"
            | "governance_evidence"
            | "issuance_model"
            | "protected" => {}
            _ => return Err(format!("{path}: unexpected section [{section}]")),
        }
    }
    let evidence = LocalSnsEvidence {
        mode: LocalSnsModeEvidence {
            network: require_simple_string(path, &doc, "mode", "network")?,
            source: require_simple_string(path, &doc, "mode", "source")?,
            dfx_sns: require_simple_string(path, &doc, "mode", "dfx_sns")?,
            io_protocol_live: require_simple_bool(path, &doc, "mode", "io_protocol_live")?,
            sns_io_ledger_mainnet_launched: require_simple_bool(
                path,
                &doc,
                "mode",
                "sns_io_ledger_mainnet_launched",
            )?,
        },
        expected: LocalSnsExpectedConfig {
            token_symbol: require_simple_string(
                path,
                &doc,
                "expected_local_sns_config",
                "token_symbol",
            )?,
            transaction_fee_e8s: require_simple_u128(
                path,
                &doc,
                "expected_local_sns_config",
                "transaction_fee_e8s",
            )?,
            total_supply_e8s: require_simple_u128(
                path,
                &doc,
                "expected_local_sns_config",
                "total_supply_e8s",
            )?,
        },
        sns_canisters: LocalSnsCanisters {
            root: parse_required_principal(path, &doc, "sns_canisters", "root")?,
            governance: parse_required_principal(path, &doc, "sns_canisters", "governance")?,
            ledger: parse_required_principal(path, &doc, "sns_canisters", "ledger")?,
            index: parse_required_principal(path, &doc, "sns_canisters", "index")?,
            swap: parse_required_principal(path, &doc, "sns_canisters", "swap")?,
            archive: parse_optional_principal_string(path, &doc, "sns_canisters", "archive")?,
        },
        io_dapp_canisters: LocalSnsIoDappCanisters {
            io_stream_manager: parse_required_principal(
                path,
                &doc,
                "io_dapp_canisters",
                "io_stream_manager",
            )?,
            io_nns_neuron_manager: parse_required_principal(
                path,
                &doc,
                "io_dapp_canisters",
                "io_nns_neuron_manager",
            )?,
            io_historian: parse_required_principal(
                path,
                &doc,
                "io_dapp_canisters",
                "io_historian",
            )?,
            frontend: parse_required_principal(path, &doc, "io_dapp_canisters", "frontend")?,
        },
        ledger: LocalSnsLedgerEvidence {
            token_symbol: require_simple_string(path, &doc, "ledger_evidence", "token_symbol")?,
            transaction_fee_e8s: require_simple_u128(
                path,
                &doc,
                "ledger_evidence",
                "transaction_fee_e8s",
            )?,
            total_supply_e8s: require_simple_u128(
                path,
                &doc,
                "ledger_evidence",
                "total_supply_e8s",
            )?,
            protocol_reserve_account_owner: parse_required_principal(
                path,
                &doc,
                "ledger_evidence",
                "protocol_reserve_account_owner",
            )?,
            protocol_reserve_subaccount_hex: parse_subaccount_hex(
                path,
                &doc,
                "ledger_evidence",
                "protocol_reserve_subaccount_hex",
            )?,
            protocol_reserve_balance_e8s: require_simple_u128(
                path,
                &doc,
                "ledger_evidence",
                "protocol_reserve_balance_e8s",
            )?,
            reserve_transfer_block_index: require_simple_u64(
                path,
                &doc,
                "ledger_evidence",
                "reserve_transfer_block_index",
            )?,
            redemption_return_block_index: require_simple_u64(
                path,
                &doc,
                "ledger_evidence",
                "redemption_return_block_index",
            )?,
            reserve_transfer_amount_e8s: require_simple_u128(
                path,
                &doc,
                "ledger_evidence",
                "reserve_transfer_amount_e8s",
            )?,
            redemption_return_amount_e8s: require_simple_u128(
                path,
                &doc,
                "ledger_evidence",
                "redemption_return_amount_e8s",
            )?,
            bad_fee_error_observed: require_simple_bool(
                path,
                &doc,
                "ledger_evidence",
                "bad_fee_error_observed",
            )?,
            insufficient_funds_error_observed: require_simple_bool(
                path,
                &doc,
                "ledger_evidence",
                "insufficient_funds_error_observed",
            )?,
            duplicate_transfer_observed: require_simple_bool(
                path,
                &doc,
                "ledger_evidence",
                "duplicate_transfer_observed",
            )?,
            duplicate_block_verified: require_simple_bool(
                path,
                &doc,
                "ledger_evidence",
                "duplicate_block_verified",
            )?,
            duplicate_of_block_index: parse_optional_u64(
                path,
                &doc,
                "ledger_evidence",
                "duplicate_of_block_index",
            )?,
            index_account_history_observed: require_simple_bool(
                path,
                &doc,
                "ledger_evidence",
                "index_account_history_observed",
            )?,
            index_history_order: require_simple_string(
                path,
                &doc,
                "ledger_evidence",
                "index_history_order",
            )?,
            index_lag_or_archive_required_observed: require_simple_string(
                path,
                &doc,
                "ledger_evidence",
                "index_lag_or_archive_required_observed",
            )?,
        },
        governance: LocalSnsGovernanceEvidence {
            governance_available: require_simple_bool(
                path,
                &doc,
                "governance_evidence",
                "governance_available",
            )?,
            root_available: require_simple_bool(
                path,
                &doc,
                "governance_evidence",
                "root_available",
            )?,
            swap_available: require_simple_bool(
                path,
                &doc,
                "governance_evidence",
                "swap_available",
            )?,
            dapp_controller_state_checked: require_simple_bool(
                path,
                &doc,
                "governance_evidence",
                "dapp_controller_state_checked",
            )?,
            governance_upgrade_proposal_tested: require_simple_bool(
                path,
                &doc,
                "governance_evidence",
                "governance_upgrade_proposal_tested",
            )?,
            governance_upgrade_gap: require_simple_string(
                path,
                &doc,
                "governance_evidence",
                "governance_upgrade_gap",
            )?,
        },
        issuance: LocalSnsIssuanceModel {
            resolved_as: require_simple_string(path, &doc, "issuance_model", "resolved_as")?,
            minting_assumed: require_simple_bool(path, &doc, "issuance_model", "minting_assumed")?,
            treasury_transfer_assumed: require_simple_bool(
                path,
                &doc,
                "issuance_model",
                "treasury_transfer_assumed",
            )?,
            total_supply_constant_across_issuance_redemption: require_simple_bool(
                path,
                &doc,
                "issuance_model",
                "total_supply_constant_across_issuance_redemption",
            )?,
        },
    };
    validate_local_sns_evidence(path, text, &doc, &evidence)?;
    Ok(evidence)
}

fn parse_required_principal(
    path: &str,
    doc: &SimpleTomlDocument,
    section: &str,
    key: &str,
) -> Result<Principal, String> {
    let value = require_simple_string(path, doc, section, key)?;
    Principal::from_text(&value)
        .map_err(|err| format!("{path}: {section}.{key} is not a principal: {err}"))
}

fn parse_optional_principal_string(
    path: &str,
    doc: &SimpleTomlDocument,
    section: &str,
    key: &str,
) -> Result<Option<Principal>, String> {
    let value = require_simple_string(path, doc, section, key)?;
    if value == "none" || value == "not-created" {
        return Ok(None);
    }
    Principal::from_text(&value)
        .map(Some)
        .map_err(|err| format!("{path}: {section}.{key} is not a principal or none: {err}"))
}

fn parse_optional_u64(
    path: &str,
    doc: &SimpleTomlDocument,
    section: &str,
    key: &str,
) -> Result<Option<u64>, String> {
    match require_simple_value(path, doc, section, key)? {
        SimpleTomlValue::String(value) if value == "none" => Ok(None),
        SimpleTomlValue::String(value) => value
            .replace('_', "")
            .parse::<u64>()
            .map(Some)
            .map_err(|err| format!("{path}: {section}.{key} is not a u64 or none: {err}")),
        SimpleTomlValue::Integer(value) => (*value)
            .try_into()
            .map(Some)
            .map_err(|_| format!("{path}: {section}.{key} does not fit u64")),
        other => Err(format!(
            "{path}: expected {section}.{key} to be integer, numeric string, or none, got {other:?}"
        )),
    }
}

fn parse_subaccount_hex(
    path: &str,
    doc: &SimpleTomlDocument,
    section: &str,
    key: &str,
) -> Result<Option<String>, String> {
    let value = require_simple_string(path, doc, section, key)?;
    if value == "none" {
        return Ok(None);
    }
    if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Ok(Some(value.to_ascii_lowercase()));
    }
    Err(format!(
        "{path}: {section}.{key} must be \"none\" or 32-byte lowercase hex"
    ))
}

fn validate_local_sns_evidence(
    path: &str,
    text: &str,
    doc: &SimpleTomlDocument,
    evidence: &LocalSnsEvidence,
) -> Result<(), String> {
    if evidence.mode.network != "local"
        || evidence.mode.source != "official-local-sns-rehearsal"
        || evidence.mode.dfx_sns != "manual-local-only"
    {
        return Err(format!(
            "{path}: mode must describe official manual local-only SNS evidence"
        ));
    }
    if evidence.mode.io_protocol_live {
        return Err(format!("{path}: mode.io_protocol_live must remain false"));
    }
    if evidence.mode.sns_io_ledger_mainnet_launched {
        return Err(format!(
            "{path}: mode.sns_io_ledger_mainnet_launched must remain false"
        ));
    }
    validate_protected_reminders(path, doc)?;
    validate_no_forbidden_local_ids(path, text, doc)?;
    let principals = [
        evidence.sns_canisters.root,
        evidence.sns_canisters.governance,
        evidence.sns_canisters.ledger,
        evidence.sns_canisters.index,
        evidence.sns_canisters.swap,
        evidence.io_dapp_canisters.io_stream_manager,
        evidence.io_dapp_canisters.io_nns_neuron_manager,
        evidence.io_dapp_canisters.io_historian,
        evidence.io_dapp_canisters.frontend,
        evidence.ledger.protocol_reserve_account_owner,
    ];
    let mut unique = BTreeSet::new();
    for principal in principals {
        if !unique.insert(principal.to_text()) {
            return Err(format!(
                "{path}: local SNS/dapp principal {principal} is reused"
            ));
        }
    }
    if let Some(archive) = evidence.sns_canisters.archive {
        validate_local_principal_value(path, "sns_canisters.archive", &archive.to_text())?;
    }
    if evidence.expected.token_symbol != "IO" || evidence.ledger.token_symbol != "IO" {
        return Err(format!(
            "{path}: local SNS rehearsal token symbol must be IO"
        ));
    }
    if evidence.ledger.transaction_fee_e8s != evidence.expected.transaction_fee_e8s {
        return Err(format!(
            "{path}: observed transaction_fee_e8s {} does not match expected {}",
            evidence.ledger.transaction_fee_e8s, evidence.expected.transaction_fee_e8s
        ));
    }
    if evidence.ledger.total_supply_e8s != evidence.expected.total_supply_e8s {
        return Err(format!(
            "{path}: observed total_supply_e8s {} does not match expected {}",
            evidence.ledger.total_supply_e8s, evidence.expected.total_supply_e8s
        ));
    }
    if evidence.ledger.protocol_reserve_balance_e8s == 0 {
        return Err(format!("{path}: protocol reserve balance must be nonzero"));
    }
    if evidence.ledger.reserve_transfer_amount_e8s == 0
        || evidence.ledger.redemption_return_amount_e8s == 0
    {
        return Err(format!(
            "{path}: issuance and redemption rehearsal transfer amounts must be nonzero"
        ));
    }
    if !evidence.ledger.bad_fee_error_observed {
        return Err(format!("{path}: bad fee error must be observed"));
    }
    if !evidence.ledger.insufficient_funds_error_observed {
        return Err(format!("{path}: insufficient funds error must be observed"));
    }
    if evidence.ledger.duplicate_transfer_observed {
        if !evidence.ledger.duplicate_block_verified {
            return Err(format!(
                "{path}: duplicate transfer observation requires duplicate_block_verified = true"
            ));
        }
        if evidence.ledger.duplicate_of_block_index.is_none() {
            return Err(format!(
                "{path}: duplicate transfer observation requires duplicate_of_block_index"
            ));
        }
    }
    if !evidence.ledger.index_account_history_observed {
        return Err(format!("{path}: index account history must be observed"));
    }
    if evidence.ledger.index_history_order.trim().is_empty()
        || evidence
            .ledger
            .index_lag_or_archive_required_observed
            .trim()
            .is_empty()
    {
        return Err(format!(
            "{path}: index history order and lag/archive status must be recorded"
        ));
    }
    if !evidence.governance.governance_available
        || !evidence.governance.root_available
        || !evidence.governance.swap_available
        || !evidence.governance.dapp_controller_state_checked
    {
        return Err(format!(
            "{path}: governance/root/swap availability and dapp controller state must be checked"
        ));
    }
    if !evidence.governance.governance_upgrade_proposal_tested
        && evidence.governance.governance_upgrade_gap.trim().is_empty()
    {
        return Err(format!(
            "{path}: governance upgrade gap is required when upgrade proposal was not tested"
        ));
    }
    if evidence.issuance.resolved_as != "protocol_reserve_transfer" {
        return Err(format!(
            "{path}: issuance_model.resolved_as must be protocol_reserve_transfer"
        ));
    }
    if evidence.issuance.minting_assumed {
        return Err(format!("{path}: minting_assumed must be false"));
    }
    if evidence.issuance.treasury_transfer_assumed {
        return Err(format!("{path}: treasury_transfer_assumed must be false"));
    }
    if !evidence
        .issuance
        .total_supply_constant_across_issuance_redemption
    {
        return Err(format!(
            "{path}: total supply must be constant across issuance/redemption"
        ));
    }
    let _ = evidence.ledger.protocol_reserve_subaccount_hex.as_deref();
    let _ = evidence.ledger.reserve_transfer_block_index;
    let _ = evidence.ledger.redemption_return_block_index;
    Ok(())
}

fn validate_protected_reminders(path: &str, doc: &SimpleTomlDocument) -> Result<(), String> {
    let canister = require_simple_string(
        path,
        doc,
        "protected",
        "must_not_touch_neuron_owner_canister",
    )?;
    if canister != PROTECTED_IO_NEURON_OWNER_CANISTER {
        return Err(format!(
            "{path}: protected.must_not_touch_neuron_owner_canister must remain {PROTECTED_IO_NEURON_OWNER_CANISTER}"
        ));
    }
    let neuron = require_simple_string(path, doc, "protected", "must_not_touch_io_nns_neuron_id")?;
    if neuron != PROTECTED_IO_NNS_NEURON_ID.to_string() {
        return Err(format!(
            "{path}: protected.must_not_touch_io_nns_neuron_id must remain {}",
            PROTECTED_IO_NNS_NEURON_ID
        ));
    }
    Ok(())
}

fn validate_no_forbidden_local_ids(
    path: &str,
    text: &str,
    doc: &SimpleTomlDocument,
) -> Result<(), String> {
    for (section, values) in doc {
        for (key, value) in values {
            let SimpleTomlValue::String(value) = value else {
                continue;
            };
            if section == "protected" {
                continue;
            }
            validate_local_principal_value(path, &format!("{section}.{key}"), value)?;
            if value == &PROTECTED_IO_NNS_NEURON_ID.to_string() {
                return Err(format!(
                    "{path}: {section}.{key} must not reference protected IO neuron {}",
                    PROTECTED_IO_NNS_NEURON_ID
                ));
            }
        }
    }
    for mainnet_id in LOCAL_SNS_MAINNET_CANISTER_IDS {
        if text.contains(mainnet_id) {
            return Err(format!(
                "{path}: local evidence must not contain known mainnet/prior canister {mainnet_id}"
            ));
        }
    }
    Ok(())
}

fn validate_local_principal_value(path: &str, field: &str, value: &str) -> Result<(), String> {
    if value == PROTECTED_IO_NEURON_OWNER_CANISTER {
        return Err(format!(
            "{path}: {field} must not reference protected canister {PROTECTED_IO_NEURON_OWNER_CANISTER}"
        ));
    }
    for mainnet_id in LOCAL_SNS_MAINNET_CANISTER_IDS {
        if value == *mainnet_id {
            return Err(format!(
                "{path}: {field} must not reference known mainnet/prior canister {mainnet_id}"
            ));
        }
    }
    Ok(())
}

fn check_local_sns_ledger_at(root: &Path) -> Result<bool, String> {
    let path = "deploy/local-sns-rehearsal/canister-ids.local.toml";
    let full_path = root.join(path);
    if !full_path.exists() {
        return Ok(false);
    }
    let text = require_file(root, path)?;
    parse_local_sns_evidence(path, &text)?;
    Ok(true)
}

fn check_e2e_coverage_matrix_at(root: &Path) -> Result<(), String> {
    let matrix_path = "docs/testing/e2e-coverage-matrix.md";
    let inventory_path = "docs/testing/current-test-inventory.md";
    let scenarios_path = "docs/testing/e2e-scenario-specs.md";
    let matrix = require_file(root, matrix_path)?;
    require_present(
        matrix_path,
        &matrix,
        &[
            "real SNS ledger",
            "real SNS index",
            "real SNS governance",
            "SNS neuron staking",
            "APY increase",
            "reserve transfer",
            "duplicate proof",
            "index lag",
            "archive-required",
            "mid-flight upgrade",
            "historian/frontend honesty",
            "No current all-real-canister PocketIC E2E test exists",
        ],
    )?;
    let inventory = require_file(root, inventory_path)?;
    require_present(
        inventory_path,
        &inventory,
        &[
            "real SNS neuron staking",
            "real SNS governance maturity/rewards",
            "real SNS index account history",
            "IO APY increase from real SNS staked IO",
            "full ICP -> IO -> stake -> APY -> redemption E2E",
            "not proved",
        ],
    )?;
    let scenarios = require_file(root, scenarios_path)?;
    require_present(
        scenarios_path,
        &scenarios,
        &[
            "ICP deposit -> IO reserve issuance",
            "User stakes IO into SNS neuron",
            "User increases IO neuron stake",
            "SNS governance unavailable",
            "SNS index lag",
            "Archive-required account history",
            "Mid-flight upgrade",
            "Frontend never calls value-moving canisters",
        ],
    )?;
    Ok(())
}

fn check_real_canister_harness_at(root: &Path) -> Result<(), String> {
    let plan_path = "docs/testing/real-canister-pocketic-plan.md";
    let cargo_path = "tests/e2e_real_canisters/Cargo.toml";
    let harness_path = "tests/e2e_real_canisters/src/lib.rs";
    let manifest_path = "tests/e2e_real_canisters/wasms.example.toml";
    let plan = require_file(root, plan_path)?;
    require_present(
        plan_path,
        &plan,
        &[
            "Real-framework PocketIC",
            "IO_REAL_SNS_WASM_DIR",
            "IO_REAL_SNS_WASM_MANIFEST",
            "Do not download unpinned Wasms in CI",
            "real SNS ledger",
            "real SNS index",
            "real SNS governance",
            "real SNS root",
            "real SNS swap",
            "SNS-W",
            "blocked",
        ],
    )?;
    let cargo = require_file(root, cargo_path)?;
    require_present(
        cargo_path,
        &cargo,
        &[
            "name = \"e2e-real-canisters\"",
            "pocket-ic.workspace = true",
            "io-ledger-types.workspace = true",
        ],
    )?;
    let manifest = require_file(root, manifest_path)?;
    require_present(
        manifest_path,
        &manifest,
        &[
            "sns_ledger_wasm",
            "sns_ledger_sha256",
            "sns_index_wasm",
            "sns_index_sha256",
            "sns_governance_wasm",
            "nns_governance_wasm",
        ],
    )?;
    let harness = require_file(root, harness_path)?;
    require_present(
        harness_path,
        &harness,
        &[
            "real_sns_ledger_index_smoke",
            "real_sns_ledger_index_same_wasm_upgrade_preserves_balances_history_and_duplicates",
            "real_sns_governance_staking_smoke",
            "real_canister_e2e_icp_to_io_stake_reward_redemption",
        ],
    )?;
    let artifacts = require_file(root, "tests/e2e_real_canisters/src/artifacts.rs")?;
    require_present(
        "tests/e2e_real_canisters/src/artifacts.rs",
        &artifacts,
        &["IO_REAL_SNS_WASM_DIR", "IO_REAL_SNS_WASM_MANIFEST"],
    )?;
    let pocketic_env = require_file(root, "tests/e2e_real_canisters/src/pocketic_env.rs")?;
    require_present(
        "tests/e2e_real_canisters/src/pocketic_env.rs",
        &pocketic_env,
        &[
            "POCKET_IC_BIN",
            "with_nns_subnet()",
            "with_sns_subnet()",
            "with_application_subnet()",
            "create_sns_canister",
            "create_application_canister",
            "create_canister_on_subnet",
        ],
    )?;
    let ledger_index = require_file(root, "tests/e2e_real_canisters/src/sns_ledger_index.rs")?;
    require_present(
        "tests/e2e_real_canisters/src/sns_ledger_index.rs",
        &ledger_index,
        &["create_sns_canister"],
    )?;
    require_absent(harness_path, &harness, &["--network ic", "dfx "])?;
    for path in [
        cargo_path,
        manifest_path,
        "tests/e2e_real_canisters/src/artifacts.rs",
        "tests/e2e_real_canisters/src/icrc.rs",
        "tests/e2e_real_canisters/src/pocketic_env.rs",
        "tests/e2e_real_canisters/src/sns_ledger_index.rs",
    ] {
        let text = require_file(root, path)?;
        require_absent(
            path,
            &text,
            &[
                "--network ic",
                "https://",
                "http://",
                "download",
                "dfx ",
                "oae4c-3iaaa-aaaar-qb5qq-cai",
                "6345890886899317159",
            ],
        )?;
    }
    let root_cargo = require_file(root, "Cargo.toml")?;
    require_absent(
        "Cargo.toml",
        &root_cargo,
        &["pocket-ic.workspace = true\nio-stream-manager"],
    )?;
    Ok(())
}

fn check_real_canister_artifact_manifest_at(root: &Path, required: bool) -> Result<bool, String> {
    let wasm_dir = match env::var_os("IO_REAL_SNS_WASM_DIR") {
        Some(value) => PathBuf::from(value),
        None if required => {
            return Err("IO_REAL_SNS_WASM_DIR is required".to_string());
        }
        None => return Ok(false),
    };
    if !wasm_dir.is_dir() {
        return Err(format!(
            "IO_REAL_SNS_WASM_DIR must point to an existing directory: {}",
            wasm_dir.display()
        ));
    }
    let manifest_path = env::var_os("IO_REAL_SNS_WASM_MANIFEST")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("tests/e2e_real_canisters/wasms.local.toml"));
    if !manifest_path.is_file() {
        if required {
            return Err(format!(
                "IO_REAL_SNS_WASM_MANIFEST or {} is required",
                root.join("tests/e2e_real_canisters/wasms.local.toml")
                    .display()
            ));
        }
        return Ok(false);
    }
    let manifest = fs::read_to_string(&manifest_path)
        .map_err(|err| format!("{}: {err}", manifest_path.display()))?;
    for key in [
        "sns_ledger_wasm",
        "sns_ledger_sha256",
        "sns_index_wasm",
        "sns_index_sha256",
    ] {
        if !manifest.contains(key) {
            return Err(format!("{}: missing {key}", manifest_path.display()));
        }
    }
    for artifact in ["sns_ledger", "sns_index"] {
        let wasm_key = format!("{artifact}_wasm");
        let hash_key = format!("{artifact}_sha256");
        let file_name = simple_manifest_value(&manifest, &wasm_key)
            .ok_or_else(|| format!("{}: missing artifacts.{wasm_key}", manifest_path.display()))?;
        let expected = simple_manifest_value(&manifest, &hash_key)
            .ok_or_else(|| format!("{}: missing artifacts.{hash_key}", manifest_path.display()))?;
        if expected.starts_with('<') {
            return Err(format!(
                "{}: artifacts.{hash_key} must be a pinned SHA-256, not a placeholder",
                manifest_path.display()
            ));
        }
        let wasm_path = wasm_dir.join(file_name);
        let bytes =
            fs::read(&wasm_path).map_err(|err| format!("{}: {err}", wasm_path.display()))?;
        let actual = hex_sha256(&bytes);
        if actual != expected.to_ascii_lowercase() {
            return Err(format!(
                "{}: SHA-256 mismatch; expected {}, got {actual}",
                wasm_path.display(),
                expected
            ));
        }
    }
    Ok(true)
}

fn simple_manifest_value(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|raw| {
        let line = raw.split_once('#').map_or(raw, |(prefix, _)| prefix).trim();
        let (left, right) = line.split_once('=')?;
        if left.trim() != key {
            return None;
        }
        let value = right.trim();
        (value.starts_with('"') && value.ends_with('"') && value.len() >= 2)
            .then(|| value[1..value.len() - 1].to_string())
    })
}

fn hex_sha256(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(bytes))
}

fn check_sns_harness_at(root: &Path) -> Result<(), String> {
    let local_sns_doc = require_file(root, "docs/operations/local-sns-testing.md")?;
    require_present(
        "docs/operations/local-sns-testing.md",
        &local_sns_doc,
        &[
            "Pure model tests remain the main accounting guardrail",
            "Mock and PocketIC tests remain the main journal, retry, and upgrade guardrail",
            "We currently run SNS-shaped mock/PocketIC tests.",
            "Four-Layer Compatibility Model",
            "Official SNS Local Launch Rehearsal",
            "optional, local-only, and not part of `test_ci` or `verify_release`",
            "IO-Owned PocketIC SNS Harness",
            "must not call mainnet",
            "must not use `--network ic`",
            "not production launch configuration",
            "not official SNS launch tests",
        ],
    )?;

    let sns_readme = require_file(root, "tools/sns/README.md")?;
    require_present(
        "tools/sns/README.md",
        &sns_readme,
        &[
            "official SNS compatibility package",
            "not production launch configuration",
            "must not depend on `dfx`",
            "must not use `--network ic`",
            "placeholder principals",
            "IO_TEST ledger is non-canonical",
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
            "fallback_controller_principals",
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

    check_sns_config_at(root)?;
    check_sns_official_testing_at(root)?;

    check_required_executable_scripts_at(root)?;
    Ok(())
}

fn check_sns_root_lifecycle_at(root: &Path) -> Result<(), String> {
    let root_doc = require_file(root, "docs/architecture/sns-root-lifecycle.md")?;
    require_present(
        "docs/architecture/sns-root-lifecycle.md",
        &root_doc,
        &[
            "mock/PocketIC only",
            "does not run the official SNS launch or decentralization swap flow",
            "does not call mainnet",
            "records an approved upgrade intent",
            "test harness executes the PocketIC upgrade",
            "release-artifacts/manifest.json",
            "Production SNS root/governance wiring remains future work",
        ],
    )?;
    require_absent(
        "docs/architecture/sns-root-lifecycle.md",
        &root_doc,
        &["--network ic"],
    )?;

    let local_sns_doc = require_file(root, "docs/operations/local-sns-testing.md")?;
    require_present(
        "docs/operations/local-sns-testing.md",
        &local_sns_doc,
        &[
            "SNS root/controller lifecycle",
            "mock/PocketIC only",
            "sns_root_lifecycle_tests",
            "sns_root_lifecycle_required",
        ],
    )?;

    let testing_doc = require_file(root, "docs/development/testing.md")?;
    require_present(
        "docs/development/testing.md",
        &testing_doc,
        &[
            "sns_root_lifecycle_tests",
            "sns_root_lifecycle_required",
            "POCKET_IC_BIN",
            "does not use `dfx`",
        ],
    )?;

    for path in [
        "tests/mocks/mock_sns_root/src/lib.rs",
        "tests/mocks/mock_sns_governance/src/lib.rs",
    ] {
        let text = require_file(root, path)?;
        require_present(path, &text, &["debug_"])?;
    }

    check_did_surface_at(root, false)?;
    check_required_executable_scripts_at(root)?;
    Ok(())
}

fn check_dev_mainnet_no_deployment_scripts(root: &Path) -> Result<(), String> {
    let phase_dir = root.join("deploy/mainnet-dev/legacy-phase1");
    let entries = fs::read_dir(&phase_dir)
        .map_err(|err| format!("deploy/mainnet-dev/legacy-phase1: {err}"))?;
    for entry in entries {
        let entry = entry.map_err(|err| format!("deploy/mainnet-dev/legacy-phase1: {err}"))?;
        let file_type = entry
            .file_type()
            .map_err(|err| format!("{}: {err}", entry.path().display()))?;
        if !file_type.is_file() {
            continue;
        }
        let path = entry.path();
        let rel = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .to_string();
        if path.extension().is_some_and(|extension| extension == "sh") {
            return Err(format!(
                "{rel}: deployment scripts are not allowed in DevMainnet record"
            ));
        }
        let text = fs::read_to_string(&path).map_err(|err| format!("{rel}: {err}"))?;
        require_absent(
            &rel,
            &text,
            &["#!/", "dfx deploy", "dfx canister", "--network ic"],
        )?;
    }
    Ok(())
}

fn check_prelaunch_public_shell_at(root: &Path) -> Result<(), String> {
    let config = require_file(root, DEV_MAINNET_CONFIG_PATH)?;
    let dev_gateway_url = format!("https://{DEV_MAINNET_FRONTEND_CANISTER_ID}.icp0.io/");
    let dev_raw_url = format!("https://{DEV_MAINNET_FRONTEND_CANISTER_ID}.raw.icp0.io/");
    let dev_historian_env = format!("CANISTER_ID_IO_HISTORIAN={DEV_MAINNET_HISTORIAN_CANISTER_ID}");
    require_toml_string(
        DEV_MAINNET_CONFIG_PATH,
        &config,
        "environment",
        "name",
        "DevMainnet",
    )?;
    require_toml_string(
        DEV_MAINNET_CONFIG_PATH,
        &config,
        "environment",
        "network",
        "ic",
    )?;
    require_toml_string(
        DEV_MAINNET_CONFIG_PATH,
        &config,
        "environment",
        "status",
        "DevOnly",
    )?;
    require_toml_bool(
        DEV_MAINNET_CONFIG_PATH,
        &config,
        "environment",
        "production",
        false,
    )?;
    require_toml_string(
        DEV_MAINNET_CONFIG_PATH,
        &config,
        "phase",
        "mode",
        DEV_MAINNET_MODE,
    )?;
    require_toml_string(
        DEV_MAINNET_CONFIG_PATH,
        &config,
        "phase",
        "release_artifact_manifest",
        MANIFEST_PATH,
    )?;
    require_toml_string(
        DEV_MAINNET_CONFIG_PATH,
        &config,
        "canisters",
        "frontend",
        DEV_MAINNET_FRONTEND_CANISTER_ID,
    )?;
    require_toml_string(
        DEV_MAINNET_CONFIG_PATH,
        &config,
        "canisters",
        "io_historian",
        DEV_MAINNET_HISTORIAN_CANISTER_ID,
    )?;
    require_toml_string(
        DEV_MAINNET_CONFIG_PATH,
        &config,
        "frontend",
        "gateway_url",
        &dev_gateway_url,
    )?;
    require_toml_string(
        DEV_MAINNET_CONFIG_PATH,
        &config,
        "frontend",
        "raw_url",
        &dev_raw_url,
    )?;
    require_toml_string(
        DEV_MAINNET_CONFIG_PATH,
        &config,
        "frontend",
        "built_with_canister_id_io_historian",
        DEV_MAINNET_HISTORIAN_CANISTER_ID,
    )?;
    require_toml_bool(
        DEV_MAINNET_CONFIG_PATH,
        &config,
        "not_deployed",
        "io_stream_manager",
        true,
    )?;
    require_toml_bool(
        DEV_MAINNET_CONFIG_PATH,
        &config,
        "not_deployed",
        "io_nns_neuron_manager",
        true,
    )?;
    require_toml_string(
        DEV_MAINNET_CONFIG_PATH,
        &config,
        "not_touched",
        "existing_io_neuron_owner_canister",
        KNOWN_CONTROLLER_CANISTER_PRINCIPAL,
    )?;
    require_toml_string(
        DEV_MAINNET_CONFIG_PATH,
        &config,
        "not_touched",
        "io_neuron_id",
        &KNOWN_TWO_YEAR_NNS_NEURON_ID.to_string(),
    )?;
    for key in [
        "io_protocol_live",
        "sns_io_ledger_launched",
        "io_issuance_live",
        "io_redemption_live",
    ] {
        require_toml_bool(DEV_MAINNET_CONFIG_PATH, &config, "status", key, false)?;
    }

    let phase_readme = require_file(root, DEV_MAINNET_README_PATH)?;
    let phase_status = require_file(root, DEV_MAINNET_STATUS_PATH)?;
    let docs = [
        "docs/operations/mainnet-readiness.md",
        "docs/operations/mainnet-prelaunch-dry-run.md",
        "docs/architecture/canister-roles.md",
        "docs/architecture/historian.md",
        "canisters/frontend/README.md",
        "canisters/io_historian/README.md",
    ];
    let mut combined = format!("{phase_readme}\n{phase_status}\n{config}\n");
    for path in docs {
        combined.push_str(&require_file(root, path)?);
        combined.push('\n');
    }
    require_present(
        "Phase 1 prelaunch docs",
        &combined,
        &[
            DEV_MAINNET_MODE,
            "DevMainnet",
            "dev/test",
            "superseded as production targets",
            "not on the fiduciary subnet",
            "not production IO protocol canisters",
            DEV_MAINNET_FRONTEND_CANISTER_ID,
            DEV_MAINNET_HISTORIAN_CANISTER_ID,
            &dev_gateway_url,
            &dev_raw_url,
            &dev_historian_env,
            "No value-moving protocol canister",
            "not deployed",
            "not touched",
            KNOWN_CONTROLLER_CANISTER_PRINCIPAL,
            "6345890886899317159",
            "IO protocol is not live",
            "canonical SNS IO ledger is not launched",
            "IO issuance is not live",
            "IO redemption is not live",
            "public read model",
            "not protocol truth",
            MANIFEST_PATH,
        ],
    )?;

    check_dev_mainnet_no_deployment_scripts(root)?;
    check_required_executable_scripts_at(root)?;
    check_did_surface_at(root, false)?;
    Ok(())
}

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
        (
            "io_nns_neuron_manager",
            PRODUCTION_IO_NNS_NEURON_MANAGER_CANISTER_ID,
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
        &[
            DEV_MAINNET_FRONTEND_CANISTER_ID,
            DEV_MAINNET_HISTORIAN_CANISTER_ID,
        ],
    )
}

fn canonical_production_mapping() -> [(&'static str, &'static str); 4] {
    [
        (
            "io_stream_manager",
            PRODUCTION_IO_STREAM_MANAGER_CANISTER_ID,
        ),
        (
            "io_nns_neuron_manager",
            PRODUCTION_IO_NNS_NEURON_MANAGER_CANISTER_ID,
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
    canonical_production_mapping()
        .iter()
        .find_map(|(name, _)| (title == *name).then_some(*name))
}

fn check_production_mapping_text(path: &str, text: &str) -> Result<(), String> {
    let mapping = canonical_production_mapping();
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
    }
    Ok(())
}

fn check_production_wiring_at(root: &Path) -> Result<(), String> {
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
                DEV_MAINNET_FRONTEND_CANISTER_ID,
                DEV_MAINNET_HISTORIAN_CANISTER_ID,
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
            "6345890886899317159",
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

    let phase1 = require_file(root, DEV_MAINNET_CONFIG_PATH)?;
    require_toml_string(
        DEV_MAINNET_CONFIG_PATH,
        &phase1,
        "environment",
        "name",
        "DevMainnet",
    )?;
    require_toml_bool(
        DEV_MAINNET_CONFIG_PATH,
        &phase1,
        "environment",
        "production",
        false,
    )?;
    require_toml_bool(
        DEV_MAINNET_CONFIG_PATH,
        &phase1,
        "not_deployed",
        "io_stream_manager",
        true,
    )?;
    require_toml_bool(
        DEV_MAINNET_CONFIG_PATH,
        &phase1,
        "not_deployed",
        "io_nns_neuron_manager",
        true,
    )?;
    require_toml_string(
        DEV_MAINNET_CONFIG_PATH,
        &phase1,
        "not_touched",
        "existing_io_neuron_owner_canister",
        PROTECTED_IO_NEURON_OWNER_CANISTER,
    )?;
    require_toml_string(
        DEV_MAINNET_CONFIG_PATH,
        &phase1,
        "not_touched",
        "io_neuron_id",
        &PROTECTED_IO_NNS_NEURON_ID.to_string(),
    )?;

    check_did_surface_at(root, false)?;
    check_required_executable_scripts_at(root)?;
    Ok(())
}

fn check_historian_freshness_at(root: &Path) -> Result<(), String> {
    check_did_surface_at(root, false)?;
    check_prelaunch_public_shell_at(root)?;

    let historian_source = require_file(root, "canisters/io_historian/src/lib.rs")?;
    require_present(
        "canisters/io_historian/src/lib.rs",
        &historian_source,
        &[
            "HistorianIngestionSource",
            "HistorianObservation",
            "IngestionBatch",
            "IngestionSourceKind",
            "ObservationFreshness",
            "SourceHealth",
            "IngestionCursor",
            "IngestionWatermark",
            "StalenessPolicy",
            "ReleaseArtifacts",
            "CanisterStatusModuleHash",
            "IcpIndexHealth",
            "FutureIoSnsIndexHealth",
            "NnsGovernanceFreshness",
            "SnsGovernanceFreshness",
            "ProtocolSnapshot",
            "ReserveSnapshot",
            "FrontendDashboardFreshness",
            "Fresh",
            "Stale",
            "Missing",
            "Incomplete",
            "ObservedOnly",
            "PrelaunchNotApplicable",
            "ErrorRetryable",
            "Unknown",
            "EXPECTED_RELEASE_ARTIFACT_CANISTERS",
            "source_health_from_state",
            "canonical SNS IO ledger is not launched",
            "index canisters remain the normal account-history abstraction",
        ],
    )?;
    require_absent(
        "canisters/io_historian/src/lib.rs",
        &historian_source,
        &[
            "bounded_wait(canister, \"debug_",
            "bounded_wait(canister, \"get_state\"",
            "bounded_wait(canister, \"redeem\"",
        ],
    )?;
    check_historian_current_time_path("canisters/io_historian/src/lib.rs", &historian_source)?;

    let historian_did = require_file(root, "canisters/io_historian/io_historian.did")?;
    require_present(
        "canisters/io_historian/io_historian.did",
        &historian_did,
        &["SourceHealth", "source_health", "ObservationFreshness"],
    )?;
    require_absent(
        "canisters/io_historian/io_historian.did",
        &historian_did,
        &["debug_", " ingest_", " update"],
    )?;

    let debug_did = require_file(root, "canisters/io_historian/io_historian_debug.did")?;
    require_present(
        "canisters/io_historian/io_historian_debug.did",
        &debug_did,
        &[
            "debug_ingest_protocol_snapshot",
            "debug_ingest_index_health",
        ],
    )?;

    let frontend_transform = require_file(
        root,
        "canisters/frontend/web/src/data/dashboard-transforms.js",
    )?;
    require_present(
        "canisters/frontend/web/src/data/dashboard-transforms.js",
        &frontend_transform,
        &[
            "sourceHealthWarnings",
            "source_health",
            "sourceHealthSummary",
        ],
    )?;
    let frontend_transform_test = require_file(
        root,
        "canisters/frontend/web/test/dashboard-transforms.test.mjs",
    )?;
    require_present(
        "canisters/frontend/web/test/dashboard-transforms.test.mjs",
        &frontend_transform_test,
        &[
            "PrelaunchNotApplicable",
            "Stale",
            "Incomplete",
            "Missing",
            "not deployed/not allocated",
        ],
    )?;
    let frontend_renderer =
        require_file(root, "canisters/frontend/web/src/ui/dashboard-renderer.js")?;
    require_present(
        "canisters/frontend/web/src/ui/dashboard-renderer.js",
        &frontend_renderer,
        &["data-list='sourceHealth'", "sourceHealthSummary"],
    )?;
    let frontend_template = require_file(root, "canisters/frontend/web/index.template.html")?;
    require_present(
        "canisters/frontend/web/index.template.html",
        &frontend_template,
        &["Source health", "data-list=\"sourceHealth\""],
    )?;

    for path in [
        "canisters/frontend/web/src/app/agent.js",
        "canisters/frontend/web/src/data/historian-loaders.js",
        "canisters/frontend/web/src/data/dashboard-transforms.js",
        "canisters/frontend/web/src/ui/dashboard-renderer.js",
        "canisters/frontend/web/declarations/io_historian/io_historian.did.js",
        "canisters/frontend/web/declarations/io_historian/index.js",
    ] {
        let text = require_file(root, path)?;
        require_absent(
            path,
            &text,
            &[
                ".dfx",
                "src/declarations",
                "io_historian_debug",
                "io_stream_manager",
                "io_nns_neuron_manager",
                "debug_",
            ],
        )?;
    }
    check_historian_js_declaration_at(root)?;

    let phase1 = require_file(root, DEV_MAINNET_CONFIG_PATH)?;
    for key in [
        "io_protocol_live",
        "sns_io_ledger_launched",
        "io_issuance_live",
        "io_redemption_live",
    ] {
        require_toml_bool(DEV_MAINNET_CONFIG_PATH, &phase1, "status", key, false)?;
    }
    require_toml_bool(
        DEV_MAINNET_CONFIG_PATH,
        &phase1,
        "not_deployed",
        "io_stream_manager",
        true,
    )?;
    require_toml_bool(
        DEV_MAINNET_CONFIG_PATH,
        &phase1,
        "not_deployed",
        "io_nns_neuron_manager",
        true,
    )?;
    require_toml_string(
        DEV_MAINNET_CONFIG_PATH,
        &phase1,
        "not_touched",
        "existing_io_neuron_owner_canister",
        PROTECTED_IO_NEURON_OWNER_CANISTER,
    )?;
    require_toml_string(
        DEV_MAINNET_CONFIG_PATH,
        &phase1,
        "not_touched",
        "io_neuron_id",
        &PROTECTED_IO_NNS_NEURON_ID.to_string(),
    )?;

    for path in [
        "docs/architecture/historian-ingestion.md",
        "docs/operations/historian-freshness.md",
        "docs/architecture/historian.md",
        "docs/operations/mainnet-readiness.md",
        "canisters/io_historian/README.md",
        "canisters/frontend/README.md",
    ] {
        let text = require_file(root, path)?;
        require_present(
            path,
            &text,
            &[
                "public read model",
                "rebuildable",
                "not canonical protocol truth",
                "not a value-moving authority",
                "IO protocol is not live",
                "SNS IO ledger remains not launched",
                "missing/stale/incomplete",
                "index canisters",
            ],
        )?;
    }
    let freshness_doc = require_file(root, "docs/operations/historian-freshness.md")?;
    require_present(
        "docs/operations/historian-freshness.md",
        &freshness_doc,
        &[
            "current historian/canister time",
            "source timestamps or source watermarks",
            "no newer observations arrive",
        ],
    )?;

    Ok(())
}

fn check_stable_storage_at(root: &Path) -> Result<(), String> {
    check_did_surface_at(root, false)?;
    check_prelaunch_public_shell_at(root)?;
    check_production_wiring_at(root)?;
    check_historian_freshness_at(root)?;

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
            }
        }
    }

    let stable_storage_doc = require_file(root, "docs/architecture/stable-storage.md")?;
    require_present(
        "docs/architecture/stable-storage.md",
        &stable_storage_doc,
        &[
            "io_stream_manager",
            "io_nns_neuron_manager",
            "io_historian",
            "corrupt value-moving state must fail closed",
            "missing first-install state",
            "stable-state fixtures are local/test fixtures, not live snapshots",
            "IO protocol remains not live",
            "SNS IO ledger remains not launched",
        ],
    )?;

    let compaction_doc = require_file(root, "docs/architecture/journal-compaction.md")?;
    require_present(
        "docs/architecture/journal-compaction.md",
        &compaction_doc,
        &[
            "pending operation journals",
            "processed transaction IDs",
            "must never be compacted before audit/activation",
            "duplicate retry/idempotency",
            "historian read model",
        ],
    )?;

    let stable_structures_doc =
        require_file(root, "docs/architecture/stable-structures-evaluation.md")?;
    require_present(
        "docs/architecture/stable-structures-evaluation.md",
        &stable_structures_doc,
        &[
            "serialized whole-state snapshots",
            "ic-stable-structures",
            "defer",
            "schema evolution",
            "test requirements",
        ],
    )?;

    let readiness = require_file(root, "docs/operations/mainnet-readiness.md")?;
    let production_wiring = require_file(root, "docs/operations/production-wiring.md")?;
    let combined_ops = format!("{readiness}\n{production_wiring}");
    require_present(
        "stable storage operations docs",
        &combined_ops,
        &[
            "No value-moving IO canister is deployed to production",
            "production adapters are not active",
            "historian is a rebuildable read model",
            "protected canister/neuron remain untouched",
            PROTECTED_IO_NEURON_OWNER_CANISTER,
            "6345890886899317159",
        ],
    )?;
    require_absent(
        "stable storage operations docs",
        &combined_ops,
        &["--network ic", "dfx canister", "dfx deploy"],
    )?;

    let stream_source = require_file(root, "canisters/io_stream_manager/src/lib.rs")?;
    let nns_source = require_file(root, "canisters/io_nns_neuron_manager/src/lib.rs")?;
    let historian_source = require_file(root, "canisters/io_historian/src/lib.rs")?;
    require_present(
        "stable migration source",
        &format!("{stream_source}\n{nns_source}\n{historian_source}"),
        &[
            "migrate_stable_state",
            "UnsupportedFutureVersion",
            "default_first_install_stable_state",
            "stable state is missing or corrupt during upgrade",
        ],
    )?;
    Ok(())
}

fn check_historian_current_time_path(path: &str, text: &str) -> Result<(), String> {
    require_present(
        path,
        text,
        &[
            "pub fn source_health_from_state_at",
            "fn historian_now_timestamp_nanos() -> u64",
            "ic_cdk::api::time()",
            "source_health_from_state_at(state, historian_now_timestamp_nanos())",
            "source_health_from_state_at(&state,",
        ],
    )?;
    require_absent(
        path,
        text,
        &["pub fn source_health_from_state(state: &StableState) -> Vec<SourceHealth> {\n    let now = latest_observation_timestamp(state);"],
    )
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
    eprintln!("known: test_all, test_ci, verify_release, security_scan, security_scan_required, validate_install_args, validate_prelaunch_public_shell, validate_production_wiring, validate_historian_freshness, validate_stable_storage, validate_local_sns_rehearsal, validate_local_sns_ledger, validate_local_sns_scripts, e2e_coverage_matrix_check, real_canister_harness_check, real_canister_artifact_manifest_check, real_sns_ledger_index_tests, real_sns_ledger_index_required, real_sns_governance_tests, real_sns_governance_required, real_io_e2e_tests, real_io_e2e_required, e2e_real_coverage_check, local_sns_evidence_tests, sns_apy_policy_tests, frontend_setup, frontend_build, frontend_unit, frontend_certified_asset_tests, frontend_required, frontend_all, historian_tests, historian_required, sns_harness_check, sns_config_validate, sns_config_validate_official, sns_official_testing_check, sns_launch_readiness_check, sns_governance_read_tests, sns_governance_read_required, sns_ledger_index_tests, sns_ledger_index_required, sns_root_lifecycle_tests, sns_root_lifecycle_required, sns_pocketic_smoke, sns_pocketic_required, test_pocketic_required, preflight, check, fmt_check, did_surface, build_canisters, verify_artifacts, build_debug_canisters, test_unit, test_pocketic_integration, test_local_integration, test_e2e, stream_manager_unit, nns_neuron_manager_unit, historian_pocketic_integration, stream_manager_pocketic_integration, nns_neuron_manager_pocketic_integration");
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
            ok &= run_subcommand("frontend_setup");
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
        "validate_prelaunch_public_shell" => match check_prelaunch_public_shell_at(&root) {
            Ok(()) => eprintln!("✓ validate_prelaunch_public_shell"),
            Err(err) => {
                eprintln!("✗ validate_prelaunch_public_shell: {err}");
                ok = false;
            }
        },
        "validate_production_wiring" => match check_production_wiring_at(&root) {
            Ok(()) => eprintln!("✓ validate_production_wiring"),
            Err(err) => {
                eprintln!("✗ validate_production_wiring: {err}");
                ok = false;
            }
        },
        "validate_historian_freshness" => match check_historian_freshness_at(&root) {
            Ok(()) => eprintln!("✓ validate_historian_freshness"),
            Err(err) => {
                eprintln!("✗ validate_historian_freshness: {err}");
                ok = false;
            }
        },
        "validate_stable_storage" => match check_stable_storage_at(&root) {
            Ok(()) => eprintln!("✓ validate_stable_storage"),
            Err(err) => {
                eprintln!("✗ validate_stable_storage: {err}");
                ok = false;
            }
        },
        "frontend_setup" => {
            ok &= run("frontend: npm ci", npm(&["run", "setup:frontend"]));
        }
        "frontend_build" => {
            ok &= run(
                "frontend: build browser bundle",
                npm(&["run", "build:frontend"]),
            );
        }
        "frontend_unit" => {
            ok &= run("frontend: unit tests", npm(&["run", "test:frontend-unit"]));
        }
        "frontend_certified_asset_tests" => {
            ok &= run_subcommand("frontend_build");
            ok &= run(
                "unit: io-frontend assets",
                cargo_test(&["-p", "io-frontend"]),
            );
            if env::var_os("POCKET_IC_BIN").is_some() {
                ok &= run_subcommand("build_debug_canisters");
                ok &= run(
                    "pocketic: io-frontend",
                    cargo_test(&["-p", "io-frontend", "--test", "io_frontend_pocketic"]),
                );
            } else {
                eprintln!("skipping frontend PocketIC smoke: POCKET_IC_BIN is not set");
            }
        }
        "frontend_required" => {
            ok &= run_subcommand("frontend_setup");
            ok &= run_subcommand("frontend_build");
            ok &= run_subcommand("frontend_unit");
            ok &= run_subcommand("frontend_certified_asset_tests");
        }
        "frontend_all" => {
            ok &= run_subcommand("frontend_required");
        }
        "sns_harness_check" => match check_sns_harness_at(&root) {
            Ok(()) => eprintln!("✓ sns_harness_check"),
            Err(err) => {
                eprintln!("✗ sns_harness_check: {err}");
                ok = false;
            }
        },
        "sns_config_validate" => match check_sns_config_at(&root) {
            Ok(()) => eprintln!("✓ sns_config_validate"),
            Err(err) => {
                eprintln!("✗ sns_config_validate: {err}");
                ok = false;
            }
        },
        "sns_config_validate_official" => {
            if env::var_os("IO_RUN_DFX_SNS_VALIDATE").is_none() {
                eprintln!(
                    "skipping sns_config_validate_official: set IO_RUN_DFX_SNS_VALIDATE=1 to run optional dfx sns validation"
                );
            } else if Command::new("dfx").arg("--version").status().is_err() {
                eprintln!("skipping sns_config_validate_official: dfx is unavailable");
            } else {
                ok &= run(
                    "optional dfx sns init-config-file validate",
                    dfx(&[
                        "sns",
                        "init-config-file",
                        "validate",
                        "tools/sns/sns_init.io.local.yaml",
                    ]),
                );
            }
        }
        "sns_official_testing_check" => match check_sns_official_testing_at(&root) {
            Ok(()) => eprintln!("✓ sns_official_testing_check"),
            Err(err) => {
                eprintln!("✗ sns_official_testing_check: {err}");
                ok = false;
            }
        },
        "sns_launch_readiness_check" => {
            let strict = args.iter().any(|arg| arg == "--strict");
            match check_sns_launch_readiness_at(&root, strict) {
                Ok(incomplete) => {
                    eprintln!(
                        "✓ sns_launch_readiness_check: {incomplete} incomplete item(s) remain"
                    );
                }
                Err(err) => {
                    eprintln!("✗ sns_launch_readiness_check: {err}");
                    ok = false;
                }
            }
        }
        "validate_local_sns_rehearsal" => match check_local_sns_rehearsal_at(&root) {
            Ok(()) => eprintln!("✓ validate_local_sns_rehearsal"),
            Err(err) => {
                eprintln!("✗ validate_local_sns_rehearsal: {err}");
                ok = false;
            }
        },
        "validate_local_sns_ledger" => match check_local_sns_ledger_at(&root) {
            Ok(true) => eprintln!("✓ validate_local_sns_ledger"),
            Ok(false) => {
                eprintln!(
                    "skipping validate_local_sns_ledger: deploy/local-sns-rehearsal/canister-ids.local.toml is absent"
                );
            }
            Err(err) => {
                eprintln!("✗ validate_local_sns_ledger: {err}");
                ok = false;
            }
        },
        "validate_local_sns_scripts" => match validate_local_sns_scripts_at(&root) {
            Ok(()) => eprintln!("✓ validate_local_sns_scripts"),
            Err(err) => {
                eprintln!("✗ validate_local_sns_scripts: {err}");
                ok = false;
            }
        },
        "e2e_coverage_matrix_check" => match check_e2e_coverage_matrix_at(&root) {
            Ok(()) => eprintln!("✓ e2e_coverage_matrix_check"),
            Err(err) => {
                eprintln!("✗ e2e_coverage_matrix_check: {err}");
                ok = false;
            }
        },
        "real_canister_harness_check" => match check_real_canister_harness_at(&root) {
            Ok(()) => eprintln!("✓ real_canister_harness_check"),
            Err(err) => {
                eprintln!("✗ real_canister_harness_check: {err}");
                ok = false;
            }
        },
        "real_canister_artifact_manifest_check" => {
            let required = args.iter().any(|arg| arg == "--required");
            match check_real_canister_artifact_manifest_at(&root, required) {
                Ok(true) => eprintln!("✓ real_canister_artifact_manifest_check"),
                Ok(false) => {
                    eprintln!("skipping real_canister_artifact_manifest_check: real Wasm artifacts are not configured")
                }
                Err(err) => {
                    eprintln!("✗ real_canister_artifact_manifest_check: {err}");
                    ok = false;
                }
            }
        }
        "real_sns_ledger_index_tests" => {
            ok &= run_subcommand("real_canister_harness_check");
            ok &= run(
                "unit: e2e-real-canisters artifact harness",
                cargo_test(&["-p", "e2e-real-canisters"]),
            );
            match check_real_canister_artifact_manifest_at(&root, false) {
                Ok(true) => {
                    if env::var_os("POCKET_IC_BIN").is_none() {
                        eprintln!(
                            "✗ real_sns_ledger_index_tests: artifacts are configured but POCKET_IC_BIN is not set"
                        );
                        ok = false;
                    } else {
                        ok &= run(
                            "real-framework: SNS ledger/index smoke",
                            cargo_test(&[
                                "-p",
                                "e2e-real-canisters",
                                "real_sns_ledger_index_smoke",
                                "--",
                                "--ignored",
                                "--nocapture",
                            ]),
                        );
                        ok &= run(
                            "real-framework: SNS ledger/index same-Wasm upgrade",
                            cargo_test(&[
                                "-p",
                                "e2e-real-canisters",
                                "real_sns_ledger_index_same_wasm_upgrade_preserves_balances_history_and_duplicates",
                                "--",
                                "--ignored",
                                "--nocapture",
                            ]),
                        );
                    }
                }
                Ok(false) => eprintln!(
                    "skipping real_sns_ledger_index_tests ignored layer: real Wasm artifacts are not configured"
                ),
                Err(err) => {
                    eprintln!("✗ real_sns_ledger_index_tests: {err}");
                    ok = false;
                }
            }
        }
        "real_sns_ledger_index_required" => {
            match check_real_canister_artifact_manifest_at(&root, true) {
                Ok(true) => {}
                Ok(false) => {
                    eprintln!(
                        "✗ real_sns_ledger_index_required: real Wasm artifacts are not configured"
                    );
                    ok = false;
                }
                Err(err) => {
                    eprintln!("✗ real_sns_ledger_index_required: {err}");
                    ok = false;
                }
            }
            if env::var_os("POCKET_IC_BIN").is_none() {
                eprintln!("✗ real_sns_ledger_index_required: POCKET_IC_BIN is not set");
                ok = false;
            }
            if ok {
                ok &= run_subcommand("real_sns_ledger_index_tests");
            }
        }
        "real_sns_governance_tests" => {
            ok &= run_subcommand("real_canister_harness_check");
            ok &= run(
                "unit: e2e-real-canisters governance placeholder registration",
                cargo_test(&[
                    "-p",
                    "e2e-real-canisters",
                    "real_sns_governance_staking_smoke",
                ]),
            );
            eprintln!(
                "skipping real_sns_governance_tests ignored layer: normal SNS staking driver is blocked until pinned governance/root artifacts and init shape are supplied"
            );
        }
        "real_sns_governance_required" => {
            eprintln!(
                "✗ real_sns_governance_required: pinned real SNS governance/root artifacts and normal staking init driver are not implemented"
            );
            ok = false;
        }
        "real_io_e2e_tests" => {
            ok &= run_subcommand("real_canister_harness_check");
            ok &= run(
                "unit: e2e-real-canisters full E2E placeholder registration",
                cargo_test(&[
                    "-p",
                    "e2e-real-canisters",
                    "real_canister_e2e_icp_to_io_stake_reward_redemption",
                ]),
            );
            eprintln!(
                "skipping real_io_e2e_tests ignored layer: all-real ICP/SNS/NNS framework artifacts and IO integration driver are not complete"
            );
        }
        "real_io_e2e_required" => {
            eprintln!(
                "✗ real_io_e2e_required: all-real ICP/SNS/NNS framework artifacts and IO integration driver are not complete"
            );
            ok = false;
        }
        "e2e_real_coverage_check" => {
            ok &= run_subcommand("e2e_coverage_matrix_check");
            ok &= run_subcommand("real_canister_harness_check");
            ok &= run_subcommand("real_canister_artifact_manifest_check");
            ok &= run_subcommand("real_sns_ledger_index_tests");
        }
        "local_sns_evidence_tests" => {
            if env::var("IO_LOCAL_SNS_REHEARSAL_ACK").as_deref() != Ok("local-only") {
                eprintln!(
                    "skipping local_sns_evidence_tests: set IO_LOCAL_SNS_REHEARSAL_ACK=local-only"
                );
            } else {
                let path = env::var("IO_LOCAL_SNS_EVIDENCE").unwrap_or_else(|_| {
                    "deploy/local-sns-rehearsal/canister-ids.local.toml".into()
                });
                if !Path::new(&path).exists() {
                    eprintln!("skipping local_sns_evidence_tests: {path} is absent");
                } else {
                    match fs::read_to_string(&path)
                        .map_err(|err| format!("{path}: {err}"))
                        .and_then(|text| parse_local_sns_evidence(&path, &text).map(|_| ()))
                    {
                        Ok(()) => eprintln!("✓ local_sns_evidence_tests"),
                        Err(err) => {
                            eprintln!("✗ local_sns_evidence_tests: {err}");
                            ok = false;
                        }
                    }
                }
            }
        }
        "sns_apy_policy_tests" => {
            ok &= run(
                "unit: io-reward-policy SNS/APY policy",
                cargo_test(&["-p", "io-reward-policy"]),
            );
        }
        "historian_tests" => {
            ok &= run_subcommand("did_surface");
            ok &= run(
                "unit: io-historian",
                cargo_test(&["-p", "io-historian", "--lib"]),
            );
        }
        "historian_required" => {
            if env::var_os("POCKET_IC_BIN").is_none() {
                eprintln!("✗ historian_required: POCKET_IC_BIN is not set");
                ok = false;
            } else {
                ok &= run_subcommand("historian_tests");
                ok &= run_subcommand("build_debug_canisters");
                ok &= run(
                    "pocketic: io-historian",
                    cargo_test(&["-p", "io-historian", "--test", "io_historian_pocketic"]),
                );
            }
        }
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
        "sns_root_lifecycle_tests" => {
            match check_sns_root_lifecycle_at(&root) {
                Ok(()) => eprintln!("✓ sns_root_lifecycle guardrails"),
                Err(err) => {
                    eprintln!("✗ sns_root_lifecycle guardrails: {err}");
                    ok = false;
                }
            }
            ok &= run(
                "unit: io-sns-lifecycle",
                cargo_test(&["-p", "io-sns-lifecycle"]),
            );
            ok &= run("unit: mock-sns-root", cargo_test(&["-p", "mock-sns-root"]));
            ok &= run(
                "unit: mock-sns-governance upgrade proposals",
                cargo_test(&["-p", "mock-sns-governance", "upgrade_proposal"]),
            );
            ok &= run(
                "unit: xtask sns root lifecycle",
                cargo_test(&["-p", "xtask", "sns_root_lifecycle"]),
            );
        }
        "sns_root_lifecycle_required" => {
            if env::var_os("POCKET_IC_BIN").is_none() {
                eprintln!("✗ sns_root_lifecycle_required: POCKET_IC_BIN is not set");
                ok = false;
            } else {
                ok &= run_subcommand("sns_root_lifecycle_tests");
                ok &= run_subcommand("build_debug_canisters");
                ok &= run(
                    "pocketic: io-sns-root-lifecycle",
                    cargo_test(&[
                        "-p",
                        "io-stream-manager",
                        "--test",
                        "io_sns_root_lifecycle_pocketic",
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
                "validate_production_wiring",
                "validate_historian_freshness",
                "validate_stable_storage",
                "validate_local_sns_rehearsal",
                "validate_local_sns_scripts",
                "e2e_coverage_matrix_check",
                "real_canister_harness_check",
                "real_canister_artifact_manifest_check",
                "e2e_real_coverage_check",
                "sns_apy_policy_tests",
                "historian_tests",
                "frontend_required",
                "sns_harness_check",
                "sns_config_validate",
                "sns_official_testing_check",
                "sns_launch_readiness_check",
                "sns_governance_read_tests",
                "sns_ledger_index_tests",
                "sns_root_lifecycle_tests",
                "security_scan_required",
            ] {
                ok &= run_subcommand(sub);
            }
        }
        "build_debug_canisters" => {
            ok &= run_subcommand("frontend_setup");
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
                "mock-sns-root",
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
            ok &= run_subcommand("e2e_coverage_matrix_check");
            ok &= run_subcommand("real_canister_harness_check");
            ok &= run_subcommand("real_canister_artifact_manifest_check");
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
                "unit: historian and frontend",
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
            ok &= run(
                "pocketic: io-historian",
                cargo_test(&["-p", "io-historian", "--test", "io_historian_pocketic"]),
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
                ok &= run(
                    "pocketic: io-sns-root-lifecycle",
                    cargo_test(&[
                        "-p",
                        "io-stream-manager",
                        "--test",
                        "io_sns_root_lifecycle_pocketic",
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
        "historian_pocketic_integration" => {
            ok &= run(
                "pocketic: io-historian",
                cargo_test(&["-p", "io-historian", "--test", "io_historian_pocketic"]),
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
                "validate_production_wiring",
                "validate_historian_freshness",
                "validate_stable_storage",
                "validate_local_sns_rehearsal",
                "validate_local_sns_scripts",
                "security_scan_required",
                "test_unit",
                "frontend_required",
                "test_pocketic_required",
                "sns_root_lifecycle_required",
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
    use io_sns_lifecycle::{
        verify_manifest_entry_paths, verify_upgrade_proposal_against_manifest,
        UpgradeProposalRequest,
    };

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
We currently run SNS-shaped mock/PocketIC tests.
Pure model tests remain the main accounting guardrail.
Mock and PocketIC tests remain the main journal, retry, and upgrade guardrail.
## Four-Layer Compatibility Model
not official SNS launch tests not SNS-W not decentralization swap not mainnet testflight
## Official SNS Local Launch Rehearsal
dfx-based SNS testing for IO is optional, local-only, and not part of `test_ci` or `verify_release`.
## IO-Owned PocketIC SNS Harness
This must not call mainnet, must not use `--network ic`, and is not production launch configuration.
"#,
        );
        write(
            root,
            "tools/sns/README.md",
            "official SNS compatibility package\nLayer 1\nLayer 2\nLayer 3\nLayer 4\nnot production launch configuration\nmust not depend on `dfx`\nmust not use `--network ic`\nplaceholder principals\nIO_TEST ledger is non-canonical\n",
        );
        let sns_template = r#"# not production-ready placeholder
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
fallback_controller_principals:
  - "TODO_LOCAL_FALLBACK_CONTROLLER_PRINCIPAL_PLACEHOLDER"
dapp_canisters:
  io_stream_manager: "TODO_LOCAL_IO_STREAM_MANAGER_CANISTER_PLACEHOLDER"
  io_nns_neuron_manager: "TODO_LOCAL_IO_NNS_NEURON_MANAGER_CANISTER_PLACEHOLDER"
  io_historian: "TODO_LOCAL_IO_HISTORIAN_CANISTER_PLACEHOLDER"
  frontend: "TODO_LOCAL_FRONTEND_CANISTER_PLACEHOLDER"
io_constructor_arg_mapping:
  io_stream_manager:
    icp_ledger_principal_text: "TODO"
    icp_index_principal_text: "TODO"
    io_ledger_principal_text: "TODO"
    io_index_principal_text: "TODO"
    io_sns_ledger_principal_text: "TODO_LOCAL_SNS_LEDGER_PLACEHOLDER"
    io_sns_index_principal_text: "TODO_LOCAL_SNS_INDEX_PLACEHOLDER"
    sns_governance_principal_text: "TODO_LOCAL_SNS_GOVERNANCE_PLACEHOLDER"
  io_nns_neuron_manager:
    nns_governance_principal_text: "TODO"
    icp_ledger_principal_text: "TODO"
    icp_index_principal_text: "TODO"
canonical_ledger_note: "IO_TEST ledger is non-canonical"
"#;
        write(root, "tools/sns/sns_init.io.local.yaml", sns_template);
        write(root, "tools/sns/sns_init.io.template.yaml", sns_template);
        write(
            root,
            "tools/sns/sns_init.io.testflight.template.yaml",
            &format!(
                "{sns_template}\nTODO_TESTFLIGHT_FALLBACK_CONTROLLER_PRINCIPAL_PLACEHOLDER\nTODO_TESTFLIGHT_IO_STREAM_MANAGER_CANISTER_PLACEHOLDER\nTODO_FINAL_TOKENOMICS\nTODO_FINAL_SWAP_PARAMETERS\nTODO_FINAL_DEVELOPER_NEURONS\nTODO_FINAL_TREASURY_DISTRIBUTION\nTODO_FINAL_LOGO_URL_SUMMARY\nTODO_FINAL_SNS_PROPOSAL_FORUM_URL\n"
            ),
        );
        write(
            root,
            "tools/sns/testflight/sns_init.testflight.template.yaml",
            sns_template,
        );
        write(
            root,
            "docs/operations/official-sns-testing.md",
            "We currently run SNS-shaped mock/PocketIC tests.\nWe do not currently run the official SNS launch locally in required CI.\nOfficial SNS testing is optional and heavier.\nThe current official ICP/DFINITY SNS testing documentation is the source of truth.\nThe historical standalone `dfinity/sns-testing` repository is deprecated.\nThe official SNS launch path may require `dfx sns`; this is not part of required IO workflows.\nSNS testflight is a future manual/mainnet rehearsal.\nIO's canonical IO ledger should be the SNS ledger; any IO_TEST ledger is non-canonical.\nThe existing canister that owns IO NNS neuron 6345890886899317159 is not touched by these tests.\nLayer 1\nLayer 2\nLayer 3\nLayer 4\n",
        );
        write(
            root,
            "tools/sns-testing/check-prereqs.sh",
            "#!/usr/bin/env bash\n# optional local\n",
        );
        write(
            root,
            "tools/sns-testing/deploy-io-dapp-local.sh",
            "#!/usr/bin/env bash\n# optional local\n",
        );
        write(
            root,
            "tools/sns-testing/run-local-sns-testing.sh",
            "#!/usr/bin/env bash\n# optional local\n",
        );
        write(
            root,
            "tools/sns-testing/validate-local-sns-config.sh",
            "#!/usr/bin/env bash\n# optional local\n",
        );
        write(
            root,
            "tools/sns/testflight/README.md",
            "manual mainnet not CI not a real launch no real swap\n",
        );
        write(
            root,
            "tools/sns/launch-readiness.toml",
            "[source_open]\nstatus = \"incomplete\"\n[reproducible_builds]\nstatus = \"incomplete\"\n[security_review]\nstatus = \"incomplete\"\n[sns_config_validated]\nstatus = \"incomplete\"\n[local_sns_testing_rehearsal]\nstatus = \"incomplete\"\n[mainnet_testflight]\nstatus = \"incomplete\"\n[app_canisters_stable_on_mainnet]\nstatus = \"incomplete\"\n[nns_root_co_controller_step_planned]\nstatus = \"incomplete\"\n[fallback_controllers_defined]\nstatus = \"incomplete\"\n[dapp_canisters_listed]\nstatus = \"incomplete\"\n[all_upgrades_tested_via_sns_proposal]\nstatus = \"incomplete\"\n[frontend_sns_integration_tested]\nstatus = \"incomplete\"\n[cycles_management_strategy]\nstatus = \"incomplete\"\n[custom_domain_frontend_plan]\nstatus = \"incomplete\"\n[audit_package]\nstatus = \"incomplete\"\n",
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

    fn write_local_sns_rehearsal_fixture(root: &Path) {
        write(
            root,
            "deploy/local-sns-rehearsal/README.md",
            "local-only real SNS-created IO ledger/index/governance/root stack not final tokenomics not a mainnet SNS proposal not required CI Do not use `--network ic` protocol reserve reserve-to-user transfer user-to-reserve transfer validate_local_sns_rehearsal validate_local_sns_ledger validate_local_sns_scripts Human-readable local evidence-derived wiring Not accepted by production wiring validators Do not use as install args\n",
        );
        write(
            root,
            "deploy/local-sns-rehearsal/sns_init.local.template.yaml",
            "Local-only\nNot final tokenomics\nNot a mainnet SNS proposal\nfallback_controller_principals\n{{fallback_controller_principal}}\ndapp_canisters\nToken:\nsymbol: \"IO\"\ntransaction_fee\nDistribution:\nprotocol_reserve\nSwap:\narchive_options\nissuance_model: \"protocol reserve transfer\"\nredemption_model: \"user transfer back to protocol reserve\"\nio_test_ledger_role: \"non-canonical staging only\"\nTODO_LOCAL\n",
        );
        write(
            root,
            "deploy/local-sns-rehearsal/canister-ids.local.example.toml",
            "network = \"local\"\nsource = \"official-local-sns-rehearsal\"\n[sns_canisters]\nroot = \"TODO\"\ngovernance = \"TODO\"\nledger = \"TODO\"\nindex = \"TODO\"\nswap = \"TODO\"\narchive = \"TODO\"\n[expected_local_sns_config]\ntoken_symbol = \"IO\"\ntransaction_fee_e8s = 10_000\ntotal_supply_e8s = 1\n[ledger_evidence]\ntransaction_fee_e8s = 10_000\ntotal_supply_e8s = 1\nprotocol_reserve_balance_e8s = 1\nreserve_transfer_amount_e8s = 1\nredemption_return_amount_e8s = 1\nbad_fee_error_observed = true\ninsufficient_funds_error_observed = true\nduplicate_transfer_observed = true\nduplicate_block_verified = true\nindex_account_history_observed = true\n[issuance_model]\nresolved_as = \"protocol_reserve_transfer\"\nminting_assumed = false\ntotal_supply_constant_across_issuance_redemption = true\n",
        );
        for path in [
            "deploy/local-sns-rehearsal/runbook.sh",
            "deploy/local-sns-rehearsal/scripts/00-check-prereqs.sh",
            "deploy/local-sns-rehearsal/scripts/01-render-sns-init.sh",
            "deploy/local-sns-rehearsal/scripts/02-record-canister-ids.sh",
            "deploy/local-sns-rehearsal/scripts/03-capture-ledger-evidence.sh",
            "deploy/local-sns-rehearsal/scripts/04-render-local-wiring.sh",
            "deploy/local-sns-rehearsal/scripts/05-validate-evidence.sh",
        ] {
            write(
                root,
                path,
                "#!/usr/bin/env bash\n# local-only optional\nrequire_local_script_guard \"$@\"\n: \"${IO_LOCAL_SNS_REHEARSAL_ACK:?local-only}\"\n",
            );
        }
        write(
            root,
            "deploy/local-sns-rehearsal/commands.local.example.md",
            "Local-only IO_LOCAL_SNS_REHEARSAL_ACK=local-only icrc1_symbol icrc1_fee icrc1_total_supply icrc1_balance_of icrc1_transfer get_account_transactions governance root\n",
        );
        write(
            root,
            "docs/operations/sns-testing-layers.md",
            "real SNS-created SNS-W IO_TEST non-canonical protocol reserve not launched on mainnet\n",
        );
        write(
            root,
            "docs/operations/official-local-sns-rehearsal.md",
            "real SNS-created SNS-W IO_TEST non-canonical protocol reserve not launched on mainnet\n",
        );
        write(
            root,
            "docs/operations/mainnet-readiness.md",
            "real SNS-created SNS-W IO_TEST non-canonical protocol reserve not launched on mainnet\n",
        );
    }

    fn completed_local_sns_evidence() -> String {
        r#"[mode]
network = "local"
source = "official-local-sns-rehearsal"
dfx_sns = "manual-local-only"
io_protocol_live = false
sns_io_ledger_mainnet_launched = false

[expected_local_sns_config]
token_symbol = "IO"
transaction_fee_e8s = 10000
total_supply_e8s = 100000000000000

[sns_canisters]
root = "bkyz2-fmaaa-aaaaa-qaaaq-cai"
governance = "bd3sg-teaaa-aaaaa-qaaba-cai"
ledger = "br5f7-7uaaa-aaaaa-qaaca-cai"
index = "be2us-64aaa-aaaaa-qaabq-cai"
swap = "bw4dl-smaaa-aaaaa-qaacq-cai"
archive = "by6od-j4aaa-aaaaa-qaadq-cai"

[io_dapp_canisters]
io_stream_manager = "avqkn-guaaa-aaaaa-qaaea-cai"
io_nns_neuron_manager = "aax3a-h4aaa-aaaaa-qaahq-cai"
io_historian = "ajuq4-ruaaa-aaaaa-qaaga-cai"
frontend = "b77ix-eeaaa-aaaaa-qaada-cai"

[ledger_evidence]
token_symbol = "IO"
transaction_fee_e8s = 10000
total_supply_e8s = 100000000000000
protocol_reserve_account_owner = "a3shf-5eaaa-aaaaa-qaafa-cai"
protocol_reserve_subaccount_hex = "none"
protocol_reserve_balance_e8s = 60000000000000
reserve_transfer_block_index = 1
redemption_return_block_index = 2
reserve_transfer_amount_e8s = 100000000
redemption_return_amount_e8s = 100000000
bad_fee_error_observed = true
insufficient_funds_error_observed = true
duplicate_transfer_observed = true
duplicate_block_verified = true
duplicate_of_block_index = 1
index_account_history_observed = true
index_history_order = "descending"
index_lag_or_archive_required_observed = "not-observed"

[governance_evidence]
governance_available = true
root_available = true
swap_available = true
dapp_controller_state_checked = true
governance_upgrade_proposal_tested = false
governance_upgrade_gap = "local tooling did not support upgrade proposal in this run"

[issuance_model]
resolved_as = "protocol_reserve_transfer"
minting_assumed = false
treasury_transfer_assumed = false
total_supply_constant_across_issuance_redemption = true

[protected]
must_not_touch_neuron_owner_canister = "oae4c-3iaaa-aaaar-qb5qq-cai"
must_not_touch_io_nns_neuron_id = "6345890886899317159"
"#
        .to_string()
    }

    fn write_completed_local_sns_evidence(root: &Path) {
        write(
            root,
            "deploy/local-sns-rehearsal/canister-ids.local.toml",
            &completed_local_sns_evidence(),
        );
    }

    fn assert_local_sns_evidence_rejects(mutator: impl FnOnce(String) -> String, needle: &str) {
        let text = mutator(completed_local_sns_evidence());
        let err =
            parse_local_sns_evidence("deploy/local-sns-rehearsal/canister-ids.local.toml", &text)
                .unwrap_err();
        assert!(
            err.contains(needle),
            "expected {err:?} to contain {needle:?}"
        );
    }

    fn write_did_surface_fixture(root: &Path) {
        write(
            root,
            "canisters/io_stream_manager/io_stream_manager.did",
            "type InitArgs = record {};\nservice : (InitArgs) -> {}\n",
        );
        write(
            root,
            "canisters/io_nns_neuron_manager/io_nns_neuron_manager.did",
            "type InitArgs = record {};\nservice : (InitArgs) -> {}\n",
        );
        write(
            root,
            "canisters/io_historian/io_historian.did",
            "service : {\n  get_dashboard_state : () -> (text) query;\n  get_protocol_snapshot : () -> (text) query;\n  get_redemption_rate : () -> (text) query;\n  list_streams : () -> (text) query;\n  list_redemptions : () -> (text) query;\n  list_rewards : () -> (text) query;\n  list_nns_lifecycle_events : () -> (text) query;\n  get_index_health : () -> (text) query;\n  get_governance_summary : () -> (text) query;\n  get_release_artifacts : () -> (text) query;\n  get_canister_status_summary : () -> (text) query;\n  get_public_status : () -> (text) query;\n  get_reserve_snapshot : () -> (text) query;\n  list_governance_participation : () -> (text) query;\n  version : () -> (text) query;\n}\n",
        );
        write(
            root,
            "canisters/io_stream_manager/io_stream_manager_debug.did",
            "service : {\n  debug_get_state : () -> (text) query;\n  debug_get_redemption_rate : () -> (text) query;\n  debug_process_stream_event : () -> (text);\n  debug_redeem : () -> (text);\n  debug_tick : () -> (text);\n}\n",
        );
        write(
            root,
            "canisters/io_nns_neuron_manager/io_nns_neuron_manager_debug.did",
            "service : {\n  debug_get_config : () -> (text) query;\n  debug_get_state : () -> (text) query;\n  debug_plan_rebalance : () -> (text);\n  debug_advance_model_time : () -> (text);\n  debug_tick : () -> (text);\n}\n",
        );
        write(
            root,
            "canisters/io_historian/io_historian_debug.did",
            "service : {\n  debug_clear : () -> ();\n  debug_ingest_ledger_flow : () -> ();\n  debug_ingest_stream_record : () -> ();\n  debug_ingest_redemption_record : () -> ();\n  debug_ingest_reward_record : () -> ();\n  debug_ingest_index_health : () -> ();\n  debug_ingest_governance_snapshot : () -> ();\n  debug_ingest_canister_artifact_status : () -> ();\n}\n",
        );
        write(
            root,
            "canisters/frontend/web/declarations/io_historian/io_historian.did.js",
            "export const idlFactory = ({ IDL }) => IDL.Service({\n  get_canister_status_summary: IDL.Func([], [], [\"query\"]),\n  get_dashboard_state: IDL.Func([], [], [\"query\"]),\n  get_governance_summary: IDL.Func([], [], [\"query\"]),\n  get_index_health: IDL.Func([], [], [\"query\"]),\n  get_protocol_snapshot: IDL.Func([], [], [\"query\"]),\n  get_public_status: IDL.Func([], [], [\"query\"]),\n  get_redemption_rate: IDL.Func([], [], [\"query\"]),\n  get_release_artifacts: IDL.Func([], [], [\"query\"]),\n  get_reserve_snapshot: IDL.Func([], [], [\"query\"]),\n  list_governance_participation: IDL.Func([], [], [\"query\"]),\n  list_nns_lifecycle_events: IDL.Func([], [], [\"query\"]),\n  list_redemptions: IDL.Func([], [], [\"query\"]),\n  list_rewards: IDL.Func([], [], [\"query\"]),\n  list_streams: IDL.Func([], [], [\"query\"]),\n  version: IDL.Func([], [], [\"query\"]),\n});\n",
        );
        write(
            root,
            "canisters/frontend/web/declarations/io_historian/index.js",
            "import { idlFactory } from \"./io_historian.did.js\";\nexport { idlFactory };\n",
        );
    }

    fn dev_mainnet_config() -> String {
        format!(
            r#"[environment]
name = "DevMainnet"
network = "ic"
subnet_type = "non_fiduciary_or_unknown"
status = "DevOnly"
production = false

[phase]
mode = "LegacyPhase1DevPublicShell"
record_date = "2026-06-06"
release_artifact_manifest = "release-artifacts/manifest.json"

[canisters]
io_historian = "{DEV_MAINNET_HISTORIAN_CANISTER_ID}"
frontend = "{DEV_MAINNET_FRONTEND_CANISTER_ID}"

[frontend]
gateway_url = "https://{DEV_MAINNET_FRONTEND_CANISTER_ID}.icp0.io/"
raw_url = "https://{DEV_MAINNET_FRONTEND_CANISTER_ID}.raw.icp0.io/"
built_with_canister_id_io_historian = "{DEV_MAINNET_HISTORIAN_CANISTER_ID}"

[not_deployed]
io_stream_manager = true
io_nns_neuron_manager = true

[not_touched]
existing_io_neuron_owner_canister = "oae4c-3iaaa-aaaar-qb5qq-cai"
io_neuron_id = "6345890886899317159"

[status]
io_protocol_live = false
sns_io_ledger_launched = false
io_issuance_live = false
io_redemption_live = false
"#
        )
    }

    fn write_dev_mainnet_public_shell_fixture(root: &Path) {
        write_did_surface_fixture(root);
        write(root, DEV_MAINNET_CONFIG_PATH, &dev_mainnet_config());
        let phase_doc = format!(
            r#"# Legacy Phase 1 Dev
DevMainnet
LegacyPhase1DevPublicShell
superseded as production targets
dev/test
not on the fiduciary subnet
not production IO protocol canisters
frontend {DEV_MAINNET_FRONTEND_CANISTER_ID}
io_historian {DEV_MAINNET_HISTORIAN_CANISTER_ID}
https://{DEV_MAINNET_FRONTEND_CANISTER_ID}.icp0.io/
https://{DEV_MAINNET_FRONTEND_CANISTER_ID}.raw.icp0.io/
CANISTER_ID_IO_HISTORIAN={DEV_MAINNET_HISTORIAN_CANISTER_ID}
No value-moving protocol canister is live.
io_stream_manager is not deployed.
io_nns_neuron_manager is not deployed.
The existing IO neuron-owner canister oae4c-3iaaa-aaaar-qb5qq-cai is not touched.
IO neuron 6345890886899317159 is not touched.
IO protocol is not live.
The canonical SNS IO ledger is not launched.
IO issuance is not live.
IO redemption is not live.
Historian is a public read model, not protocol truth.
release-artifacts/manifest.json
"#
        );
        write(root, DEV_MAINNET_README_PATH, &phase_doc);
        write(root, DEV_MAINNET_STATUS_PATH, &phase_doc);
        for path in [
            "docs/operations/mainnet-readiness.md",
            "docs/operations/mainnet-prelaunch-dry-run.md",
            "docs/architecture/canister-roles.md",
            "docs/architecture/historian.md",
            "canisters/frontend/README.md",
            "canisters/io_historian/README.md",
        ] {
            write(root, path, &phase_doc);
        }
        write(
            root,
            "tools/scripts/required-check",
            "#!/usr/bin/env bash\ncargo test\n",
        );
    }

    fn production_wiring_template(mode: &str) -> String {
        format!(
            r#"[environment]
mode = "{mode}"
io_ledger_role = "FutureCanonicalSnsIo"
fixture_marked = false
status = "ReservedNotLive"
io_protocol_live = false
value_moving_logic_installed = false
io_issuance_live = false
io_redemption_live = false

[principals]
icp_ledger = "ryjl3-tyaaa-aaaaa-aaaba-cai"
icp_index = "qhbym-qaaaa-aaaaa-aaafq-cai"
nns_governance = "rrkah-fqaaa-aaaaa-aaaaq-cai"
nns_ledger = "ryjl3-tyaaa-aaaaa-aaaba-cai"
nns_index = "qhbym-qaaaa-aaaaa-aaafq-cai"
sns_root = "qaa6y-5yaaa-aaaaa-aaafa-cai"
sns_governance = "r7inp-6aaaa-aaaaa-aaabq-cai"
sns_ledger = "qjdve-lqaaa-aaaaa-aaaeq-cai"
sns_index = "renrk-eyaaa-aaaaa-aaada-cai"
io_ledger = "qjdve-lqaaa-aaaaa-aaaeq-cai"
io_index = "renrk-eyaaa-aaaaa-aaada-cai"

[fees]
icp_transfer_fee_e8s = 10_000
io_ledger_transfer_fee_e8s = 10_000
tiny_value_policy_max_fee_e8s = 1_000_000
allow_zero_fees_for_mock_or_local = false

[protected]
neuron_owner_canister = "oae4c-3iaaa-aaaar-qb5qq-cai"
io_nns_neuron_id = 6_345_890_886_899_317_159

[deployment_targets]
io_stream_manager = "thset-pqaaa-aaaar-qb7wa-cai"
io_nns_neuron_manager = "tatch-ciaaa-aaaar-qb7wq-cai"
mutation_target_principals = []
mutation_target_nns_neuron_ids = []
"#
        )
    }

    fn production_canister_ids() -> &'static str {
        r#"[environment]
name = "Production"
network = "ic"
subnet_type = "fiduciary"
status = "ReservedNotLive"
io_protocol_live = false
value_moving_logic_installed = false
io_issuance_live = false
io_redemption_live = false

[canisters]
io_stream_manager = "thset-pqaaa-aaaar-qb7wa-cai"
io_nns_neuron_manager = "tatch-ciaaa-aaaar-qb7wq-cai"
io_historian = "tjqj3-uaaaa-aaaar-qb7xa-cai"
frontend = "torpp-zyaaa-aaaar-qb7xq-cai"

[notes]
description = "Production fiduciary-subnet canisters are reserved placeholders only. They are not live protocol deployments."
"#
    }

    fn production_mapping_doc() -> &'static str {
        r#"
io_stream_manager thset-pqaaa-aaaar-qb7wa-cai
io_nns_neuron_manager tatch-ciaaa-aaaar-qb7wq-cai
io_historian tjqj3-uaaaa-aaaar-qb7xa-cai
frontend torpp-zyaaa-aaaar-qb7xq-cai
"#
    }

    fn production_canister_roles_doc() -> &'static str {
        r#"
## io_nns_neuron_manager
Production fiduciary status: reserved as `tatch-ciaaa-aaaar-qb7wq-cai`, `ReservedNotLive`.

## io_stream_manager
Production fiduciary status: reserved as `thset-pqaaa-aaaar-qb7wa-cai`, `ReservedNotLive`.

## io_historian
Production fiduciary status: reserved as `tjqj3-uaaaa-aaaar-qb7xa-cai`, `ReservedNotLive`.

## frontend
Production fiduciary status: reserved as `torpp-zyaaa-aaaar-qb7xq-cai`, `ReservedNotLive`.
"#
    }

    fn write_production_wiring_fixture(root: &Path) {
        write_dev_mainnet_public_shell_fixture(root);
        write(
            root,
            "deploy/production-wiring/template.toml",
            &production_wiring_template("ProductionPlanned"),
        );
        write(
            root,
            "deploy/production-wiring/dry-run.example.toml",
            &production_wiring_template("DryRun"),
        );
        write(
            root,
            PRODUCTION_CANISTER_IDS_PATH,
            production_canister_ids(),
        );
        let doc = r#"
dry-run/config validation only
No production execution is active
IO protocol remains not live
SNS IO ledger is not launched
production activation is a later audited milestone
oae4c-3iaaa-aaaar-qb5qq-cai
6345890886899317159
use `icp-cli` convention
required workflows do not use `dfx`
IO_TEST ledger is non-canonical
Production Wiring Checklist
ReservedNotLive
reserved
empty/inert
not live
no value-moving Wasm installed
no production activation has happened
no IO issuance/redemption is enabled
io_stream_manager thset-pqaaa-aaaar-qb7wa-cai
io_nns_neuron_manager tatch-ciaaa-aaaar-qb7wq-cai
io_historian tjqj3-uaaaa-aaaar-qb7xa-cai
frontend torpp-zyaaa-aaaar-qb7xq-cai
thset-pqaaa-aaaar-qb7wa-cai
tatch-ciaaa-aaaar-qb7wq-cai
tjqj3-uaaaa-aaaar-qb7xa-cai
torpp-zyaaa-aaaar-qb7xq-cai
Template SNS principal values are planned wiring placeholders only.
"#;
        write(root, "deploy/production-wiring/README.md", doc);
        write(root, "docs/operations/production-wiring.md", doc);
        write(root, "docs/operations/prelaunch-config-validation.md", doc);
        write(
            root,
            "docs/operations/mainnet-readiness.md",
            production_mapping_doc(),
        );
        write(
            root,
            "docs/architecture/canister-roles.md",
            production_canister_roles_doc(),
        );
        write(root, "README.md", production_mapping_doc());
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
    fn sns_root_lifecycle_manifest_resolves_known_upgrade_artifacts() {
        let root = temp_root("sns-root-lifecycle-manifest-good");
        write_artifact_set(&root);
        let manifest = io_sns_lifecycle::read_manifest(root.join(MANIFEST_PATH)).unwrap();

        for canister in ["io_stream_manager", "io_nns_neuron_manager"] {
            let entry = io_sns_lifecycle::resolve_manifest_entry(&manifest, canister).unwrap();
            verify_manifest_entry_paths(&root, entry).unwrap();
            let request = UpgradeProposalRequest {
                target_canister: Principal::anonymous(),
                wasm_sha256: entry.raw_wasm_sha256.clone(),
                wasm_gz_sha256: entry.gz_wasm_sha256.clone(),
                artifact_name: canister.to_string(),
                artifact_path: entry.raw_wasm_path.clone(),
                expected_module_hash: Some(entry.raw_wasm_sha256.clone()),
            };
            assert_eq!(
                verify_upgrade_proposal_against_manifest(&manifest, canister, &request)
                    .unwrap()
                    .canister,
                canister
            );
        }
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn sns_root_lifecycle_manifest_rejects_missing_and_mismatched_upgrade_artifacts() {
        let root = temp_root("sns-root-lifecycle-manifest-bad");
        write_artifact_set(&root);
        let manifest = io_sns_lifecycle::read_manifest(root.join(MANIFEST_PATH)).unwrap();
        assert!(
            io_sns_lifecycle::resolve_manifest_entry(&manifest, "missing_canister")
                .unwrap_err()
                .contains("missing artifact")
        );

        let entry =
            io_sns_lifecycle::resolve_manifest_entry(&manifest, "io_stream_manager").unwrap();
        let mut request = UpgradeProposalRequest {
            target_canister: Principal::anonymous(),
            wasm_sha256: entry.raw_wasm_sha256.clone(),
            wasm_gz_sha256: entry.gz_wasm_sha256.clone(),
            artifact_name: "io_stream_manager".to_string(),
            artifact_path: entry.raw_wasm_path.clone(),
            expected_module_hash: None,
        };
        request.wasm_gz_sha256 = "wrong".to_string();
        assert!(
            verify_upgrade_proposal_against_manifest(&manifest, "io_stream_manager", &request)
                .unwrap_err()
                .contains("gz wasm hash mismatch")
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn sns_root_lifecycle_manifest_rejects_stale_entry_size() {
        let root = temp_root("sns-root-lifecycle-manifest-stale");
        write_artifact_set(&root);
        let manifest = io_sns_lifecycle::read_manifest(root.join(MANIFEST_PATH)).unwrap();
        write(
            &root,
            "release-artifacts/io_stream_manager.wasm",
            "changed bytes",
        );
        let entry =
            io_sns_lifecycle::resolve_manifest_entry(&manifest, "io_stream_manager").unwrap();
        assert!(verify_manifest_entry_paths(&root, entry)
            .unwrap_err()
            .contains("stale size"));
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
    fn sns_official_testing_check_rejects_dfx_start_in_optional_deploy_script() {
        let root = temp_root("sns-official-testing-bad-deploy-script");
        write_sns_harness_fixture(&root);
        write(
            &root,
            "tools/sns-testing/deploy-io-dapp-local.sh",
            "#!/usr/bin/env bash\n# optional local\ndfx start\n",
        );
        assert!(check_sns_official_testing_at(&root)
            .unwrap_err()
            .contains("dfx start"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn sns_launch_readiness_reports_incomplete_and_strict_fails() {
        let root = temp_root("sns-launch-readiness-strict");
        write_sns_harness_fixture(&root);
        assert_eq!(check_sns_launch_readiness_at(&root, false).unwrap(), 15);
        assert!(check_sns_launch_readiness_at(&root, true)
            .unwrap_err()
            .contains("incomplete item"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn local_sns_rehearsal_check_accepts_fixture() {
        let root = temp_root("local-sns-rehearsal-good");
        write_local_sns_rehearsal_fixture(&root);
        check_local_sns_rehearsal_at(&root).unwrap();
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn local_sns_ledger_check_skips_without_completed_evidence() {
        let root = temp_root("local-sns-ledger-skip");
        write_local_sns_rehearsal_fixture(&root);
        assert!(!check_local_sns_ledger_at(&root).unwrap());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn local_sns_ledger_check_accepts_completed_evidence() {
        let root = temp_root("local-sns-ledger-good");
        write_local_sns_rehearsal_fixture(&root);
        write_completed_local_sns_evidence(&root);
        assert!(check_local_sns_ledger_at(&root).unwrap());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn local_sns_ledger_check_rejects_placeholders() {
        let root = temp_root("local-sns-ledger-placeholder");
        write_local_sns_rehearsal_fixture(&root);
        write_completed_local_sns_evidence(&root);
        let path = root.join("deploy/local-sns-rehearsal/canister-ids.local.toml");
        let text = fs::read_to_string(&path)
            .unwrap()
            .replace("br5f7-7uaaa-aaaaa-qaaca-cai", "TODO_LOCAL_SNS_LEDGER");
        fs::write(&path, text).unwrap();
        assert!(check_local_sns_ledger_at(&root)
            .unwrap_err()
            .contains("TODO_"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn local_sns_ledger_check_rejects_mainnet_icp_ledger_principal() {
        assert_local_sns_evidence_rejects(
            |text| text.replace("br5f7-7uaaa-aaaaa-qaaca-cai", "ryjl3-tyaaa-aaaaa-aaaba-cai"),
            "known mainnet",
        );
    }

    #[test]
    fn local_sns_ledger_check_rejects_protected_canister_in_local_field() {
        assert_local_sns_evidence_rejects(
            |text| {
                text.replace(
                    "br5f7-7uaaa-aaaaa-qaaca-cai",
                    PROTECTED_IO_NEURON_OWNER_CANISTER,
                )
            },
            "protected canister",
        );
    }

    #[test]
    fn local_sns_ledger_check_rejects_protected_neuron_outside_reminder() {
        assert_local_sns_evidence_rejects(
            |text| {
                text.replace(
                    "index_history_order = \"descending\"",
                    "index_history_order = \"6345890886899317159\"",
                )
            },
            "protected IO neuron",
        );
    }

    #[test]
    fn local_sns_ledger_check_rejects_live_protocol_claim() {
        assert_local_sns_evidence_rejects(
            |text| text.replace("io_protocol_live = false", "io_protocol_live = true"),
            "io_protocol_live",
        );
    }

    #[test]
    fn local_sns_ledger_check_rejects_mainnet_sns_ledger_claim() {
        assert_local_sns_evidence_rejects(
            |text| {
                text.replace(
                    "sns_io_ledger_mainnet_launched = false",
                    "sns_io_ledger_mainnet_launched = true",
                )
            },
            "sns_io_ledger_mainnet_launched",
        );
    }

    #[test]
    fn local_sns_ledger_check_rejects_minting_assumption() {
        assert_local_sns_evidence_rejects(
            |text| text.replace("minting_assumed = false", "minting_assumed = true"),
            "minting_assumed",
        );
    }

    #[test]
    fn local_sns_ledger_check_rejects_treasury_transfer_assumption() {
        assert_local_sns_evidence_rejects(
            |text| {
                text.replace(
                    "treasury_transfer_assumed = false",
                    "treasury_transfer_assumed = true",
                )
            },
            "treasury_transfer_assumed",
        );
    }

    #[test]
    fn local_sns_ledger_check_rejects_missing_duplicate_proof() {
        assert_local_sns_evidence_rejects(
            |text| {
                text.replace(
                    "duplicate_of_block_index = 1",
                    "duplicate_of_block_index = \"none\"",
                )
            },
            "duplicate_of_block_index",
        );
    }

    #[test]
    fn local_sns_ledger_check_rejects_zero_reserve_balance() {
        assert_local_sns_evidence_rejects(
            |text| {
                text.replace(
                    "protocol_reserve_balance_e8s = 60000000000000",
                    "protocol_reserve_balance_e8s = 0",
                )
            },
            "reserve balance",
        );
    }

    #[test]
    fn local_sns_ledger_check_rejects_fee_mismatch() {
        assert_local_sns_evidence_rejects(
            |text| {
                text.replace(
                    "transaction_fee_e8s = 10000\ntotal_supply_e8s = 100000000000000\nprotocol_reserve_account_owner",
                    "transaction_fee_e8s = 10001\ntotal_supply_e8s = 100000000000000\nprotocol_reserve_account_owner",
                )
            },
            "transaction_fee_e8s",
        );
    }

    #[test]
    fn local_sns_ledger_check_rejects_invalid_principal() {
        assert_local_sns_evidence_rejects(
            |text| text.replace("br5f7-7uaaa-aaaaa-qaaca-cai", "not-a-principal"),
            "not a principal",
        );
    }

    #[test]
    fn local_sns_ledger_check_rejects_missing_governance_upgrade_gap() {
        assert_local_sns_evidence_rejects(
            |text| {
                text.replace(
                    "governance_upgrade_gap = \"local tooling did not support upgrade proposal in this run\"",
                    "governance_upgrade_gap = \"\"",
                )
            },
            "governance upgrade gap",
        );
    }

    #[test]
    fn prelaunch_canister_ids_parse_from_dev_mainnet_config() {
        let config = dev_mainnet_config();
        assert_eq!(
            parse_toml_string(&config, "canisters", "frontend").unwrap(),
            DEV_MAINNET_FRONTEND_CANISTER_ID
        );
        assert_eq!(
            parse_toml_string(&config, "canisters", "io_historian").unwrap(),
            DEV_MAINNET_HISTORIAN_CANISTER_ID
        );
    }

    #[test]
    fn prelaunch_status_booleans_are_false() {
        let config = dev_mainnet_config();
        for key in ["io_protocol_live", "io_issuance_live", "io_redemption_live"] {
            assert!(!parse_toml_bool(&config, "status", key).unwrap());
        }
    }

    #[test]
    fn prelaunch_docs_contain_phase1_ids_and_not_touched_records() {
        let root = temp_root("prelaunch-docs");
        write_dev_mainnet_public_shell_fixture(&root);
        let docs = [
            DEV_MAINNET_README_PATH,
            DEV_MAINNET_STATUS_PATH,
            "docs/operations/mainnet-readiness.md",
            "docs/operations/mainnet-prelaunch-dry-run.md",
            "docs/architecture/canister-roles.md",
            "docs/architecture/historian.md",
            "canisters/frontend/README.md",
            "canisters/io_historian/README.md",
        ];
        let mut combined = String::new();
        for path in docs {
            combined.push_str(&read_file(&root, path).unwrap());
        }
        require_present(
            "fixture docs",
            &combined,
            &[
                DEV_MAINNET_FRONTEND_CANISTER_ID,
                DEV_MAINNET_HISTORIAN_CANISTER_ID,
                "not touched",
                KNOWN_CONTROLLER_CANISTER_PRINCIPAL,
                "6345890886899317159",
            ],
        )
        .unwrap();
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn prelaunch_public_shell_validation_accepts_fixture() {
        let root = temp_root("prelaunch-good");
        write_dev_mainnet_public_shell_fixture(&root);
        check_prelaunch_public_shell_at(&root).unwrap();
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn production_wiring_validation_accepts_fixture() {
        let root = temp_root("production-wiring-good");
        write_production_wiring_fixture(&root);
        check_production_wiring_at(&root).unwrap();
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn production_wiring_validation_rejects_swapped_doc_mapping() {
        let root = temp_root("production-wiring-swapped-doc-mapping");
        write_production_wiring_fixture(&root);
        write(
            &root,
            "docs/architecture/canister-roles.md",
            &production_canister_roles_doc()
                .replace(
                    "io_nns_neuron_manager\nProduction fiduciary status: reserved as `tatch-ciaaa-aaaar-qb7wq-cai`",
                    "io_nns_neuron_manager\nProduction fiduciary status: reserved as `thset-pqaaa-aaaar-qb7wa-cai`",
                )
                .replace(
                    "io_stream_manager\nProduction fiduciary status: reserved as `thset-pqaaa-aaaar-qb7wa-cai`",
                    "io_stream_manager\nProduction fiduciary status: reserved as `tatch-ciaaa-aaaar-qb7wq-cai`",
                ),
        );

        let err = check_production_wiring_at(&root).unwrap_err();
        assert!(
            err.contains("io_nns_neuron_manager") || err.contains("io_stream_manager"),
            "expected swapped mapping error, got {err:?}"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn production_wiring_validation_rejects_protected_target() {
        let root = temp_root("production-wiring-protected-target");
        write_production_wiring_fixture(&root);
        let bad = production_wiring_template("ProductionPlanned").replace(
            "io_stream_manager = \"thset-pqaaa-aaaar-qb7wa-cai\"",
            &format!("io_stream_manager = \"{PROTECTED_IO_NEURON_OWNER_CANISTER}\""),
        );
        write(
            root.as_path(),
            "deploy/production-wiring/template.toml",
            &bad,
        );

        assert!(check_production_wiring_at(&root).is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn production_wiring_validation_rejects_system_canister_deployment_targets() {
        for (name, field, canister_id) in [
            (
                "internet-identity",
                "io_stream_manager",
                "rdmx6-jaaaa-aaaaa-aaadq-cai",
            ),
            (
                "nns-dapp",
                "io_nns_neuron_manager",
                "qoctq-giaaa-aaaaa-aaaea-cai",
            ),
        ] {
            let root = temp_root(&format!("production-wiring-system-target-{name}"));
            write_production_wiring_fixture(&root);
            let bad = production_wiring_template("ProductionPlanned").replace(
                &format!(
                    "{field} = \"{}\"",
                    if field == "io_stream_manager" {
                        PRODUCTION_IO_STREAM_MANAGER_CANISTER_ID
                    } else {
                        PRODUCTION_IO_NNS_NEURON_MANAGER_CANISTER_ID
                    }
                ),
                &format!("{field} = \"{canister_id}\""),
            );
            write(
                root.as_path(),
                "deploy/production-wiring/template.toml",
                &bad,
            );

            assert!(check_production_wiring_at(&root).is_err());
            let _ = fs::remove_dir_all(root);
        }
    }

    #[test]
    fn prelaunch_public_shell_rejects_value_moving_canister_marked_deployed() {
        let root = temp_root("prelaunch-value-moving-deployed");
        write_dev_mainnet_public_shell_fixture(&root);
        write(
            &root,
            DEV_MAINNET_CONFIG_PATH,
            &dev_mainnet_config().replace("io_stream_manager = true", "io_stream_manager = false"),
        );
        assert!(check_prelaunch_public_shell_at(&root)
            .unwrap_err()
            .contains("not_deployed.io_stream_manager"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn prelaunch_public_shell_rejects_sns_io_ledger_launched() {
        let root = temp_root("prelaunch-sns-ledger-launched");
        write_dev_mainnet_public_shell_fixture(&root);
        write(
            &root,
            DEV_MAINNET_CONFIG_PATH,
            &dev_mainnet_config().replace(
                "sns_io_ledger_launched = false",
                "sns_io_ledger_launched = true",
            ),
        );
        assert!(check_prelaunch_public_shell_at(&root)
            .unwrap_err()
            .contains("status.sns_io_ledger_launched"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn prelaunch_public_shell_rejects_io_protocol_live() {
        let root = temp_root("prelaunch-protocol-live");
        write_dev_mainnet_public_shell_fixture(&root);
        write(
            &root,
            DEV_MAINNET_CONFIG_PATH,
            &dev_mainnet_config().replace("io_protocol_live = false", "io_protocol_live = true"),
        );
        assert!(check_prelaunch_public_shell_at(&root)
            .unwrap_err()
            .contains("status.io_protocol_live"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn prelaunch_public_shell_rejects_io_issuance_or_redemption_live() {
        let root = temp_root("prelaunch-issuance-live");
        write_dev_mainnet_public_shell_fixture(&root);
        write(
            &root,
            DEV_MAINNET_CONFIG_PATH,
            &dev_mainnet_config().replace("io_issuance_live = false", "io_issuance_live = true"),
        );
        assert!(check_prelaunch_public_shell_at(&root)
            .unwrap_err()
            .contains("status.io_issuance_live"));

        write(
            &root,
            DEV_MAINNET_CONFIG_PATH,
            &dev_mainnet_config()
                .replace("io_redemption_live = false", "io_redemption_live = true"),
        );
        assert!(check_prelaunch_public_shell_at(&root)
            .unwrap_err()
            .contains("status.io_redemption_live"));
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

    #[test]
    fn historian_freshness_gate_requires_dashboard_current_time_path() {
        let good = r#"
pub fn source_health_from_state_at(
    state: &StableState,
    now_timestamp_nanos: u64,
) -> Vec<SourceHealth> {
    vec![]
}

#[cfg(target_family = "wasm")]
fn historian_now_timestamp_nanos() -> u64 {
    ic_cdk::api::time()
}

pub fn source_health_from_state(state: &StableState) -> Vec<SourceHealth> {
    source_health_from_state_at(state, historian_now_timestamp_nanos())
}

pub fn get_dashboard_state() -> PublicDashboardState {
    STATE.with(|cell| {
        let state = cell.borrow();
        PublicDashboardState {
            source_health: source_health_from_state_at(&state, historian_now_timestamp_nanos()),
        }
    })
}
"#;
        check_historian_current_time_path("lib.rs", good).unwrap();

        let bad = r#"
pub fn source_health_from_state(state: &StableState) -> Vec<SourceHealth> {
    let now = latest_observation_timestamp(state);
    source_health_from_state_at(state, now)
}
"#;
        assert!(check_historian_current_time_path("lib.rs", bad).is_err());
    }

    fn historian_did() -> &'static str {
        "service : {\n  get_dashboard_state : () -> (text) query;\n  version : () -> (text) query;\n}\n"
    }

    fn historian_js() -> &'static str {
        "export const idlFactory = ({ IDL }) => IDL.Service({\n  get_dashboard_state: IDL.Func([], [IDL.Text], [\"query\"]),\n  version: IDL.Func([], [IDL.Text], [\"query\"]),\n});\n"
    }

    #[test]
    fn historian_js_declaration_matching_method_sets_pass() {
        assert!(check_historian_js_declaration_text(
            "io_historian.did",
            historian_did(),
            "io_historian.did.js",
            historian_js(),
            "index.js",
            "",
        )
        .is_ok());
    }

    #[test]
    fn historian_js_declaration_rejects_missing_method() {
        let js = "export const idlFactory = ({ IDL }) => IDL.Service({\n  version: IDL.Func([], [IDL.Text], [\"query\"]),\n});\n";
        let err = check_historian_js_declaration_text(
            "io_historian.did",
            historian_did(),
            "io_historian.did.js",
            js,
            "index.js",
            "",
        )
        .unwrap_err();
        assert!(err.contains("missing"));
        assert!(err.contains("get_dashboard_state"));
    }

    #[test]
    fn historian_js_declaration_rejects_extra_method() {
        let js = "export const idlFactory = ({ IDL }) => IDL.Service({\n  get_dashboard_state: IDL.Func([], [IDL.Text], [\"query\"]),\n  version: IDL.Func([], [IDL.Text], [\"query\"]),\n  extra: IDL.Func([], [IDL.Text], [\"query\"]),\n});\n";
        let err = check_historian_js_declaration_text(
            "io_historian.did",
            historian_did(),
            "io_historian.did.js",
            js,
            "index.js",
            "",
        )
        .unwrap_err();
        assert!(err.contains("absent"));
        assert!(err.contains("extra"));
    }

    #[test]
    fn historian_js_declaration_rejects_debug_method() {
        let js = "export const idlFactory = ({ IDL }) => IDL.Service({\n  get_dashboard_state: IDL.Func([], [IDL.Text], [\"query\"]),\n  version: IDL.Func([], [IDL.Text], [\"query\"]),\n  debug_clear: IDL.Func([], [], []),\n});\n";
        let err = check_historian_js_declaration_text(
            "io_historian.did",
            historian_did(),
            "io_historian.did.js",
            js,
            "index.js",
            "",
        )
        .unwrap_err();
        assert!(err.contains("debug_"));
    }

    #[test]
    fn historian_js_declaration_rejects_forbidden_generated_import_path() {
        let err = check_historian_js_declaration_text(
            "io_historian.did",
            historian_did(),
            "io_historian.did.js",
            historian_js(),
            "index.js",
            "import { idlFactory } from '../../../.dfx/local/canisters/io_historian';",
        )
        .unwrap_err();
        assert!(err.contains(".dfx"));
    }
}
