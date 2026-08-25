use candid::Principal;
use io_production_wiring::{
    template_paths, validate_template_text, PRODUCTION_FRONTEND_CANISTER_ID,
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

mod production_wiring;
mod sns_framework;
mod stable_schema;
mod workflow_validation;

use production_wiring::check_production_wiring_at;
use stable_schema::check_stable_storage_at;
use workflow_validation::check_required_workflows_at;

const RELEASE_PROFILE: &str = "release";
const WASM_TARGET: &str = "wasm32-unknown-unknown";
const MANIFEST_PATH: &str = "release-artifacts/manifest.json";
const CURRENT_CANONICAL_SELECTOR: &str =
    "deploy/local-sns-rehearsal/evidence/current-canonical.toml";
const KNOWN_TWO_YEAR_NNS_NEURON_ID: u64 = PROTECTED_IO_NNS_NEURON_ID;
const KNOWN_CONTROLLER_CANISTER_PRINCIPAL: &str = PROTECTED_IO_NEURON_OWNER_CANISTER;
const PRODUCTION_CANISTER_IDS_PATH: &str = "deploy/production-wiring/canister-ids.toml";
const NNS_GOVERNANCE_SOURCE_COMMIT: &str = "8aa4680e378f3248e7e7b9b8237915aded999bd9";
const ICP_LEDGER_SOURCE_COMMIT: &str = "021bf342f66296d5605b355a61b2430406a83783";
const NNS_GOVERNANCE_SOURCE_SHA256: &str =
    "b41a5add38d54751d53fb4f0c826b09aaee38e0c5bea632400f1dbaaa11cfd4b";
const NNS_GOVERNANCE_WASM_SHA256: &str =
    "eaa2da45722d980b25405525873571ab7dad426a93e1d4971f6b555d80906d85";
const ICP_LEDGER_SOURCE_SHA256: &str =
    "5d69ec2e26e5546fe7e94bab721d6c4ed840106f9e2e69d11a8f3ee6e7721df0";
const ICP_LEDGER_WASM_SHA256: &str =
    "9c1ff658635daabb7a3e9dcc5dca337eee5008bc2033d0e929c3fae53814f91c";
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
    " prove_maturity_mint :",
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

fn check_obsolete_economics_guard_at(root: &Path) -> Result<(), String> {
    const ALLOWED_PREFIXES: &[&str] = &[
        "deploy/local-sns-rehearsal/evidence/",
        "deploy/local-sns-rehearsal/generated/",
        "deploy/local-sns-rehearsal/install-args.local/",
        "canisters/frontend/public/generated/",
        "docs/research/",
        "docs/architecture/adr-protected-reward-backing-nns-neuron.md",
        "docs/operations/p0-simplified-composition-evidence.md",
    ];
    let forbidden = [
        concat!("seeded_two_week", "_principal_e8s"),
        concat!("two_week", "_receipt_source"),
        concat!("two_week", "_fee_float_e8s"),
        concat!("reward_backing", "_neuron_id"),
        concat!("reconcile_two_week", "_backing_readiness"),
        concat!("Claim", "Route"),
        concat!("prepare_", "backing_inflow"),
        concat!("prove_", "backing_inflow"),
        concat!("Backing", "InflowDelivery"),
        concat!("prepare_", "jupiter_receipt"),
        concat!("liquid_icp_reserve", " / redeemable_io_supply"),
        concat!("Only liquid ICP counts", " as redemption NAV"),
        concat!("NNS liquid maturity", " leg"),
        concat!("jointly frozen", " physical backing route"),
        concat!("finite joint route", "/reward planner"),
        concat!("prove_maturity", "_mint"),
        concat!("MintProof", "State"),
        concat!("Mint", "Evidence"),
        concat!("MaturityEvidence", "Source"),
        concat!("Permanent", "Maturity"),
        concat!("Pooled", "Maturity"),
        concat!("permanent", " maturity"),
        concat!("pooled", " maturity"),
        concat!("source_operation", "_id"),
        concat!("stream_receipt", "_fingerprint"),
        concat!("pub maturity", "_staging"),
        concat!("maturity_staging", " : Account"),
        concat!("maturity_staging", " ="),
    ];
    fn visit(
        root: &Path,
        directory: &Path,
        forbidden: &[&str],
        violations: &mut Vec<String>,
    ) -> Result<(), String> {
        for entry in fs::read_dir(directory)
            .map_err(|error| format!("failed to scan {}: {error}", directory.display()))?
        {
            let entry = entry.map_err(|error| format!("source scan failed: {error}"))?;
            let path = entry.path();
            let relative = path
                .strip_prefix(root)
                .map_err(|error| format!("source path escaped root: {error}"))?
                .to_string_lossy()
                .replace('\\', "/");
            if entry
                .file_type()
                .map_err(|error| format!("failed to inspect {relative}: {error}"))?
                .is_dir()
            {
                if matches!(relative.as_str(), ".git" | "target" | "release-artifacts")
                    || relative.starts_with("node_modules/")
                    || relative.starts_with("debug-artifacts/")
                {
                    continue;
                }
                visit(root, &path, forbidden, violations)?;
                continue;
            }
            if ALLOWED_PREFIXES
                .iter()
                .any(|allowed| relative == *allowed || relative.starts_with(allowed))
                || relative == "tools/xtask/src/main.rs"
            {
                continue;
            }
            let Some(extension) = path.extension().and_then(|value| value.to_str()) else {
                continue;
            };
            if !matches!(
                extension,
                "rs" | "md" | "did" | "sh" | "toml" | "yaml" | "yml" | "js" | "mjs"
            ) {
                continue;
            }
            let text = fs::read_to_string(&path)
                .map_err(|error| format!("failed to read {relative}: {error}"))?;
            for needle in forbidden {
                if text.contains(needle) {
                    violations.push(format!("{relative}: obsolete active assumption {needle:?}"));
                }
            }
        }
        Ok(())
    }
    let mut violations = Vec::new();
    visit(root, root, &forbidden, &mut violations)?;
    if violations.is_empty() {
        Ok(())
    } else {
        Err(violations.join("\n"))
    }
}

fn require_debug_some_value(
    path: &str,
    text: &str,
    field: &str,
    expected: &str,
) -> Result<(), String> {
    let marker = format!("{field}: Some(");
    let remainder = text
        .split_once(&marker)
        .map(|(_, remainder)| remainder)
        .ok_or_else(|| format!("{path} must contain {marker:?}"))?;
    let actual = remainder
        .trim_start()
        .split_once(',')
        .map(|(value, _)| value.trim())
        .ok_or_else(|| format!("{path} has malformed debug value for {field}"))?;
    if actual != expected {
        return Err(format!(
            "{path} must record {field}=Some({expected}), observed Some({actual})"
        ));
    }
    Ok(())
}

fn quoted_rust_const(text: &str, name: &str) -> Result<String, String> {
    let prefix = format!("pub const {name}:");
    let line = text
        .lines()
        .find(|line| line.trim_start().starts_with(&prefix))
        .ok_or_else(|| format!("missing Rust constant {name}"))?;
    let value = line
        .split_once('=')
        .map(|(_, value)| value.trim().trim_end_matches(';'))
        .ok_or_else(|| format!("malformed Rust constant {name}"))?;
    if value.len() < 2 || !value.starts_with('"') || !value.ends_with('"') {
        return Err(format!("Rust constant {name} must be one quoted string"));
    }
    Ok(value[1..value.len() - 1].to_string())
}

fn check_nns_boundary_pin_at(root: &Path) -> Result<(), String> {
    let implementation_path = "crates/io_nns_types/src/jupiter.rs";
    let manifest_path = "tests/e2e_real_canisters/wasms.example.toml";
    let evidence_path = "docs/testing/nns-boundary-pin.md";
    let implementation = require_file(root, implementation_path)?;
    let governance_pin = quoted_rust_const(&implementation, "PINNED_NNS_GOVERNANCE_COMMIT")?;
    let ledger_pin = quoted_rust_const(&implementation, "PINNED_ICP_LEDGER_COMMIT")?;
    if governance_pin != NNS_GOVERNANCE_SOURCE_COMMIT || ledger_pin != ICP_LEDGER_SOURCE_COMMIT {
        return Err(format!(
            "{implementation_path}: implementation component pins do not equal the approved boundaries"
        ));
    }

    let manifest = require_file(root, manifest_path)?;
    for (artifact, component_pin) in [
        ("nns_governance", governance_pin.as_str()),
        ("nns_ledger", ledger_pin.as_str()),
        ("icp_ledger", ledger_pin.as_str()),
    ] {
        require_toml_string(
            manifest_path,
            &manifest,
            "artifacts",
            &format!("{artifact}_upstream_rev"),
            component_pin,
        )?;
        let source_url =
            parse_toml_string(&manifest, "artifacts", &format!("{artifact}_source_url"))?;
        if !source_url.contains(&format!("/ic/{component_pin}/")) {
            return Err(format!(
                "{manifest_path}: {artifact} source URL does not contain implementation revision"
            ));
        }
    }
    for (artifact, source_hash, wasm_hash) in [
        (
            "nns_governance",
            NNS_GOVERNANCE_SOURCE_SHA256,
            NNS_GOVERNANCE_WASM_SHA256,
        ),
        (
            "nns_ledger",
            ICP_LEDGER_SOURCE_SHA256,
            ICP_LEDGER_WASM_SHA256,
        ),
        (
            "icp_ledger",
            ICP_LEDGER_SOURCE_SHA256,
            ICP_LEDGER_WASM_SHA256,
        ),
    ] {
        require_toml_string(
            manifest_path,
            &manifest,
            "artifacts",
            &format!("{artifact}_source_sha256"),
            source_hash,
        )?;
        require_toml_string(
            manifest_path,
            &manifest,
            "artifacts",
            &format!("{artifact}_sha256"),
            wasm_hash,
        )?;
    }

    let evidence = require_file(root, evidence_path)?;
    require_present(
        evidence_path,
        &evidence,
        &[
            NNS_GOVERNANCE_SOURCE_COMMIT,
            ICP_LEDGER_SOURCE_COMMIT,
            NNS_GOVERNANCE_SOURCE_SHA256,
            NNS_GOVERNANCE_WASM_SHA256,
            ICP_LEDGER_SOURCE_SHA256,
            ICP_LEDGER_WASM_SHA256,
            "6e9a397f4bf0adc913980ef6c176e765534617d0ce59d52e7bcc66add2b0cd71",
            "45a6f13779ead0f7247b728f7a8953d649173863fea1f01fbf7c04f30589aad7",
            "100_000_000",
            "604_800",
            "native memo",
            "no ICRC memo",
        ],
    )
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

#[derive(Clone, Debug, Eq, PartialEq)]
struct CurrentCanonicalSelector {
    package: String,
    io_release_source_commit: String,
    io_artifact_recording_commit: String,
    release_manifest_sha256: String,
    package_manifest_sha256: String,
    package_sha256s_sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ValidatedEvidencePackage {
    complete: bool,
    monitoring: bool,
    canonical_economics: bool,
    io_release_source_commit: Option<String>,
    io_artifact_recording_commit: Option<String>,
}

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

fn validate_lower_hex(path: &str, field: &str, value: &str, length: usize) -> Result<(), String> {
    if value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        Ok(())
    } else {
        Err(format!(
            "{path}: {field} must be exact lowercase {length}-hex"
        ))
    }
}

fn parse_current_canonical_selector(
    path: &str,
    text: &str,
) -> Result<CurrentCanonicalSelector, String> {
    let mut schema_sections = 0_usize;
    let mut current_sections = 0_usize;
    for raw_line in text.lines() {
        match raw_line.split('#').next().unwrap_or("").trim() {
            "[schema]" => schema_sections += 1,
            "[current]" => current_sections += 1,
            _ => {}
        }
    }
    if schema_sections != 1 || current_sections != 1 {
        return Err(format!(
            "{path}: selector must contain exactly one [schema] and one [current] section"
        ));
    }
    let doc = parse_simple_toml_document(path, text)?;
    let expected_sections = ["current".to_string(), "schema".to_string()]
        .into_iter()
        .collect::<BTreeSet<_>>();
    let actual_sections = doc.keys().cloned().collect::<BTreeSet<_>>();
    if actual_sections != expected_sections {
        return Err(format!(
            "{path}: selector sections must be exactly [schema] and [current]"
        ));
    }
    let schema = require_simple_section(path, &doc, "schema")?;
    if schema.keys().map(String::as_str).collect::<Vec<_>>() != ["version"] {
        return Err(format!("{path}: [schema] fields must be exactly version"));
    }
    if require_simple_u64(path, &doc, "schema", "version")? != 1 {
        return Err(format!("{path}: unsupported selector schema version"));
    }
    let expected_current_fields = [
        "io_artifact_recording_commit",
        "io_release_source_commit",
        "package",
        "package_manifest_sha256",
        "package_sha256s_sha256",
        "release_manifest_sha256",
    ];
    let current = require_simple_section(path, &doc, "current")?;
    if current.keys().map(String::as_str).collect::<Vec<_>>() != expected_current_fields {
        return Err(format!(
            "{path}: [current] contains missing or unexpected fields"
        ));
    }
    let selector = CurrentCanonicalSelector {
        package: require_simple_string(path, &doc, "current", "package")?,
        io_release_source_commit: require_simple_string(
            path,
            &doc,
            "current",
            "io_release_source_commit",
        )?,
        io_artifact_recording_commit: require_simple_string(
            path,
            &doc,
            "current",
            "io_artifact_recording_commit",
        )?,
        release_manifest_sha256: require_simple_string(
            path,
            &doc,
            "current",
            "release_manifest_sha256",
        )?,
        package_manifest_sha256: require_simple_string(
            path,
            &doc,
            "current",
            "package_manifest_sha256",
        )?,
        package_sha256s_sha256: require_simple_string(
            path,
            &doc,
            "current",
            "package_sha256s_sha256",
        )?,
    };
    let package_path = Path::new(&selector.package);
    let components = package_path.components().collect::<Vec<_>>();
    if selector.package.is_empty()
        || selector.package.contains('/')
        || selector.package.contains('\\')
        || package_path.is_absolute()
        || components.len() != 1
        || !matches!(components[0], std::path::Component::Normal(_))
        || selector.package == "."
        || selector.package == ".."
    {
        return Err(format!(
            "{path}: current.package must be one traversal-free leaf directory name"
        ));
    }
    validate_lower_hex(
        path,
        "current.io_release_source_commit",
        &selector.io_release_source_commit,
        40,
    )?;
    validate_lower_hex(
        path,
        "current.io_artifact_recording_commit",
        &selector.io_artifact_recording_commit,
        40,
    )?;
    for (field, value) in [
        (
            "current.release_manifest_sha256",
            selector.release_manifest_sha256.as_str(),
        ),
        (
            "current.package_manifest_sha256",
            selector.package_manifest_sha256.as_str(),
        ),
        (
            "current.package_sha256s_sha256",
            selector.package_sha256s_sha256.as_str(),
        ),
    ] {
        validate_lower_hex(path, field, value, 64)?;
    }
    Ok(selector)
}

fn read_current_canonical_selector(root: &Path) -> Result<CurrentCanonicalSelector, String> {
    let path = root.join(CURRENT_CANONICAL_SELECTOR);
    let metadata = fs::symlink_metadata(&path).map_err(|err| {
        format!("{CURRENT_CANONICAL_SELECTOR}: required selector is missing: {err}")
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!(
            "{CURRENT_CANONICAL_SELECTOR}: selector must be a regular non-symlink file"
        ));
    }
    let text =
        fs::read_to_string(&path).map_err(|err| format!("{CURRENT_CANONICAL_SELECTOR}: {err}"))?;
    parse_current_canonical_selector(CURRENT_CANONICAL_SELECTOR, &text)
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
            .filter(|line| !line.trim().is_empty())
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
        "set_timer_interval",
    ];

    let mut combined_lines = 0usize;
    let mut stream_lines = 0usize;
    for directory in VALUE_MOVING_DIRS {
        for path in rust_files_below(root, directory)? {
            let text = fs::read_to_string(&path)
                .map_err(|error| format!("{}: {error}", path.display()))?;
            let lines = production_line_count(&text);
            if lines > 1_000 {
                return Err(format!(
                    "{} has {lines} production lines; the per-file limit is 1000",
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
    let reward_policy_lines = rust_files_below(root, "crates/io_reward_policy/src")?
        .into_iter()
        .try_fold(0usize, |sum, path| {
            fs::read_to_string(&path)
                .map(|text| sum + production_line_count(&text))
                .map_err(|error| format!("{}: {error}", path.display()))
        })?;
    if reward_policy_lines > 450 {
        return Err(format!(
            "io-reward-policy has {reward_policy_lines} production lines"
        ));
    }
    let reward_boundary_lines = rust_files_below(root, "crates/io_sns_reward_boundary/src")?
        .into_iter()
        .try_fold(0usize, |sum, path| {
            fs::read_to_string(&path)
                .map(|text| sum + production_line_count(&text))
                .map_err(|error| format!("{}: {error}", path.display()))
        })?;
    if reward_boundary_lines > 500 {
        return Err(format!(
            "SNS reward-event boundary has {reward_boundary_lines} production lines"
        ));
    }
    let account_lines = rust_files_below(root, "crates/io_accounts/src")?
        .into_iter()
        .try_fold(0usize, |sum, path| {
            fs::read_to_string(&path)
                .map(|text| sum + production_line_count(&text))
                .map_err(|error| format!("{}: {error}", path.display()))
        })?;
    combined_lines = combined_lines
        .checked_add(reward_boundary_lines)
        .and_then(|lines| lines.checked_add(account_lines))
        .ok_or_else(|| "combined production line count overflow".to_string())?;
    for directory in [
        "crates/io_core_model/src",
        "crates/io_ledger_boundary/src",
        "crates/io_reward_policy/src",
    ] {
        for path in rust_files_below(root, directory)? {
            let text = fs::read_to_string(&path)
                .map_err(|error| format!("{}: {error}", path.display()))?;
            combined_lines += production_line_count(&text);
        }
    }
    for directory in ["crates/io_nns_types/src", "crates/io_receipt_types/src"] {
        for path in rust_files_below(root, directory)? {
            let text = fs::read_to_string(&path)
                .map_err(|error| format!("{}: {error}", path.display()))?;
            let lines = production_line_count(&text);
            if lines > 500 {
                return Err(format!("{} has {lines} production lines", path.display()));
            }
            combined_lines += lines;
        }
    }
    if stream_lines > 5_600 {
        return Err(format!(
            "stream-manager production Rust has {stream_lines} lines"
        ));
    }
    if combined_lines > 14_660 {
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
    if nns_lines > 6_180 {
        return Err(format!("NNS-manager production Rust has {nns_lines} lines"));
    }
    let tree = Command::new("cargo")
        .args([
            "tree",
            "-p",
            "io-stream-manager",
            "-e",
            "normal",
            "--prefix",
            "none",
        ])
        .current_dir(root)
        .output()
        .map_err(|error| format!("cargo tree for io-stream-manager failed: {error}"))?;
    if !tree.status.success() {
        return Err(format!(
            "cargo tree for io-stream-manager failed: {}",
            String::from_utf8_lossy(&tree.stderr)
        ));
    }
    if String::from_utf8_lossy(&tree.stdout)
        .lines()
        .any(|line| line.split_whitespace().next() == Some("io-governance-types"))
    {
        return Err(
            "io-stream-manager has forbidden transitive production dependency io-governance-types"
                .into(),
        );
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
        "simplicity metrics: stream_manager={stream_lines} nns_manager={nns_lines} accounts={account_lines} ledger_boundary={boundary_lines} economics={economics_lines} reward_policy={reward_policy_lines} sns_reward_boundary={reward_boundary_lines} combined={combined_lines}"
    );
    Ok(())
}

fn check_did_surface_at(root: &Path, check_wasm: bool) -> Result<(), String> {
    let stream_production_path = "canisters/io_stream_manager/io_stream_manager.did";
    let stream_debug_path = "canisters/io_stream_manager/io_stream_manager_debug.did";
    let nns_production_path = "canisters/io_nns_neuron_manager/io_nns_neuron_manager.did";
    let nns_debug_path = "canisters/io_nns_neuron_manager/io_nns_neuron_manager_debug.did";
    let historian_production_path = "canisters/io_historian/io_historian.did";

    let stream_production = read_file(root, stream_production_path)?;
    let nns_production = read_file(root, nns_production_path)?;
    if root.join(stream_debug_path).exists() || root.join(nns_debug_path).exists() {
        return Err("value-moving debug DIDs must be deleted".into());
    }
    let historian_production = read_file(root, historian_production_path)?;

    check_minimal_value_moving_did(stream_production_path, &stream_production)?;
    check_minimal_value_moving_did(nns_production_path, &nns_production)?;

    require_present(
        stream_production_path,
        &stream_production,
        &[
            "  redeem :",
            "  prepare_claim_backing_receipt :",
            "  prove_claim_backing_receipt :",
            "  resume :",
            "  prove_active_transfer :",
            "  set_paused :",
            "  validate_set_paused :",
            "  get_status :",
        ],
    )?;
    require_present(
        nns_production_path,
        &nns_production,
        &[
            "  notify_jupiter_deposit :",
            "  prepare_pool_reconciliation :",
            "  observe_claim_assets :",
            "  observe_pool_policy :",
            "  prepare_two_week_maturity :",
            "  start_maturity :",
            "  resume :",
            "  prove_active_transfer :",
            "  set_paused :",
            "  validate_set_paused :",
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
            "get_claim_rate",
            "ObservationConfig",
            "service : (opt ObservationConfig)",
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

fn expected_release_artifact_names() -> BTreeSet<String> {
    expected_release_artifacts()
        .into_iter()
        .map(|path| {
            path.strip_prefix("release-artifacts/")
                .expect("release artifact path prefix")
                .to_string()
        })
        .collect()
}

fn release_artifact_directory_files(dir: &Path) -> Result<BTreeSet<String>, String> {
    let mut files = BTreeSet::new();
    for entry in fs::read_dir(dir).map_err(|err| format!("{}: {err}", dir.display()))? {
        let entry = entry.map_err(|err| format!("{}: {err}", dir.display()))?;
        let file_type = entry
            .file_type()
            .map_err(|err| format!("{}: {err}", entry.path().display()))?;
        if !file_type.is_file() {
            return Err(format!(
                "unexpected non-file release artifact {}",
                entry.path().display()
            ));
        }
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| format!("non-UTF-8 release artifact in {}", dir.display()))?;
        files.insert(name);
    }
    let expected = expected_release_artifact_names();
    if files != expected {
        let missing = expected.difference(&files).cloned().collect::<Vec<_>>();
        let unexpected = files.difference(&expected).cloned().collect::<Vec<_>>();
        return Err(format!(
            "release artifact file set mismatch in {}: missing [{}], unexpected [{}]",
            dir.display(),
            missing.join(", "),
            unexpected.join(", ")
        ));
    }
    Ok(files)
}

fn compare_release_artifact_dirs(first: &Path, second: &Path) -> Result<(), String> {
    let files = release_artifact_directory_files(first)?;
    release_artifact_directory_files(second)?;
    for name in files {
        let first_path = first.join(&name);
        let second_path = second.join(&name);
        let first_size = fs::metadata(&first_path)
            .map_err(|err| format!("{}: {err}", first_path.display()))?
            .len();
        let second_size = fs::metadata(&second_path)
            .map_err(|err| format!("{}: {err}", second_path.display()))?
            .len();
        if first_size != second_size {
            return Err(format!(
                "release artifact size mismatch for {name}: {} has {first_size} bytes, {} has {second_size} bytes",
                first.display(),
                second.display()
            ));
        }
        let first_bytes =
            fs::read(&first_path).map_err(|err| format!("{}: {err}", first_path.display()))?;
        let second_bytes =
            fs::read(&second_path).map_err(|err| format!("{}: {err}", second_path.display()))?;
        if first_bytes != second_bytes {
            return Err(format!(
                "release artifact byte mismatch for {name}: {} != {}",
                first.display(),
                second.display()
            ));
        }
    }
    Ok(())
}

fn sha256_hex(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|err| format!("{}: {err}", path.display()))?;
    let digest = Sha256::digest(bytes);
    Ok(digest.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn nervous_system_domain_subaccount(controller: Principal, domain: &[u8], nonce: u64) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update([domain.len() as u8]);
    hasher.update(domain);
    hasher.update(controller.as_slice());
    hasher.update(nonce.to_be_bytes());
    hasher.finalize().into()
}

fn nns_neuron_staking_subaccount(controller: Principal, nonce: u64) -> String {
    hex::encode(nervous_system_domain_subaccount(
        controller,
        b"neuron-stake",
        nonce,
    ))
}

pub(crate) fn sns_distribution_subaccount(controller: Principal, nonce: u64) -> String {
    hex::encode(nervous_system_domain_subaccount(
        controller,
        b"token-distribution",
        nonce,
    ))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RedemptionEconomics {
    excluded_total_e8s: u128,
    redeemable_supply_e8s: u128,
    gross_icp_e8s: u128,
    net_icp_e8s: u128,
}

fn calculate_redemption_economics(
    total_supply_e8s: u128,
    protocol_reserve_e8s: u128,
    excluded_balances_e8s: &[u128],
    liquid_icp_e8s: u128,
    redeemed_io_e8s: u128,
    icp_fee_e8s: u128,
) -> Result<RedemptionEconomics, String> {
    let excluded_total_e8s = excluded_balances_e8s
        .iter()
        .try_fold(0_u128, |sum, balance| {
            sum.checked_add(*balance)
                .ok_or_else(|| "excluded IO balance sum overflow".to_string())
        })?;
    let non_redeemable = protocol_reserve_e8s
        .checked_add(excluded_total_e8s)
        .ok_or_else(|| "protocol reserve plus excluded IO overflow".to_string())?;
    let redeemable_supply_e8s = total_supply_e8s
        .checked_sub(non_redeemable)
        .ok_or_else(|| "total IO supply is less than reserve plus excluded IO".to_string())?;
    if redeemable_supply_e8s == 0 && redeemed_io_e8s != 0 {
        return Err("redeemable IO supply is zero for a claimed redemption".into());
    }
    let gross_icp_e8s = redeemed_io_e8s
        .checked_mul(liquid_icp_e8s)
        .ok_or_else(|| "redemption numerator overflow".to_string())?
        .checked_div(redeemable_supply_e8s)
        .ok_or_else(|| "redeemable IO supply is zero".to_string())?;
    let net_icp_e8s = gross_icp_e8s
        .checked_sub(icp_fee_e8s)
        .ok_or_else(|| "quoted gross ICP is below the ICP fee".to_string())?;
    Ok(RedemptionEconomics {
        excluded_total_e8s,
        redeemable_supply_e8s,
        gross_icp_e8s,
        net_icp_e8s,
    })
}

fn candid_blob_literal_from_hex(value: &str) -> Result<String, String> {
    if !value.len().is_multiple_of(2)
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err("blob hex must be even-length lowercase hexadecimal".into());
    }
    Ok(value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| format!("\\{}", std::str::from_utf8(pair).expect("hex is ASCII")))
        .collect())
}

fn candid_nat_field(text: &str, key: &str) -> Result<u128, String> {
    let marker = format!("{key} = ");
    let start = text
        .find(&marker)
        .ok_or_else(|| format!("missing Candid field {key}"))?
        + marker.len();
    let digits = text[start..]
        .chars()
        .take_while(|character| character.is_ascii_digit() || *character == '_')
        .filter(|character| *character != '_')
        .collect::<String>();
    if digits.is_empty() {
        return Err(format!("Candid field {key} is not an unsigned integer"));
    }
    digits
        .parse::<u128>()
        .map_err(|error| format!("invalid Candid field {key}: {error}"))
}

fn candid_index_transaction_records(text: &str) -> Result<Vec<&str>, String> {
    let marker = "transactions = vec {";
    let transactions = text
        .find(marker)
        .ok_or_else(|| "index response omits transactions vector".to_string())?
        + marker.len();
    let bytes = text.as_bytes();
    let mut depth = 1_i32;
    let mut cursor = transactions;
    let mut record_start = None;
    let mut records = Vec::new();
    while cursor < bytes.len() && depth > 0 {
        if bytes[cursor..].starts_with(b"record {") && depth == 1 {
            record_start = Some(cursor);
        }
        match bytes[cursor] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 1 {
                    if let Some(start) = record_start.take() {
                        records.push(&text[start..=cursor]);
                    }
                }
            }
            _ => {}
        }
        cursor += 1;
    }
    if depth != 0 {
        return Err("unterminated index transactions vector".into());
    }
    Ok(records)
}

fn unique_index_transfer_record<'a>(
    text: &'a str,
    amount_e8s: u128,
    memo_hex: &str,
) -> Result<&'a str, String> {
    let memo = format!(
        "memo = opt blob \"{}\"",
        candid_blob_literal_from_hex(memo_hex)?
    );
    let matching = candid_index_transaction_records(text)?
        .into_iter()
        .filter(|record| {
            record.contains("kind = \"transfer\"")
                && record.contains(&memo)
                && candid_nat_field(record, "amount") == Ok(amount_e8s)
        })
        .collect::<Vec<_>>();
    if matching.len() != 1 {
        return Err(format!(
            "expected one index transfer for amount {amount_e8s} and memo {memo_hex}, found {}",
            matching.len()
        ));
    }
    Ok(matching[0])
}

fn index_transfer_block(text: &str, amount_e8s: u128, memo_hex: &str) -> Result<u64, String> {
    let id = candid_nat_field(
        unique_index_transfer_record(text, amount_e8s, memo_hex)?,
        "id",
    )?;
    u64::try_from(id).map_err(|_| "index transaction id exceeds u64".to_string())
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

fn current_git_commit(root: &Path) -> Result<String, String> {
    let output = Command::new("git")
        .current_dir(root)
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

fn validate_release_source_commit(root: &Path, git_commit: &str) -> Result<(), String> {
    if !is_full_lowercase_hex_sha(git_commit) {
        return Err(format!(
            "release source commit must be a full 40-character lowercase hexadecimal SHA, got {git_commit:?}"
        ));
    }

    let cat_file = Command::new("git")
        .current_dir(root)
        .args(["cat-file", "-e", &format!("{git_commit}^{{commit}}")])
        .output()
        .map_err(|err| format!("git cat-file {git_commit}: {err}"))?;
    if !cat_file.status.success() {
        return Err(format!(
            "release source commit {git_commit} does not resolve locally as a commit"
        ));
    }

    Ok(())
}

fn validate_release_source_ancestor(root: &Path, git_commit: &str) -> Result<(), String> {
    validate_release_source_commit(root, git_commit)?;
    let status = Command::new("git")
        .current_dir(root)
        .args(["merge-base", "--is-ancestor", git_commit, "HEAD"])
        .status()
        .map_err(|err| format!("git merge-base --is-ancestor {git_commit} HEAD: {err}"))?;
    if !status.success() {
        return Err(format!(
            "release source commit {git_commit} is not an ancestor of HEAD"
        ));
    }
    Ok(())
}

fn release_tail_evidence_path_allowed(path: &str) -> bool {
    path.starts_with("deploy/local-sns-rehearsal/evidence/")
        || path.starts_with("docs/")
        || path.starts_with(".github/workflows/")
        || path == "tools/sns/launch-readiness.toml"
}

fn validate_release_commit_paths(
    commit_label: &str,
    paths: &[String],
    artifact_recording: bool,
) -> Result<(), String> {
    if paths.is_empty() {
        return Err(format!(
            "release-tail commit {commit_label} changes no paths"
        ));
    }
    for path in paths {
        let allowed = if artifact_recording {
            path.starts_with("release-artifacts/")
        } else {
            release_tail_evidence_path_allowed(path)
        };
        if !allowed {
            let phase = if artifact_recording {
                "artifact-recording"
            } else {
                "evidence/documentation"
            };
            return Err(format!(
                "release-tail {phase} commit {commit_label} changes forbidden path {path}"
            ));
        }
    }
    Ok(())
}

fn git_output(root: &Path, args: &[&str], label: &str) -> Result<String, String> {
    let output = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .map_err(|error| format!("{label}: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "{label} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn validate_release_tail(root: &Path, source_commit: &str) -> Result<(), String> {
    validate_release_source_ancestor(root, source_commit)?;
    let head = current_git_commit(root)?;
    if head == source_commit {
        return Ok(());
    }
    let commits = git_output(
        root,
        &[
            "rev-list",
            "--reverse",
            "--ancestry-path",
            &format!("{source_commit}..HEAD"),
        ],
        "git rev-list release tail",
    )?;
    let commits = commits.lines().collect::<Vec<_>>();
    let artifact_commit = commits
        .first()
        .ok_or_else(|| "release tail unexpectedly contains no commits".to_string())?;
    let artifact_parent = git_output(
        root,
        &["rev-parse", &format!("{artifact_commit}^1")],
        "git rev-parse artifact-recording parent",
    )?;
    if artifact_parent != source_commit {
        return Err(format!(
            "artifact-recording commit {artifact_commit} must directly follow source-finalization commit {source_commit}"
        ));
    }
    for (index, commit) in commits.iter().enumerate() {
        let paths = git_output(
            root,
            &[
                "diff-tree",
                "--no-commit-id",
                "--name-only",
                "--no-renames",
                "-r",
                commit,
            ],
            "git diff-tree release-tail commit",
        )?
        .lines()
        .map(str::to_string)
        .collect::<Vec<_>>();
        validate_release_commit_paths(commit, &paths, index == 0)?;
    }
    Ok(())
}

fn validate_release_source_tree(root: &Path, git_commit: &str) -> Result<(), String> {
    validate_release_source_commit(root, git_commit)?;
    let tree_diff = Command::new("git")
        .current_dir(root)
        .args([
            "diff",
            "--quiet",
            git_commit,
            "HEAD",
            "--",
            ".",
            ":(exclude)release-artifacts",
        ])
        .output()
        .map_err(|err| format!("git diff {git_commit} HEAD: {err}"))?;
    if !tree_diff.status.success() {
        return Err(format!(
            "release source commit {git_commit} does not match the exact source tree at HEAD"
        ));
    }

    let dirty = Command::new("git")
        .current_dir(root)
        .args([
            "status",
            "--porcelain=v1",
            "--untracked-files=all",
            "--",
            ".",
            ":(exclude)release-artifacts",
        ])
        .output()
        .map_err(|err| format!("git status for release source: {err}"))?;
    if !dirty.status.success() {
        return Err("git status for release source failed".into());
    }
    if !dirty.stdout.is_empty() {
        return Err("release source checkout has dirty files outside release-artifacts".into());
    }
    Ok(())
}

fn validate_release_build_checkout(root: &Path, git_commit: &str) -> Result<(), String> {
    validate_release_source_tree(root, git_commit)?;
    let head = current_git_commit(root)?;
    if head != git_commit {
        return Err(format!(
            "release build must run at exact source commit {git_commit}, not {head}; use tools/scripts/build-release-from-source"
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
    validate_release_source_commit(root, &git_commit)
        .map_err(|err| format!("{MANIFEST_PATH}: {err}"))?;
    Ok(Some(git_commit))
}

fn release_source_commit(root: &Path) -> Result<String, String> {
    let git_commit = match env::var(RELEASE_SOURCE_COMMIT_ENV) {
        Ok(value) => value,
        Err(env::VarError::NotPresent) => match manifest_source_commit(root)? {
            Some(value) => value,
            None => current_git_commit(root)?,
        },
        Err(err) => return Err(format!("{RELEASE_SOURCE_COMMIT_ENV}: {err}")),
    };
    validate_release_build_checkout(root, &git_commit)?;
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
    validate_release_tail(root, &source_commit).map_err(|err| format!("{MANIFEST_PATH}: {err}"))?;
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

fn validate_pooled_claim_topology(stream: &str, nns: &str) -> Result<(), String> {
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
    let jupiter_staging = nns
        .lines()
        .find(|line| line.contains("jupiter_staging"))
        .ok_or_else(|| "missing topology field jupiter_staging".to_string())?;
    if !jupiter_staging.contains("subaccount = null") {
        return Err("jupiter_staging must use the NNS manager default Account".into());
    }
    for (text, field, token) in [
        (
            stream,
            "nns_manager",
            "TODO_EXISTING_NNS_CONTROLLER_PRINCIPAL",
        ),
        (nns, "jupiter_staging", "TODO_EXISTING_NNS_CONTROLLER_SELF"),
        (
            nns,
            "jupiter_activation_block_floor",
            "TODO_JUPITER_ACTIVATION_BLOCK_FLOOR",
        ),
        (
            nns,
            "audited_permanent_principal_e8s",
            "TODO_AUDITED_PERMANENT_PRINCIPAL_E8S",
        ),
        (nns, "pooled_parent_memo", "TODO_POOLED_PARENT_MEMO"),
        (
            nns,
            "pooled_parent_followee_id",
            "TODO_POOLED_PARENT_FOLLOWEE_ID",
        ),
    ] {
        require_field_token(text, field, token)?;
    }
    require_field_token(stream, "liquid_icp", "TODO_STREAM_LIQUID_SUBACCOUNT")?;
    require_field_token(
        nns,
        "stream_liquid_account",
        "TODO_STREAM_LIQUID_SUBACCOUNT",
    )?;
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
        validate_pooled_claim_topology(&stream_args, &nns_args)?;
        validate_historian_install_args_did(root, "canisters/io_historian/io_historian.did")
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
    let protected_neuron_id = PROTECTED_IO_NNS_NEURON_ID.to_string();
    require_present(
        "docs/operations/official-sns-testing.md",
        &doc,
        &[
            "IO runs SNS-shaped mock/PocketIC tests, pinned real-canister profiles, and an optional maintained source-built local SNS-W rehearsal.",
            "We do not currently run the official SNS launch locally in required CI.",
            "Official SNS testing is optional and heavier.",
            "current official ICP/DFINITY SNS testing documentation is the source of truth",
            "historical standalone `dfinity/sns-testing` repository is deprecated",
            "The maintained official local SNS flow uses the source-built `sns` CLI",
            "not part of required IO workflows",
            "SNS testflight remains a separately authorized mainnet rehearsal.",
            "IO's canonical IO ledger should be the SNS ledger; any IO_TEST ledger is non-canonical.",
            "NNS Manager execution canister",
            PROTECTED_IO_NEURON_OWNER_CANISTER,
            "two-year protected NNS neuron",
            &protected_neuron_id,
            "are not touched by these tests.",
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
            "[sns_controlled_dapp_upgrade_path_proved]",
            "[official_reward_share_release]",
            "[frontend_sns_integration_tested]",
            "[cycles_management_strategy]",
            "[custom_domain_frontend_plan]",
            "[audit_package]",
        ],
    )?;
    require_present(
        "tools/sns/launch-readiness.toml",
        &checklist,
        &[
            "same-source candidate Governance/Root compatibility",
            "official reviewed SNS Governance release containing the capability",
            "upstream non-blocking tooling defect",
        ],
    )?;
    require_absent(
        "tools/sns/launch-readiness.toml",
        &checklist,
        &[
            "Completion is blocked by candidate-Governance/official-Root ChangeCanisterRequest incompatibility",
            "A reviewed mutually compatible SNS release/bundle is required.",
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
    let gitignore = require_file(root, ".gitignore")?;
    require_present(
        ".gitignore",
        &gitignore,
        &["deploy/local-sns-rehearsal/sns_init.local.yaml"],
    )?;
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
            "symbol: \"IOLO\"",
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
        &["--network ic", PROTECTED_IO_NEURON_OWNER_CANISTER],
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
        "deploy/local-sns-rehearsal/scripts/12-provision-local-nns-readiness.sh",
        "deploy/local-sns-rehearsal/scripts/13-propose-and-finalize-sns.sh",
        "deploy/local-sns-rehearsal/scripts/14-discover-sns-canisters.sh",
        "deploy/local-sns-rehearsal/scripts/15-exercise-ledger.sh",
        "deploy/local-sns-rehearsal/scripts/16-exercise-index-and-archives.sh",
        "deploy/local-sns-rehearsal/scripts/17-exercise-governance-and-controllers.sh",
        "deploy/local-sns-rehearsal/scripts/17-observe-one-day-reward.sh",
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
    let publication_phase = require_file(
        root,
        "deploy/local-sns-rehearsal/scripts/13-propose-and-finalize-sns.sh",
    )?;
    require_present(
        "deploy/local-sns-rehearsal/scripts/13-propose-and-finalize-sns.sh",
        &publication_phase,
        &[
            "publish_sns_wasm_via_nns",
            "sns_governance_source_sha256",
            "sns_root_source_sha256",
            "Governance",
            "Root",
            "get_metadata",
        ],
    )?;
    require_absent(
        "deploy/local-sns-rehearsal/scripts/13-propose-and-finalize-sns.sh",
        &publication_phase,
        &["add-sns-wasm-for-tests"],
    )?;
    let deployment_phase = require_file(
        root,
        "deploy/local-sns-rehearsal/scripts/12-deploy-local-dapps.sh",
    )?;
    require_present(
        "deploy/local-sns-rehearsal/scripts/12-deploy-local-dapps.sh",
        &deployment_phase,
        &[
            "dfx canister id",
            "differs from planned",
            "isolated lifecycle inputs",
            "sns_governance_source_sha256",
            "governance_sed_blob",
        ],
    )?;
    require_absent(
        "deploy/local-sns-rehearsal/scripts/12-deploy-local-dapps.sh",
        &deployment_phase,
        &["--specified-id"],
    )?;
    let provisioning_phase = require_file(
        root,
        "deploy/local-sns-rehearsal/scripts/12-provision-local-nns-readiness.sh",
    )?;
    require_present(
        "deploy/local-sns-rehearsal/scripts/12-provision-local-nns-readiness.sh",
        &provisioning_phase,
        &[
            "icrc1_transfer",
            "claim_or_refresh_neuron_from_account",
            "update_neuron",
            "63115200",
            "auto_stake_maturity = opt false",
            "maturity_disbursements_in_progress = opt vec {}",
            "two_year_neuron_id",
            "pooled_parent_memo",
            "pooled_parent_followee_id",
            "minimum_parent_stake",
        ],
    )?;
    require_absent(
        "deploy/local-sns-rehearsal/scripts/12-provision-local-nns-readiness.sh",
        &provisioning_phase,
        &["10292412127977304661"],
    )?;
    let nns_test_did = require_file(root, "deploy/local-sns-rehearsal/nns-governance-test.did")?;
    require_present(
        "deploy/local-sns-rehearsal/nns-governance-test.did",
        &nns_test_did,
        &["Local sns-testing", "update_neuron", "service :"],
    )?;
    require_present(
        "deploy/local-sns-rehearsal/scripts/14-discover-sns-canisters.sh",
        &require_file(
            root,
            "deploy/local-sns-rehearsal/scripts/14-discover-sns-canisters.sh",
        )?,
        &[
            "ManageNervousSystemParameters",
            "max_number_of_neurons",
            "1_000",
        ],
    )?;
    let local_library = require_file(root, "deploy/local-sns-rehearsal/scripts/lib-local-sns.sh")?;
    require_present(
        "deploy/local-sns-rehearsal/scripts/lib-local-sns.sh",
        &local_library,
        &[
            "nns_function = 30",
            "manage_neuron",
            "get_proposal_info",
            "get_latest_sns_version_pretty",
            "executed_timestamp_seconds",
            "extract_proposal_id",
            "already-published",
            "get_proposal",
            "e8s_to_decimal_tokens",
        ],
    )?;
    require_present(
        "deploy/local-sns-rehearsal/scripts/lib-local-sns.sh",
        &local_library,
        &["https://forum.dfinity.org/t/io-local-rehearsal/0"],
    )?;
    require_absent(
        "deploy/local-sns-rehearsal/scripts/lib-local-sns.sh",
        &local_library,
        &["https://example.invalid"],
    )?;
    let governance_phase = require_file(
        root,
        "deploy/local-sns-rehearsal/scripts/17-exercise-governance-and-controllers.sh",
    )?;
    require_present(
        "deploy/local-sns-rehearsal/scripts/17-exercise-governance-and-controllers.sh",
        &governance_phase,
        &[
            "upgrade-sns-controlled-canister",
            "submit_inline_sns_upgrade",
            "AddGenericNervousSystemFunction",
            "validate_set_paused",
            "ExecuteGenericNervousSystemFunction",
            "sns_governance_source_sha256",
            "sns_root_source_sha256",
            "sns_ledger_source_sha256",
            "sns_index_source_sha256",
            "sns_swap_source_sha256",
        ],
    )?;
    require_absent(
        "deploy/local-sns-rehearsal/scripts/17-exercise-governance-and-controllers.sh",
        &governance_phase,
        &["dfx canister install"],
    )?;
    let exact_release_phase = require_file(
        root,
        "deploy/local-sns-rehearsal/scripts/11-build-local-io-canisters.sh",
    )?;
    require_present(
        "deploy/local-sns-rehearsal/scripts/11-build-local-io-canisters.sh",
        &exact_release_phase,
        &[
            "git -C \"$REPO_ROOT\" diff --quiet",
            "artifact_commit=",
            "git -C \"$REPO_ROOT\" show",
            "tracked_clean=true",
        ],
    )?;
    let packaging_phase = require_file(
        root,
        "deploy/local-sns-rehearsal/scripts/18-package-evidence.sh",
    )?;
    if packaging_phase.contains("corrected pooled claim-backing canonical evidence is missing") {
        require_present(
            "deploy/local-sns-rehearsal/scripts/18-package-evidence.sh",
            &packaging_phase,
            &["record_blocker", "exit 2"],
        )?;
    } else {
        require_present(
            "deploy/local-sns-rehearsal/scripts/18-package-evidence.sh",
            &packaging_phase,
            &[
                "mktemp -d",
                "validate_local_sns_evidence_package",
                "current-canonical.toml",
                "mv \"$selector_temporary\" \"$selector_path\"",
                "preceding selector restored and candidate removed",
                "after_module_sha256",
                "proposal_adopted = true",
                "proposal_executed = true",
            ],
        )?;
    }
    require_present(
        "deploy/local-sns-rehearsal/scripts/17-observe-one-day-reward.sh",
        &require_file(
            root,
            "deploy/local-sns-rehearsal/scripts/17-observe-one-day-reward.sh",
        )?,
        &[
            "IO_LOCAL_REWARD_ADVANCE_SECONDS=86400",
            "IncreaseDissolveDelay",
            "DissolveDelaySeconds = 1209600",
            "resume_reward_work",
            "ProposalBearing",
            "processed_reward_event_count: 1",
            "accumulated_policy_credit: 1000000000000000000",
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
token_symbol = "IOLO"
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
token_symbol = "IOLO"
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
token_symbol = "IOLO"
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
must_not_touch_io_nns_neuron_id = "10292412127977304661"
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
            "ryjl3-tyaaa-aaaaa-aaaba-cai",
            "qhbym-qaaaa-aaaaa-aaafq-cai",
            "rrkah-fqaaa-aaaaa-aaaaq-cai",
        ],
    )?;

    let evidence_path = temp_rehearsal.join("canister-ids.local.toml");
    if evidence_path.exists() {
        fs::remove_file(&evidence_path)
            .map_err(|err| format!("{}: {err}", evidence_path.display()))?;
    }
    run_rehearsal_script(&runbook, &["record-ids"], &xtask, true)?;
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
    validate_protected_reminders(
        path,
        doc,
        PROTECTED_IO_NEURON_OWNER_CANISTER,
        PROTECTED_IO_NNS_NEURON_ID,
    )?;
    validate_no_forbidden_local_ids(path, text, doc, PROTECTED_IO_NNS_NEURON_ID)?;
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
    if evidence.expected.token_symbol != "IOLO" || evidence.ledger.token_symbol != "IOLO" {
        return Err(format!(
            "{path}: local SNS rehearsal token symbol must be IOLO"
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

fn validate_protected_reminders(
    path: &str,
    doc: &SimpleTomlDocument,
    expected_owner: &str,
    expected_neuron_id: u64,
) -> Result<(), String> {
    let canister = require_simple_string(
        path,
        doc,
        "protected",
        "must_not_touch_neuron_owner_canister",
    )?;
    if canister != expected_owner {
        return Err(format!(
            "{path}: protected.must_not_touch_neuron_owner_canister must remain {expected_owner}"
        ));
    }
    let neuron = require_simple_string(path, doc, "protected", "must_not_touch_io_nns_neuron_id")?;
    if neuron != expected_neuron_id.to_string() {
        return Err(format!(
            "{path}: protected.must_not_touch_io_nns_neuron_id must remain {}",
            expected_neuron_id
        ));
    }
    Ok(())
}

fn validate_no_forbidden_local_ids(
    path: &str,
    text: &str,
    doc: &SimpleTomlDocument,
    recorded_protected_neuron_id: u64,
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
            if value == &recorded_protected_neuron_id.to_string()
                || value == &PROTECTED_IO_NNS_NEURON_ID.to_string()
            {
                return Err(format!(
                    "{path}: {section}.{key} must not reference protected IO neuron {}",
                    value
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
    for obsolete in [
        concat!("production-", "redemption-v1"),
        concat!("reward_backing", "_neuron_id"),
        "252460800",
        concat!("seeded", "_principal"),
    ] {
        if text.contains(obsolete) {
            return Err(format!(
                "{path}: obsolete pre-pool rehearsal evidence is not current authority ({obsolete}); corrected pooled-claim-backing rehearsal evidence missing"
            ));
        }
    }
    Err(format!(
        "{path}: corrected pooled-claim-backing rehearsal evidence missing; no completed current schema is authorized"
    ))
}

fn validate_production_redemption_evidence(
    path: &str,
    text: &str,
    expected_protected_owner: &str,
    expected_protected_neuron_id: u64,
) -> Result<(), String> {
    let doc = parse_simple_toml_document(path, text)?;
    if require_simple_string(path, &doc, "evidence", "schema")? != "production-redemption-v1"
        || require_simple_string(path, &doc, "evidence", "network")? != "local"
        || require_simple_string(path, &doc, "evidence", "source")?
            != "official-local-sns-rehearsal"
        || !require_simple_bool(path, &doc, "evidence", "complete")?
        || require_simple_bool(path, &doc, "evidence", "io_protocol_live")?
    {
        return Err(format!(
            "{path}: invalid completed local production-redemption evidence mode"
        ));
    }
    let commit = require_simple_string(path, &doc, "provenance", "official_ic_source_commit")?;
    if commit.len() != 40 || !commit.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!(
            "{path}: official IC source commit must be exact 40-hex"
        ));
    }
    let payload_key = if doc
        .get("provenance")
        .is_some_and(|section| section.contains_key("historian_payload_wasm_sha256"))
    {
        "historian_payload_wasm_sha256"
    } else {
        "historian_payload_gzip_sha256"
    };
    for key in [
        "sns_governance_raw_sha256",
        "sns_root_raw_sha256",
        "historian_before_module_sha256",
        payload_key,
        "historian_release_raw_sha256",
    ] {
        let value = require_simple_string(path, &doc, "provenance", key)?;
        if value.len() != 64
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return Err(format!(
                "{path}: provenance.{key} must be exact lowercase SHA-256"
            ));
        }
    }
    let mut principals = BTreeSet::new();
    for (section, key) in [
        ("sns_canisters", "root"),
        ("sns_canisters", "governance"),
        ("sns_canisters", "ledger"),
        ("sns_canisters", "index"),
        ("sns_canisters", "swap"),
        ("io_dapp_canisters", "io_stream_manager"),
        ("io_dapp_canisters", "io_nns_neuron_manager"),
        ("io_dapp_canisters", "io_historian"),
        ("io_dapp_canisters", "frontend"),
    ] {
        let principal = parse_required_principal(path, &doc, section, key)?;
        if !principals.insert(principal) {
            return Err(format!(
                "{path}: duplicate SNS/dapp principal in {section}.{key}"
            ));
        }
    }
    let fee = require_simple_u128(path, &doc, "ledger", "transaction_fee_e8s")?;
    let funded = require_simple_u128(path, &doc, "ledger", "reserve_funding_e8s")?;
    let redeemed = require_simple_u128(path, &doc, "ledger", "redemption_io_e8s")?;
    let reserve = require_simple_u128(path, &doc, "ledger", "final_reserve_balance_e8s")?;
    if fee != 10_000 || funded == 0 || redeemed == 0 || reserve != funded + redeemed {
        return Err(format!(
            "{path}: canonical fee/reserve/redemption identity failed"
        ));
    }
    for key in [
        "bad_fee_observed",
        "insufficient_funds_observed",
        "duplicate_observed",
        "identical_redemption_replay_observed",
        "index_histories_observed",
    ] {
        if !require_simple_bool(path, &doc, "ledger", key)? {
            return Err(format!("{path}: ledger.{key} must be true"));
        }
    }
    if require_simple_u64(path, &doc, "ledger", "approval_block")? == 0
        || require_simple_u64(path, &doc, "ledger", "io_redemption_block")? == 0
        || require_simple_u64(path, &doc, "ledger", "icp_payout_block")? == 0
    {
        return Err(format!(
            "{path}: canonical redemption block indexes must be nonzero"
        ));
    }
    for key in [
        "create_sns",
        "module_upgrade",
        "stream_function_registration",
        "stream_activation",
        "nns_function_registration",
        "nns_activation",
        "reward_motion",
    ] {
        if require_simple_u64(path, &doc, "proposals", key)? == 0 {
            return Err(format!("{path}: proposals.{key} must be nonzero"));
        }
    }
    if !require_simple_bool(path, &doc, "readiness", "stream_ready")?
        || !require_simple_bool(path, &doc, "readiness", "nns_manager_ready")?
        || !require_simple_bool(path, &doc, "readiness", "two_week_baseline_reconciled")?
        || require_simple_u128(path, &doc, "readiness", "jupiter_staging_e8s")? < 20_000
        || require_simple_u128(path, &doc, "readiness", "two_week_staging_e8s")? < 10_000
        || require_simple_u64(path, &doc, "readiness", "reward_backing_neuron_id")? == 0
        || require_simple_u64(path, &doc, "readiness", "two_year_neuron_id")? == 0
    {
        return Err(format!(
            "{path}: canonical local readiness evidence is incomplete"
        ));
    }
    if require_simple_string(path, &doc, "archive", "ledger_observation")? != "none"
        || require_simple_string(path, &doc, "archive", "root_observation")? != "none"
        || require_simple_string(path, &doc, "reward", "classification")? != "ProposalBearing"
        || require_simple_u64(path, &doc, "reward", "processed_count")? != 1
        || require_simple_u128(path, &doc, "reward", "policy_credit")? != 1_000_000_000_000_000_000
    {
        return Err(format!("{path}: archive or daily reward evidence mismatch"));
    }
    validate_protected_reminders(
        path,
        &doc,
        expected_protected_owner,
        expected_protected_neuron_id,
    )?;
    validate_no_forbidden_local_ids(path, text, &doc, expected_protected_neuron_id)?;
    Ok(())
}

fn check_local_sns_committed_evidence_at(root: &Path) -> Result<(), String> {
    let evidence_root = root.join("deploy/local-sns-rehearsal/evidence");
    if !evidence_root.exists() {
        return Err(format!(
            "{CURRENT_CANONICAL_SELECTOR}: required selector is missing"
        ));
    }
    let selector = read_current_canonical_selector(root)?;
    let mut entries = fs::read_dir(&evidence_root)
        .map_err(|err| format!("{}: {err}", evidence_root.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("{}: {err}", evidence_root.display()))?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in &entries {
        let entry_type = entry
            .file_type()
            .map_err(|err| format!("{}: {err}", entry.path().display()))?;
        if entry.file_name() == "current-canonical.toml" {
            if entry_type.is_symlink() || !entry_type.is_file() {
                return Err(format!(
                    "{}: selector must be a regular non-symlink file",
                    entry.path().display()
                ));
            }
        } else if !entry_type.is_dir() {
            return Err(format!(
                "{}: evidence root entries must be the exact selector or regular package directories",
                entry.path().display()
            ));
        }
    }
    let mut selected_found = 0_usize;
    for entry in entries {
        if entry.file_name() == "current-canonical.toml" {
            continue;
        }
        let rel = entry
            .path()
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let selected_current = entry.file_name().to_string_lossy() == selector.package;
        let validated = validate_local_sns_evidence_package_at(root, &rel, selected_current)?;
        if selected_current {
            selected_found += 1;
            validate_current_selector_binding(root, &rel, &validated, &selector)?;
        }
    }
    if selected_found != 1 {
        return Err(format!(
            "{CURRENT_CANONICAL_SELECTOR}: selected package {:?} was not encountered exactly once",
            selector.package
        ));
    }
    Ok(())
}

fn validate_local_sns_evidence_package_at(
    root: &Path,
    rel: &str,
    selected_current: bool,
) -> Result<ValidatedEvidencePackage, String> {
    let package_dir = if Path::new(rel).is_absolute() {
        PathBuf::from(rel)
    } else {
        root.join(rel)
    };
    let metadata = fs::symlink_metadata(&package_dir)
        .map_err(|err| format!("{}: {err}", package_dir.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(format!(
            "{}: evidence package must be a regular non-symlink directory",
            package_dir.display()
        ));
    }
    let package_files = list_regular_files_relative(&package_dir)?;
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
    let monitoring = doc
        .get("provenance")
        .and_then(|section| section.get("monitoring"))
        == Some(&SimpleTomlValue::Bool(true));
    let canonical_economics = doc
        .get("provenance")
        .and_then(|section| section.get("canonical_redemption_economics"))
        == Some(&SimpleTomlValue::Bool(true));
    if selected_current && (!complete || !monitoring || !canonical_economics) {
        return Err(format!(
                "{manifest_path}: selected current package must be complete, monitoring, and canonical-redemption-economics evidence"
            ));
    }
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
        let mut files = [
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
        .collect::<BTreeSet<_>>();
        if monitoring {
            files.insert("release-evidence.toml".into());
            files.insert("historian-dashboard.log".into());
        }
        if canonical_economics {
            for file in [
                "stream-install-args.did",
                "nns-manager-install-args.did",
                "historian-observation-config.did",
                "account-map.toml",
                "redemption-economics.toml",
                "treasury-account-history.log",
                "archive-observation.log",
            ] {
                files.insert(file.into());
            }
        }
        files
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
    validate_evidence_package_sha256s(root, rel, &package_files)?;
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
        if monitoring {
            validate_monitoring_evidence(root, rel, &doc, selected_current)?;
        }
        if canonical_economics {
            if !monitoring {
                return Err(format!(
                        "{manifest_path}: canonical redemption economics must be release-bound monitoring evidence"
                    ));
            }
            validate_canonical_redemption_evidence(root, rel)?;
        }
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
    Ok(ValidatedEvidencePackage {
        complete,
        monitoring,
        canonical_economics,
        io_release_source_commit: if monitoring {
            Some(require_simple_string(
                &manifest_path,
                &doc,
                "provenance",
                "io_release_source_commit",
            )?)
        } else {
            None
        },
        io_artifact_recording_commit: if monitoring {
            Some(require_simple_string(
                &manifest_path,
                &doc,
                "provenance",
                "io_artifact_recording_commit",
            )?)
        } else {
            None
        },
    })
}

fn validate_current_selector_binding(
    root: &Path,
    package: &str,
    validated: &ValidatedEvidencePackage,
    selector: &CurrentCanonicalSelector,
) -> Result<(), String> {
    if !validated.complete || !validated.monitoring || !validated.canonical_economics {
        return Err(format!(
            "{package}: selected current package is not complete monitoring canonical evidence"
        ));
    }
    if validated.io_release_source_commit.as_deref()
        != Some(selector.io_release_source_commit.as_str())
    {
        return Err(format!(
            "{CURRENT_CANONICAL_SELECTOR}: selected package release source commit mismatch"
        ));
    }
    if validated.io_artifact_recording_commit.as_deref()
        != Some(selector.io_artifact_recording_commit.as_str())
    {
        return Err(format!(
            "{CURRENT_CANONICAL_SELECTOR}: selected package artifact-recording commit mismatch"
        ));
    }
    for (field, path, expected) in [
        (
            "release_manifest_sha256",
            MANIFEST_PATH.to_string(),
            selector.release_manifest_sha256.as_str(),
        ),
        (
            "package_manifest_sha256",
            format!("{package}/manifest.toml"),
            selector.package_manifest_sha256.as_str(),
        ),
        (
            "package_sha256s_sha256",
            format!("{package}/SHA256SUMS"),
            selector.package_sha256s_sha256.as_str(),
        ),
    ] {
        let bytes = fs::read(root.join(&path)).map_err(|err| format!("{path}: {err}"))?;
        if hex_sha256(&bytes) != expected {
            return Err(format!(
                "{CURRENT_CANONICAL_SELECTOR}: current.{field} does not match {path}"
            ));
        }
    }
    Ok(())
}

fn debug_some_u128(path: &str, text: &str, field: &str) -> Result<u128, String> {
    let marker = format!("{field}: Some(");
    let start = text
        .find(&marker)
        .ok_or_else(|| format!("{path}: missing {marker}"))?
        + marker.len();
    let value = text[start..]
        .chars()
        .skip_while(|character| character.is_ascii_whitespace())
        .take_while(|character| character.is_ascii_digit() || *character == '_')
        .filter(|character| *character != '_')
        .collect::<String>();
    if value.is_empty() {
        return Err(format!(
            "{path}: {field} does not contain a numeric Some value"
        ));
    }
    value
        .parse::<u128>()
        .map_err(|error| format!("{path}: invalid {field}: {error}"))
}

fn checked_add(path: &str, left: u128, right: u128, identity: &str) -> Result<u128, String> {
    left.checked_add(right)
        .ok_or_else(|| format!("{path}: overflow checking {identity}"))
}

fn checked_sub(path: &str, left: u128, right: u128, identity: &str) -> Result<u128, String> {
    left.checked_sub(right)
        .ok_or_else(|| format!("{path}: underflow checking {identity}"))
}

fn validate_canonical_redemption_evidence(root: &Path, package: &str) -> Result<(), String> {
    let ids_path = format!("{package}/canister-ids.local.toml");
    let ids_text = require_file(root, &ids_path)?;
    let ids = parse_simple_toml_document(&ids_path, &ids_text)?;
    let governance = require_simple_string(&ids_path, &ids, "sns_canisters", "governance")?;

    let map_path = format!("{package}/account-map.toml");
    let map_text = require_file(root, &map_path)?;
    let map = parse_simple_toml_document(&map_path, &map_text)?;
    let excluded_sections = map
        .keys()
        .filter(|section| section.starts_with("excluded_"))
        .collect::<Vec<_>>();
    if excluded_sections.len() != 1 || excluded_sections[0].as_str() != "excluded_sns_treasury" {
        return Err(format!(
            "{map_path}: exact fixture excluded set must contain only excluded_sns_treasury"
        ));
    }
    if require_simple_string(&map_path, &map, "excluded_sns_treasury", "name")? != "sns-treasury"
        || require_simple_string(&map_path, &map, "excluded_sns_treasury", "owner")? != governance
        || require_simple_u64(
            &map_path,
            &map,
            "excluded_sns_treasury",
            "distribution_nonce",
        )? != 0
        || require_simple_string(&map_path, &map, "excluded_sns_treasury", "domain")?
            != "token-distribution"
        || !require_simple_bool(&map_path, &map, "excluded_sns_treasury", "expected_nonzero")?
    {
        return Err(format!(
            "{map_path}: canonical SNS treasury metadata mismatch"
        ));
    }
    let governance_principal = Principal::from_text(&governance)
        .map_err(|error| format!("{map_path}: invalid Governance principal: {error}"))?;
    let treasury_subaccount =
        require_simple_string(&map_path, &map, "excluded_sns_treasury", "subaccount_hex")?;
    let derived = sns_distribution_subaccount(governance_principal, 0);
    if treasury_subaccount != derived {
        return Err(format!(
            "{map_path}: SNS treasury subaccount {treasury_subaccount} does not match canonical derivation {derived}"
        ));
    }
    let treasury_blob = candid_blob_literal_from_hex(&treasury_subaccount)?;
    let exact_account =
        format!("owner = principal \"{governance}\"; subaccount = opt blob \"{treasury_blob}\"");

    let stream_path = format!("{package}/stream-install-args.did");
    let stream = require_file(root, &stream_path)?;
    if stream.matches("excluded_io_accounts =").count() != 1
        || stream.matches(&exact_account).count() != 1
        || stream.contains(&format!(
            "excluded_io_accounts = vec {{ record {{ owner = principal \"{governance}\"; subaccount = null"
        ))
    {
        return Err(format!(
            "{stream_path}: Stream excluded set is not the one exact canonical SNS treasury Account"
        ));
    }
    let historian_path = format!("{package}/historian-observation-config.did");
    let historian_config = require_file(root, &historian_path)?;
    if historian_config.matches("name = \"sns-treasury\"").count() != 2
        || historian_config.matches(&exact_account).count() != 2
        || historian_config.contains("name = \"sns-governance\"")
    {
        return Err(format!(
            "{historian_path}: historian excluded/history Accounts do not equal Stream canonical excluded Account"
        ));
    }

    let nns_path = format!("{package}/nns-manager-install-args.did");
    let nns = require_file(root, &nns_path)?;
    for (section, config_path, config) in [
        ("jupiter_io", stream_path.as_str(), stream.as_str()),
        ("jupiter_icp_staging", nns_path.as_str(), nns.as_str()),
        ("two_week_maturity_staging", nns_path.as_str(), nns.as_str()),
        ("liquid_icp_reserve", nns_path.as_str(), nns.as_str()),
    ] {
        let owner = require_simple_string(&map_path, &map, section, "owner")?;
        if !config.contains(&format!("owner = principal \"{owner}\"")) {
            return Err(format!(
                "{config_path}: missing Account-map owner for {section}"
            ));
        }
        if let Ok(subaccount) = require_simple_string(&map_path, &map, section, "subaccount_hex") {
            let blob = candid_blob_literal_from_hex(&subaccount)?;
            if !config.contains(&format!("subaccount = opt blob \"{blob}\"")) {
                return Err(format!(
                    "{config_path}: missing Account-map subaccount for {section}"
                ));
            }
        }
    }

    let economics_path = format!("{package}/redemption-economics.toml");
    let economics_text = require_file(root, &economics_path)?;
    let economics = parse_simple_toml_document(&economics_path, &economics_text)?;
    let total = require_simple_u128(
        &economics_path,
        &economics,
        "snapshot",
        "total_io_supply_e8s",
    )?;
    let reserve = require_simple_u128(
        &economics_path,
        &economics,
        "snapshot",
        "protocol_reserve_io_e8s",
    )?;
    let excluded = require_simple_u128(
        &economics_path,
        &economics,
        "excluded_sns_treasury",
        "balance_e8s",
    )?;
    if excluded == 0
        || require_simple_string(&economics_path, &economics, "excluded_sns_treasury", "name")?
            != "sns-treasury"
        || require_simple_string(
            &economics_path,
            &economics,
            "excluded_sns_treasury",
            "owner",
        )? != governance
        || require_simple_string(
            &economics_path,
            &economics,
            "excluded_sns_treasury",
            "subaccount_hex",
        )? != treasury_subaccount
        || !require_simple_bool(
            &economics_path,
            &economics,
            "excluded_sns_treasury",
            "expected_nonzero",
        )?
    {
        return Err(format!(
            "{economics_path}: missing, zero, or mismatched excluded Account evidence"
        ));
    }
    let liquid = require_simple_u128(
        &economics_path,
        &economics,
        "snapshot",
        "liquid_icp_reserve_e8s",
    )?;
    let redeemed = require_simple_u128(
        &economics_path,
        &economics,
        "snapshot",
        "redemption_io_amount_e8s",
    )?;
    let io_fee = require_simple_u128(&economics_path, &economics, "snapshot", "io_fee_e8s")?;
    let icp_fee = require_simple_u128(&economics_path, &economics, "snapshot", "icp_fee_e8s")?;
    let calculated =
        calculate_redemption_economics(total, reserve, &[excluded], liquid, redeemed, icp_fee)?;
    for (field, observed, expected) in [
        (
            "excluded_io_total_e8s",
            require_simple_u128(
                &economics_path,
                &economics,
                "snapshot",
                "excluded_io_total_e8s",
            )?,
            calculated.excluded_total_e8s,
        ),
        (
            "redeemable_io_supply_e8s",
            require_simple_u128(
                &economics_path,
                &economics,
                "snapshot",
                "redeemable_io_supply_e8s",
            )?,
            calculated.redeemable_supply_e8s,
        ),
        (
            "quoted_gross_icp_e8s",
            require_simple_u128(
                &economics_path,
                &economics,
                "snapshot",
                "quoted_gross_icp_e8s",
            )?,
            calculated.gross_icp_e8s,
        ),
        (
            "quoted_net_icp_e8s",
            require_simple_u128(
                &economics_path,
                &economics,
                "snapshot",
                "quoted_net_icp_e8s",
            )?,
            calculated.net_icp_e8s,
        ),
    ] {
        if observed != expected {
            return Err(format!(
                "{economics_path}: {field}={observed} does not equal checked calculation {expected}"
            ));
        }
    }
    if require_simple_u128(
        &economics_path,
        &economics,
        "stream_result",
        "gross_icp_e8s",
    )? != calculated.gross_icp_e8s
        || require_simple_u128(&economics_path, &economics, "stream_result", "net_icp_e8s")?
            != calculated.net_icp_e8s
        || !require_simple_bool(
            &economics_path,
            &economics,
            "stream_result",
            "identical_replay",
        )?
    {
        return Err(format!(
            "{economics_path}: Stream result or identical replay does not match checked quote"
        ));
    }

    let balances = |key| require_simple_u128(&economics_path, &economics, "ledger_balances", key);
    let post_total = balances("io_total_after_e8s")?;
    let post_reserve = balances("protocol_reserve_after_e8s")?;
    let post_excluded = balances("excluded_after_e8s")?;
    let post_liquid = balances("liquid_icp_after_e8s")?;
    if balances("io_total_before_e8s")? != total
        || balances("protocol_reserve_before_e8s")? != reserve
        || balances("excluded_before_e8s")? != excluded
        || balances("liquid_icp_before_e8s")? != liquid
        || post_total != checked_sub(&economics_path, total, io_fee, "IO supply fee burn")?
        || post_reserve != checked_add(&economics_path, reserve, redeemed, "reserve IO pull")?
        || post_excluded != excluded
        || post_liquid
            != checked_sub(
                &economics_path,
                liquid,
                calculated.gross_icp_e8s,
                "liquid ICP payout",
            )?
        || balances("user_io_after_e8s")?
            != checked_sub(
                &economics_path,
                checked_sub(
                    &economics_path,
                    balances("user_io_before_e8s")?,
                    redeemed,
                    "user redeemed IO",
                )?,
                io_fee,
                "user IO fee",
            )?
        || balances("user_icp_after_e8s")?
            != checked_add(
                &economics_path,
                balances("user_icp_before_e8s")?,
                calculated.net_icp_e8s,
                "user ICP receipt",
            )?
    {
        return Err(format!(
            "{economics_path}: ledger balance changes do not match redemption and fee identities"
        ));
    }

    let reserve_path = format!("{package}/reserve-funding-evidence.toml");
    let reserve_text = require_file(root, &reserve_path)?;
    let reserve_doc = parse_simple_toml_document(&reserve_path, &reserve_text)?;
    let funding = require_simple_u128(
        &reserve_path,
        &reserve_doc,
        "reserve",
        "treasury_transfer_amount_e8s",
    )?;
    let fee = require_simple_u128(&reserve_path, &reserve_doc, "reserve", "transfer_fee_e8s")?;
    let before = require_simple_u128(
        &reserve_path,
        &reserve_doc,
        "reserve",
        "treasury_balance_before_e8s",
    )?;
    let after_reserve = require_simple_u128(
        &reserve_path,
        &reserve_doc,
        "reserve",
        "treasury_balance_after_reserve_e8s",
    )?;
    let after_user = require_simple_u128(
        &reserve_path,
        &reserve_doc,
        "reserve",
        "treasury_balance_after_user_e8s",
    )?;
    let user_funding = require_simple_u128(&ids_path, &ids, "ledger", "user_funding_e8s")?;
    if before == 0
        || after_reserve
            != checked_sub(
                &reserve_path,
                checked_sub(&reserve_path, before, funding, "treasury reserve funding")?,
                fee,
                "treasury reserve-funding fee",
            )?
        || after_user
            != checked_sub(
                &reserve_path,
                checked_sub(
                    &reserve_path,
                    after_reserve,
                    user_funding,
                    "treasury user funding",
                )?,
                fee,
                "treasury user-funding fee",
            )?
        || require_simple_string(&reserve_path, &reserve_doc, "reserve", "treasury_name")?
            != "sns-treasury"
        || require_simple_string(&reserve_path, &reserve_doc, "reserve", "treasury_owner")?
            != governance
        || require_simple_string(
            &reserve_path,
            &reserve_doc,
            "reserve",
            "treasury_subaccount_hex",
        )? != treasury_subaccount
    {
        return Err(format!(
            "{reserve_path}: treasury funding balances or canonical Account mismatch"
        ));
    }

    let history_path = format!("{package}/treasury-account-history.log");
    let history = require_file(root, &history_path)?;
    let reserve_record = unique_index_transfer_record(&history, funding, "00000000000005dd")?;
    let user_record = unique_index_transfer_record(&history, user_funding, "00000000000005de")?;
    for (label, record, block_key) in [
        ("reserve", reserve_record, "transfer_block"),
        ("user", user_record, "user_funding_transfer_block"),
    ] {
        if !record.contains(&format!("owner = principal \"{governance}\""))
            || !record.contains(&format!("subaccount = opt blob \"{treasury_blob}\""))
        {
            return Err(format!(
                "{history_path}: {label} treasury transfer source is not the canonical SNS treasury Account"
            ));
        }
        let block = candid_nat_field(record, "id")?;
        if block != require_simple_u128(&reserve_path, &reserve_doc, "reserve", block_key)? {
            return Err(format!(
                "{reserve_path}: {label} treasury transfer block does not match account history"
            ));
        }
    }

    let ledger_path = format!("{package}/ledger-evidence.toml");
    let ledger_text = require_file(root, &ledger_path)?;
    let ledger = parse_simple_toml_document(&ledger_path, &ledger_text)?;
    for (key, expected) in [
        ("io_amount_e8s", redeemed),
        ("gross_icp_e8s", calculated.gross_icp_e8s),
        ("net_icp_e8s", calculated.net_icp_e8s),
        ("excluded_io_total_e8s", excluded),
        ("redeemable_io_supply_e8s", calculated.redeemable_supply_e8s),
    ] {
        if require_simple_u128(&ledger_path, &ledger, "ledger", key)? != expected {
            return Err(format!("{ledger_path}: ledger.{key} mismatch"));
        }
    }
    let index_synced = require_simple_u128(&ledger_path, &ledger, "ledger", "index_synced_blocks")?;
    let redemption_block =
        require_simple_u128(&ledger_path, &ledger, "ledger", "redemption_io_block")?;
    if index_synced <= redemption_block {
        return Err(format!(
            "{ledger_path}: index has not synchronized through the redemption block"
        ));
    }

    let archive_path = format!("{package}/archive-evidence.toml");
    let archive_text = require_file(root, &archive_path)?;
    let archive = parse_simple_toml_document(&archive_path, &archive_text)?;
    if require_simple_u128(&archive_path, &archive, "archive", "ledger_archive_count")? != 0
        || require_simple_u128(&archive_path, &archive, "archive", "root_archive_count")? != 0
        || require_simple_string(&archive_path, &archive, "archive", "ledger_observation")?
            != "none"
        || require_simple_string(&archive_path, &archive, "archive", "root_observation")? != "none"
        || !require_simple_bool(&archive_path, &archive, "archive", "observation_consistent")?
    {
        return Err(format!("{archive_path}: explicit archive result mismatch"));
    }
    let archive_log_path = format!("{package}/archive-observation.log");
    let archive_log = require_file(root, &archive_log_path)?;
    for required in [
        "icrc3_get_archives",
        "(vec {})",
        "list_sns_canisters",
        "archives = vec {};",
    ] {
        if !archive_log.contains(required) {
            return Err(format!(
                "{archive_log_path}: missing exact archive observation {required:?}"
            ));
        }
    }

    let dashboard_path = format!("{package}/historian-dashboard.log");
    let dashboard = require_file(root, &dashboard_path)?;
    let historian_total = debug_some_u128(&dashboard_path, &dashboard, "total_io_supply_e8s")?;
    let historian_reserve =
        debug_some_u128(&dashboard_path, &dashboard, "protocol_reserve_io_e8s")?;
    let historian_excluded = debug_some_u128(&dashboard_path, &dashboard, "excluded_io_e8s")?;
    let historian_redeemable =
        debug_some_u128(&dashboard_path, &dashboard, "redeemable_io_supply_e8s")?;
    let historian_liquid = debug_some_u128(&dashboard_path, &dashboard, "liquid_icp_reserve_e8s")?;
    if historian_total != post_total
        || historian_reserve != post_reserve
        || historian_excluded != post_excluded
        || historian_liquid != post_liquid
    {
        return Err(format!(
            "{dashboard_path}: historian monetary snapshot does not match post-redemption ledgers"
        ));
    }
    let historian_calculated = calculate_redemption_economics(
        historian_total,
        historian_reserve,
        &[historian_excluded],
        historian_liquid,
        redeemed,
        icp_fee,
    )?;
    if historian_redeemable != historian_calculated.redeemable_supply_e8s
        || historian_calculated.gross_icp_e8s != calculated.gross_icp_e8s
        || historian_calculated.net_icp_e8s != calculated.net_icp_e8s
    {
        return Err(format!(
            "{dashboard_path}: historian rate is inconsistent with the Stream quote snapshot"
        ));
    }
    Ok(())
}

fn protected_identity_at_source(root: &Path, source_commit: &str) -> Result<(String, u64), String> {
    const SOURCE_PATH: &str = "crates/io_production_wiring/src/lib.rs";
    let output = Command::new("git")
        .current_dir(root)
        .args(["show", &format!("{source_commit}:{SOURCE_PATH}")])
        .output()
        .map_err(|err| format!("git show {source_commit}:{SOURCE_PATH}: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "{source_commit}:{SOURCE_PATH}: protected-identity source is unavailable"
        ));
    }
    let source = String::from_utf8(output.stdout)
        .map_err(|err| format!("{source_commit}:{SOURCE_PATH}: non-UTF-8 source: {err}"))?;
    let owner_prefix = "pub const PROTECTED_IO_NEURON_OWNER_CANISTER: &str = \"";
    let owner = source
        .lines()
        .find_map(|line| {
            line.trim()
                .strip_prefix(owner_prefix)
                .and_then(|value| value.strip_suffix("\";"))
        })
        .ok_or_else(|| {
            format!("{source_commit}:{SOURCE_PATH}: protected owner constant is missing")
        })?
        .to_string();
    let neuron_prefix = "pub const PROTECTED_IO_NNS_NEURON_ID: u64 = ";
    let neuron = source
        .lines()
        .find_map(|line| {
            line.trim()
                .strip_prefix(neuron_prefix)
                .and_then(|value| value.strip_suffix(';'))
        })
        .ok_or_else(|| {
            format!("{source_commit}:{SOURCE_PATH}: protected neuron constant is missing")
        })?
        .replace('_', "")
        .parse::<u64>()
        .map_err(|err| {
            format!("{source_commit}:{SOURCE_PATH}: invalid protected neuron constant: {err}")
        })?;
    Ok((owner, neuron))
}

fn validate_monitoring_evidence(
    root: &Path,
    package: &str,
    package_manifest: &SimpleTomlDocument,
    selected_current: bool,
) -> Result<(), String> {
    let source_commit = require_simple_string(
        &format!("{package}/manifest.toml"),
        package_manifest,
        "provenance",
        "io_release_source_commit",
    )?;
    let artifact_commit = require_simple_string(
        &format!("{package}/manifest.toml"),
        package_manifest,
        "provenance",
        "io_artifact_recording_commit",
    )?;
    validate_release_source_ancestor(root, &source_commit)?;
    validate_release_source_ancestor(root, &artifact_commit)?;

    let recorded_manifest = Command::new("git")
        .current_dir(root)
        .args(["show", &format!("{artifact_commit}:{MANIFEST_PATH}")])
        .output()
        .map_err(|err| format!("git show monitoring artifact manifest: {err}"))?;
    if !recorded_manifest.status.success() {
        return Err(format!(
            "{package}: artifact-recording commit does not contain a release manifest"
        ));
    }
    let evidence_manifest: ArtifactManifest = serde_json::from_slice(&recorded_manifest.stdout)
        .map_err(|error| format!("{package}: recorded release manifest is invalid: {error}"))?;
    if evidence_manifest.git_commit.as_deref() != Some(&source_commit) {
        return Err(format!(
            "{package}: artifact-recording manifest source does not match package source commit"
        ));
    }
    if selected_current {
        let current_manifest_text = require_file(root, MANIFEST_PATH)?;
        if recorded_manifest.stdout != current_manifest_text.as_bytes() {
            return Err(format!(
                "{package}: selected current package artifact commit does not contain the exact current release manifest"
            ));
        }
    }

    let release_path = format!("{package}/release-evidence.toml");
    let release_text = require_file(root, &release_path)?;
    let release = parse_simple_toml_document(&release_path, &release_text)?;
    if require_simple_string(&release_path, &release, "release", "source_commit")? != source_commit
        || require_simple_string(
            &release_path,
            &release,
            "release",
            "artifact_recording_commit",
        )? != artifact_commit
        || require_simple_string(&release_path, &release, "release", "manifest_sha256")?
            != hex_sha256(&recorded_manifest.stdout)
    {
        return Err(format!("{release_path}: release identity mismatch"));
    }
    for (canister, section) in [
        ("io_stream_manager", "io_stream_manager"),
        ("io_nns_neuron_manager", "io_nns_neuron_manager"),
        ("io_historian", "io_historian"),
        ("frontend", "io_frontend"),
    ] {
        let expected = evidence_manifest
            .artifacts
            .iter()
            .find(|entry| entry.canister == canister)
            .ok_or_else(|| format!("{MANIFEST_PATH}: missing {canister}"))?;
        if require_simple_string(&release_path, &release, section, "raw_wasm_sha256")?
            != expected.raw_wasm_sha256
            || require_simple_string(&release_path, &release, section, "gzip_wasm_sha256")?
                != expected.gz_wasm_sha256
        {
            return Err(format!(
                "{release_path}: {section} hashes do not match the current manifest"
            ));
        }
    }

    let ids_path = format!("{package}/canister-ids.local.toml");
    let ids_text = require_file(root, &ids_path)?;
    let (recorded_protected_owner, recorded_protected_neuron_id) =
        protected_identity_at_source(root, &source_commit)?;
    validate_production_redemption_evidence(
        &ids_path,
        &ids_text,
        &recorded_protected_owner,
        recorded_protected_neuron_id,
    )?;
    let ids = parse_simple_toml_document(&ids_path, &ids_text)?;
    if selected_current {
        validate_protected_reminders(
            &ids_path,
            &ids,
            PROTECTED_IO_NEURON_OWNER_CANISTER,
            PROTECTED_IO_NNS_NEURON_ID,
        )?;
    }
    if require_simple_string(&ids_path, &ids, "provenance", "io_release_source_commit")?
        != source_commit
        || require_simple_string(
            &ids_path,
            &ids,
            "provenance",
            "io_artifact_recording_commit",
        )? != artifact_commit
        || require_simple_string(
            &ids_path,
            &ids,
            "provenance",
            "historian_release_raw_sha256",
        )? != require_simple_string(&release_path, &release, "io_historian", "raw_wasm_sha256")?
        || require_simple_string(
            &ids_path,
            &ids,
            "provenance",
            "historian_payload_wasm_sha256",
        )? != require_simple_string(&release_path, &release, "io_historian", "raw_wasm_sha256")?
    {
        return Err(format!(
            "{ids_path}: monitoring release provenance is not cross-consistent"
        ));
    }
    let governance_path = format!("{package}/governance-evidence.toml");
    let governance_text = require_file(root, &governance_path)?;
    let governance = parse_simple_toml_document(&governance_path, &governance_text)?;
    let historian_release_raw =
        require_simple_string(&release_path, &release, "io_historian", "raw_wasm_sha256")?;
    let has_complete_upgrade_transition = governance
        .get("upgrade")
        .is_some_and(|section| section.contains_key("after_module_sha256"));
    if selected_current && !has_complete_upgrade_transition {
        return Err(format!(
            "{governance_path}: selected current package must record the complete historian module transition"
        ));
    }
    if has_complete_upgrade_transition {
        let before = require_simple_string(
            &governance_path,
            &governance,
            "upgrade",
            "before_module_sha256",
        )?;
        let payload = require_simple_string(
            &governance_path,
            &governance,
            "upgrade",
            "payload_wasm_sha256",
        )?;
        let after = require_simple_string(
            &governance_path,
            &governance,
            "upgrade",
            "after_module_sha256",
        )?;
        let recorded_release = require_simple_string(
            &governance_path,
            &governance,
            "upgrade",
            "release_raw_sha256",
        )?;
        if before == after
            || payload != historian_release_raw
            || after != historian_release_raw
            || recorded_release != historian_release_raw
            || !require_simple_bool(&governance_path, &governance, "upgrade", "proposal_adopted")?
            || !require_simple_bool(
                &governance_path,
                &governance,
                "upgrade",
                "proposal_executed",
            )?
            || !require_simple_bool(&governance_path, &governance, "upgrade", "executed")?
        {
            return Err(format!(
                "{governance_path}: historian Governance upgrade does not prove an adopted, executed, hash-changing transition to the recorded release"
            ));
        }
    }
    let eligible_credit = require_simple_u128(&ids_path, &ids, "reward", "eligible_credit")?;
    if eligible_credit == 0
        || require_simple_u128(&governance_path, &governance, "reward", "eligible_credit")?
            != eligible_credit
        || require_simple_u128(&governance_path, &governance, "reward", "policy_credit")?
            != require_simple_u128(&ids_path, &ids, "reward", "policy_credit")?
    {
        return Err(format!(
            "{governance_path}: reward totals are not cross-consistent with {ids_path}"
        ));
    }

    let dashboard_path = format!("{package}/historian-dashboard.log");
    let dashboard = require_file(root, &dashboard_path)?;
    require_present(
        &dashboard_path,
        &dashboard,
        &[
            "historian_dashboard=Dashboard",
            "configured: true",
            "freshness: Fresh",
            "module_match: Matching",
            "controllers: Some",
            "total_io_supply_e8s: Some",
            "protocol_reserve_io_e8s: Some",
            "liquid_icp_reserve_e8s: Some",
            "redemption_rate: Some",
            "lifecycle: Ready",
            "two_week_maturity_baseline_reconciled: true",
            "latest_two_week_target: None",
            "nns_governance: Some",
            "build_metadata:",
            "RewardBacking",
            "TwoYearProtected",
            "staked_maturity_e8s:",
            "archive_canisters: []",
            "num_blocks_synced:",
            "transactions:",
        ],
    )?;
    require_debug_some_value(&dashboard_path, &dashboard, "max_number_of_neurons", "1000")?;
    require_debug_some_value(
        &dashboard_path,
        &dashboard,
        "native_initial_reward_rate_basis_points",
        "0",
    )?;
    require_debug_some_value(
        &dashboard_path,
        &dashboard,
        "native_final_reward_rate_basis_points",
        "0",
    )
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
                "10292412127977304661",
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
            "10292412127977304661",
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

fn check_historian_freshness_at(root: &Path) -> Result<(), String> {
    check_did_surface_at(root, false)?;

    let historian_source = [
        "canisters/io_historian/src/lib.rs",
        "canisters/io_historian/src/model.rs",
        "canisters/io_historian/src/adapters.rs",
    ]
    .into_iter()
    .map(|path| require_file(root, path))
    .collect::<Result<Vec<_>, _>>()?
    .join("\n");
    require_present(
        "canisters/io_historian/src/lib.rs",
        &historian_source,
        &[
            "ObservationConfig",
            "ObservationFreshness",
            "SourceHealth",
            "ProtocolSnapshot",
            "CanisterObservation",
            "StreamStatus",
            "NnsManagerStatus",
            "SnsStatus",
            "IndexStatus",
            "Fresh",
            "Stale",
            "Missing",
            "PrelaunchNotConfigured",
            "ErrorRetryable",
            "Unknown",
            "coherent_protocol_snapshot",
            "get_sns_canisters_summary",
            "get_nervous_system_parameters",
            "get_latest_reward_event",
            "get_account_transactions",
            "icrc1_total_supply",
            "icrc1_balance_of",
            "set_timer",
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
    require_absent(
        "historian production sources",
        &historian_source,
        &[
            "pub fn configure",
            "pub fn ingest",
            "frozen_cohort",
            "participation_numerator",
            "scan_blocks",
        ],
    )?;

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
            "PrelaunchNotConfigured",
            "Stale",
            "ErrorRetryable",
            "Missing",
            "never displayed as zero",
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
                "missing/stale/error",
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
            "timestamp of the coherent refresh generation",
            "no newer observations arrive",
        ],
    )?;

    Ok(())
}

fn check_exact_two_week_policy_at(root: &Path) -> Result<(), String> {
    let reward_policy = require_file(root, "crates/io_reward_policy/src/lib.rs")?;
    require_present(
        "daily entitlement allocation policy",
        &reward_policy,
        &[
            "unequal_credits_allocate_a_large_pool_one_to_two_to_three",
            "tiny_pool_has_deterministic_dust_and_conserves_the_pool",
            "zero_eligible_credit_forfeits_the_full_pool",
        ],
    )?;
    let stream_state = require_file(root, "canisters/io_stream_manager/src/state.rs")?;
    require_present(
        "stream-manager bounded entitlement slots",
        &stream_state,
        &[
            "BackingRewardRecord",
            "PendingEntitlementBatch",
            "MAX_ENTRIES",
        ],
    )?;
    let rewards = require_file(root, "canisters/io_stream_manager/src/rewards.rs")?;
    let reward_evidence = require_file(root, "canisters/io_stream_manager/src/reward_evidence.rs")?;
    let backing_registry =
        require_file(root, "canisters/io_stream_manager/src/backing_registry.rs")?;
    require_present(
        "stream-manager daily event and backing separation",
        &format!("{rewards}\n{reward_evidence}\n{backing_registry}"),
        &["event_credits", "apply_credits", "freeze_and_prepare"],
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

fn validate_historian_install_args_did(root: &Path, path: &str) -> Result<(), String> {
    let text = read_file(root, path)?;
    require_present(
        path,
        &text,
        &[
            "type ObservationConfig",
            "service : (opt ObservationConfig)",
        ],
    )?;
    require_absent(path, &text, &[" configure :", " ingest :", " set_config :"])
}

fn run_security_scan(required: bool) -> bool {
    let mode = if required { "required" } else { "permissive" };
    run(
        &format!("security scan: {mode}"),
        script("tools/scripts/security-scan", &[mode]),
    )
}

fn print_known_commands() {
    eprintln!("known: test_all, test_ci, verify_release, simplicity_check, validate_workflows, validate_obsolete_economics_guard, validate_nns_boundary_pin, security_scan, security_scan_required, validate_install_args, validate_production_wiring, validate_historian_freshness, validate_stable_storage, validate_local_sns_rehearsal, validate_local_sns_ledger, validate_local_sns_evidence_package, validate_local_sns_committed_evidence, validate_local_sns_scripts, e2e_coverage_matrix_check, live_stream_manager_pocketic_gate_check, real_canister_harness_check, real_canister_artifact_manifest_check, verify_real_canister_artifacts, fetch_real_canister_artifacts, real_sns_ledger_index_tests, real_sns_ledger_index_required, real_sns_governance_tests, real_sns_governance_required, real_io_e2e_tests, real_io_e2e_required, e2e_real_coverage_check, local_sns_evidence_tests, sns_apy_policy_tests, frontend_setup, frontend_build, frontend_unit, frontend_certified_asset_tests, frontend_required, frontend_all, historian_tests, historian_required, sns_harness_check, sns_config_validate, sns_config_validate_official, sns_launch_readiness_check, sns_governance_read_tests, sns_governance_read_required, sns_ledger_index_tests, sns_ledger_index_required, sns_root_lifecycle_tests, sns_root_lifecycle_required, sns_pocketic_smoke, sns_pocketic_required, test_pocketic_required, preflight, check, fmt_check, did_surface, build_canisters, build_recorded_source, verify_recorded_source, compare_release_artifact_dirs, nns_neuron_staking_subaccount, sns_distribution_subaccount, calculate_redemption_economics, index_transfer_block, verify_artifacts, build_debug_canisters, test_unit, test_pocketic_integration, test_local_integration, test_e2e, stream_manager_unit, nns_neuron_manager_unit, historian_pocketic_integration, stream_manager_pocketic_integration, nns_neuron_manager_pocketic_integration");
}

fn main() -> ExitCode {
    let mut args = env::args().skip(1).collect::<Vec<_>>();
    let cmd = if args.is_empty() {
        "test_all".to_string()
    } else {
        args.remove(0)
    };
    if cmd == "sns_framework" {
        return sns_framework::run(&args);
    }
    let root = PathBuf::from(".");
    let mut ok = true;
    match cmd.as_str() {
        "check" => {
            ok &= run(
                "check: workspace all targets",
                cargo_check(&["--workspace", "--all-targets"]),
            );
        }
        "validate_workflows" => match check_required_workflows_at(&root) {
            Ok(()) => eprintln!("✓ required workflows validate the exact event source SHA"),
            Err(err) => {
                eprintln!("✗ required workflow validation: {err}");
                ok = false;
            }
        },
        "validate_obsolete_economics_guard" => match check_obsolete_economics_guard_at(&root) {
            Ok(()) => eprintln!("✓ validate_obsolete_economics_guard"),
            Err(err) => {
                eprintln!("✗ validate_obsolete_economics_guard: {err}");
                ok = false;
            }
        },
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
        "validate_nns_boundary_pin" => match check_nns_boundary_pin_at(&root) {
            Ok(()) => eprintln!("✓ validate_nns_boundary_pin"),
            Err(err) => {
                eprintln!("✗ validate_nns_boundary_pin: {err}");
                ok = false;
            }
        },
        "build_canisters" => {
            let source_commit = release_source_commit(&root);
            if let Err(err) = &source_commit {
                eprintln!("✗ build_canisters source: {err}");
                ok = false;
            }
            if ok {
                ok &= run_subcommand("frontend_setup");
            }
            if ok {
                match release_source_commit(&root) {
                    Ok(_) => eprintln!("✓ build_canisters source unchanged after frontend setup"),
                    Err(err) => {
                        eprintln!("✗ build_canisters post-frontend source: {err}");
                        ok = false;
                    }
                }
            }
            for canister in RELEASE_CANISTERS {
                if ok {
                    ok &= run(
                        &format!("build canister: {}", canister.package),
                        build_canister(canister.package, RELEASE_PROFILE),
                    );
                }
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
        "build_recorded_source" => match manifest_source_commit(&root) {
            Ok(Some(source_commit)) => {
                ok &= run(
                    "build exact recorded release source",
                    script("tools/scripts/build-release-from-source", &[&source_commit]),
                );
            }
            Ok(None) => {
                eprintln!("✗ build_recorded_source: {MANIFEST_PATH} is missing");
                ok = false;
            }
            Err(err) => {
                eprintln!("✗ build_recorded_source: {err}");
                ok = false;
            }
        },
        "verify_recorded_source" => match manifest_source_commit(&root) {
            Ok(Some(source_commit)) => {
                ok &= run(
                    "verify checked-in artifacts and repeated exact-source builds",
                    script(
                        "tools/scripts/verify-release-from-source",
                        &[&source_commit],
                    ),
                );
            }
            Ok(None) => {
                eprintln!("✗ verify_recorded_source: {MANIFEST_PATH} is missing");
                ok = false;
            }
            Err(err) => {
                eprintln!("✗ verify_recorded_source: {err}");
                ok = false;
            }
        },
        "compare_release_artifact_dirs" => {
            if args.len() != 2 {
                eprintln!("✗ compare_release_artifact_dirs: expected <first-directory> <second-directory>");
                return ExitCode::from(2);
            }
            match compare_release_artifact_dirs(Path::new(&args[0]), Path::new(&args[1])) {
                Ok(()) => eprintln!("✓ compare_release_artifact_dirs"),
                Err(err) => {
                    eprintln!("✗ compare_release_artifact_dirs: {err}");
                    ok = false;
                }
            }
        }
        "nns_neuron_staking_subaccount" => {
            if args.len() != 2 {
                eprintln!(
                    "✗ nns_neuron_staking_subaccount: expected <controller-principal> <nonce>"
                );
                return ExitCode::from(2);
            }
            let controller = match Principal::from_text(&args[0]) {
                Ok(controller) => controller,
                Err(err) => {
                    eprintln!("✗ nns_neuron_staking_subaccount: invalid principal: {err}");
                    return ExitCode::from(2);
                }
            };
            let nonce = match args[1].parse::<u64>() {
                Ok(nonce) => nonce,
                Err(err) => {
                    eprintln!("✗ nns_neuron_staking_subaccount: invalid nonce: {err}");
                    return ExitCode::from(2);
                }
            };
            println!("{}", nns_neuron_staking_subaccount(controller, nonce));
        }
        "sns_distribution_subaccount" => {
            if args.len() != 2 {
                eprintln!("✗ sns_distribution_subaccount: expected <governance-principal> <nonce>");
                return ExitCode::from(2);
            }
            let controller = match Principal::from_text(&args[0]) {
                Ok(value) => value,
                Err(err) => {
                    eprintln!("✗ sns_distribution_subaccount: invalid principal: {err}");
                    return ExitCode::from(2);
                }
            };
            let nonce = match args[1].parse::<u64>() {
                Ok(value) => value,
                Err(err) => {
                    eprintln!("✗ sns_distribution_subaccount: invalid nonce: {err}");
                    return ExitCode::from(2);
                }
            };
            println!("{}", sns_distribution_subaccount(controller, nonce));
        }
        "calculate_redemption_economics" => {
            if args.len() != 6 {
                eprintln!("✗ calculate_redemption_economics: expected <total> <reserve> <excluded> <liquid> <redeemed> <icp-fee>");
                return ExitCode::from(2);
            }
            let parsed = args
                .iter()
                .map(|value| value.parse::<u128>())
                .collect::<Result<Vec<_>, _>>();
            let values = match parsed {
                Ok(values) => values,
                Err(err) => {
                    eprintln!("✗ calculate_redemption_economics: invalid integer: {err}");
                    return ExitCode::from(2);
                }
            };
            match calculate_redemption_economics(
                values[0],
                values[1],
                &[values[2]],
                values[3],
                values[4],
                values[5],
            ) {
                Ok(result) => {
                    println!("excluded_total_e8s={}", result.excluded_total_e8s);
                    println!("redeemable_supply_e8s={}", result.redeemable_supply_e8s);
                    println!("gross_icp_e8s={}", result.gross_icp_e8s);
                    println!("net_icp_e8s={}", result.net_icp_e8s);
                }
                Err(err) => {
                    eprintln!("✗ calculate_redemption_economics: {err}");
                    return ExitCode::FAILURE;
                }
            }
        }
        "index_transfer_block" => {
            if args.len() != 3 {
                eprintln!(
                    "✗ index_transfer_block: expected <history-file> <amount-e8s> <memo-hex>"
                );
                return ExitCode::from(2);
            }
            let amount = match args[1].parse::<u128>() {
                Ok(value) => value,
                Err(err) => {
                    eprintln!("✗ index_transfer_block: invalid amount: {err}");
                    return ExitCode::from(2);
                }
            };
            let text = match fs::read_to_string(&args[0]) {
                Ok(value) => value,
                Err(err) => {
                    eprintln!("✗ index_transfer_block: {}: {err}", args[0]);
                    return ExitCode::from(2);
                }
            };
            match index_transfer_block(&text, amount, &args[2]) {
                Ok(block) => println!("{block}"),
                Err(err) => {
                    eprintln!("✗ index_transfer_block: {err}");
                    return ExitCode::FAILURE;
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
                    "corrected pooled-claim-backing rehearsal evidence missing: deploy/local-sns-rehearsal/canister-ids.local.toml is absent"
                );
            }
            Err(err) => {
                eprintln!("✗ validate_local_sns_ledger: {err}");
                ok = false;
            }
        },
        "validate_local_sns_evidence_package" => {
            if args.len() != 1 {
                eprintln!("✗ validate_local_sns_evidence_package: expected <package-directory>");
                return ExitCode::from(2);
            }
            match validate_local_sns_evidence_package_at(&root, &args[0], false) {
                Ok(validated)
                    if validated.complete
                        && validated.monitoring
                        && validated.canonical_economics =>
                {
                    eprintln!("✓ validate_local_sns_evidence_package")
                }
                Ok(_) => {
                    eprintln!(
                        "✗ validate_local_sns_evidence_package: candidate must be complete monitoring canonical evidence"
                    );
                    ok = false;
                }
                Err(err) => {
                    eprintln!("✗ validate_local_sns_evidence_package: {err}");
                    ok = false;
                }
            }
        }
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
                "unit: canonical SNS reward boundary",
                cargo_test(&["-p", "io-sns-reward-boundary"]),
            );
            ok &= run(
                "unit: stream daily reward evidence",
                cargo_test(&["-p", "io-stream-manager", "--lib", "reward_evidence"]),
            );
        }
        "sns_governance_read_required" => {
            ok &= run_subcommand("sns_governance_read_tests");
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
                        "--",
                        "--test-threads=1",
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
                "validate_workflows",
                "validate_obsolete_economics_guard",
                "did_surface",
                "validate_nns_boundary_pin",
                "verify_recorded_source",
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
            ok &= run_subcommand("validate_workflows");
            ok &= run_subcommand("did_surface");
            ok &= run_subcommand("validate_nns_boundary_pin");
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
                    "pocketic: io-sns-root-lifecycle",
                    cargo_test(&[
                        "-p",
                        "io-stream-manager",
                        "--test",
                        "io_sns_root_lifecycle_pocketic",
                        "--",
                        "--test-threads=1",
                    ]),
                );
            }
        }
        "test_local_integration" => {
            ok &= run_subcommand("verify_artifacts");
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
                "validate_workflows",
                "validate_obsolete_economics_guard",
                "simplicity_check",
                "did_surface",
                "validate_nns_boundary_pin",
                "verify_recorded_source",
                "verify_artifacts",
                "validate_install_args",
                "validate_production_wiring",
                "validate_historian_freshness",
                "validate_stable_storage",
                "validate_local_sns_rehearsal",
                "validate_local_sns_ledger",
                "validate_local_sns_committed_evidence",
                "validate_local_sns_scripts",
                "security_scan_required",
                "test_unit",
                "frontend_required",
                "test_pocketic_required",
                "sns_pocketic_required",
                "sns_root_lifecycle_required",
                "test_local_integration",
                "test_e2e",
            ] {
                ok &= run_subcommand(sub);
            }
            ok &= run("test: workspace", cargo_test(&["--workspace"]));
            ok &= run(
                "check: value-moving canisters wasm32",
                cargo_check(&[
                    "-p",
                    "io-stream-manager",
                    "-p",
                    "io-nns-neuron-manager",
                    "--target",
                    "wasm32-unknown-unknown",
                ]),
            );
            ok &= run(
                "clippy: workspace all targets",
                cargo_clippy(&[
                    "--workspace",
                    "--all-targets",
                    "--all-features",
                    "--",
                    "-D",
                    "warnings",
                ]),
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
mod tests;
