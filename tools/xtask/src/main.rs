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
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

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
    " redeem_to :",
    " mark_complete :",
    " force_retry :",
    " force_success :",
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
    " redeem_to :",
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
    "debug_get_transactions",
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
    if !stripped.contains("service : (InitArgs) -> {") {
        return Err(format!("{path} must declare the launch InitArgs service"));
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

fn rust_files_below(root: &Path, relative: &str) -> Result<Vec<PathBuf>, String> {
    fn walk(directory: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
        for entry in
            fs::read_dir(directory).map_err(|error| format!("{}: {error}", directory.display()))?
        {
            let path = entry.map_err(|error| error.to_string())?.path();
            if path.is_dir() {
                walk(&path, files)?;
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                files.push(path);
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    walk(&root.join(relative), &mut files)?;
    Ok(files)
}

fn normative_markdown_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    fn walk(root: &Path, directory: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
        for entry in
            fs::read_dir(directory).map_err(|error| format!("{}: {error}", directory.display()))?
        {
            let path = entry.map_err(|error| error.to_string())?.path();
            let relative = path.strip_prefix(root).map_err(|error| error.to_string())?;
            if path.is_dir() {
                if matches!(
                    relative
                        .components()
                        .next()
                        .and_then(|value| value.as_os_str().to_str()),
                    Some(
                        ".git"
                            | "target"
                            | "node_modules"
                            | "release-artifacts"
                            | "debug-artifacts"
                            | ".real-canister-wasms"
                    )
                ) || relative.starts_with("docs/research")
                {
                    continue;
                }
                walk(root, &path, files)?;
            } else if path.extension().is_some_and(|extension| extension == "md") {
                files.push(path);
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    walk(root, root, &mut files)?;
    Ok(files)
}

fn check_simplicity_at(root: &Path) -> Result<(), String> {
    fn production_line_count(text: &str) -> usize {
        text.split("\n#[cfg(test)]")
            .next()
            .unwrap_or(text)
            .lines()
            .count()
    }

    const VALUE_MOVING_DIRS: &[&str] = &[
        "canisters/io_stream_manager/src",
        "canisters/io_nns_neuron_manager/src",
    ];
    const FORBIDDEN: &[&str] = &[
        "AccountHistoryScanState",
        "LedgerIndexClient",
        "CompleteRangeEvidence",
        "CanonicalRangeJob",
        "StableLiability",
        "MockLedgerCanisterClient",
        "debug_get_transactions",
        "redemption_intake",
        "redemption intake",
        "redemption_return",
        "redemption IO return",
        "rejected_refund",
        "automatic rejected-transfer refund",
        "generic operation journal",
        "automatic proof of absence",
        "process_stream_event",
        "pub fn process_event",
        "pub async fn process_event",
        "process_stream",
        "pub fn tick",
        "pub async fn tick",
        "native ICP transfer submission",
        "parallel old/new execution",
        "IcpTransfer",
        "Legacy",
        "SchemaV2",
        "SchemaV3",
        "SchemaV4",
        "SchemaV5",
        "SchemaV6",
    ];

    let mut combined_lines = 0usize;
    let mut stream_lines = 0usize;
    for directory in VALUE_MOVING_DIRS {
        for path in rust_files_below(root, directory)? {
            let text = fs::read_to_string(&path)
                .map_err(|error| format!("{}: {error}", path.display()))?;
            let lines = production_line_count(&text);
            if lines > 1_100 {
                return Err(format!(
                    "{} has {lines} production lines; the per-file limit is 1100",
                    path.display()
                ));
            }
            for needle in FORBIDDEN {
                if text.contains(needle) {
                    return Err(format!("{} contains forbidden {needle:?}", path.display()));
                }
            }
            combined_lines += lines;
            if directory.contains("io_stream_manager") {
                stream_lines += lines;
            }
        }
    }
    let economics_lines = rust_files_below(root, "crates/io_core_model/src")?
        .into_iter()
        .try_fold(0usize, |sum, path| {
            fs::read_to_string(&path)
                .map(|text| sum + production_line_count(&text))
                .map_err(|error| format!("{}: {error}", path.display()))
        })?;
    if economics_lines > 220 {
        return Err(format!("pure economics module has {economics_lines} lines"));
    }
    let boundary_files = rust_files_below(root, "crates/io_ledger_boundary/src")?;
    let boundary_lines = boundary_files.iter().try_fold(0usize, |sum, path| {
        fs::read_to_string(path)
            .map(|text| sum + production_line_count(&text))
            .map_err(|error| format!("{}: {error}", path.display()))
    })?;
    if boundary_lines > 650 {
        return Err(format!(
            "io-ledger-boundary has {boundary_lines} production lines"
        ));
    }
    for path in &boundary_files {
        let text =
            fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
        let lines = production_line_count(&text);
        if lines > 500 {
            return Err(format!(
                "{} has {lines} production lines; the boundary file limit is 500",
                path.display()
            ));
        }
        for needle in [
            "LedgerIndexClient",
            "AccountHistoryScanState",
            "CompleteRangeEvidence",
            "CanonicalRangeJob",
            "StableLiability",
            "MockLedgerCanisterClient",
        ] {
            if text.contains(needle) {
                return Err(format!("{} contains forbidden {needle:?}", path.display()));
            }
        }
    }
    for manifest in [
        "canisters/io_stream_manager/Cargo.toml",
        "canisters/io_nns_neuron_manager/Cargo.toml",
    ] {
        let text = require_file(root, manifest)?;
        for dependency in ["io-ledger-types", "io-governance-types"] {
            if text.contains(dependency) {
                return Err(format!(
                    "{manifest} contains forbidden production dependency {dependency}"
                ));
            }
        }
    }
    for directory in ["crates/io_core_model/src", "crates/io_ledger_boundary/src"] {
        for path in rust_files_below(root, directory)? {
            let text = fs::read_to_string(&path)
                .map_err(|error| format!("{}: {error}", path.display()))?;
            combined_lines += production_line_count(&text);
        }
    }
    if stream_lines > 3_500 {
        return Err(format!(
            "stream-manager production Rust has {stream_lines} lines"
        ));
    }
    if combined_lines > 9_000 {
        return Err(format!(
            "combined production Rust has {combined_lines} lines; simplified limit not met"
        ));
    }
    let nns_lines = rust_files_below(root, "canisters/io_nns_neuron_manager/src")?
        .into_iter()
        .try_fold(0usize, |sum, path| {
            fs::read_to_string(&path)
                .map(|text| sum + production_line_count(&text))
                .map_err(|error| format!("{}: {error}", path.display()))
        })?;
    if nns_lines > 3_500 {
        return Err(format!("NNS-manager production Rust has {nns_lines} lines"));
    }
    for path in normative_markdown_files(root)? {
        let text =
            fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        let lower = text.to_ascii_lowercase();
        for needle in [
            "production DIDs remain constructor-only",
            "constructor-only monetary DIDs",
            "ledger/index-driven monetary intent",
            "redemption intake Account",
            "redemption return leg",
            "automatic rejected-redemption refund",
            "scanner-driven settlement",
            "scanner-driven intent",
            "automatic proof of absence",
            "cursor recovery",
            "timer-driven monetary execution",
        ] {
            if lower.contains(&needle.to_ascii_lowercase()) {
                return Err(format!(
                    "{} contains stale normative phrase {needle:?}",
                    path.display()
                ));
            }
        }
    }
    for path in [
        "canisters/io_stream_manager/mainnet-install-args.did",
        "canisters/io_nns_neuron_manager/mainnet-install-args.did",
    ] {
        let text = require_file(root, path)?;
        require_present(path, &text, &["NON-RUNNABLE TEMPLATE", "TODO_"])?;
    }
    eprintln!(
        "simplicity metrics: stream_manager={stream_lines} nns_manager={nns_lines} ledger_boundary={boundary_lines} economics={economics_lines} combined={combined_lines}"
    );
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
    let nns_production = read_file(root, nns_production_path)?;
    if root.join(stream_debug_path).exists() || root.join(nns_debug_path).exists() {
        return Err("value-moving debug DIDs must be deleted".into());
    }
    let historian_production = read_file(root, historian_production_path)?;
    let historian_debug = read_file(root, historian_debug_path)?;

    check_minimal_value_moving_did(stream_production_path, &stream_production)?;
    check_minimal_value_moving_did(nns_production_path, &nns_production)?;

    require_present(
        stream_production_path,
        &stream_production,
        &[
            "  redeem :",
            "  prepare_liquid_receipt :",
            "  complete_liquid_receipt :",
            "  resume :",
            "  prove_active_transfer :",
            "  set_paused :",
            "  get_status :",
        ],
    )?;
    require_present(
        nns_production_path,
        &nns_production,
        &[
            "  notify_jupiter_deposit :",
            "  set_two_week_target :",
            "  resume :",
            "  prove_active_transfer :",
            "  set_paused :",
            "  get_status :",
        ],
    )?;

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

const RELEASE_SOURCE_COMMIT_ENV: &str = "IO_RELEASE_SOURCE_COMMIT";

fn current_git_commit() -> Result<String, String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .map_err(|err| format!("git rev-parse HEAD: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "git rev-parse HEAD failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn is_full_lowercase_hex_sha(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn validate_release_source_commit(git_commit: &str) -> Result<(), String> {
    if !is_full_lowercase_hex_sha(git_commit) {
        return Err(format!(
            "release source commit must be a full 40-character lowercase hexadecimal SHA, got {git_commit:?}"
        ));
    }

    let cat_file = Command::new("git")
        .args(["cat-file", "-e", &format!("{git_commit}^{{commit}}")])
        .output()
        .map_err(|err| format!("git cat-file {git_commit}: {err}"))?;
    if !cat_file.status.success() {
        return Err(format!(
            "release source commit {git_commit} does not resolve locally as a commit"
        ));
    }

    let ancestor = Command::new("git")
        .args(["merge-base", "--is-ancestor", git_commit, "HEAD"])
        .output()
        .map_err(|err| format!("git merge-base --is-ancestor {git_commit} HEAD: {err}"))?;
    if !ancestor.status.success() {
        return Err(format!(
            "release source commit {git_commit} is not an ancestor of HEAD"
        ));
    }

    Ok(())
}

fn manifest_source_commit(root: &Path) -> Result<Option<String>, String> {
    if !root.join(MANIFEST_PATH).is_file() {
        return Ok(None);
    }
    let manifest = read_manifest(root)?;
    let git_commit = manifest
        .git_commit
        .ok_or_else(|| format!("{MANIFEST_PATH}: git_commit is required"))?;
    validate_release_source_commit(&git_commit).map_err(|err| format!("{MANIFEST_PATH}: {err}"))?;
    Ok(Some(git_commit))
}

fn release_source_commit(root: &Path) -> Result<String, String> {
    let git_commit = match env::var(RELEASE_SOURCE_COMMIT_ENV) {
        Ok(value) => value,
        Err(env::VarError::NotPresent) => match manifest_source_commit(root)? {
            Some(value) => value,
            None => current_git_commit()?,
        },
        Err(err) => return Err(format!("{RELEASE_SOURCE_COMMIT_ENV}: {err}")),
    };
    validate_release_source_commit(&git_commit)?;
    Ok(git_commit)
}

fn build_manifest_for_commit(root: &Path, git_commit: String) -> Result<ArtifactManifest, String> {
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
            git_commit: Some(git_commit.clone()),
        });
    }

    Ok(ArtifactManifest {
        schema_version: 1,
        build_profile: RELEASE_PROFILE.to_string(),
        target: WASM_TARGET.to_string(),
        git_commit: Some(git_commit),
        artifacts,
    })
}

fn build_manifest(root: &Path) -> Result<ArtifactManifest, String> {
    build_manifest_for_commit(root, release_source_commit(root)?)
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
    if actual.schema_version != 1 {
        return Err(format!(
            "{MANIFEST_PATH}: unsupported schema_version {}",
            actual.schema_version
        ));
    }
    let source_commit = actual
        .git_commit
        .clone()
        .ok_or_else(|| format!("{MANIFEST_PATH}: git_commit is required"))?;
    validate_release_source_commit(&source_commit)
        .map_err(|err| format!("{MANIFEST_PATH}: {err}"))?;
    for entry in &actual.artifacts {
        if entry.git_commit.as_deref() != Some(source_commit.as_str()) {
            return Err(format!(
                "{MANIFEST_PATH}: artifact {} git_commit must equal top-level git_commit",
                entry.canister
            ));
        }
    }

    let expected = build_manifest_for_commit(root, source_commit)?;
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

fn validate_simplified_receipt_topology(stream: &str, nns: &str) -> Result<(), String> {
    fn require_field_token(text: &str, field: &str, token: &str) -> Result<(), String> {
        let line = text
            .lines()
            .find(|line| line.contains(field))
            .ok_or_else(|| format!("missing topology field {field}"))?;
        if !line.contains(token) {
            return Err(format!("{field} must use shared topology token {token}"));
        }
        Ok(())
    }
    for (text, field) in [(stream, "jupiter_receipt_source"), (nns, "jupiter_staging")] {
        let line = text
            .lines()
            .find(|line| line.contains(field))
            .ok_or_else(|| format!("missing topology field {field}"))?;
        if !line.contains("subaccount = null") {
            return Err(format!("{field} must use the NNS manager default Account"));
        }
    }
    for (stream_field, nns_field, token) in [
        (
            "two_week_receipt_source",
            "two_week_maturity_staging",
            "TODO_TWO_WEEK_STAGING",
        ),
        (
            "liquid_icp",
            "stream_liquid_account",
            "TODO_STREAM_LIQUID_SUBACCOUNT",
        ),
    ] {
        require_field_token(stream, stream_field, token)?;
        require_field_token(nns, nns_field, token)?;
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
        for (name, args) in [
            ("io_stream_manager", stream_args.as_str()),
            ("io_nns_neuron_manager", nns_args.as_str()),
        ] {
            if !args.contains("NON-RUNNABLE TEMPLATE") || !args.contains("TODO_") {
                return Err(format!(
                    "{name} mainnet install args must remain an explicit non-runnable TODO template"
                ));
            }
        }
        validate_simplified_receipt_topology(&stream_args, &nns_args)?;
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
            "The maintained official local SNS flow uses the source-built `sns` CLI",
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
            "Required CI uses SNS-shaped mock/PocketIC tests.",
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
            "user-to-redemption transfer",
            "redemption-to-reserve transfer",
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
            "treasury: \"800_000 tokens\"",
            "swap: \"100_000 tokens\"",
            "Swap:",
            "start_time:",
            "NnsProposal:",
            "{{",
        ],
    )?;
    require_absent(
        "deploy/local-sns-rehearsal/sns_init.local.template.yaml",
        &sns_init,
        &[
            "protocol_reserve:",
            "archive_options:",
            "io_rehearsal_notes:",
        ],
    )?;
    validate_local_sns_yaml_structure(
        "deploy/local-sns-rehearsal/sns_init.local.template.yaml",
        &sns_init,
    )?;
    validate_local_sns_logo_files(root)?;
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
            "[toolchain_provenance]",
            "official_ic_source_commit",
            "sns_testing_source_path",
            "operator_identity_principal",
            "local_network_url",
            "official_tooling = \"manual-local-only\"",
            "sns_cli_sha256",
            "sns_testing_init_sha256",
            "sns_testing_cli_sha256",
            "[expected_local_sns_config]",
            "transaction_fee_e8s",
            "total_supply_e8s",
            "protocol_reserve_funding_amount_e8s",
            "[ledger_evidence]",
            "transaction_fee_e8s",
            "total_supply_e8s",
            "protocol_reserve_balance_e8s",
            "reserve_transfer_amount_e8s",
            "redemption_return_amount_e8s",
            "bad_fee_error_observed = true",
            "insufficient_funds_error_observed = true",
            "duplicate_tested_transfer",
            "index_account_history_observed = true",
            "[reserve_funding_transfer]",
            "sns_proposal_id",
            "proposal_adopted = true",
            "proposal_executed = true",
            "created_at_time_nanos",
            "memo_hex",
            "[transfer_reserve_to_user]",
            "[transfer_user_to_redemption]",
            "[transfer_redemption_to_reserve]",
            "from_owner",
            "from_subaccount_hex",
            "to_owner",
            "to_subaccount_hex",
            "fee_disposition",
            "sender_balance_before_e8s",
            "recipient_balance_after_e8s",
            "total_supply_before_e8s",
            "total_supply_after_e8s",
            "proof_source_canister",
            "proof_source",
            "proof_method",
            "archive_canister",
            "[duplicate_test]",
            "[issuance_model]",
            "resolved_as = \"protocol_reserve_transfer\"",
            "minting_assumed = false",
            "treasury_transfer_assumed = true",
            "fee_disposition_mode",
            "total_supply_changes_explained = true",
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
        "deploy/local-sns-rehearsal/scripts/lib-local-sns.sh",
        "deploy/local-sns-rehearsal/scripts/01-render-sns-init.sh",
        "deploy/local-sns-rehearsal/scripts/02-record-canister-ids.sh",
        "deploy/local-sns-rehearsal/scripts/03-capture-ledger-evidence.sh",
        "deploy/local-sns-rehearsal/scripts/04-render-local-wiring.sh",
        "deploy/local-sns-rehearsal/scripts/05-validate-evidence.sh",
        "deploy/local-sns-rehearsal/scripts/10-bootstrap-official-network.sh",
        "deploy/local-sns-rehearsal/scripts/11-build-local-io-canisters.sh",
        "deploy/local-sns-rehearsal/scripts/12-deploy-local-dapps.sh",
        "deploy/local-sns-rehearsal/scripts/13-propose-and-finalize-sns.sh",
        "deploy/local-sns-rehearsal/scripts/14-discover-sns-canisters.sh",
        "deploy/local-sns-rehearsal/scripts/15-exercise-ledger.sh",
        "deploy/local-sns-rehearsal/scripts/16-exercise-index-and-archives.sh",
        "deploy/local-sns-rehearsal/scripts/17-exercise-governance-and-controllers.sh",
        "deploy/local-sns-rehearsal/scripts/18-package-evidence.sh",
        "deploy/local-sns-rehearsal/scripts/19-cleanup-official-network.sh",
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
    require_absent(
        "deploy/local-sns-rehearsal/scripts/00-check-prereqs.sh",
        &require_file(
            root,
            "deploy/local-sns-rehearsal/scripts/00-check-prereqs.sh",
        )?,
        &["source-built sns"],
    )?;
    require_present(
        "deploy/local-sns-rehearsal/scripts/10-bootstrap-official-network.sh",
        &require_file(
            root,
            "deploy/local-sns-rehearsal/scripts/10-bootstrap-official-network.sh",
        )?,
        &[
            "//rs/sns/testing:sns-testing-init",
            "//rs/sns/testing:sns-testing",
            "//rs/sns/cli:sns",
            "sns init-config-file --init-config-file-path",
            ". scripts/env.sh",
        ],
    )?;
    validate_loopback_url_guardrails()?;

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

fn validate_local_sns_yaml_structure(path: &str, text: &str) -> Result<(), String> {
    let lines: Vec<&str> = text.lines().collect();
    let mut in_swap = false;
    let mut start_time_count = 0_u8;
    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if line
            .chars()
            .next()
            .is_some_and(|character| !character.is_whitespace())
        {
            in_swap = trimmed == "Swap:";
            continue;
        }
        if !in_swap {
            continue;
        }
        if let Some(value) = trimmed.strip_prefix("start_time:") {
            start_time_count += 1;
            if value.trim() != "null" {
                return Err(format!(
                    "{path}: Swap.start_time must be the YAML null value for local rehearsal"
                ));
            }
        }
        if let Some(value) = trimmed.strip_prefix("restricted_countries:") {
            let inline = value.trim();
            if inline == "[]" {
                return Err(format!(
                    "{path}: Swap.restricted_countries must be omitted or non-empty"
                ));
            }
            if inline.is_empty() {
                let indent = line.len() - line.trim_start().len();
                let has_item = lines[index + 1..]
                    .iter()
                    .take_while(|candidate| {
                        candidate.trim().is_empty()
                            || candidate.len() - candidate.trim_start().len() > indent
                    })
                    .any(|candidate| candidate.trim_start().starts_with("- "));
                if !has_item {
                    return Err(format!(
                        "{path}: Swap.restricted_countries must be omitted or non-empty"
                    ));
                }
            }
        }
    }
    if start_time_count != 1 {
        return Err(format!(
            "{path}: Swap.start_time must appear exactly once and be null"
        ));
    }
    Ok(())
}

fn validate_local_sns_logo_files(root: &Path) -> Result<(), String> {
    let vars_path = "deploy/local-sns-rehearsal/local-vars.example.toml";
    let text = require_file(root, vars_path)?;
    let doc = parse_simple_toml_document(vars_path, &text)?;
    let rehearsal_dir = root.join("deploy/local-sns-rehearsal");
    for (path_key, hash_key) in [
        ("logo_path", "logo_sha256"),
        ("token_logo_path", "token_logo_sha256"),
    ] {
        let relative = require_simple_string(vars_path, &doc, "local", path_key)?;
        let hash = require_simple_string(vars_path, &doc, "local", hash_key)?;
        let relative_path = Path::new(&relative);
        if relative_path.is_absolute()
            || relative.contains("://")
            || relative.contains('\\')
            || relative_path
                .components()
                .any(|component| !matches!(component, std::path::Component::Normal(_)))
        {
            return Err(format!(
                "{vars_path}: local.{path_key} must be a traversal-free relative local path"
            ));
        }
        if hash.len() != 64
            || !hash
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return Err(format!(
                "{vars_path}: local.{hash_key} must be an exact lowercase SHA-256"
            ));
        }
        let full_path = rehearsal_dir.join(relative_path);
        let metadata = fs::symlink_metadata(&full_path)
            .map_err(|err| format!("{}: {err}", full_path.display()))?;
        if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
            return Err(format!(
                "{}: local logo must be a regular non-symlink file",
                full_path.display()
            ));
        }
        let bytes =
            fs::read(&full_path).map_err(|err| format!("{}: {err}", full_path.display()))?;
        if hex_sha256(&bytes) != hash {
            return Err(format!(
                "{}: local logo SHA-256 does not match {vars_path}",
                full_path.display()
            ));
        }
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
logo_path = "assets/io-local-logo.svg"
logo_sha256 = "241b04223fe83bfe8dfc6f5ef3de168cc4ef8b24107402773166566bf6ed962e"
token_logo_path = "assets/io-local-token-logo.svg"
token_logo_sha256 = "61ce92c31189e825ce0f277c73bb09d8905d0ab161f60f2bebedae802bbb48d8"

[expected_local_sns_config]
token_symbol = "IO"
transaction_fee_e8s = 10_000
total_supply_e8s = 100_000_000_000_000
treasury_initial_balance_e8s = 80_000_000_000_000
protocol_reserve_funding_amount_e8s = 60_000_000_000_000
minimum_remaining_treasury_e8s = 19_999_999_990_000
"#
    )
}

fn completed_local_sns_evidence() -> String {
    r#"[mode]
network = "local"
source = "official-local-sns-rehearsal"
official_tooling = "manual-local-only"
io_protocol_live = false
sns_io_ledger_mainnet_launched = false

[toolchain_provenance]
official_ic_repository = "dfinity/ic"
official_ic_source_commit = "2d7f90fb23672cc3b81c216a33d04c75672dd308"
sns_testing_source_path = "rs/sns/testing"
dfx_version = "dfx 0.27.0"
bazel_version = "bazel 7.4.1"
pocket_ic_version = "pocket-ic-server 14.0.0"
sns_cli_version = "source-built sns 1.0.0"
sns_cli_sha256 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
sns_testing_init_sha256 = "1123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
sns_testing_cli_version = "source-built sns-testing"
sns_testing_cli_sha256 = "2123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
quill_version = "quill 0.4.4"
rustc_version = "rustc repository-toolchain"
cargo_version = "cargo repository-toolchain"
operator_identity_principal = "bd3sg-teaaa-aaaaa-qaaba-cai"
local_network_url = "http://127.0.0.1:8080"
network_alias = "sns-testing"
locally_built_binary_sha256 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"

[expected_local_sns_config]
token_symbol = "IO"
transaction_fee_e8s = 10000
total_supply_e8s = 100000000000000
treasury_initial_balance_e8s = 80000000000000
protocol_reserve_funding_amount_e8s = 60000000000000
minimum_remaining_treasury_e8s = 19999999990000

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

[archive_evidence]
archive_canister = "by6od-j4aaa-aaaaa-qaadq-cai"
discovered_from_ledger = true
discovered_from_root = true
range_start = 0
range_end = 100

[ledger_evidence]
token_symbol = "IO"
transaction_fee_e8s = 10000
total_supply_e8s = 99999999960000
protocol_reserve_account_owner = "avqkn-guaaa-aaaaa-qaaea-cai"
protocol_reserve_subaccount_hex = "3333333333333333333333333333333333333333333333333333333333333333"
protocol_reserve_balance_e8s = 59999999970000
reserve_transfer_block_index = 11
redemption_return_block_index = 13
reserve_transfer_amount_e8s = 100000000
redemption_return_amount_e8s = 99980000
bad_fee_error_observed = true
insufficient_funds_error_observed = true
duplicate_of_block_index = 11
duplicate_tested_transfer = "transfer_reserve_to_user"
index_account_history_observed = true
index_history_order = "descending"
index_lag_or_archive_required_observed = "not-observed"

[reserve_funding_transfer]
sns_proposal_id = 1
proposal_adopted = true
proposal_executed = true
created_at_time_nanos = 1785196799000000000
memo_hex = "494f5f524553455256455f46554e44494e475f5631"
block_index = 10
from_owner = "bd3sg-teaaa-aaaaa-qaaba-cai"
from_subaccount_hex = "none"
to_owner = "avqkn-guaaa-aaaaa-qaaea-cai"
to_subaccount_hex = "3333333333333333333333333333333333333333333333333333333333333333"
requested_amount_e8s = 60000000000000
observed_fee_e8s = 10000
fee_disposition = "burned"
sender_balance_before_e8s = 80000000000000
sender_balance_after_e8s = 19999999990000
recipient_balance_before_e8s = 0
recipient_balance_after_e8s = 60000000000000
fee_collector_owner = "none"
fee_collector_subaccount_hex = "none"
fee_collector_balance_before_e8s = "none"
fee_collector_balance_after_e8s = "none"
total_supply_before_e8s = 100000000000000
total_supply_after_e8s = 99999999990000
reserve_balance_before_e8s = 0
reserve_balance_after_e8s = 60000000000000
ledger_tip_block_index = 10
index_synced_through_block_index = 10
proof_source = "SnsLedgerBlock"
proof_source_canister = "br5f7-7uaaa-aaaaa-qaaca-cai"
proof_method = "Icrc3GetBlocks"
proof_account_owner = "avqkn-guaaa-aaaaa-qaaea-cai"
proof_account_subaccount_hex = "3333333333333333333333333333333333333333333333333333333333333333"
archive_canister = "none"
archive_range_start = "none"
archive_range_end = "none"
archive_involvement = "none"
observation_timestamp = "2026-07-27T23:59:59Z"

[transfer_reserve_to_user]
block_index = 11
from_owner = "avqkn-guaaa-aaaaa-qaaea-cai"
from_subaccount_hex = "3333333333333333333333333333333333333333333333333333333333333333"
to_owner = "bd3sg-teaaa-aaaaa-qaaba-cai"
to_subaccount_hex = "1111111111111111111111111111111111111111111111111111111111111111"
requested_amount_e8s = 100000000
observed_fee_e8s = 10000
fee_disposition = "burned"
sender_balance_before_e8s = 60000000000000
sender_balance_after_e8s = 59999899990000
recipient_balance_before_e8s = 0
recipient_balance_after_e8s = 100000000
fee_collector_owner = "none"
fee_collector_subaccount_hex = "none"
fee_collector_balance_before_e8s = "none"
fee_collector_balance_after_e8s = "none"
total_supply_before_e8s = 99999999990000
total_supply_after_e8s = 99999999980000
reserve_balance_before_e8s = 60000000000000
reserve_balance_after_e8s = 59999899990000
ledger_tip_block_index = 11
index_synced_through_block_index = 11
proof_source = "SnsIndexAccountHistory"
proof_source_canister = "be2us-64aaa-aaaaa-qaabq-cai"
proof_method = "IcrcIndexGetAccountTransactions"
proof_account_owner = "avqkn-guaaa-aaaaa-qaaea-cai"
proof_account_subaccount_hex = "3333333333333333333333333333333333333333333333333333333333333333"
archive_canister = "none"
archive_range_start = "none"
archive_range_end = "none"
archive_involvement = "none"
observation_timestamp = "2026-07-28T00:00:00Z"

[transfer_user_to_redemption]
block_index = 12
from_owner = "bd3sg-teaaa-aaaaa-qaaba-cai"
from_subaccount_hex = "1111111111111111111111111111111111111111111111111111111111111111"
to_owner = "avqkn-guaaa-aaaaa-qaaea-cai"
to_subaccount_hex = "2222222222222222222222222222222222222222222222222222222222222222"
requested_amount_e8s = 99990000
observed_fee_e8s = 10000
fee_disposition = "burned"
sender_balance_before_e8s = 100000000
sender_balance_after_e8s = 0
recipient_balance_before_e8s = 0
recipient_balance_after_e8s = 99990000
fee_collector_owner = "none"
fee_collector_subaccount_hex = "none"
fee_collector_balance_before_e8s = "none"
fee_collector_balance_after_e8s = "none"
total_supply_before_e8s = 99999999980000
total_supply_after_e8s = 99999999970000
reserve_balance_before_e8s = 59999899990000
reserve_balance_after_e8s = 59999899990000
ledger_tip_block_index = 12
index_synced_through_block_index = 12
proof_source = "SnsIndexAccountHistory"
proof_source_canister = "be2us-64aaa-aaaaa-qaabq-cai"
proof_method = "IcrcIndexGetAccountTransactions"
proof_account_owner = "bd3sg-teaaa-aaaaa-qaaba-cai"
proof_account_subaccount_hex = "1111111111111111111111111111111111111111111111111111111111111111"
archive_canister = "none"
archive_range_start = "none"
archive_range_end = "none"
archive_involvement = "none"
observation_timestamp = "2026-07-28T00:00:01Z"

[transfer_redemption_to_reserve]
block_index = 13
from_owner = "avqkn-guaaa-aaaaa-qaaea-cai"
from_subaccount_hex = "2222222222222222222222222222222222222222222222222222222222222222"
to_owner = "avqkn-guaaa-aaaaa-qaaea-cai"
to_subaccount_hex = "3333333333333333333333333333333333333333333333333333333333333333"
requested_amount_e8s = 99980000
observed_fee_e8s = 10000
fee_disposition = "burned"
sender_balance_before_e8s = 99990000
sender_balance_after_e8s = 0
recipient_balance_before_e8s = 59999899990000
recipient_balance_after_e8s = 59999999970000
fee_collector_owner = "none"
fee_collector_subaccount_hex = "none"
fee_collector_balance_before_e8s = "none"
fee_collector_balance_after_e8s = "none"
total_supply_before_e8s = 99999999970000
total_supply_after_e8s = 99999999960000
reserve_balance_before_e8s = 59999899990000
reserve_balance_after_e8s = 59999999970000
ledger_tip_block_index = 13
index_synced_through_block_index = 13
proof_source = "SnsIndexAccountHistory"
proof_source_canister = "be2us-64aaa-aaaaa-qaabq-cai"
proof_method = "IcrcIndexGetAccountTransactions"
proof_account_owner = "avqkn-guaaa-aaaaa-qaaea-cai"
proof_account_subaccount_hex = "2222222222222222222222222222222222222222222222222222222222222222"
archive_canister = "none"
archive_range_start = "none"
archive_range_end = "none"
archive_involvement = "none"
observation_timestamp = "2026-07-28T00:00:02Z"

[duplicate_test]
original_transfer = "transfer_reserve_to_user"
duplicate_of_block_index = 11
proof_source = "SnsIndexAccountHistory"
proof_method = "IcrcIndexGetAccountTransactions"
proof_source_canister = "be2us-64aaa-aaaaa-qaabq-cai"
proof_account_owner = "avqkn-guaaa-aaaaa-qaaea-cai"
proof_account_subaccount_hex = "3333333333333333333333333333333333333333333333333333333333333333"
observation_timestamp = "2026-07-28T00:00:03Z"

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
treasury_transfer_assumed = true
fee_disposition_mode = "burned"
total_supply_changes_explained = true

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
    let rendered_sns_path = temp_rehearsal.join("sns_init.local.yaml");
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
    toolchain: LocalSnsToolchainProvenance,
    expected: LocalSnsExpectedConfig,
    sns_canisters: LocalSnsCanisters,
    io_dapp_canisters: LocalSnsIoDappCanisters,
    archive: LocalSnsArchiveEvidence,
    ledger: LocalSnsLedgerEvidence,
    reserve_funding_transfer: LocalSnsReserveFundingEvidence,
    reserve_to_user_transfer: LocalSnsTransferEvidence,
    user_to_redemption_transfer: LocalSnsTransferEvidence,
    redemption_to_reserve_transfer: LocalSnsTransferEvidence,
    duplicate_test: LocalSnsDuplicateTestEvidence,
    governance: LocalSnsGovernanceEvidence,
    issuance: LocalSnsIssuanceModel,
}

#[derive(Clone, Debug)]
struct LocalSnsModeEvidence {
    network: String,
    source: String,
    official_tooling: String,
    io_protocol_live: bool,
    sns_io_ledger_mainnet_launched: bool,
}

#[derive(Clone, Debug)]
struct LocalSnsToolchainProvenance {
    official_ic_repository: String,
    official_ic_source_commit: String,
    sns_testing_source_path: String,
    dfx_version: String,
    bazel_version: String,
    pocket_ic_version: String,
    sns_cli_version: String,
    sns_cli_sha256: String,
    sns_testing_init_sha256: String,
    sns_testing_cli_version: String,
    sns_testing_cli_sha256: String,
    quill_version: String,
    rustc_version: String,
    cargo_version: String,
    operator_identity_principal: Principal,
    local_network_url: String,
    network_alias: String,
    locally_built_binary_sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct LocalSnsAccountEvidence {
    owner: Principal,
    subaccount_hex: Option<String>,
}

#[derive(Clone, Debug)]
struct LocalSnsExpectedConfig {
    token_symbol: String,
    transaction_fee_e8s: u128,
    total_supply_e8s: u128,
    treasury_initial_balance_e8s: u128,
    protocol_reserve_funding_amount_e8s: u128,
    minimum_remaining_treasury_e8s: u128,
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
struct LocalSnsArchiveEvidence {
    archive_canister: Option<Principal>,
    discovered_from_ledger: bool,
    discovered_from_root: bool,
    range_start: Option<u64>,
    range_end: Option<u64>,
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
    duplicate_of_block_index: Option<u64>,
    duplicate_tested_transfer: String,
    index_account_history_observed: bool,
    index_history_order: String,
    index_lag_or_archive_required_observed: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LocalSnsProofSource {
    LedgerBlock,
    IndexAccountHistory,
    LedgerArchive,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LocalSnsProofMethod {
    Icrc3GetBlocks,
    IcrcIndexGetAccountTransactions,
    ArchiveGetBlocks,
}

#[derive(Clone, Debug)]
struct LocalSnsTransferEvidence {
    block_index: u64,
    from_account: LocalSnsAccountEvidence,
    to_account: LocalSnsAccountEvidence,
    requested_amount_e8s: u128,
    observed_fee_e8s: u128,
    fee_disposition: String,
    sender_balance_before_e8s: u128,
    sender_balance_after_e8s: u128,
    recipient_balance_before_e8s: u128,
    recipient_balance_after_e8s: u128,
    fee_collector_account: Option<LocalSnsAccountEvidence>,
    fee_collector_balance_before_e8s: Option<u128>,
    fee_collector_balance_after_e8s: Option<u128>,
    total_supply_before_e8s: u128,
    total_supply_after_e8s: u128,
    reserve_balance_before_e8s: u128,
    reserve_balance_after_e8s: u128,
    ledger_tip_block_index: u64,
    index_synced_through_block_index: u64,
    proof_source: LocalSnsProofSource,
    proof_source_canister: Principal,
    proof_method: LocalSnsProofMethod,
    proof_account: LocalSnsAccountEvidence,
    archive_canister: Option<Principal>,
    archive_range_start: Option<u64>,
    archive_range_end: Option<u64>,
    archive_involvement: String,
    observation_timestamp: String,
}

#[derive(Clone, Debug)]
struct LocalSnsReserveFundingEvidence {
    sns_proposal_id: u64,
    proposal_adopted: bool,
    proposal_executed: bool,
    created_at_time_nanos: Option<u64>,
    memo_hex: Option<String>,
    transfer: LocalSnsTransferEvidence,
}

#[derive(Clone, Debug)]
struct LocalSnsDuplicateTestEvidence {
    original_transfer: String,
    duplicate_of_block_index: u64,
    proof_source: LocalSnsProofSource,
    proof_method: LocalSnsProofMethod,
    proof_source_canister: Principal,
    proof_account: LocalSnsAccountEvidence,
    observation_timestamp: String,
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
    fee_disposition_mode: String,
    total_supply_changes_explained: bool,
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
            | "toolchain_provenance"
            | "expected_local_sns_config"
            | "sns_canisters"
            | "io_dapp_canisters"
            | "archive_evidence"
            | "ledger_evidence"
            | "reserve_funding_transfer"
            | "transfer_reserve_to_user"
            | "transfer_user_to_redemption"
            | "transfer_redemption_to_reserve"
            | "duplicate_test"
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
            official_tooling: require_simple_string(path, &doc, "mode", "official_tooling")?,
            io_protocol_live: require_simple_bool(path, &doc, "mode", "io_protocol_live")?,
            sns_io_ledger_mainnet_launched: require_simple_bool(
                path,
                &doc,
                "mode",
                "sns_io_ledger_mainnet_launched",
            )?,
        },
        toolchain: LocalSnsToolchainProvenance {
            official_ic_repository: require_simple_string(
                path,
                &doc,
                "toolchain_provenance",
                "official_ic_repository",
            )?,
            official_ic_source_commit: require_simple_string(
                path,
                &doc,
                "toolchain_provenance",
                "official_ic_source_commit",
            )?,
            sns_testing_source_path: require_simple_string(
                path,
                &doc,
                "toolchain_provenance",
                "sns_testing_source_path",
            )?,
            dfx_version: require_simple_string(path, &doc, "toolchain_provenance", "dfx_version")?,
            bazel_version: require_simple_string(
                path,
                &doc,
                "toolchain_provenance",
                "bazel_version",
            )?,
            pocket_ic_version: require_simple_string(
                path,
                &doc,
                "toolchain_provenance",
                "pocket_ic_version",
            )?,
            sns_cli_version: require_simple_string(
                path,
                &doc,
                "toolchain_provenance",
                "sns_cli_version",
            )?,
            sns_cli_sha256: require_simple_string(
                path,
                &doc,
                "toolchain_provenance",
                "sns_cli_sha256",
            )?,
            sns_testing_init_sha256: require_simple_string(
                path,
                &doc,
                "toolchain_provenance",
                "sns_testing_init_sha256",
            )?,
            sns_testing_cli_version: require_simple_string(
                path,
                &doc,
                "toolchain_provenance",
                "sns_testing_cli_version",
            )?,
            sns_testing_cli_sha256: require_simple_string(
                path,
                &doc,
                "toolchain_provenance",
                "sns_testing_cli_sha256",
            )?,
            quill_version: require_simple_string(
                path,
                &doc,
                "toolchain_provenance",
                "quill_version",
            )?,
            rustc_version: require_simple_string(
                path,
                &doc,
                "toolchain_provenance",
                "rustc_version",
            )?,
            cargo_version: require_simple_string(
                path,
                &doc,
                "toolchain_provenance",
                "cargo_version",
            )?,
            operator_identity_principal: parse_required_principal(
                path,
                &doc,
                "toolchain_provenance",
                "operator_identity_principal",
            )?,
            local_network_url: require_simple_string(
                path,
                &doc,
                "toolchain_provenance",
                "local_network_url",
            )?,
            network_alias: require_simple_string(
                path,
                &doc,
                "toolchain_provenance",
                "network_alias",
            )?,
            locally_built_binary_sha256: require_simple_string(
                path,
                &doc,
                "toolchain_provenance",
                "locally_built_binary_sha256",
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
            treasury_initial_balance_e8s: require_simple_u128(
                path,
                &doc,
                "expected_local_sns_config",
                "treasury_initial_balance_e8s",
            )?,
            protocol_reserve_funding_amount_e8s: require_simple_u128(
                path,
                &doc,
                "expected_local_sns_config",
                "protocol_reserve_funding_amount_e8s",
            )?,
            minimum_remaining_treasury_e8s: require_simple_u128(
                path,
                &doc,
                "expected_local_sns_config",
                "minimum_remaining_treasury_e8s",
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
        archive: LocalSnsArchiveEvidence {
            archive_canister: parse_optional_principal_string(
                path,
                &doc,
                "archive_evidence",
                "archive_canister",
            )?,
            discovered_from_ledger: require_simple_bool(
                path,
                &doc,
                "archive_evidence",
                "discovered_from_ledger",
            )?,
            discovered_from_root: require_simple_bool(
                path,
                &doc,
                "archive_evidence",
                "discovered_from_root",
            )?,
            range_start: parse_optional_u64(path, &doc, "archive_evidence", "range_start")?,
            range_end: parse_optional_u64(path, &doc, "archive_evidence", "range_end")?,
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
            duplicate_of_block_index: parse_optional_u64(
                path,
                &doc,
                "ledger_evidence",
                "duplicate_of_block_index",
            )?,
            duplicate_tested_transfer: require_simple_string(
                path,
                &doc,
                "ledger_evidence",
                "duplicate_tested_transfer",
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
        reserve_funding_transfer: LocalSnsReserveFundingEvidence {
            sns_proposal_id: require_simple_u64(
                path,
                &doc,
                "reserve_funding_transfer",
                "sns_proposal_id",
            )?,
            proposal_adopted: require_simple_bool(
                path,
                &doc,
                "reserve_funding_transfer",
                "proposal_adopted",
            )?,
            proposal_executed: require_simple_bool(
                path,
                &doc,
                "reserve_funding_transfer",
                "proposal_executed",
            )?,
            created_at_time_nanos: parse_optional_u64(
                path,
                &doc,
                "reserve_funding_transfer",
                "created_at_time_nanos",
            )?,
            memo_hex: parse_optional_hex(path, &doc, "reserve_funding_transfer", "memo_hex")?,
            transfer: parse_local_sns_transfer_evidence(path, &doc, "reserve_funding_transfer")?,
        },
        reserve_to_user_transfer: parse_local_sns_transfer_evidence(
            path,
            &doc,
            "transfer_reserve_to_user",
        )?,
        user_to_redemption_transfer: parse_local_sns_transfer_evidence(
            path,
            &doc,
            "transfer_user_to_redemption",
        )?,
        redemption_to_reserve_transfer: parse_local_sns_transfer_evidence(
            path,
            &doc,
            "transfer_redemption_to_reserve",
        )?,
        duplicate_test: LocalSnsDuplicateTestEvidence {
            original_transfer: require_simple_string(
                path,
                &doc,
                "duplicate_test",
                "original_transfer",
            )?,
            duplicate_of_block_index: require_simple_u64(
                path,
                &doc,
                "duplicate_test",
                "duplicate_of_block_index",
            )?,
            proof_source: parse_local_sns_proof_source(
                path,
                &doc,
                "duplicate_test",
                "proof_source",
            )?,
            proof_method: parse_local_sns_proof_method(
                path,
                &doc,
                "duplicate_test",
                "proof_method",
            )?,
            proof_source_canister: parse_required_principal(
                path,
                &doc,
                "duplicate_test",
                "proof_source_canister",
            )?,
            proof_account: parse_local_sns_account(path, &doc, "duplicate_test", "proof_account")?,
            observation_timestamp: require_simple_string(
                path,
                &doc,
                "duplicate_test",
                "observation_timestamp",
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
            fee_disposition_mode: require_simple_string(
                path,
                &doc,
                "issuance_model",
                "fee_disposition_mode",
            )?,
            total_supply_changes_explained: require_simple_bool(
                path,
                &doc,
                "issuance_model",
                "total_supply_changes_explained",
            )?,
        },
    };
    validate_local_sns_evidence(path, text, &doc, &evidence)?;
    Ok(evidence)
}

fn parse_local_sns_transfer_evidence(
    path: &str,
    doc: &SimpleTomlDocument,
    section: &str,
) -> Result<LocalSnsTransferEvidence, String> {
    Ok(LocalSnsTransferEvidence {
        block_index: require_simple_u64(path, doc, section, "block_index")?,
        from_account: parse_local_sns_account(path, doc, section, "from")?,
        to_account: parse_local_sns_account(path, doc, section, "to")?,
        requested_amount_e8s: require_simple_u128(path, doc, section, "requested_amount_e8s")?,
        observed_fee_e8s: require_simple_u128(path, doc, section, "observed_fee_e8s")?,
        fee_disposition: require_simple_string(path, doc, section, "fee_disposition")?,
        sender_balance_before_e8s: require_simple_u128(
            path,
            doc,
            section,
            "sender_balance_before_e8s",
        )?,
        sender_balance_after_e8s: require_simple_u128(
            path,
            doc,
            section,
            "sender_balance_after_e8s",
        )?,
        recipient_balance_before_e8s: require_simple_u128(
            path,
            doc,
            section,
            "recipient_balance_before_e8s",
        )?,
        recipient_balance_after_e8s: require_simple_u128(
            path,
            doc,
            section,
            "recipient_balance_after_e8s",
        )?,
        fee_collector_account: parse_optional_local_sns_account(
            path,
            doc,
            section,
            "fee_collector",
        )?,
        fee_collector_balance_before_e8s: parse_optional_u128(
            path,
            doc,
            section,
            "fee_collector_balance_before_e8s",
        )?,
        fee_collector_balance_after_e8s: parse_optional_u128(
            path,
            doc,
            section,
            "fee_collector_balance_after_e8s",
        )?,
        total_supply_before_e8s: require_simple_u128(
            path,
            doc,
            section,
            "total_supply_before_e8s",
        )?,
        total_supply_after_e8s: require_simple_u128(path, doc, section, "total_supply_after_e8s")?,
        reserve_balance_before_e8s: require_simple_u128(
            path,
            doc,
            section,
            "reserve_balance_before_e8s",
        )?,
        reserve_balance_after_e8s: require_simple_u128(
            path,
            doc,
            section,
            "reserve_balance_after_e8s",
        )?,
        ledger_tip_block_index: require_simple_u64(path, doc, section, "ledger_tip_block_index")?,
        index_synced_through_block_index: require_simple_u64(
            path,
            doc,
            section,
            "index_synced_through_block_index",
        )?,
        proof_source: parse_local_sns_proof_source(path, doc, section, "proof_source")?,
        proof_source_canister: parse_required_principal(
            path,
            doc,
            section,
            "proof_source_canister",
        )?,
        proof_method: parse_local_sns_proof_method(path, doc, section, "proof_method")?,
        proof_account: parse_local_sns_account(path, doc, section, "proof_account")?,
        archive_canister: parse_optional_principal_string(path, doc, section, "archive_canister")?,
        archive_range_start: parse_optional_u64(path, doc, section, "archive_range_start")?,
        archive_range_end: parse_optional_u64(path, doc, section, "archive_range_end")?,
        archive_involvement: require_simple_string(path, doc, section, "archive_involvement")?,
        observation_timestamp: require_simple_string(path, doc, section, "observation_timestamp")?,
    })
}

fn parse_local_sns_proof_source(
    path: &str,
    doc: &SimpleTomlDocument,
    section: &str,
    key: &str,
) -> Result<LocalSnsProofSource, String> {
    match require_simple_string(path, doc, section, key)?.as_str() {
        "SnsLedgerBlock" => Ok(LocalSnsProofSource::LedgerBlock),
        "SnsIndexAccountHistory" => Ok(LocalSnsProofSource::IndexAccountHistory),
        "SnsLedgerArchive" => Ok(LocalSnsProofSource::LedgerArchive),
        other => Err(format!(
            "{path}: {section}.{key} must be SnsLedgerBlock, SnsIndexAccountHistory, or SnsLedgerArchive, got {other}"
        )),
    }
}

fn parse_local_sns_proof_method(
    path: &str,
    doc: &SimpleTomlDocument,
    section: &str,
    key: &str,
) -> Result<LocalSnsProofMethod, String> {
    match require_simple_string(path, doc, section, key)?.as_str() {
        "Icrc3GetBlocks" => Ok(LocalSnsProofMethod::Icrc3GetBlocks),
        "IcrcIndexGetAccountTransactions" => {
            Ok(LocalSnsProofMethod::IcrcIndexGetAccountTransactions)
        }
        "ArchiveGetBlocks" => Ok(LocalSnsProofMethod::ArchiveGetBlocks),
        other => Err(format!(
            "{path}: {section}.{key} must be Icrc3GetBlocks, IcrcIndexGetAccountTransactions, or ArchiveGetBlocks, got {other}"
        )),
    }
}

fn parse_optional_hex(
    path: &str,
    doc: &SimpleTomlDocument,
    section: &str,
    key: &str,
) -> Result<Option<String>, String> {
    let value = require_simple_string(path, doc, section, key)?;
    if value == "none" {
        return Ok(None);
    }
    if !value.is_empty()
        && value.len() % 2 == 0
        && value.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Ok(Some(value.to_ascii_lowercase()));
    }
    Err(format!(
        "{path}: {section}.{key} must be even-length hex or none"
    ))
}

fn parse_local_sns_account(
    path: &str,
    doc: &SimpleTomlDocument,
    section: &str,
    prefix: &str,
) -> Result<LocalSnsAccountEvidence, String> {
    let owner_key = format!("{prefix}_owner");
    let subaccount_key = format!("{prefix}_subaccount_hex");
    let owner = parse_required_principal(path, doc, section, &owner_key)?;
    if owner == Principal::anonymous() || owner == Principal::management_canister() {
        return Err(format!(
            "{path}: {section}.{owner_key} must not be anonymous or management canister"
        ));
    }
    let subaccount_hex = parse_subaccount_hex(path, doc, section, &subaccount_key)?;
    Ok(LocalSnsAccountEvidence {
        owner,
        subaccount_hex,
    })
}

fn parse_optional_local_sns_account(
    path: &str,
    doc: &SimpleTomlDocument,
    section: &str,
    prefix: &str,
) -> Result<Option<LocalSnsAccountEvidence>, String> {
    let owner_key = format!("{prefix}_owner");
    let subaccount_key = format!("{prefix}_subaccount_hex");
    let owner_value = require_simple_string(path, doc, section, &owner_key)?;
    let subaccount_value = require_simple_string(path, doc, section, &subaccount_key)?;
    if owner_value == "none" && subaccount_value == "none" {
        return Ok(None);
    }
    if owner_value == "none" || subaccount_value == "none" {
        return Err(format!(
            "{path}: {section}.{prefix} owner/subaccount must both be none or both exact"
        ));
    }
    parse_local_sns_account(path, doc, section, prefix).map(Some)
}

fn parse_optional_u128(
    path: &str,
    doc: &SimpleTomlDocument,
    section: &str,
    key: &str,
) -> Result<Option<u128>, String> {
    match doc.get(section).and_then(|section| section.get(key)) {
        Some(SimpleTomlValue::String(value)) if value == "none" => Ok(None),
        Some(SimpleTomlValue::Integer(value)) => Ok(Some(*value)),
        Some(SimpleTomlValue::String(value)) => value
            .parse::<u128>()
            .map(Some)
            .map_err(|_| format!("{path}: {section}.{key} must be an integer or \"none\"")),
        Some(SimpleTomlValue::Bool(_)) => Err(format!(
            "{path}: {section}.{key} must be an integer or \"none\""
        )),
        None => Err(format!("{path}: missing {section}.{key}")),
    }
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
        || evidence.mode.official_tooling != "manual-local-only"
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
    validate_local_sns_toolchain(path, &evidence.toolchain)?;
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
    validate_local_sns_archive_evidence(path, evidence)?;
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
    let required_treasury = evidence
        .expected
        .protocol_reserve_funding_amount_e8s
        .checked_add(evidence.expected.minimum_remaining_treasury_e8s)
        .and_then(|value| {
            value.checked_add(evidence.reserve_funding_transfer.transfer.observed_fee_e8s)
        })
        .ok_or_else(|| format!("{path}: reserve funding treasury requirement overflow"))?;
    if evidence.expected.treasury_initial_balance_e8s < required_treasury {
        return Err(format!(
            "{path}: expected treasury must cover desired reserve plus remaining treasury plus one transfer fee"
        ));
    }
    let reserve_account = LocalSnsAccountEvidence {
        owner: evidence.ledger.protocol_reserve_account_owner,
        subaccount_hex: evidence.ledger.protocol_reserve_subaccount_hex.clone(),
    };
    if reserve_account.owner != evidence.io_dapp_canisters.io_stream_manager {
        return Err(format!(
            "{path}: protocol reserve owner must equal io_dapp_canisters.io_stream_manager"
        ));
    }
    if reserve_account.subaccount_hex.is_none() {
        return Err(format!(
            "{path}: protocol reserve account requires an exact configured subaccount"
        ));
    }
    let redemption_account = &evidence.user_to_redemption_transfer.to_account;
    if redemption_account == &reserve_account {
        return Err(format!(
            "{path}: protocol reserve Account must not collide with redemption Account"
        ));
    }
    validate_local_sns_transfer(
        path,
        "reserve_funding_transfer",
        &evidence.reserve_funding_transfer.transfer,
        &reserve_account,
    )?;
    validate_local_sns_reserve_funding(path, evidence, &reserve_account)?;
    validate_local_sns_transfer(
        path,
        "transfer_reserve_to_user",
        &evidence.reserve_to_user_transfer,
        &reserve_account,
    )?;
    validate_local_sns_transfer(
        path,
        "transfer_user_to_redemption",
        &evidence.user_to_redemption_transfer,
        &reserve_account,
    )?;
    validate_local_sns_transfer(
        path,
        "transfer_redemption_to_reserve",
        &evidence.redemption_to_reserve_transfer,
        &reserve_account,
    )?;
    for (section, transfer) in [
        (
            "reserve_funding_transfer",
            &evidence.reserve_funding_transfer.transfer,
        ),
        (
            "transfer_reserve_to_user",
            &evidence.reserve_to_user_transfer,
        ),
        (
            "transfer_user_to_redemption",
            &evidence.user_to_redemption_transfer,
        ),
        (
            "transfer_redemption_to_reserve",
            &evidence.redemption_to_reserve_transfer,
        ),
    ] {
        validate_local_sns_transfer_proof(path, section, transfer, evidence)?;
    }
    validate_local_sns_transfer_sequence(path, evidence, &reserve_account)?;
    validate_local_sns_duplicate_test(path, evidence)?;
    if evidence.ledger.total_supply_e8s
        != evidence
            .redemption_to_reserve_transfer
            .total_supply_after_e8s
    {
        return Err(format!(
            "{path}: ledger_evidence.total_supply_e8s must match the final observed transfer supply"
        ));
    }
    if evidence.ledger.protocol_reserve_balance_e8s
        != evidence
            .redemption_to_reserve_transfer
            .reserve_balance_after_e8s
    {
        return Err(format!(
            "{path}: ledger_evidence.protocol_reserve_balance_e8s must match final observed reserve balance"
        ));
    }
    if evidence.issuance.fee_disposition_mode == "unknown" {
        return Err(format!(
            "{path}: issuance_model.fee_disposition_mode must not be unknown in completed evidence"
        ));
    }
    for (section, transfer) in [
        (
            "transfer_reserve_to_user",
            &evidence.reserve_to_user_transfer,
        ),
        (
            "transfer_user_to_redemption",
            &evidence.user_to_redemption_transfer,
        ),
        (
            "transfer_redemption_to_reserve",
            &evidence.redemption_to_reserve_transfer,
        ),
    ] {
        if transfer.fee_disposition != evidence.issuance.fee_disposition_mode {
            return Err(format!(
                "{path}: {section}.fee_disposition must match issuance_model.fee_disposition_mode"
            ));
        }
    }
    if !evidence.issuance.total_supply_changes_explained {
        return Err(format!(
            "{path}: issuance_model.total_supply_changes_explained must be true"
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
    if evidence.ledger.duplicate_of_block_index.is_none() {
        return Err(format!(
            "{path}: top-level ledger evidence must reference the detailed duplicate test"
        ));
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
    if !evidence.issuance.treasury_transfer_assumed {
        return Err(format!(
            "{path}: treasury_transfer_assumed must be true for post-finalisation reserve funding evidence"
        ));
    }
    let _ = evidence.ledger.reserve_transfer_block_index;
    let _ = evidence.ledger.redemption_return_block_index;
    Ok(())
}

fn validate_local_sns_archive_evidence(
    path: &str,
    evidence: &LocalSnsEvidence,
) -> Result<(), String> {
    if evidence.archive.archive_canister != evidence.sns_canisters.archive {
        return Err(format!(
            "{path}: archive_evidence.archive_canister must match sns_canisters.archive"
        ));
    }
    match evidence.archive.archive_canister {
        None => {
            if evidence.archive.discovered_from_ledger
                || evidence.archive.discovered_from_root
                || evidence.archive.range_start.is_some()
                || evidence.archive.range_end.is_some()
            {
                return Err(format!(
                    "{path}: absent archive must not claim discovery or a served range"
                ));
            }
        }
        Some(_) => {
            if !evidence.archive.discovered_from_ledger || !evidence.archive.discovered_from_root {
                return Err(format!(
                    "{path}: archive canister must be present in ledger and root discovery evidence"
                ));
            }
            let start = evidence
                .archive
                .range_start
                .ok_or_else(|| format!("{path}: discovered archive requires exact range_start"))?;
            let end = evidence
                .archive
                .range_end
                .ok_or_else(|| format!("{path}: discovered archive requires exact range_end"))?;
            if start > end {
                return Err(format!("{path}: archive discovery range is inverted"));
            }
        }
    }
    Ok(())
}

fn validate_local_sns_reserve_funding(
    path: &str,
    evidence: &LocalSnsEvidence,
    reserve_account: &LocalSnsAccountEvidence,
) -> Result<(), String> {
    let funding = &evidence.reserve_funding_transfer;
    let transfer = &funding.transfer;
    if funding.sns_proposal_id == 0 || !funding.proposal_adopted || !funding.proposal_executed {
        return Err(format!(
            "{path}: reserve funding requires a nonzero adopted and executed SNS proposal"
        ));
    }
    let _dedup_metadata = (funding.created_at_time_nanos, funding.memo_hex.as_deref());
    if transfer.to_account != *reserve_account
        || transfer.requested_amount_e8s != evidence.expected.protocol_reserve_funding_amount_e8s
        || transfer.observed_fee_e8s != evidence.ledger.transaction_fee_e8s
        || transfer.block_index >= evidence.reserve_to_user_transfer.block_index
    {
        return Err(format!(
            "{path}: reserve funding transfer must fund the exact reserve before activation"
        ));
    }
    if transfer.sender_balance_before_e8s != evidence.expected.treasury_initial_balance_e8s {
        return Err(format!(
            "{path}: reserve funding treasury-before must match configured genesis treasury"
        ));
    }
    let treasury_decrease = transfer
        .sender_balance_before_e8s
        .checked_sub(transfer.sender_balance_after_e8s)
        .ok_or_else(|| format!("{path}: reserve funding treasury balance increased"))?;
    let expected_treasury_decrease = transfer
        .requested_amount_e8s
        .checked_add(transfer.observed_fee_e8s)
        .ok_or_else(|| format!("{path}: reserve funding amount plus fee overflow"))?;
    if treasury_decrease != expected_treasury_decrease {
        return Err(format!(
            "{path}: reserve funding treasury decrease must equal amount plus fee"
        ));
    }
    let reserve_increase = transfer
        .recipient_balance_after_e8s
        .checked_sub(transfer.recipient_balance_before_e8s)
        .ok_or_else(|| format!("{path}: reserve funding reserve balance decreased"))?;
    if reserve_increase != transfer.requested_amount_e8s {
        return Err(format!(
            "{path}: reserve funding reserve increase must equal funded amount"
        ));
    }
    if transfer.total_supply_before_e8s != evidence.expected.total_supply_e8s
        || transfer
            .total_supply_before_e8s
            .checked_sub(transfer.total_supply_after_e8s)
            != Some(transfer.observed_fee_e8s)
    {
        return Err(format!(
            "{path}: reserve funding total-supply decrease must equal the burned fee"
        ));
    }
    if evidence.reserve_to_user_transfer.total_supply_before_e8s != transfer.total_supply_after_e8s
    {
        return Err(format!(
            "{path}: first reserve-to-user supply before must equal reserve-funding supply after"
        ));
    }
    if evidence.reserve_to_user_transfer.reserve_balance_before_e8s
        != transfer.reserve_balance_after_e8s
    {
        return Err(format!(
            "{path}: first reserve balance before must equal reserve-funding reserve balance after"
        ));
    }
    Ok(())
}

fn validate_local_sns_toolchain(
    path: &str,
    toolchain: &LocalSnsToolchainProvenance,
) -> Result<(), String> {
    if toolchain.official_ic_repository != "dfinity/ic" {
        return Err(format!(
            "{path}: toolchain_provenance.official_ic_repository must be dfinity/ic"
        ));
    }
    if toolchain.sns_testing_source_path != "rs/sns/testing" {
        return Err(format!(
            "{path}: toolchain_provenance.sns_testing_source_path must be rs/sns/testing"
        ));
    }
    if toolchain.official_ic_source_commit.len() != 40
        || !toolchain
            .official_ic_source_commit
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(format!(
            "{path}: toolchain_provenance.official_ic_source_commit must be exact 40-hex commit"
        ));
    }
    if ["main", "master", "HEAD"].contains(&toolchain.official_ic_source_commit.as_str()) {
        return Err(format!(
            "{path}: toolchain_provenance must not use a moving branch"
        ));
    }
    for (name, value) in [
        ("dfx_version", &toolchain.dfx_version),
        ("bazel_version", &toolchain.bazel_version),
        ("pocket_ic_version", &toolchain.pocket_ic_version),
        ("sns_cli_version", &toolchain.sns_cli_version),
        (
            "sns_testing_cli_version",
            &toolchain.sns_testing_cli_version,
        ),
        ("quill_version", &toolchain.quill_version),
        ("rustc_version", &toolchain.rustc_version),
        ("cargo_version", &toolchain.cargo_version),
        ("network_alias", &toolchain.network_alias),
        (
            "locally_built_binary_sha256",
            &toolchain.locally_built_binary_sha256,
        ),
    ] {
        let normalized = value.to_ascii_lowercase();
        if value.trim().is_empty()
            || [
                "blocked",
                "unavailable",
                "not-installed",
                "unknown",
                "placeholder",
                "todo",
            ]
            .iter()
            .any(|marker| normalized.contains(marker))
            || matches!(value.as_str(), "main" | "master" | "HEAD")
        {
            return Err(format!(
                "{path}: toolchain_provenance.{name} must be exact and non-placeholder"
            ));
        }
    }
    for (name, value) in [
        ("sns_cli_sha256", &toolchain.sns_cli_sha256),
        (
            "sns_testing_init_sha256",
            &toolchain.sns_testing_init_sha256,
        ),
        ("sns_testing_cli_sha256", &toolchain.sns_testing_cli_sha256),
        (
            "locally_built_binary_sha256",
            &toolchain.locally_built_binary_sha256,
        ),
    ] {
        if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(format!(
                "{path}: toolchain_provenance.{name} must be exact 64-hex SHA-256"
            ));
        }
    }
    if !is_strict_loopback_http_url(&toolchain.local_network_url) {
        return Err(format!(
            "{path}: toolchain_provenance.local_network_url must be loopback-only"
        ));
    }
    if toolchain.operator_identity_principal == Principal::anonymous()
        || toolchain.operator_identity_principal == Principal::management_canister()
    {
        return Err(format!(
            "{path}: toolchain_provenance.operator_identity_principal must be local non-anonymous principal"
        ));
    }
    Ok(())
}

fn validate_local_sns_transfer_sequence(
    path: &str,
    evidence: &LocalSnsEvidence,
    reserve_account: &LocalSnsAccountEvidence,
) -> Result<(), String> {
    let t1 = &evidence.reserve_to_user_transfer;
    let t2 = &evidence.user_to_redemption_transfer;
    let t3 = &evidence.redemption_to_reserve_transfer;
    if t1.block_index >= t2.block_index || t2.block_index >= t3.block_index {
        return Err(format!(
            "{path}: transfer block indexes must strictly increase"
        ));
    }
    if !timestamp_leq(&t1.observation_timestamp, &t2.observation_timestamp)
        || !timestamp_leq(&t2.observation_timestamp, &t3.observation_timestamp)
    {
        return Err(format!(
            "{path}: transfer observation timestamps must be parseable RFC3339 UTC and non-decreasing"
        ));
    }
    if t1.to_account != t2.from_account {
        return Err(format!(
            "{path}: reserve-to-user.to must equal user-to-redemption.from"
        ));
    }
    if t2.to_account != t3.from_account {
        return Err(format!(
            "{path}: user-to-redemption.to must equal redemption-to-reserve.from"
        ));
    }
    if &t1.from_account != reserve_account || &t3.to_account != reserve_account {
        return Err(format!(
            "{path}: reserve transfer endpoints must match configured protocol reserve account"
        ));
    }
    let redemption_account = &t2.to_account;
    if redemption_account != &t3.from_account {
        return Err(format!(
            "{path}: exact redemption account must be stable across intake and return"
        ));
    }
    if redemption_account.owner != evidence.io_dapp_canisters.io_stream_manager {
        return Err(format!(
            "{path}: redemption account owner must equal io_dapp_canisters.io_stream_manager"
        ));
    }
    if redemption_account == reserve_account {
        return Err(format!(
            "{path}: protocol reserve Account must not collide with redemption Account"
        ));
    }
    if t1.total_supply_after_e8s != t2.total_supply_before_e8s
        || t2.total_supply_after_e8s != t3.total_supply_before_e8s
    {
        return Err(format!("{path}: transfer total supply continuity failed"));
    }
    if t1.reserve_balance_after_e8s != t2.reserve_balance_before_e8s
        || t2.reserve_balance_after_e8s != t3.reserve_balance_before_e8s
    {
        return Err(format!(
            "{path}: transfer reserve balance continuity failed"
        ));
    }
    if t1.recipient_balance_after_e8s != t2.sender_balance_before_e8s {
        return Err(format!("{path}: user account balance continuity failed"));
    }
    if t2.recipient_balance_after_e8s != t3.sender_balance_before_e8s {
        return Err(format!(
            "{path}: redemption account balance continuity failed"
        ));
    }
    if evidence.ledger.reserve_transfer_block_index != t1.block_index
        || evidence.ledger.redemption_return_block_index != t3.block_index
        || evidence.ledger.reserve_transfer_amount_e8s != t1.requested_amount_e8s
        || evidence.ledger.redemption_return_amount_e8s != t3.requested_amount_e8s
    {
        return Err(format!(
            "{path}: top-level ledger evidence must match detailed transfer records"
        ));
    }
    let expected_final_supply = evidence
        .expected
        .total_supply_e8s
        .checked_sub(evidence.reserve_funding_transfer.transfer.observed_fee_e8s)
        .and_then(|value| value.checked_sub(t1.observed_fee_e8s))
        .and_then(|value| value.checked_sub(t2.observed_fee_e8s))
        .and_then(|value| value.checked_sub(t3.observed_fee_e8s))
        .ok_or_else(|| format!("{path}: total-supply fee-burn equation underflow"))?;
    if t3.total_supply_after_e8s != expected_final_supply {
        return Err(format!(
            "{path}: final supply must equal genesis supply minus reserve-funding and subsequent transfer fees"
        ));
    }
    Ok(())
}

fn validate_local_sns_transfer_proof(
    path: &str,
    section: &str,
    transfer: &LocalSnsTransferEvidence,
    evidence: &LocalSnsEvidence,
) -> Result<(), String> {
    let (expected_canister, expected_method) = match transfer.proof_source {
        LocalSnsProofSource::LedgerBlock => (
            evidence.sns_canisters.ledger,
            LocalSnsProofMethod::Icrc3GetBlocks,
        ),
        LocalSnsProofSource::IndexAccountHistory => (
            evidence.sns_canisters.index,
            LocalSnsProofMethod::IcrcIndexGetAccountTransactions,
        ),
        LocalSnsProofSource::LedgerArchive => (
            evidence
                .archive
                .archive_canister
                .ok_or_else(|| format!("{path}: {section} archive proof lacks discovery"))?,
            LocalSnsProofMethod::ArchiveGetBlocks,
        ),
    };
    if transfer.proof_source_canister != expected_canister
        || transfer.proof_method != expected_method
    {
        return Err(format!(
            "{path}: {section} proof source/method is not bound to the recorded SNS canister role"
        ));
    }
    if transfer.proof_source == LocalSnsProofSource::LedgerArchive {
        if transfer.archive_canister != Some(expected_canister) {
            return Err(format!(
                "{path}: {section} archive proof canister must match discovered archive"
            ));
        }
        let start = transfer
            .archive_range_start
            .ok_or_else(|| format!("{path}: {section} archive proof requires exact range start"))?;
        let end = transfer
            .archive_range_end
            .ok_or_else(|| format!("{path}: {section} archive proof requires exact range end"))?;
        if start > transfer.block_index || transfer.block_index > end {
            return Err(format!(
                "{path}: {section} archive range must contain the relevant block"
            ));
        }
        if evidence.archive.range_start > Some(start) || evidence.archive.range_end < Some(end) {
            return Err(format!(
                "{path}: {section} archive proof range exceeds discovered archive range"
            ));
        }
    }
    Ok(())
}

fn detailed_transfer_by_name<'a>(
    evidence: &'a LocalSnsEvidence,
    name: &str,
) -> Option<&'a LocalSnsTransferEvidence> {
    match name {
        "reserve_funding_transfer" => Some(&evidence.reserve_funding_transfer.transfer),
        "transfer_reserve_to_user" => Some(&evidence.reserve_to_user_transfer),
        "transfer_user_to_redemption" => Some(&evidence.user_to_redemption_transfer),
        "transfer_redemption_to_reserve" => Some(&evidence.redemption_to_reserve_transfer),
        _ => None,
    }
}

fn validate_local_sns_duplicate_test(
    path: &str,
    evidence: &LocalSnsEvidence,
) -> Result<(), String> {
    let duplicate = &evidence.duplicate_test;
    let original =
        detailed_transfer_by_name(evidence, &duplicate.original_transfer).ok_or_else(|| {
            format!("{path}: duplicate_test.original_transfer is not a detailed transfer")
        })?;
    if evidence.ledger.duplicate_tested_transfer != duplicate.original_transfer
        || evidence.ledger.duplicate_of_block_index != Some(duplicate.duplicate_of_block_index)
        || duplicate.duplicate_of_block_index != original.block_index
    {
        return Err(format!(
            "{path}: top-level duplicate evidence must reference one exact detailed transfer and its successful block"
        ));
    }
    if duplicate.proof_account != original.from_account
        && duplicate.proof_account != original.to_account
    {
        return Err(format!(
            "{path}: duplicate proof Account must match the exact queried transfer Account"
        ));
    }
    if parse_rfc3339_utc(&duplicate.observation_timestamp).is_none() {
        return Err(format!(
            "{path}: duplicate_test.observation_timestamp must be RFC3339 UTC seconds"
        ));
    }
    let proof_transfer = LocalSnsTransferEvidence {
        proof_source: duplicate.proof_source,
        proof_method: duplicate.proof_method,
        proof_source_canister: duplicate.proof_source_canister,
        proof_account: duplicate.proof_account.clone(),
        ..original.clone()
    };
    validate_local_sns_transfer_proof(path, "duplicate_test", &proof_transfer, evidence)
}

fn timestamp_leq(left: &str, right: &str) -> bool {
    parse_rfc3339_utc(left)
        .zip(parse_rfc3339_utc(right))
        .map(|(left, right)| left <= right)
        .unwrap_or(false)
}

fn parse_rfc3339_utc(value: &str) -> Option<OffsetDateTime> {
    OffsetDateTime::parse(value, &Rfc3339).ok()
}

fn is_strict_loopback_http_url(value: &str) -> bool {
    if value.trim() != value || !value.starts_with("http://") || value.contains('#') {
        return false;
    }
    let rest = &value["http://".len()..];
    let authority_end = rest.find(['/', '?']).unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    if authority.is_empty() || authority.contains('@') || authority.contains('%') {
        return false;
    }
    let (host, port) = if let Some(remainder) = authority.strip_prefix("[::1]:") {
        ("::1", remainder)
    } else if authority.contains('[')
        || authority.contains(']')
        || authority.matches(':').count() != 1
    {
        return false;
    } else {
        let (host, port) = authority.split_once(':').unwrap_or(("", ""));
        (host, port)
    };
    if !matches!(host, "localhost" | "127.0.0.1" | "::1") {
        return false;
    }
    matches!(port.parse::<u16>(), Ok(1..=u16::MAX))
}

fn validate_loopback_url_guardrails() -> Result<(), String> {
    for url in [
        "http://localhost:8080",
        "http://127.0.0.1:8080",
        "http://[::1]:8080",
        "http://localhost:8080/path?x=1",
    ] {
        if !is_strict_loopback_http_url(url) {
            return Err(format!("loopback URL guard rejected valid local URL {url}"));
        }
    }
    for url in [
        "http://localhost:8080@evil.example",
        "http://127.0.0.1:8080.evil.example",
        "http://localhost.evil.example:8080",
        "http://127.0.0.1.evil:8080",
        "http://[::1]@evil.example",
        "http://LOCALHOST:8080",
        "http://%6cocalhost:8080",
        " http://localhost:8080",
        "http://localhost:8080 ",
        "https://icp-api.io",
        "https://localhost:8080",
        "http://localhost",
        "http://localhost:0",
        "http://localhost:65536",
        "http://localhost:8080#fragment",
    ] {
        if is_strict_loopback_http_url(url) {
            return Err(format!("loopback URL guard accepted unsafe URL {url}"));
        }
    }
    Ok(())
}

fn validate_local_sns_transfer(
    path: &str,
    section: &str,
    transfer: &LocalSnsTransferEvidence,
    reserve_account: &LocalSnsAccountEvidence,
) -> Result<(), String> {
    if transfer.observation_timestamp.trim().is_empty() {
        return Err(format!(
            "{path}: {section} must record exact accounts and observation timestamp"
        ));
    }
    if transfer.requested_amount_e8s == 0 {
        return Err(format!(
            "{path}: {section}.requested_amount_e8s must be nonzero"
        ));
    }
    if transfer.index_synced_through_block_index < transfer.block_index {
        return Err(format!(
            "{path}: {section} index evidence is stale or incomplete"
        ));
    }
    if transfer.ledger_tip_block_index < transfer.block_index {
        return Err(format!(
            "{path}: {section} ledger tip evidence is stale or incomplete"
        ));
    }
    if transfer.proof_account != transfer.from_account
        && transfer.proof_account != transfer.to_account
    {
        return Err(format!(
            "{path}: {section}.proof_account must match a transfer participant"
        ));
    }
    validate_local_principal_value(
        path,
        &format!("{section}.proof_source_canister"),
        &transfer.proof_source_canister.to_text(),
    )?;
    match transfer.archive_involvement.as_str() {
        "none" => {
            if transfer.archive_canister.is_some()
                || transfer.archive_range_start.is_some()
                || transfer.archive_range_end.is_some()
            {
                return Err(format!(
                    "{path}: {section} archive none must not include archive range evidence"
                ));
            }
        }
        "ledger_archived_range" | "index_followed_archive" => {
            if transfer.archive_canister.is_none()
                || transfer.archive_range_start.is_none()
                || transfer.archive_range_end.is_none()
            {
                return Err(format!(
                    "{path}: {section} complete archive proof requires canister and range"
                ));
            }
        }
        "incomplete" => {
            return Err(format!(
                "{path}: {section} incomplete archive proof cannot be completed evidence"
            ));
        }
        _ => {
            return Err(format!(
                "{path}: {section}.archive_involvement must be none, ledger_archived_range, index_followed_archive, or incomplete"
            ));
        }
    }
    if parse_rfc3339_utc(&transfer.observation_timestamp).is_none() {
        return Err(format!(
            "{path}: {section}.observation_timestamp must be RFC3339 UTC seconds"
        ));
    }
    let expected_sender_decrease = checked_i128(path, section, transfer.requested_amount_e8s)?
        .checked_add(checked_i128(path, section, transfer.observed_fee_e8s)?)
        .ok_or_else(|| format!("{path}: {section} amount plus fee overflow"))?;
    let mut deltas = BTreeMap::<LocalSnsAccountEvidence, i128>::new();
    add_delta(
        &mut deltas,
        transfer.from_account.clone(),
        -expected_sender_decrease,
    )?;
    add_delta(
        &mut deltas,
        transfer.to_account.clone(),
        checked_i128(path, section, transfer.requested_amount_e8s)?,
    )?;

    match transfer.fee_disposition.as_str() {
        "burned" => {
            if transfer.fee_collector_account.is_some()
                || transfer.fee_collector_balance_before_e8s.is_some()
                || transfer.fee_collector_balance_after_e8s.is_some()
            {
                return Err(format!(
                    "{path}: {section} burned fee mode must not claim fee collector evidence"
                ));
            }
            let supply_decrease = transfer
                .total_supply_before_e8s
                .checked_sub(transfer.total_supply_after_e8s)
                .ok_or_else(|| format!("{path}: {section} total supply increased unexpectedly"))?;
            if supply_decrease != transfer.observed_fee_e8s {
                return Err(format!(
                    "{path}: {section} burned fee supply decrease must equal fee"
                ));
            }
        }
        "unknown" => {
            return Err(format!(
                "{path}: {section}.fee_disposition must not be unknown in completed evidence"
            ));
        }
        other => {
            return Err(format!(
                "{path}: {section}.fee_disposition must be burned under standard SNS fee policy, got {other}"
            ));
        }
    }

    let mut observations = BTreeMap::new();
    validate_observed_account_delta(
        path,
        section,
        &mut observations,
        transfer.from_account.clone(),
        transfer.sender_balance_before_e8s,
        transfer.sender_balance_after_e8s,
    )?;
    validate_observed_account_delta(
        path,
        section,
        &mut observations,
        transfer.to_account.clone(),
        transfer.recipient_balance_before_e8s,
        transfer.recipient_balance_after_e8s,
    )?;
    if let Some(collector) = &transfer.fee_collector_account {
        validate_observed_account_delta(
            path,
            section,
            &mut observations,
            collector.clone(),
            transfer.fee_collector_balance_before_e8s.unwrap(),
            transfer.fee_collector_balance_after_e8s.unwrap(),
        )?;
    }
    for (account, expected_delta) in &deltas {
        let (before, after) = observations
            .get(account)
            .ok_or_else(|| format!("{path}: {section} missing observed balance for account"))?;
        let observed_delta = checked_i128(path, section, *after)?
            .checked_sub(checked_i128(path, section, *before)?)
            .ok_or_else(|| format!("{path}: {section} observed account delta overflow"))?;
        if observed_delta != *expected_delta {
            return Err(format!(
                "{path}: {section} unexplained balance movement for account"
            ));
        }
    }
    let reserve_expected_delta = *deltas.get(reserve_account).unwrap_or(&0);
    let reserve_observed_delta = checked_i128(path, section, transfer.reserve_balance_after_e8s)?
        .checked_sub(checked_i128(
            path,
            section,
            transfer.reserve_balance_before_e8s,
        )?)
        .ok_or_else(|| format!("{path}: {section} reserve delta overflow"))?;
    if reserve_observed_delta != reserve_expected_delta {
        return Err(format!(
            "{path}: {section} reserve change does not match reserve account net delta"
        ));
    }
    Ok(())
}

fn checked_i128(path: &str, section: &str, value: u128) -> Result<i128, String> {
    value
        .try_into()
        .map_err(|_| format!("{path}: {section} value does not fit signed validation range"))
}

fn add_delta(
    deltas: &mut BTreeMap<LocalSnsAccountEvidence, i128>,
    account: LocalSnsAccountEvidence,
    delta: i128,
) -> Result<(), String> {
    let entry = deltas.entry(account).or_insert(0);
    *entry = entry
        .checked_add(delta)
        .ok_or_else(|| "local SNS transfer delta overflow".to_string())?;
    Ok(())
}

fn validate_observed_account_delta(
    path: &str,
    section: &str,
    observations: &mut BTreeMap<LocalSnsAccountEvidence, (u128, u128)>,
    account: LocalSnsAccountEvidence,
    before: u128,
    after: u128,
) -> Result<(), String> {
    if let Some(existing) = observations.insert(account, (before, after)) {
        if existing != (before, after) {
            return Err(format!(
                "{path}: {section} overlapping account observations disagree"
            ));
        }
    }
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

fn check_local_sns_committed_evidence_at(root: &Path) -> Result<(), String> {
    let evidence_root = root.join("deploy/local-sns-rehearsal/evidence");
    if !evidence_root.exists() {
        return Ok(());
    }
    for entry in
        fs::read_dir(&evidence_root).map_err(|err| format!("{}: {err}", evidence_root.display()))?
    {
        let entry = entry.map_err(|err| format!("{}: {err}", evidence_root.display()))?;
        let entry_type = entry
            .file_type()
            .map_err(|err| format!("{}: {err}", entry.path().display()))?;
        if !entry_type.is_dir() {
            return Err(format!(
                "{}: evidence root entries must be regular directories",
                entry.path().display()
            ));
        }
        let rel = entry
            .path()
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let package_files = list_regular_files_relative(&entry.path())?;
        let manifest_path = format!("{rel}/manifest.toml");
        let manifest = require_file(root, &manifest_path)?;
        validate_committed_evidence_text(&manifest_path, &manifest)?;
        let doc = parse_simple_toml_document(&manifest_path, &manifest)?;
        if require_simple_string(&manifest_path, &doc, "provenance", "official_ic_repository")?
            != "dfinity/ic"
            || require_simple_string(
                &manifest_path,
                &doc,
                "provenance",
                "sns_testing_source_path",
            )? != "rs/sns/testing"
        {
            return Err(format!("{manifest_path}: invalid official SNS provenance"));
        }
        let complete = require_simple_bool(&manifest_path, &doc, "provenance", "complete")?;
        let commit = require_simple_string(
            &manifest_path,
            &doc,
            "provenance",
            "official_ic_source_commit",
        )?;
        if commit.len() != 40 || !commit.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(format!(
                "{manifest_path}: official_ic_source_commit must be exact 40-hex commit"
            ));
        }
        let expected_files: BTreeSet<String> = if complete {
            [
                "manifest.toml",
                "toolchain-provenance.toml",
                "sns_init.local.yaml",
                "canister-ids.local.toml",
                "reserve-funding-evidence.toml",
                "ledger-evidence.toml",
                "governance-evidence.toml",
                "controller-evidence.toml",
                "archive-evidence.toml",
                "commands.log",
                "SHA256SUMS",
            ]
            .into_iter()
            .map(str::to_string)
            .collect()
        } else {
            ["manifest.toml", "blocker-report.md", "SHA256SUMS"]
                .into_iter()
                .map(str::to_string)
                .collect()
        };
        if package_files != expected_files {
            let unexpected = package_files
                .difference(&expected_files)
                .cloned()
                .collect::<Vec<_>>();
            let missing = expected_files
                .difference(&package_files)
                .cloned()
                .collect::<Vec<_>>();
            return Err(format!(
                "{rel}: evidence package inventory mismatch; unexpected={unexpected:?}, missing={missing:?}"
            ));
        }
        validate_evidence_package_sha256s(root, &rel, &package_files)?;
        for file in &package_files {
            if file == "SHA256SUMS" {
                continue;
            }
            let file_path = format!("{rel}/{file}");
            let text = require_file(root, &file_path)?;
            validate_committed_evidence_text(&file_path, &text)?;
        }
        if complete {
            if doc
                .get("provenance")
                .is_some_and(|section| section.contains_key("blocker_report"))
            {
                return Err(format!(
                    "{manifest_path}: completed evidence must not contain blocker_report"
                ));
            }
            for file in &package_files {
                if file == "SHA256SUMS" {
                    continue;
                }
                let file_path = format!("{rel}/{file}");
                let text = require_file(root, &file_path)?;
                reject_completed_evidence_placeholders(&file_path, &text)?;
            }
            let toolchain_path = format!("{rel}/toolchain-provenance.toml");
            let toolchain = require_file(root, &toolchain_path)?;
            validate_completed_toolchain_provenance(&toolchain_path, &toolchain)?;
        } else {
            let blocker_report =
                require_simple_string(&manifest_path, &doc, "provenance", "blocker_report")?;
            if blocker_report != "blocker-report.md" {
                return Err(format!(
                    "{manifest_path}: incomplete package must reference blocker-report.md"
                ));
            }
            let blocker_path = format!("{rel}/blocker-report.md");
            let blocker = require_file(root, &blocker_path)?;
            require_present(
                &blocker_path,
                &blocker,
                &[
                    "official local SNS rehearsal not completed",
                    "source-built",
                    "No mainnet call",
                ],
            )?;
        }
    }
    Ok(())
}

fn validate_committed_evidence_text(path: &str, text: &str) -> Result<(), String> {
    let normalized = text.to_ascii_lowercase();
    for marker in [
        "-----begin private key-----",
        "-----begin rsa private key-----",
        "-----begin ec private key-----",
        "-----begin openssh private key-----",
        "identity.pem",
        ".pem",
        "seed phrase",
        "mnemonic phrase",
        "private_key",
        "private-key",
        "auth_token",
        "access_token",
        "--network ic",
        "-n ic",
        "icp-api.io",
        "icp0.io",
        "ic0.app",
    ] {
        if normalized.contains(marker) {
            return Err(format!(
                "{path}: committed evidence contains forbidden secret/private-key or mainnet material {marker:?}"
            ));
        }
    }
    Ok(())
}

fn reject_completed_evidence_placeholders(path: &str, text: &str) -> Result<(), String> {
    let normalized = text.to_ascii_lowercase();
    for marker in [
        "blocked",
        "unavailable",
        "not-installed",
        "unknown",
        "placeholder",
        "todo",
    ] {
        if normalized.contains(marker) {
            return Err(format!(
                "{path}: completed evidence contains forbidden placeholder marker {marker:?}"
            ));
        }
    }
    Ok(())
}

fn validate_completed_toolchain_provenance(path: &str, text: &str) -> Result<(), String> {
    let doc = parse_simple_toml_document(path, text)?;
    let mut version_count = 0_usize;
    for (section_name, section) in &doc {
        for (key, value) in section {
            if !key.ends_with("_version") {
                continue;
            }
            let SimpleTomlValue::String(version) = value else {
                return Err(format!("{path}: {section_name}.{key} must be a string"));
            };
            reject_completed_evidence_placeholders(path, version)?;
            if version.trim().is_empty() {
                return Err(format!(
                    "{path}: {section_name}.{key} must contain an exact version"
                ));
            }
            let hash_key = format!("{}_sha256", key.trim_end_matches("_version"));
            let Some(SimpleTomlValue::String(hash)) = section.get(&hash_key) else {
                return Err(format!(
                    "{path}: {section_name}.{key} requires matching {hash_key}"
                ));
            };
            if hash.len() != 64
                || !hash
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            {
                return Err(format!(
                    "{path}: {section_name}.{hash_key} must be an exact lowercase SHA-256"
                ));
            }
            version_count += 1;
        }
    }
    if version_count == 0 {
        return Err(format!(
            "{path}: completed evidence must record at least one exact tool version and matching SHA-256"
        ));
    }
    Ok(())
}

fn list_regular_files_relative(dir: &Path) -> Result<BTreeSet<String>, String> {
    fn walk(base: &Path, current: &Path, out: &mut BTreeSet<String>) -> Result<(), String> {
        for entry in fs::read_dir(current).map_err(|err| format!("{}: {err}", current.display()))? {
            let entry = entry.map_err(|err| format!("{}: {err}", current.display()))?;
            let path = entry.path();
            let ty = entry
                .file_type()
                .map_err(|err| format!("{}: {err}", path.display()))?;
            if ty.is_dir() {
                walk(base, &path, out)?;
            } else if ty.is_file() {
                let rel = path
                    .strip_prefix(base)
                    .map_err(|err| format!("{}: {err}", path.display()))?
                    .to_string_lossy()
                    .replace('\\', "/");
                if Path::new(&rel)
                    .components()
                    .any(|component| !matches!(component, std::path::Component::Normal(_)))
                {
                    return Err(format!(
                        "{}: evidence package path must be traversal-free",
                        path.display()
                    ));
                }
                if !out.insert(rel) {
                    return Err(format!(
                        "{}: duplicate evidence package path",
                        path.display()
                    ));
                }
            } else {
                return Err(format!(
                    "{}: evidence packages reject symlinks and non-regular files",
                    path.display()
                ));
            }
        }
        Ok(())
    }
    let mut out = BTreeSet::new();
    walk(dir, dir, &mut out)?;
    Ok(out)
}

fn validate_evidence_package_sha256s(
    root: &Path,
    rel: &str,
    package_files: &BTreeSet<String>,
) -> Result<(), String> {
    let sha_path = format!("{rel}/SHA256SUMS");
    let sha_text = require_file(root, &sha_path)?;
    let mut covered = BTreeSet::new();
    for line in sha_text.lines().filter(|line| !line.trim().is_empty()) {
        let (hash, file) = line
            .split_once("  ")
            .ok_or_else(|| format!("{sha_path}: invalid sha256sum line {line:?}"))?;
        if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(format!("{sha_path}: invalid SHA-256 hash for {file}"));
        }
        if file == "SHA256SUMS"
            || Path::new(file)
                .components()
                .any(|component| !matches!(component, std::path::Component::Normal(_)))
        {
            return Err(format!("{sha_path}: invalid covered path {file}"));
        }
        let bytes =
            fs::read(root.join(rel).join(file)).map_err(|err| format!("{rel}/{file}: {err}"))?;
        let actual = hex_sha256(&bytes);
        if actual != hash.to_ascii_lowercase() {
            return Err(format!("{rel}/{file}: SHA-256 mismatch"));
        }
        if !covered.insert(file.to_string()) {
            return Err(format!("{sha_path}: duplicate SHA256SUMS entry for {file}"));
        }
    }
    let expected: BTreeSet<String> = package_files
        .iter()
        .filter(|file| file.as_str() != "SHA256SUMS")
        .cloned()
        .collect();
    if covered != expected {
        return Err(format!(
            "{sha_path}: SHA256SUMS must cover every package file except itself"
        ));
    }
    Ok(())
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
            "Installed direct-reserve redemption",
            "Exact reward allocation",
            "Historian separation",
            "Scanner-era tests are historical",
        ],
    )?;
    let inventory = require_file(root, inventory_path)?;
    require_present(
        inventory_path,
        &inventory,
        &[
            "io-core-model",
            "io-reward-policy",
            "installed serialized redemption",
            "Historical scanner/journal coverage",
        ],
    )?;
    let scenarios = require_file(root, scenarios_path)?;
    require_present(
        scenarios_path,
        &scenarios,
        &[
            "Serialized redemption",
            "Jupiter 40/60",
            "Direct maturity",
            "Exact rewards",
            "One unwind child",
            "Historian and frontend",
            "Transport ambiguity",
            "Every upgrade returns Paused",
        ],
    )?;
    Ok(())
}

fn check_live_stream_manager_pocketic_gate_at(root: &Path) -> Result<(), String> {
    let script_path = "tools/scripts/run-io-stream-manager-live-pocketic";
    let docs_path = "docs/testing/current-test-inventory.md";
    let script = require_file(root, script_path)?;
    let required_tests = ["installed_stream_real_sns_icrc2_redemption"];
    require_present(
        script_path,
        &script,
        &[
            "POCKET_IC_BIN=${POCKET_IC_BIN}",
            "\"${POCKET_IC_BIN}\" --version",
            "-- --ignored --exact --list",
            "-- --ignored --exact --nocapture",
            "required test was not discovered exactly once",
            "required test did not report an explicit pass",
            "required test output looks like a skip instead of proof",
            "cargo test -p e2e-real-canisters",
            "live PocketIC tests genuinely ran:",
        ],
    )?;
    for test in required_tests {
        let count = script.matches(test).count();
        if count != 1 {
            return Err(format!(
                "{script_path}: required live test {test} appears {count} times, expected once"
            ));
        }
    }
    let docs = require_file(root, docs_path)?;
    require_present(
        docs_path,
        &docs,
        &[
            "tools/scripts/run-io-stream-manager-live-pocketic",
            "cargo run -p xtask -- live_stream_manager_pocketic_gate_check",
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
            "io-production-wiring.workspace = true",
            "io-core-model.workspace = true",
            "io-reward-policy.workspace = true",
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
            "real_sns_icrc2_direct_reserve_pull",
            "installed_stream_real_sns_icrc2_redemption",
            "real_sns_governance_staking_smoke",
            "real_canister_e2e_icp_to_io_stake_reward_redemption",
            "framework",
            "nns_setup",
            "sns_governance_setup",
            "sns_wasm_setup",
            "sns_root_setup",
            "sns_lifecycle",
            "brief_blockers",
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
            "create_empty_application_canister",
            "create_canister_on_subnet",
        ],
    )?;
    let nns_setup = require_file(root, "tests/e2e_real_canisters/src/nns_setup.rs")?;
    require_present(
        "tests/e2e_real_canisters/src/nns_setup.rs",
        &nns_setup,
        &[
            "install_minimal_nns_for_sns_w",
            "NNS_INSTALL_PLAN",
            "nns_lifeline",
            "InitPayloadDriverMissing",
            "real_nns_minimal_installer_rejects_missing_artifacts",
        ],
    )?;
    let sns_wasm_setup = require_file(root, "tests/e2e_real_canisters/src/sns_wasm_setup.rs")?;
    require_present(
        "tests/e2e_real_canisters/src/sns_wasm_setup.rs",
        &sns_wasm_setup,
        &[
            "SNS_WASM_PUBLICATION_PLAN",
            "add_all_sns_wasms_to_sns_w",
            "SnsWProposalDriverMissing",
            "real_sns_w_required_gate_fails_when_wasm_missing",
        ],
    )?;
    let sns_governance_setup =
        require_file(root, "tests/e2e_real_canisters/src/sns_governance_setup.rs")?;
    require_present(
        "tests/e2e_real_canisters/src/sns_governance_setup.rs",
        &sns_governance_setup,
        &[
            "Governance",
            "list_neurons",
            "list_proposals",
            "real_sns_governance_direct_empty_state_lists_no_neurons_or_proposals",
            "real_sns_user_stakes_io_normal_path_and_list_neurons_observes_it_direct_governance_path",
            "real_sns_user_topup_increases_existing_neuron_stake_direct_governance_path",
            "real_sns_minimum_stake_is_enforced_direct_governance_path",
            "real_sns_dissolve_delay_boundaries_are_visible_direct_governance_path",
        ],
    )?;
    let sns_root_setup = require_file(root, "tests/e2e_real_canisters/src/sns_root_setup.rs")?;
    require_present(
        "tests/e2e_real_canisters/src/sns_root_setup.rs",
        &sns_root_setup,
        &[
            "SnsRootCanister",
            "list_sns_canisters",
            "real_sns_root_control_uses_application_subnet_canister_direct_root_path",
        ],
    )?;
    let sns_lifecycle = require_file(root, "tests/e2e_real_canisters/src/sns_lifecycle.rs")?;
    require_present(
        "tests/e2e_real_canisters/src/sns_lifecycle.rs",
        &sns_lifecycle,
        &[
            "build_io_test_sns_init_payload",
            "deploy_io_test_sns_through_sns_w",
            "CreateServiceNervousSystemDtoMissing",
            "real_sns_lifecycle_deploys_sns_via_sns_w_is_blocked_on_sns_init_dto",
            "real_sns_dissolve_delay_above_two_weeks_cannot_be_applied_after_finalization",
        ],
    )?;
    let brief_blockers = require_file(root, "tests/e2e_real_canisters/src/brief_blockers.rs")?;
    require_present(
        "tests/e2e_real_canisters/src/brief_blockers.rs",
        &brief_blockers,
        &[
            "RealFrameworkBlocker",
            "historian_real_freshness_reports_stale_missing_incomplete_not_zero",
            "frontend_real_status_displays_not_live",
            "local_network_launches_with_nns_sns_features",
        ],
    )?;
    let framework = require_file(root, "tests/e2e_real_canisters/src/framework.rs")?;
    require_present(
        "tests/e2e_real_canisters/src/framework.rs",
        &framework,
        &[
            "FULL_FRAMEWORK_ARTIFACTS",
            "sns_wasm",
            "nns_governance",
            "run_full_framework_preflight",
            "create_empty_application_canister",
        ],
    )?;
    let ledger_index = require_file(root, "tests/e2e_real_canisters/src/sns_ledger_index.rs")?;
    require_present(
        "tests/e2e_real_canisters/src/sns_ledger_index.rs",
        &ledger_index,
        &[
            "create_sns_canister",
            "run_icrc2_direct_reserve_pull",
            "run_installed_stream_redemption",
        ],
    )?;
    require_absent(harness_path, &harness, &["--network ic", "dfx "])?;
    for path in [
        cargo_path,
        "tests/e2e_real_canisters/src/artifacts.rs",
        "tests/e2e_real_canisters/src/brief_blockers.rs",
        "tests/e2e_real_canisters/src/icrc.rs",
        "tests/e2e_real_canisters/src/pocketic_env.rs",
        "tests/e2e_real_canisters/src/framework.rs",
        "tests/e2e_real_canisters/src/nns_setup.rs",
        "tests/e2e_real_canisters/src/sns_governance_setup.rs",
        "tests/e2e_real_canisters/src/sns_ledger_index.rs",
        "tests/e2e_real_canisters/src/sns_lifecycle.rs",
        "tests/e2e_real_canisters/src/sns_root_setup.rs",
        "tests/e2e_real_canisters/src/sns_wasm_setup.rs",
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
    require_absent(
        manifest_path,
        &manifest,
        &[
            "--network ic",
            "dfx ",
            "oae4c-3iaaa-aaaar-qb5qq-cai",
            "6345890886899317159",
        ],
    )?;
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
    for artifact in ["sns_ledger", "sns_index"] {
        let file_name = artifact_manifest_value(&manifest, artifact, "filename")
            .or_else(|| artifact_manifest_value(&manifest, artifact, "wasm"))
            .ok_or_else(|| {
                format!(
                    "{}: missing artifacts.{artifact}.filename",
                    manifest_path.display()
                )
            })?;
        let expected = artifact_manifest_value(&manifest, artifact, "sha256").ok_or_else(|| {
            format!(
                "{}: missing artifacts.{artifact}.sha256",
                manifest_path.display()
            )
        })?;
        if expected.starts_with('<') {
            return Err(format!(
                "{}: artifacts.{artifact}.sha256 must be a pinned SHA-256, not a placeholder. Run tools/scripts/fetch-real-canister-artifacts after pinning source_url/source_sha256 to fill it.",
                manifest_path.display()
            ));
        }
        for source_field in ["source_url", "source_sha256", "source_kind"] {
            if artifact_manifest_value(&manifest, artifact, source_field)
                .filter(|value| !value.starts_with('<'))
                .is_none()
            {
                return Err(format!(
                    "{}: missing pinned artifacts.{artifact}.{source_field}",
                    manifest_path.display()
                ));
            }
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

fn artifact_manifest_value(text: &str, artifact: &str, field: &str) -> Option<String> {
    let legacy_key = if field == "filename" {
        format!("{artifact}_wasm")
    } else {
        format!("{artifact}_{field}")
    };
    let nested_section = format!("artifacts.{artifact}");
    let mut section = String::new();
    text.lines().find_map(|raw| {
        let line = raw.split_once('#').map_or(raw, |(prefix, _)| prefix).trim();
        if line.is_empty() {
            return None;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = line
                .trim_start_matches('[')
                .trim_end_matches(']')
                .trim()
                .to_string();
            return None;
        }
        let (left, right) = line.split_once('=')?;
        let left = left.trim();
        if !((section == nested_section && left == field)
            || ((section.is_empty() || section == "artifacts") && left == legacy_key))
        {
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
            "Mock and PocketIC tests exercise bounded failures, retry and upgrade behavior",
            "Required CI uses SNS-shaped mock/PocketIC tests.",
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
            "max_dissolve_delay_seconds: 1_209_600",
            "max_dissolve_delay_bonus_percentage: 0",
            "max_neuron_age_for_age_bonus: 0",
            "max_age_bonus_percentage: 0",
            "neuron_minimum_dissolve_delay_to_vote_seconds: 1_209_599",
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
            "source-built sns",
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
    let frontend_agent = require_file(root, "canisters/frontend/web/src/app/agent.js")?;
    require_present(
        "canisters/frontend/web/src/app/agent.js",
        &frontend_agent,
        &[
            "io_historian/io_historian.did.js",
            "io_stream_manager/io_stream_manager.did.js",
            "io_ledger/io_ledger.did.js",
            "createRedemptionActors",
        ],
    )?;
    require_absent(
        "canisters/frontend/web/src/app/agent.js",
        &frontend_agent,
        &[
            ".dfx",
            "src/declarations",
            "io_historian_debug",
            "io_nns_neuron_manager",
            "debug_",
        ],
    )?;
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
                    || fixture.ends_with("empty-default.fixture")
                {
                    if schema_version != entry.current_version {
                        return Err(format!(
                            "{fixture}: schema_version {schema_version} must match registry current {}",
                            entry.current_version
                        ));
                    }
                } else if fixture.ends_with("future-version.fixture") {
                    if schema_version <= entry.current_version {
                        return Err(format!(
                            "{fixture}: future fixture schema_version {schema_version} must be greater than registry current {}",
                            entry.current_version
                        ));
                    }
                } else if !accepts_schema_version(entry, schema_version) {
                    return Err(format!(
                        "{fixture}: schema_version {schema_version} is not declared as current or previous supported for {}",
                        entry.canister_name
                    ));
                }
            }
        }
        if matches!(
            entry.canister_name,
            "io_stream_manager" | "io_nns_neuron_manager"
        ) && (entry.current_version != 1
            || !entry.previous_supported_versions.is_empty()
            || !entry.migration_paths.is_empty()
            || entry.fixture_files.len() != 1
            || !entry.fixture_files[0].ends_with("launch-v1.fixture"))
        {
            return Err(format!(
                "{} must register launch V1 only",
                entry.canister_name
            ));
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
            "Install always writes Paused",
            "No stream/NNS V0",
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

fn check_exact_two_week_policy_at(root: &Path) -> Result<(), String> {
    let reward_policy = require_file(root, "crates/io_reward_policy/src/lib.rs")?;
    require_present(
        "exact two-week reward policy",
        &reward_policy,
        &[
            "allocations_plus_all_dust_equal_backed_pool",
            "forfeited_destination_share_becomes_dust_not_redistribution",
            "no_closed_proposals_has_full_participation",
        ],
    )?;
    let stream_state = require_file(root, "canisters/io_stream_manager/src/state.rs")?;
    require_present(
        "stream-manager fixed reward cohort slots",
        &stream_state,
        &["active_reward_cohort", "pending_reward_cohort"],
    )?;
    let rewards = require_file(root, "canisters/io_stream_manager/src/rewards.rs")?;
    require_present(
        "stream-manager canonical reward policy reuse",
        &rewards,
        &["pub use io_reward_policy::*"],
    )?;
    Ok(())
}

fn fixture_schema_version(fixture: &str, text: &str) -> Result<u32, String> {
    let raw = text
        .lines()
        .find_map(|line| line.strip_prefix("schema_version="))
        .ok_or_else(|| format!("{fixture}: missing schema_version line"))?;
    raw.parse::<u32>()
        .map_err(|err| format!("{fixture}: invalid schema_version {raw}: {err}"))
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
    eprintln!("known: test_all, test_ci, verify_release, simplicity_check, security_scan, security_scan_required, validate_install_args, validate_prelaunch_public_shell, validate_production_wiring, validate_historian_freshness, validate_stable_storage, validate_local_sns_rehearsal, validate_local_sns_ledger, validate_local_sns_committed_evidence, validate_local_sns_scripts, e2e_coverage_matrix_check, live_stream_manager_pocketic_gate_check, real_canister_harness_check, real_canister_artifact_manifest_check, verify_real_canister_artifacts, fetch_real_canister_artifacts, real_sns_ledger_index_tests, real_sns_ledger_index_required, real_sns_governance_tests, real_sns_governance_required, real_io_e2e_tests, real_io_e2e_required, e2e_real_coverage_check, local_sns_evidence_tests, sns_apy_policy_tests, frontend_setup, frontend_build, frontend_unit, frontend_certified_asset_tests, frontend_required, frontend_all, historian_tests, historian_required, sns_harness_check, sns_config_validate, sns_config_validate_official, sns_official_testing_check, sns_launch_readiness_check, sns_governance_read_tests, sns_governance_read_required, sns_ledger_index_tests, sns_ledger_index_required, sns_root_lifecycle_tests, sns_root_lifecycle_required, sns_pocketic_smoke, sns_pocketic_required, test_pocketic_required, preflight, check, fmt_check, did_surface, build_canisters, verify_artifacts, build_debug_canisters, test_unit, test_pocketic_integration, test_local_integration, test_e2e, stream_manager_unit, nns_neuron_manager_unit, historian_pocketic_integration, stream_manager_pocketic_integration, nns_neuron_manager_pocketic_integration");
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
        "simplicity_check" => match check_simplicity_at(&root) {
            Ok(()) => eprintln!("✓ simplicity_check"),
            Err(err) => {
                eprintln!("✗ simplicity_check: {err}");
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
            if env::var_os("IO_RUN_SOURCE_BUILT_SNS_VALIDATE").is_none() {
                eprintln!(
                    "skipping sns_config_validate_official: set IO_RUN_SOURCE_BUILT_SNS_VALIDATE=1 to run optional source-built sns validation"
                );
            } else if Command::new("sns").arg("--help").status().is_err() {
                eprintln!(
                    "skipping sns_config_validate_official: source-built sns CLI is unavailable"
                );
            } else {
                let mut c = Command::new("sns");
                c.args([
                    "init-config-file",
                    "--init-config-file-path",
                    "tools/sns/sns_init.io.local.yaml",
                    "validate",
                ]);
                ok &= run("optional source-built sns init-config-file validate", c);
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
        "validate_local_sns_committed_evidence" => {
            match check_local_sns_committed_evidence_at(&root) {
                Ok(()) => eprintln!("✓ validate_local_sns_committed_evidence"),
                Err(err) => {
                    eprintln!("✗ validate_local_sns_committed_evidence: {err}");
                    ok = false;
                }
            }
        }
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
        "live_stream_manager_pocketic_gate_check" => {
            match check_live_stream_manager_pocketic_gate_at(&root) {
                Ok(()) => eprintln!("✓ live_stream_manager_pocketic_gate_check"),
                Err(err) => {
                    eprintln!("✗ live_stream_manager_pocketic_gate_check: {err}");
                    ok = false;
                }
            }
        }
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
        "verify_real_canister_artifacts" => {
            match check_real_canister_artifact_manifest_at(&root, false) {
                Ok(true) => eprintln!("✓ verify_real_canister_artifacts"),
                Ok(false) => eprintln!("skipping verify_real_canister_artifacts: real Wasm artifacts are not configured"),
                Err(err) => {
                    eprintln!("✗ verify_real_canister_artifacts: {err}");
                    ok = false;
                }
            }
        }
        "fetch_real_canister_artifacts" => {
            ok &= run(
                "fetch_real_canister_artifacts",
                script("tools/scripts/fetch-real-canister-artifacts", &[]),
            );
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
                "unit: e2e-real-canisters governance real-test registration",
                cargo_test(&[
                    "-p",
                    "e2e-real-canisters",
                    "real_sns_governance_staking_smoke",
                ]),
            );
            eprintln!(
                "skipping real_sns_governance_tests ignored layer unless real artifacts and POCKET_IC_BIN are supplied"
            );
        }
        "real_sns_governance_required" => {
            match check_real_canister_artifact_manifest_at(&root, true) {
                Ok(true) => {}
                Ok(false) => {
                    eprintln!(
                        "✗ real_sns_governance_required: real Wasm artifacts are not configured"
                    );
                    ok = false;
                }
                Err(err) => {
                    eprintln!("✗ real_sns_governance_required: {err}");
                    ok = false;
                }
            }
            if env::var_os("POCKET_IC_BIN").is_none() {
                eprintln!("✗ real_sns_governance_required: POCKET_IC_BIN is not set");
                ok = false;
            }
            if ok {
                ok &= run(
                    "real-framework: direct real SNS governance staking/top-up/minimum-stake",
                    cargo_test(&[
                        "-p",
                        "e2e-real-canisters",
                        "real_sns_governance_staking_smoke",
                        "--",
                        "--ignored",
                        "--nocapture",
                    ]),
                );
                ok &= run(
                    "real-framework: finalized SNS above-two-week delay cannot be applied",
                    cargo_test(&[
                        "-p",
                        "e2e-real-canisters",
                        "real_sns_dissolve_delay_above_two_weeks_cannot_be_applied_after_finalization",
                        "--",
                        "--ignored",
                        "--nocapture",
                    ]),
                );
            }
        }
        "real_io_e2e_tests" => {
            ok &= run_subcommand("real_canister_harness_check");
            ok &= run(
                "unit: e2e-real-canisters full E2E registration",
                cargo_test(&[
                    "-p",
                    "e2e-real-canisters",
                    "real_canister_e2e_icp_to_io_stake_reward_redemption",
                ]),
            );
            match check_real_canister_artifact_manifest_at(&root, false) {
                Ok(true) => {
                    if env::var_os("POCKET_IC_BIN").is_none() {
                        eprintln!(
                            "✗ real_io_e2e_tests: artifacts are configured but POCKET_IC_BIN is not set"
                        );
                        ok = false;
                    } else {
                        ok &= run(
                            "real-ledger: exact Jupiter/hold/stake/redeem economics E2E",
                            cargo_test(&[
                                "-p",
                                "e2e-real-canisters",
                                "real_canister_e2e_icp_to_io_stake_reward_redemption",
                                "--",
                                "--ignored",
                                "--nocapture",
                            ]),
                        );
                        ok &= run(
                            "real-stack: strict finalized-SNS four-role reward reconciliation",
                            cargo_test(&[
                                "-p",
                                "e2e-real-canisters",
                                "real_finalized_sns_four_role_reward_reconciles_exactly_once",
                                "--",
                                "--ignored",
                                "--nocapture",
                            ]),
                        );
                        ok &= run(
                            "real-stack: finalized-SNS two-cohort frozen reward proof",
                            cargo_test(&[
                                "-p",
                                "e2e-real-canisters",
                                "real_finalized_sns_frozen_cohort_blocks_late_stake_and_duration_bonus",
                                "--",
                                "--ignored",
                                "--nocapture",
                            ]),
                        );
                        ok &= run(
                            "real-stack: finalized-SNS zero-recipient reward dust retention",
                            cargo_test(&[
                                "-p",
                                "e2e-real-canisters",
                                "real_finalized_sns_zero_recipient_reward_retains_full_pool_as_dust",
                                "--",
                                "--ignored",
                                "--nocapture",
                            ]),
                        );
                        ok &= run(
                            "real-stack: rejected refund TooOld waits for index proof without double refund",
                            cargo_test(&[
                                "-p",
                                "e2e-real-canisters",
                                "real_stack_rejected_refund_too_old_waits_for_index_proof_no_double_refund",
                                "--",
                                "--ignored",
                                "--nocapture",
                            ]),
                        );
                    }
                }
                Ok(false) => eprintln!(
                    "skipping real_io_e2e_tests ignored layer: real Wasm artifacts are not configured"
                ),
                Err(err) => {
                    eprintln!("✗ real_io_e2e_tests: {err}");
                    ok = false;
                }
            }
        }
        "real_io_e2e_required" => {
            match check_real_canister_artifact_manifest_at(&root, true) {
                Ok(true) => {}
                Ok(false) => {
                    eprintln!("✗ real_io_e2e_required: real Wasm artifacts are not configured");
                    ok = false;
                }
                Err(err) => {
                    eprintln!("✗ real_io_e2e_required: {err}");
                    ok = false;
                }
            }
            if env::var_os("POCKET_IC_BIN").is_none() {
                eprintln!("✗ real_io_e2e_required: POCKET_IC_BIN is not set");
                ok = false;
            }
            if ok {
                ok &= run_subcommand("real_io_e2e_tests");
            }
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
                "validate_local_sns_committed_evidence",
                "validate_local_sns_scripts",
                "e2e_coverage_matrix_check",
                "live_stream_manager_pocketic_gate_check",
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
            ok &= run_subcommand("live_stream_manager_pocketic_gate_check");
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
                "local-cli: io-stream-manager API contract",
                cargo_test(&["-p", "io-stream-manager", "--lib"]),
            );
            ok &= run(
                "local-cli: io-nns-neuron-manager API contract",
                cargo_test(&["-p", "io-nns-neuron-manager", "--lib"]),
            );
        }
        "test_e2e" => {
            ok &= run(
                "e2e: simplified stream boundary",
                cargo_test(&[
                    "-p",
                    "io-stream-manager",
                    "--test",
                    "io_stream_manager_pocketic",
                ]),
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
                "validate_local_sns_committed_evidence",
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

    #[test]
    fn simplified_receipt_topology_requires_shared_account_tokens() {
        let stream = "jupiter_receipt_source = record { subaccount = null }\n\
                      two_week_receipt_source = TODO_TWO_WEEK_STAGING\n\
                      liquid_icp = TODO_STREAM_LIQUID_SUBACCOUNT";
        let nns = "jupiter_staging = record { subaccount = null }\n\
                   two_week_maturity_staging = TODO_TWO_WEEK_STAGING\n\
                   stream_liquid_account = TODO_STREAM_LIQUID_SUBACCOUNT";
        validate_simplified_receipt_topology(stream, nns).unwrap();
        assert!(validate_simplified_receipt_topology(
            &stream.replace("subaccount = null", "subaccount = opt TODO_WRONG",),
            nns,
        )
        .is_err());
    }

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

    fn write_evidence_sha256s(root: &Path, package: &str, files: &[&str]) {
        let mut lines = String::new();
        for file in files {
            let bytes = fs::read(root.join(package).join(file)).unwrap();
            lines.push_str(&format!("{}  {file}\n", hex_sha256(&bytes)));
        }
        write(root, &format!("{package}/SHA256SUMS"), &lines);
    }

    fn write_incomplete_evidence_package(root: &Path) -> String {
        let package = "deploy/local-sns-rehearsal/evidence/2026-07-29-0123456".to_string();
        write(
            root,
            &format!("{package}/manifest.toml"),
            "[provenance]\nofficial_ic_repository = \"dfinity/ic\"\nofficial_ic_source_commit = \"0123456789abcdef0123456789abcdef01234567\"\nsns_testing_source_path = \"rs/sns/testing\"\ncomplete = false\nblocker_report = \"blocker-report.md\"\n",
        );
        write(
            root,
            &format!("{package}/blocker-report.md"),
            "# Blocker\n\nThe official local SNS rehearsal not completed.\n\nsource-built SNS tools were not prepared.\n\nNo mainnet call was made.\n",
        );
        write_evidence_sha256s(root, &package, &["manifest.toml", "blocker-report.md"]);
        package
    }

    fn write_completed_evidence_package(root: &Path) -> String {
        let package = "deploy/local-sns-rehearsal/evidence/2026-07-29-fedcba9".to_string();
        let manifest = "[provenance]\nofficial_ic_repository = \"dfinity/ic\"\nofficial_ic_source_commit = \"fedcba98765432100123456789abcdef01234567\"\nsns_testing_source_path = \"rs/sns/testing\"\ncomplete = true\n";
        write(root, &format!("{package}/manifest.toml"), manifest);
        write(
            root,
            &format!("{package}/toolchain-provenance.toml"),
            "[tools]\nbazelisk_version = \"1.26.0\"\nbazelisk_sha256 = \"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\"\nsns_version = \"source-2d7f90f\"\nsns_sha256 = \"1123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\"\n",
        );
        write(
            root,
            &format!("{package}/sns_init.local.yaml"),
            "Swap:\n  start_time: null\n",
        );
        for file in [
            "canister-ids.local.toml",
            "reserve-funding-evidence.toml",
            "ledger-evidence.toml",
            "governance-evidence.toml",
            "controller-evidence.toml",
            "archive-evidence.toml",
        ] {
            write(
                root,
                &format!("{package}/{file}"),
                "[evidence]\nobserved = true\n",
            );
        }
        write(
            root,
            &format!("{package}/commands.log"),
            "command: local-proof\nexit_status=0\n",
        );
        write_evidence_sha256s(
            root,
            &package,
            &[
                "manifest.toml",
                "toolchain-provenance.toml",
                "sns_init.local.yaml",
                "canister-ids.local.toml",
                "reserve-funding-evidence.toml",
                "ledger-evidence.toml",
                "governance-evidence.toml",
                "controller-evidence.toml",
                "archive-evidence.toml",
                "commands.log",
            ],
        );
        package
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

    fn write_artifact_manifest(root: &Path, manifest: &ArtifactManifest) {
        let text = serde_json::to_string_pretty(manifest).unwrap();
        write(root, MANIFEST_PATH, &format!("{text}\n"));
    }

    fn read_artifact_manifest(root: &Path) -> ArtifactManifest {
        read_manifest(root).unwrap()
    }

    fn parent_git_commit() -> String {
        let output = Command::new("git")
            .args(["rev-parse", "HEAD^"])
            .output()
            .unwrap();
        assert!(output.status.success());
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    fn create_unreachable_commit() -> String {
        let tree = Command::new("git")
            .args(["mktree"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .and_then(|mut child| {
                child.stdin.take().unwrap();
                child.wait_with_output()
            })
            .unwrap();
        assert!(tree.status.success());
        let tree = String::from_utf8_lossy(&tree.stdout).trim().to_string();

        let commit = Command::new("git")
            .args(["commit-tree", &tree, "-m", "xtask non-ancestor test commit"])
            .env("GIT_AUTHOR_NAME", "IO xtask test")
            .env("GIT_AUTHOR_EMAIL", "io-xtask@example.invalid")
            .env("GIT_COMMITTER_NAME", "IO xtask test")
            .env("GIT_COMMITTER_EMAIL", "io-xtask@example.invalid")
            .output()
            .unwrap();
        assert!(commit.status.success());
        String::from_utf8_lossy(&commit.stdout).trim().to_string()
    }

    fn write_sns_harness_fixture(root: &Path) {
        write(
            root,
            "docs/operations/local-sns-testing.md",
            r#"# Local SNS Testing
Required CI uses SNS-shaped mock/PocketIC tests.
Pure model tests remain the main accounting guardrail.
Mock and PocketIC tests exercise bounded failures, retry and upgrade behavior.
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
  max_dissolve_delay_seconds: 1_209_600
  max_dissolve_delay_bonus_percentage: 0
  max_neuron_age_for_age_bonus: 0
  max_age_bonus_percentage: 0
  neuron_minimum_dissolve_delay_to_vote_seconds: 1_209_599
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
            "We currently run SNS-shaped mock/PocketIC tests.\nWe do not currently run the official SNS launch locally in required CI.\nOfficial SNS testing is optional and heavier.\nThe current official ICP/DFINITY SNS testing documentation is the source of truth.\nThe historical standalone `dfinity/sns-testing` repository is deprecated.\nThe maintained official local SNS flow uses the source-built `sns` CLI; this is not part of required IO workflows.\nSNS testflight is a future manual/mainnet rehearsal.\nIO's canonical IO ledger should be the SNS ledger; any IO_TEST ledger is non-canonical.\nThe existing canister that owns IO NNS neuron 6345890886899317159 is not touched by these tests.\nLayer 1\nLayer 2\nLayer 3\nLayer 4\n",
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
            "optional local-only not part of `test_ci` not used by `verify_release` must not call mainnet source-built sns Do not use --network ic\n",
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
            "local-only real SNS-created IO ledger/index/governance/root stack not final tokenomics not a mainnet SNS proposal not required CI Do not use `--network ic` protocol reserve reserve-to-user transfer user-to-redemption transfer redemption-to-reserve transfer validate_local_sns_rehearsal validate_local_sns_ledger validate_local_sns_scripts Human-readable local evidence-derived wiring Not accepted by production wiring validators Do not use as install args\n",
        );
        write(
            root,
            "deploy/local-sns-rehearsal/sns_init.local.template.yaml",
            "Local-only\nNot final tokenomics\nNot a mainnet SNS proposal\nfallback_controller_principals\n{{fallback_controller_principal}}\ndapp_canisters\nToken:\nsymbol: \"IO\"\ntransaction_fee\nDistribution:\ntreasury: \"800_000 tokens\"\nswap: \"100_000 tokens\"\nSwap:\n  start_time: null\nNnsProposal:\nTODO_LOCAL\n",
        );
        write(
            root,
            "deploy/local-sns-rehearsal/local-vars.example.toml",
            &fixture_local_vars(
                "avqkn-guaaa-aaaaa-qaaea-cai",
                "aax3a-h4aaa-aaaaa-qaahq-cai",
                "ajuq4-ruaaa-aaaaa-qaaga-cai",
                "b77ix-eeaaa-aaaaa-qaada-cai",
            ),
        );
        write(
            root,
            "deploy/local-sns-rehearsal/assets/io-local-logo.svg",
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 512 512\" role=\"img\" aria-label=\"IO local rehearsal\">\n  <rect width=\"512\" height=\"512\" rx=\"96\" fill=\"#111827\"/>\n  <circle cx=\"256\" cy=\"256\" r=\"154\" fill=\"none\" stroke=\"#22d3ee\" stroke-width=\"36\"/>\n  <path d=\"M192 160v192M304 160c64 0 64 192 0 192s-64-192 0-192Z\" fill=\"none\" stroke=\"#f8fafc\" stroke-linecap=\"round\" stroke-width=\"32\"/>\n</svg>\n",
        );
        write(
            root,
            "deploy/local-sns-rehearsal/assets/io-local-token-logo.svg",
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 512 512\" role=\"img\" aria-label=\"IO local rehearsal token\">\n  <circle cx=\"256\" cy=\"256\" r=\"240\" fill=\"#0f172a\"/>\n  <circle cx=\"256\" cy=\"256\" r=\"180\" fill=\"#0891b2\"/>\n  <path d=\"M180 150v212M302 150c76 0 76 212 0 212s-76-212 0-212Z\" fill=\"none\" stroke=\"#fff\" stroke-linecap=\"round\" stroke-width=\"36\"/>\n</svg>\n",
        );
        write(
            root,
            "deploy/local-sns-rehearsal/canister-ids.local.example.toml",
            "network = \"local\"\nsource = \"official-local-sns-rehearsal\"\nofficial_tooling = \"manual-local-only\"\n[toolchain_provenance]\nofficial_ic_source_commit = \"0123456789abcdef0123456789abcdef01234567\"\nsns_testing_source_path = \"rs/sns/testing\"\noperator_identity_principal = \"bd3sg-teaaa-aaaaa-qaaba-cai\"\nlocal_network_url = \"http://127.0.0.1:8080\"\nsns_cli_sha256 = \"TODO\"\nsns_testing_init_sha256 = \"TODO\"\nsns_testing_cli_sha256 = \"TODO\"\n[sns_canisters]\nroot = \"TODO\"\ngovernance = \"TODO\"\nledger = \"TODO\"\nindex = \"TODO\"\nswap = \"TODO\"\narchive = \"TODO\"\n[expected_local_sns_config]\ntoken_symbol = \"IO\"\ntransaction_fee_e8s = 10_000\ntotal_supply_e8s = 1\nprotocol_reserve_funding_amount_e8s = 1\n[ledger_evidence]\ntransaction_fee_e8s = 10_000\ntotal_supply_e8s = 1\nprotocol_reserve_balance_e8s = 1\nreserve_transfer_amount_e8s = 1\nredemption_return_amount_e8s = 1\nbad_fee_error_observed = true\ninsufficient_funds_error_observed = true\nduplicate_tested_transfer = \"transfer_reserve_to_user\"\nindex_account_history_observed = true\n[reserve_funding_transfer]\nsns_proposal_id = 1\nproposal_adopted = true\nproposal_executed = true\ncreated_at_time_nanos = \"none\"\nmemo_hex = \"none\"\nproof_source = \"SnsLedgerBlock\"\nproof_source_canister = \"TODO\"\nproof_method = \"Icrc3GetBlocks\"\narchive_canister = \"none\"\n[transfer_reserve_to_user]\nfrom_owner = \"TODO\"\nfrom_subaccount_hex = \"none\"\nto_owner = \"TODO\"\nto_subaccount_hex = \"none\"\nfee_disposition = \"burned\"\nsender_balance_before_e8s = 1\nrecipient_balance_after_e8s = 1\ntotal_supply_before_e8s = 1\ntotal_supply_after_e8s = 1\nproof_source = \"SnsIndexAccountHistory\"\nproof_source_canister = \"TODO\"\nproof_method = \"IcrcIndexGetAccountTransactions\"\narchive_canister = \"none\"\n[transfer_user_to_redemption]\nfrom_owner = \"TODO\"\nfrom_subaccount_hex = \"none\"\nto_owner = \"TODO\"\nto_subaccount_hex = \"none\"\nfee_disposition = \"burned\"\nsender_balance_before_e8s = 1\nrecipient_balance_after_e8s = 1\ntotal_supply_before_e8s = 1\ntotal_supply_after_e8s = 1\nproof_source = \"SnsIndexAccountHistory\"\nproof_source_canister = \"TODO\"\nproof_method = \"IcrcIndexGetAccountTransactions\"\narchive_canister = \"none\"\n[transfer_redemption_to_reserve]\nfrom_owner = \"TODO\"\nfrom_subaccount_hex = \"none\"\nto_owner = \"TODO\"\nto_subaccount_hex = \"none\"\nfee_disposition = \"burned\"\nsender_balance_before_e8s = 1\nrecipient_balance_after_e8s = 1\ntotal_supply_before_e8s = 1\ntotal_supply_after_e8s = 1\nproof_source = \"SnsIndexAccountHistory\"\nproof_source_canister = \"TODO\"\nproof_method = \"IcrcIndexGetAccountTransactions\"\narchive_canister = \"none\"\n[duplicate_test]\n[issuance_model]\nresolved_as = \"protocol_reserve_transfer\"\nminting_assumed = false\ntreasury_transfer_assumed = true\nfee_disposition_mode = \"burned\"\ntotal_supply_changes_explained = true\n",
        );
        for path in [
            "deploy/local-sns-rehearsal/runbook.sh",
            "deploy/local-sns-rehearsal/scripts/lib-local-sns.sh",
            "deploy/local-sns-rehearsal/scripts/00-check-prereqs.sh",
            "deploy/local-sns-rehearsal/scripts/01-render-sns-init.sh",
            "deploy/local-sns-rehearsal/scripts/02-record-canister-ids.sh",
            "deploy/local-sns-rehearsal/scripts/03-capture-ledger-evidence.sh",
            "deploy/local-sns-rehearsal/scripts/04-render-local-wiring.sh",
            "deploy/local-sns-rehearsal/scripts/05-validate-evidence.sh",
            "deploy/local-sns-rehearsal/scripts/10-bootstrap-official-network.sh",
            "deploy/local-sns-rehearsal/scripts/11-build-local-io-canisters.sh",
            "deploy/local-sns-rehearsal/scripts/12-deploy-local-dapps.sh",
            "deploy/local-sns-rehearsal/scripts/13-propose-and-finalize-sns.sh",
            "deploy/local-sns-rehearsal/scripts/14-discover-sns-canisters.sh",
            "deploy/local-sns-rehearsal/scripts/15-exercise-ledger.sh",
            "deploy/local-sns-rehearsal/scripts/16-exercise-index-and-archives.sh",
            "deploy/local-sns-rehearsal/scripts/17-exercise-governance-and-controllers.sh",
            "deploy/local-sns-rehearsal/scripts/18-package-evidence.sh",
            "deploy/local-sns-rehearsal/scripts/19-cleanup-official-network.sh",
        ] {
            write(
                root,
                path,
                "#!/usr/bin/env bash\n# local-only optional\n# Requires IO_LOCAL_SNS_REHEARSAL_ACK=local-only.\nrequire_local_script_guard \"$@\"\n: \"${IO_LOCAL_SNS_REHEARSAL_ACK:?local-only}\"\n# . scripts/env.sh //rs/sns/testing:sns-testing-init //rs/sns/testing:sns-testing //rs/sns/cli:sns sns init-config-file --init-config-file-path\n",
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
        crate::completed_local_sns_evidence()
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
            "type InitArgs = record {};\nservice : (InitArgs) -> {\n  redeem : () -> ();\n  prepare_liquid_receipt : () -> ();\n  complete_liquid_receipt : () -> ();\n  resume : () -> ();\n  prove_active_transfer : () -> ();\n  set_paused : () -> ();\n  get_status : () -> () query;\n}\n",
        );
        write(
            root,
            "canisters/io_nns_neuron_manager/io_nns_neuron_manager.did",
            "type InitArgs = record {};\nservice : (InitArgs) -> {\n  notify_jupiter_deposit : () -> ();\n  set_two_week_target : () -> ();\n  resume : () -> ();\n  prove_active_transfer : () -> ();\n  set_paused : () -> ();\n  get_status : () -> () query;\n}\n",
        );
        write(
            root,
            "canisters/io_historian/io_historian.did",
            "service : {\n  get_dashboard_state : () -> (text) query;\n  get_protocol_snapshot : () -> (text) query;\n  get_redemption_rate : () -> (text) query;\n  list_streams : () -> (text) query;\n  list_redemptions : () -> (text) query;\n  list_rewards : () -> (text) query;\n  list_nns_lifecycle_events : () -> (text) query;\n  get_index_health : () -> (text) query;\n  get_governance_summary : () -> (text) query;\n  get_release_artifacts : () -> (text) query;\n  get_canister_status_summary : () -> (text) query;\n  get_public_status : () -> (text) query;\n  get_reserve_snapshot : () -> (text) query;\n  list_governance_participation : () -> (text) query;\n  version : () -> (text) query;\n}\n",
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
    fn artifact_manifest_accepts_reachable_ancestor_source_commit() {
        let root = temp_root("manifest-ancestor-source");
        write_artifact_set(&root);
        let source_commit = parent_git_commit();
        let manifest = build_manifest_for_commit(&root, source_commit).unwrap();
        write_artifact_manifest(&root, &manifest);
        verify_artifacts_at(&root).unwrap();
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn artifact_manifest_rejects_unresolved_source_commit() {
        let root = temp_root("manifest-unresolved-source");
        write_artifact_set(&root);
        let mut manifest = read_artifact_manifest(&root);
        let missing = "0123456789abcdef0123456789abcdef01234567".to_string();
        manifest.git_commit = Some(missing.clone());
        for entry in &mut manifest.artifacts {
            entry.git_commit = Some(missing.clone());
        }
        write_artifact_manifest(&root, &manifest);
        assert!(verify_artifacts_at(&root)
            .unwrap_err()
            .contains("does not resolve locally as a commit"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn artifact_manifest_rejects_non_ancestor_source_commit() {
        let root = temp_root("manifest-non-ancestor-source");
        write_artifact_set(&root);
        let non_ancestor = create_unreachable_commit();
        let mut manifest = read_artifact_manifest(&root);
        manifest.git_commit = Some(non_ancestor.clone());
        for entry in &mut manifest.artifacts {
            entry.git_commit = Some(non_ancestor.clone());
        }
        write_artifact_manifest(&root, &manifest);
        assert!(verify_artifacts_at(&root)
            .unwrap_err()
            .contains("is not an ancestor of HEAD"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn artifact_manifest_rejects_mixed_per_artifact_source_commits() {
        let root = temp_root("manifest-mixed-source");
        write_artifact_set(&root);
        let mut manifest = read_artifact_manifest(&root);
        manifest.artifacts[0].git_commit =
            Some("0123456789abcdef0123456789abcdef01234567".to_string());
        write_artifact_manifest(&root, &manifest);
        assert!(verify_artifacts_at(&root)
            .unwrap_err()
            .contains("must equal top-level git_commit"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn artifact_manifest_still_rejects_wrong_hash_or_size() {
        let root = temp_root("manifest-wrong-hash-or-size");
        write_artifact_set(&root);
        let mut manifest = read_artifact_manifest(&root);
        manifest.artifacts[0].raw_wasm_sha256 =
            "0000000000000000000000000000000000000000000000000000000000000000".to_string();
        write_artifact_manifest(&root, &manifest);
        assert!(verify_artifacts_at(&root)
            .unwrap_err()
            .contains("manifest does not match current artifacts"));

        let mut manifest = read_artifact_manifest(&root);
        manifest.artifacts[0].raw_wasm_sha256 =
            sha256_hex(&root.join(&manifest.artifacts[0].raw_wasm_path)).unwrap();
        manifest.artifacts[0].raw_wasm_bytes += 1;
        write_artifact_manifest(&root, &manifest);
        assert!(verify_artifacts_at(&root)
            .unwrap_err()
            .contains("manifest does not match current artifacts"));
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
    fn local_sns_rehearsal_rejects_non_null_local_start_time() {
        let root = temp_root("local-sns-rehearsal-start-time");
        write_local_sns_rehearsal_fixture(&root);
        let path = root.join("deploy/local-sns-rehearsal/sns_init.local.template.yaml");
        let text = fs::read_to_string(&path)
            .unwrap()
            .replace("start_time: null", "start_time: \"2026-07-29 12:00:00Z\"");
        fs::write(path, text).unwrap();
        assert!(check_local_sns_rehearsal_at(&root)
            .unwrap_err()
            .contains("Swap.start_time"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn local_sns_rehearsal_rejects_empty_restricted_country_list() {
        let root = temp_root("local-sns-rehearsal-empty-countries");
        write_local_sns_rehearsal_fixture(&root);
        let path = root.join("deploy/local-sns-rehearsal/sns_init.local.template.yaml");
        let text = fs::read_to_string(&path).unwrap().replace(
            "  start_time: null",
            "  start_time: null\n  restricted_countries: []",
        );
        fs::write(path, text).unwrap();
        assert!(check_local_sns_rehearsal_at(&root)
            .unwrap_err()
            .contains("restricted_countries"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn local_sns_rehearsal_rejects_logo_hash_mismatch() {
        let root = temp_root("local-sns-rehearsal-logo-hash");
        write_local_sns_rehearsal_fixture(&root);
        write(
            &root,
            "deploy/local-sns-rehearsal/assets/io-local-logo.svg",
            "<svg>changed</svg>\n",
        );
        assert!(check_local_sns_rehearsal_at(&root)
            .unwrap_err()
            .contains("SHA-256"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn local_sns_committed_evidence_accepts_exact_incomplete_inventory() {
        let root = temp_root("local-sns-evidence-incomplete");
        write_incomplete_evidence_package(&root);
        check_local_sns_committed_evidence_at(&root).unwrap();
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn local_sns_committed_evidence_accepts_exact_completed_inventory() {
        let root = temp_root("local-sns-evidence-completed");
        write_completed_evidence_package(&root);
        check_local_sns_committed_evidence_at(&root).unwrap();
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn local_sns_committed_evidence_rejects_duplicate_checksum_entry() {
        let root = temp_root("local-sns-evidence-duplicate-sha");
        let package = write_incomplete_evidence_package(&root);
        let sha_path = root.join(&package).join("SHA256SUMS");
        let first = fs::read_to_string(&sha_path)
            .unwrap()
            .lines()
            .next()
            .unwrap()
            .to_string();
        let mut text = fs::read_to_string(&sha_path).unwrap();
        text.push_str(&format!("{first}\n"));
        fs::write(sha_path, text).unwrap();
        assert!(check_local_sns_committed_evidence_at(&root)
            .unwrap_err()
            .contains("duplicate SHA256SUMS"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn local_sns_committed_evidence_rejects_unexpected_file() {
        let root = temp_root("local-sns-evidence-unexpected");
        let package = write_incomplete_evidence_package(&root);
        write(&root, &format!("{package}/extra.txt"), "extra\n");
        assert!(check_local_sns_committed_evidence_at(&root)
            .unwrap_err()
            .contains("inventory mismatch"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn local_sns_committed_evidence_rejects_completed_placeholder_version() {
        let root = temp_root("local-sns-evidence-version-placeholder");
        let package = write_completed_evidence_package(&root);
        let path = root.join(&package).join("toolchain-provenance.toml");
        let text = fs::read_to_string(&path)
            .unwrap()
            .replace("1.26.0", "not-installed");
        fs::write(&path, text).unwrap();
        let files = [
            "manifest.toml",
            "toolchain-provenance.toml",
            "sns_init.local.yaml",
            "canister-ids.local.toml",
            "reserve-funding-evidence.toml",
            "ledger-evidence.toml",
            "governance-evidence.toml",
            "controller-evidence.toml",
            "archive-evidence.toml",
            "commands.log",
        ];
        write_evidence_sha256s(&root, &package, &files);
        assert!(check_local_sns_committed_evidence_at(&root)
            .unwrap_err()
            .contains("placeholder marker"));
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn local_sns_committed_evidence_rejects_symlink() {
        use std::os::unix::fs::symlink;

        let root = temp_root("local-sns-evidence-symlink");
        let package = write_incomplete_evidence_package(&root);
        symlink(
            root.join(&package).join("manifest.toml"),
            root.join(&package).join("linked-manifest.toml"),
        )
        .unwrap();
        assert!(check_local_sns_committed_evidence_at(&root)
            .unwrap_err()
            .contains("reject symlinks"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn local_sns_run_logged_records_failed_child_status_under_errexit() {
        let root = temp_root("local-sns-run-logged");
        let log = root.join("failed-command.log");
        let library = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../deploy/local-sns-rehearsal/scripts/lib-local-sns.sh");
        let output = Command::new("bash")
            .args([
                "-c",
                "set -e; source \"$1\"; if run_logged \"$2\" sh -c 'exit 7'; then exit 90; fi",
                "run-logged-test",
            ])
            .arg(&library)
            .arg(&log)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let log = fs::read_to_string(log).unwrap();
        assert!(log.contains("exit_status=7"));
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
    fn local_sns_ledger_check_rejects_missing_treasury_transfer_assumption() {
        assert_local_sns_evidence_rejects(
            |text| {
                text.replace(
                    "treasury_transfer_assumed = true",
                    "treasury_transfer_assumed = false",
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
                    "duplicate_of_block_index = 11\nduplicate_tested_transfer",
                    "duplicate_of_block_index = \"none\"\nduplicate_tested_transfer",
                )
            },
            "top-level duplicate evidence",
        );
    }

    #[test]
    fn local_sns_ledger_check_rejects_zero_reserve_balance() {
        assert_local_sns_evidence_rejects(
            |text| {
                text.replace(
                    "protocol_reserve_balance_e8s = 59999999970000",
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
                    "transaction_fee_e8s = 10000\ntotal_supply_e8s = 99999999960000\nprotocol_reserve_account_owner",
                    "transaction_fee_e8s = 10001\ntotal_supply_e8s = 99999999960000\nprotocol_reserve_account_owner",
                )
            },
            "transaction_fee_e8s",
        );
    }

    #[test]
    fn local_sns_ledger_check_rejects_unknown_fee_disposition() {
        assert_local_sns_evidence_rejects(
            |text| {
                text.replace(
                    "fee_disposition_mode = \"burned\"",
                    "fee_disposition_mode = \"unknown\"",
                )
            },
            "fee_disposition_mode",
        );
    }

    #[test]
    fn local_sns_ledger_check_rejects_stale_index_evidence() {
        assert_local_sns_evidence_rejects(
            |text| {
                text.replacen(
                    "index_synced_through_block_index = 13",
                    "index_synced_through_block_index = 12",
                    1,
                )
            },
            "stale or incomplete",
        );
    }

    #[test]
    fn local_sns_ledger_check_rejects_constant_supply_claim_with_burn() {
        assert_local_sns_evidence_rejects(
            |text| {
                text.replace(
                    "total_supply_after_e8s = 99999999990000",
                    "total_supply_after_e8s = 100000000000000",
                )
            },
            "supply decrease",
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
    fn local_sns_ledger_check_rejects_moving_official_source() {
        assert_local_sns_evidence_rejects(
            |text| {
                text.replace(
                    "official_ic_source_commit = \"2d7f90fb23672cc3b81c216a33d04c75672dd308\"",
                    "official_ic_source_commit = \"main\"",
                )
            },
            "official_ic_source_commit",
        );
    }

    #[test]
    fn local_sns_ledger_check_rejects_mainnet_network_url() {
        assert_local_sns_evidence_rejects(
            |text| {
                text.replace(
                    "local_network_url = \"http://127.0.0.1:8080\"",
                    "local_network_url = \"https://icp-api.io/\"",
                )
            },
            "local_network_url",
        );
    }

    #[test]
    fn local_sns_ledger_check_rejects_anonymous_account_owner() {
        assert_local_sns_evidence_rejects(
            |text| {
                text.replacen(
                    "from_owner = \"bd3sg-teaaa-aaaaa-qaaba-cai\"",
                    "from_owner = \"2vxsx-fae\"",
                    1,
                )
            },
            "anonymous",
        );
    }

    #[test]
    fn local_sns_ledger_check_rejects_bad_subaccount_length() {
        assert_local_sns_evidence_rejects(
            |text| {
                text.replacen("to_subaccount_hex = \"1111111111111111111111111111111111111111111111111111111111111111\"", "to_subaccount_hex = \"abcd\"", 1)
            },
            "32-byte",
        );
    }

    #[test]
    fn local_sns_ledger_check_rejects_account_sequence_break() {
        assert_local_sns_evidence_rejects(
            |text| {
                text.replacen("to_subaccount_hex = \"1111111111111111111111111111111111111111111111111111111111111111\"", "to_subaccount_hex = \"3333333333333333333333333333333333333333333333333333333333333333\"", 1)
            },
            "reserve-to-user.to",
        );
    }

    #[test]
    fn local_sns_ledger_check_rejects_supply_continuity_break() {
        assert_local_sns_evidence_rejects(
            |text| {
                text.replacen(
                    "total_supply_before_e8s = 99999999980000",
                    "total_supply_before_e8s = 99999999985000",
                    1,
                )
                .replacen(
                    "total_supply_after_e8s = 99999999970000",
                    "total_supply_after_e8s = 99999999975000",
                    1,
                )
            },
            "total supply continuity",
        );
    }

    #[test]
    fn local_sns_ledger_check_rejects_top_level_amount_mismatch() {
        assert_local_sns_evidence_rejects(
            |text| {
                text.replace(
                    "reserve_transfer_amount_e8s = 100000000",
                    "reserve_transfer_amount_e8s = 99999999",
                )
            },
            "top-level ledger evidence",
        );
    }

    #[test]
    fn local_sns_ledger_check_rejects_non_monotonic_timestamp() {
        assert_local_sns_evidence_rejects(
            |text| {
                text.replace(
                    "observation_timestamp = \"2026-07-28T00:00:01Z\"",
                    "observation_timestamp = \"2026-07-27T23:59:59Z\"",
                )
            },
            "observation timestamps",
        );
    }

    #[test]
    fn local_sns_ledger_check_rejects_incomplete_archive_evidence() {
        assert_local_sns_evidence_rejects(
            |text| {
                text.replacen(
                    "archive_involvement = \"none\"",
                    "archive_involvement = \"incomplete\"",
                    1,
                )
            },
            "incomplete archive proof",
        );
    }

    #[test]
    fn local_sns_ledger_check_rejects_reserve_owner_not_stream_manager() {
        assert_local_sns_evidence_rejects(
            |text| {
                text.replacen(
                    "protocol_reserve_account_owner = \"avqkn-guaaa-aaaaa-qaaea-cai\"",
                    "protocol_reserve_account_owner = \"a3shf-5eaaa-aaaaa-qaafa-cai\"",
                    1,
                )
            },
            "reserve owner",
        );
    }

    #[test]
    fn local_sns_ledger_check_rejects_exact_reserve_redemption_collision() {
        assert_local_sns_evidence_rejects(
            |text| {
                text.replace(
                    "2222222222222222222222222222222222222222222222222222222222222222",
                    "3333333333333333333333333333333333333333333333333333333333333333",
                )
            },
            "must not collide",
        );
    }

    #[test]
    fn local_sns_ledger_check_accepts_same_owner_distinct_subaccounts() {
        let evidence = completed_local_sns_evidence();
        let parsed = parse_local_sns_evidence("fixture", &evidence).unwrap();
        assert_eq!(
            parsed.ledger.protocol_reserve_account_owner,
            parsed.user_to_redemption_transfer.to_account.owner
        );
        assert_ne!(
            parsed.ledger.protocol_reserve_subaccount_hex,
            parsed.user_to_redemption_transfer.to_account.subaccount_hex
        );
    }

    #[test]
    fn local_sns_ledger_check_rejects_swapped_dapp_roles() {
        assert_local_sns_evidence_rejects(
            |text| {
                text.replace(
                    "io_stream_manager = \"avqkn-guaaa-aaaaa-qaaea-cai\"",
                    "io_stream_manager = \"TEMP_ROLE\"",
                )
                .replace(
                    "io_nns_neuron_manager = \"aax3a-h4aaa-aaaaa-qaahq-cai\"",
                    "io_nns_neuron_manager = \"avqkn-guaaa-aaaaa-qaaea-cai\"",
                )
                .replace(
                    "io_stream_manager = \"TEMP_ROLE\"",
                    "io_stream_manager = \"aax3a-h4aaa-aaaaa-qaahq-cai\"",
                )
            },
            "reserve owner",
        );
    }

    #[test]
    fn local_sns_ledger_check_rejects_proof_canister_role_mismatch() {
        assert_local_sns_evidence_rejects(
            |text| {
                text.replacen(
                    "proof_source_canister = \"be2us-64aaa-aaaaa-qaabq-cai\"",
                    "proof_source_canister = \"br5f7-7uaaa-aaaaa-qaaca-cai\"",
                    1,
                )
            },
            "not bound",
        );
    }

    fn local_account(owner: &str, subaccount_hex: Option<&str>) -> LocalSnsAccountEvidence {
        LocalSnsAccountEvidence {
            owner: Principal::from_text(owner).unwrap(),
            subaccount_hex: subaccount_hex.map(str::to_string),
        }
    }

    fn collected_overlap_transfer(
        sender: LocalSnsAccountEvidence,
        recipient: LocalSnsAccountEvidence,
        collector: LocalSnsAccountEvidence,
        sender_balance: (u128, u128),
        recipient_balance: (u128, u128),
        collector_balance: (u128, u128),
        reserve_balance: (u128, u128),
    ) -> LocalSnsTransferEvidence {
        LocalSnsTransferEvidence {
            block_index: 42,
            from_account: sender.clone(),
            to_account: recipient.clone(),
            requested_amount_e8s: 100,
            observed_fee_e8s: 10,
            fee_disposition: "collected".to_string(),
            sender_balance_before_e8s: sender_balance.0,
            sender_balance_after_e8s: sender_balance.1,
            recipient_balance_before_e8s: recipient_balance.0,
            recipient_balance_after_e8s: recipient_balance.1,
            fee_collector_account: Some(collector.clone()),
            fee_collector_balance_before_e8s: Some(collector_balance.0),
            fee_collector_balance_after_e8s: Some(collector_balance.1),
            total_supply_before_e8s: 1_000,
            total_supply_after_e8s: 1_000,
            reserve_balance_before_e8s: reserve_balance.0,
            reserve_balance_after_e8s: reserve_balance.1,
            ledger_tip_block_index: 42,
            index_synced_through_block_index: 42,
            proof_source: LocalSnsProofSource::IndexAccountHistory,
            proof_source_canister: Principal::from_text("be2us-64aaa-aaaaa-qaabq-cai").unwrap(),
            proof_method: LocalSnsProofMethod::IcrcIndexGetAccountTransactions,
            proof_account: sender,
            archive_canister: None,
            archive_range_start: None,
            archive_range_end: None,
            archive_involvement: "none".to_string(),
            observation_timestamp: "2026-07-28T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn local_sns_transfer_validator_rejects_fee_collector_mode() {
        let sender = local_account(
            "bd3sg-teaaa-aaaaa-qaaba-cai",
            Some("1111111111111111111111111111111111111111111111111111111111111111"),
        );
        let recipient = local_account(
            "avqkn-guaaa-aaaaa-qaaea-cai",
            Some("2222222222222222222222222222222222222222222222222222222222222222"),
        );
        let collector = local_account("a3shf-5eaaa-aaaaa-qaafa-cai", None);
        let reserve = collector.clone();
        let err = validate_local_sns_transfer(
            "fixture",
            "transfer",
            &collected_overlap_transfer(
                sender.clone(),
                recipient.clone(),
                collector.clone(),
                (1_000, 890),
                (0, 100),
                (10_000, 10_010),
                (10_000, 10_010),
            ),
            &reserve,
        )
        .unwrap_err();
        assert!(err.contains("standard SNS fee policy"));
    }

    #[test]
    fn local_sns_transfer_validator_accepts_sender_equals_recipient_burn() {
        let account = local_account(
            "bd3sg-teaaa-aaaaa-qaaba-cai",
            Some("1111111111111111111111111111111111111111111111111111111111111111"),
        );
        let reserve = local_account("a3shf-5eaaa-aaaaa-qaafa-cai", None);
        let mut transfer = collected_overlap_transfer(
            account.clone(),
            account.clone(),
            reserve.clone(),
            (1_000, 990),
            (1_000, 990),
            (0, 0),
            (0, 0),
        );
        transfer.fee_disposition = "burned".to_string();
        transfer.fee_collector_account = None;
        transfer.fee_collector_balance_before_e8s = None;
        transfer.fee_collector_balance_after_e8s = None;
        transfer.total_supply_after_e8s = 990;
        validate_local_sns_transfer("fixture", "transfer", &transfer, &reserve).unwrap();
    }

    #[test]
    fn local_sns_transfer_validator_rejects_unexplained_balance_movement() {
        let sender = local_account(
            "bd3sg-teaaa-aaaaa-qaaba-cai",
            Some("1111111111111111111111111111111111111111111111111111111111111111"),
        );
        let recipient = local_account(
            "avqkn-guaaa-aaaaa-qaaea-cai",
            Some("2222222222222222222222222222222222222222222222222222222222222222"),
        );
        let collector = local_account("a3shf-5eaaa-aaaaa-qaafa-cai", None);
        let transfer = collected_overlap_transfer(
            sender,
            recipient,
            collector.clone(),
            (1_000, 889),
            (0, 100),
            (10_000, 10_010),
            (10_000, 10_010),
        );
        let mut transfer = transfer;
        transfer.fee_disposition = "burned".to_string();
        transfer.fee_collector_account = None;
        transfer.fee_collector_balance_before_e8s = None;
        transfer.fee_collector_balance_after_e8s = None;
        transfer.total_supply_after_e8s = 990;
        assert!(
            validate_local_sns_transfer("fixture", "transfer", &transfer, &collector)
                .unwrap_err()
                .contains("unexplained balance movement")
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
    fn production_did_and_release_surface_have_no_debug_fee_dependency() {
        assert!(STREAM_PRODUCTION_FORBIDDEN_DID.contains(&"debug_"));
        assert!(PRODUCTION_WASM_FORBIDDEN_METHOD_STRINGS.contains(&"debug_get_transactions"));
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
