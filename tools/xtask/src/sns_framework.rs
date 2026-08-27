use candid::Principal;
use io_sns_manifest::SnsManifest;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const OFFICIAL_LOCK: &str = "tests/e2e_real_canisters/wasms.example.toml";
const GOVERNANCE_DID: &str = "rs/sns/governance/canister/governance.did";
const ROOT_DID: &str = "rs/sns/root/canister/root.did";
const CAPABILITY_FIELD: &str = "latest_reward_event_participation";

const ALL_ARTIFACTS: &[&str] = &[
    "sns_ledger",
    "sns_index",
    "sns_governance",
    "sns_root",
    "sns_swap",
    "sns_archive",
    "sns_wasm",
    "nns_governance",
    "nns_ledger",
    "nns_root",
    "nns_lifeline",
    "nns_registry",
    "cmc",
    "icp_ledger",
    "icp_index",
];

const GOVERNANCE_TARGET: &str = "//rs/sns/governance:sns-governance-canister";
const ROOT_TARGET: &str = "//rs/sns/root:sns-root-canister";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Source {
    Official,
    Local,
    Bundle,
}

impl Source {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "official" => Ok(Self::Official),
            "local" => Ok(Self::Local),
            "bundle" => Ok(Self::Bundle),
            _ => Err(format!(
                "unsupported SNS source {value:?}; expected official, local, or bundle"
            )),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Official => "official",
            Self::Local => "local",
            Self::Bundle => "bundle",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Scope {
    Governance,
    GovernanceRoot,
}

impl Scope {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "governance" => Ok(Self::Governance),
            "governance-root" => Ok(Self::GovernanceRoot),
            _ => Err(format!(
                "unsupported SNS scope {value:?}; expected governance or governance-root"
            )),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Governance => "governance",
            Self::GovernanceRoot => "governance-root",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Profile {
    Contract,
    Io,
    Upgrade,
    Lifecycle,
}

impl Profile {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "contract" => Ok(Self::Contract),
            "io" => Ok(Self::Io),
            "upgrade" => Ok(Self::Upgrade),
            "lifecycle" => Ok(Self::Lifecycle),
            _ => Err(format!(
                "unsupported SNS profile {value:?}; expected contract, io, upgrade, or lifecycle"
            )),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Contract => "contract",
            Self::Io => "io",
            Self::Upgrade => "upgrade",
            Self::Lifecycle => "lifecycle",
        }
    }
}

#[derive(Debug)]
struct Options {
    source: Source,
    scope: Scope,
    profile: Profile,
    ic_repo: Option<PathBuf>,
    bundle: Option<PathBuf>,
    cache_dir: Option<PathBuf>,
    official_manifest: PathBuf,
    required_capability: Option<String>,
    bazel_version: Option<String>,
    reject_dirty: bool,
    prepare_only: bool,
    run_id: String,
}

#[derive(Debug)]
struct LocalProvenance {
    head: String,
    branch: String,
    merge_base: String,
    clean: bool,
    diff_hash: String,
    bazel_version: String,
}

#[derive(Debug)]
struct ResolvedBundle {
    manifest: PathBuf,
    wasm_dir: PathBuf,
    source: Source,
    scope: Scope,
    profile: Profile,
    official_baseline: String,
    ic_commit: String,
    clean: Option<bool>,
    diff_hash: String,
    overrides: Vec<String>,
    governance_hash: String,
    governance_did_hash: String,
    capability: bool,
}

pub fn run(args: &[String]) -> ExitCode {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_help();
        return ExitCode::SUCCESS;
    }
    match parse_options(args).and_then(resolve_and_run) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("SNS framework error: {err}");
            ExitCode::FAILURE
        }
    }
}

fn print_help() {
    println!(
        r#"Resolve immutable SNS framework bytes and run the shared IO tests.

USAGE
  tools/scripts/test-sns-framework [OPTIONS]

SOURCE (flag overrides environment)
  --source official|local|bundle  IO_SNS_SOURCE; default: official
  --ic-repo PATH                 IO_IC_REPO; default: <IO root>/../ic
  --bundle PATH                  IO_SNS_BUNDLE; required for bundle source
  --official-manifest PATH       alternate deterministic official lock
  --require-capability NAME      require latest_reward_event_participation

BUILD AND TEST
  --scope governance|governance-root
                                 IO_SNS_SCOPE; same-source component overlay
  --profile contract|io|upgrade|lifecycle
                                 implemented profiles
                                 IO_SNS_PROFILE; default: contract
  --cache-dir PATH               IO_SNS_CACHE_DIR; default:
                                 ${{XDG_CACHE_HOME:-$HOME/.cache}}/io/sns-framework
  --reject-dirty                 reject a dirty local IC checkout (CI sets this)
  --bazel-version VERSION        explicit Bazelisk override, recorded in provenance
  --prepare-only                 resolve/validate bytes but do not run tests

EXAMPLES
  tools/scripts/test-sns-framework --source official
  tools/scripts/test-sns-framework --source local
  tools/scripts/test-sns-framework --source local --scope governance --profile io
  tools/scripts/test-sns-framework --source bundle --bundle /abs/bundle --profile upgrade

Every mode resolves to one manifest and uses the existing real-canister loader.
Local scope overlays Governance, or Governance and Root from the same IC commit,
onto the checked-in official baseline. The lifecycle profile is a thin adapter
to the guarded official local rehearsal and requires its loopback runtime inputs.
No command in this workflow contacts an IC mainnet endpoint."#
    );
}

fn parse_options(args: &[String]) -> Result<Options, String> {
    let source_env = env::var("IO_SNS_SOURCE").unwrap_or_else(|_| "official".into());
    let scope_env = env::var("IO_SNS_SCOPE").unwrap_or_else(|_| "governance".into());
    let profile_env = env::var("IO_SNS_PROFILE").unwrap_or_else(|_| "contract".into());
    let mut source = Source::parse(&source_env)?;
    let mut scope = Scope::parse(&scope_env)?;
    let mut profile = Profile::parse(&profile_env)?;
    let mut ic_repo = env::var_os("IO_IC_REPO").map(PathBuf::from);
    let mut bundle = env::var_os("IO_SNS_BUNDLE").map(PathBuf::from);
    let mut cache_dir = env::var_os("IO_SNS_CACHE_DIR").map(PathBuf::from);
    let mut official_manifest = PathBuf::from(OFFICIAL_LOCK);
    let mut required_capability = None;
    let mut bazel_version = None;
    let mut reject_dirty = env::var_os("CI").is_some();
    let mut prepare_only = false;
    let mut index = 0;
    while index < args.len() {
        let flag = &args[index];
        let takes_value = matches!(
            flag.as_str(),
            "--source"
                | "--scope"
                | "--profile"
                | "--ic-repo"
                | "--bundle"
                | "--cache-dir"
                | "--official-manifest"
                | "--require-capability"
                | "--bazel-version"
        );
        let value = if takes_value {
            index += 1;
            Some(
                args.get(index)
                    .ok_or_else(|| format!("{flag} requires a value"))?,
            )
        } else {
            None
        };
        match flag.as_str() {
            "--source" => source = Source::parse(value.expect("value checked"))?,
            "--scope" => scope = Scope::parse(value.expect("value checked"))?,
            "--profile" => profile = Profile::parse(value.expect("value checked"))?,
            "--ic-repo" => ic_repo = Some(PathBuf::from(value.expect("value checked"))),
            "--bundle" => bundle = Some(PathBuf::from(value.expect("value checked"))),
            "--cache-dir" => cache_dir = Some(PathBuf::from(value.expect("value checked"))),
            "--official-manifest" => {
                official_manifest = PathBuf::from(value.expect("value checked"));
            }
            "--require-capability" => {
                required_capability = Some(value.expect("value checked").clone());
            }
            "--bazel-version" => bazel_version = Some(value.expect("value checked").clone()),
            "--reject-dirty" => reject_dirty = true,
            "--prepare-only" => prepare_only = true,
            unknown => return Err(format!("unknown option {unknown:?}; use --help")),
        }
        index += 1;
    }
    if source == Source::Bundle && bundle.is_none() {
        return Err("--source bundle requires --bundle or IO_SNS_BUNDLE".into());
    }
    if required_capability
        .as_deref()
        .is_some_and(|capability| capability != CAPABILITY_FIELD)
    {
        return Err(format!(
            "unsupported required capability; expected {CAPABILITY_FIELD}"
        ));
    }
    Ok(Options {
        source,
        scope,
        profile,
        ic_repo,
        bundle,
        cache_dir,
        official_manifest,
        required_capability,
        bazel_version,
        reject_dirty,
        prepare_only,
        run_id: new_run_id()?,
    })
}

fn new_run_id() -> Result<String, String> {
    let epoch_nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock is before Unix epoch: {error}"))?
        .as_nanos();
    Ok(format!("sns-{epoch_nanos}-{}", std::process::id()))
}

fn resolve_and_run(options: Options) -> Result<(), String> {
    reject_relative_unsafe(&options.official_manifest)?;
    let io_root = canonical_io_root()?;
    let official_manifest = canonical_or_join(&io_root, &options.official_manifest)?;
    let bundle = match options.source {
        Source::Official => resolve_official_bundle(&io_root, &official_manifest, &options)?,
        Source::Local => resolve_local_overlay(&io_root, &official_manifest, &options)?,
        Source::Bundle => resolve_existing_bundle(&options)?,
    };
    print_summary(&bundle);
    if options.required_capability.is_some() && !bundle.capability {
        return Err(format!(
            "required capability {CAPABILITY_FIELD} is absent from the resolved Governance DID"
        ));
    }
    if options.prepare_only {
        return Ok(());
    }
    let profile_run = ProfileRun::new(cache_root(&options)?, &options.run_id)?;
    eprintln!("SNS profile run ID: {}", profile_run.id);
    dispatch_profile(&io_root, &bundle, &profile_run)
}

fn canonical_io_root() -> Result<PathBuf, String> {
    let output = command_output(Command::new("git").args(["rev-parse", "--show-toplevel"]))?;
    fs::canonicalize(output.trim())
        .map_err(|err| format!("failed to resolve IO repository root: {err}"))
}

fn canonical_or_join(root: &Path, path: &Path) -> Result<PathBuf, String> {
    let candidate = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    fs::canonicalize(&candidate)
        .map_err(|err| format!("failed to resolve {}: {err}", candidate.display()))
}

fn default_cache() -> Result<PathBuf, String> {
    if let Some(path) = env::var_os("XDG_CACHE_HOME") {
        return Ok(PathBuf::from(path).join("io/sns-framework"));
    }
    env::var_os("HOME")
        .map(PathBuf::from)
        .map(|path| path.join(".cache/io/sns-framework"))
        .ok_or_else(|| "HOME or IO_SNS_CACHE_DIR is required for the local cache".into())
}

fn cache_root(options: &Options) -> Result<PathBuf, String> {
    let path = options
        .cache_dir
        .clone()
        .map(Ok)
        .unwrap_or_else(default_cache)?;
    fs::create_dir_all(&path)
        .map_err(|err| format!("failed to create cache {}: {err}", path.display()))?;
    fs::canonicalize(&path)
        .map_err(|err| format!("failed to resolve cache {}: {err}", path.display()))
}

fn resolve_official_bundle(
    io_root: &Path,
    official_lock: &Path,
    options: &Options,
) -> Result<ResolvedBundle, String> {
    let lock_text = fs::read_to_string(official_lock)
        .map_err(|error| format!("failed to read {}: {error}", official_lock.display()))?;
    let lock = SnsManifest::parse(&lock_text)?;
    validate_official_lock(&lock)?;
    let baseline = required_value(&lock, "metadata", "version")?.to_string();
    let lock_hash = sha256(lock_text.as_bytes());
    let id = format!("official-{}-{}", safe_id(&baseline), &lock_hash[..16]);
    let root = cache_root(options)?.join(id);
    if !root.exists() {
        prepare_official_bundle(io_root, &root, &lock_text, &lock, &options.run_id)?;
    }
    validate_bundle(&root)?;
    bundle_from_root(
        root,
        Source::Official,
        options.scope,
        options.profile,
        baseline,
        String::new(),
        None,
        String::new(),
        vec![],
    )
}

fn prepare_official_bundle(
    io_root: &Path,
    destination: &Path,
    lock_text: &str,
    lock: &SnsManifest,
    run_id: &str,
) -> Result<(), String> {
    let cache = destination
        .parent()
        .ok_or_else(|| "official cache destination has no parent".to_string())?;
    let staging = cache.join(format!(".building-official-{run_id}"));
    ensure_new_staging(&staging)?;
    let wasms = staging.join("wasms");
    fs::create_dir(&wasms).map_err(|err| format!("failed to create {}: {err}", wasms.display()))?;

    let mut resolved = lock.clone();
    for component in ALL_ARTIFACTS {
        let source_url = required_artifact(lock, component, "source_url")?;
        let basename = source_url
            .rsplit('/')
            .next()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| format!("official {component} source URL has no filename"))?;
        resolved.set_artifact(
            component,
            "source_filename",
            format!("{component}-{basename}"),
        )?;
    }
    let resolved_text = resolved.to_toml()?;
    let fetch_manifest = staging.join("fetch-manifest.toml");
    fs::write(&fetch_manifest, &resolved_text)
        .map_err(|err| format!("failed to prepare fetch manifest: {err}"))?;
    let status = Command::new(io_root.join("tools/scripts/fetch-real-canister-artifacts"))
        .current_dir(io_root)
        .env("IO_REAL_SNS_WASM_MANIFEST", &fetch_manifest)
        .env("IO_REAL_SNS_WASM_DIR", &wasms)
        .status()
        .map_err(|err| format!("failed to invoke official artifact fetcher: {err}"))?;
    if !status.success() {
        return Err(format!("official artifact fetcher failed with {status}"));
    }
    fs::remove_file(&fetch_manifest)
        .map_err(|err| format!("failed to remove resolver fetch manifest: {err}"))?;
    if !official_artifacts_available(&resolved, &wasms)? {
        return Err(
            "official fetch completed without every verified raw and compressed artifact".into(),
        );
    }
    if lock.bool_value("capabilities", CAPABILITY_FIELD) == Some(true) {
        let did_source = canonical_or_join(
            io_root,
            Path::new(required_value(lock, "contract", "governance_did")?),
        )?;
        if !did_source.starts_with(io_root) {
            return Err("official Governance DID must resolve inside the IO repository".into());
        }
        verify_hash(
            &did_source,
            required_value(lock, "contract", "governance_did_sha256")?,
        )?;
        let did_text = fs::read_to_string(&did_source)
            .map_err(|err| format!("failed to read {}: {err}", did_source.display()))?;
        verify_candidate_did(&did_text)?;
        fs::copy(&did_source, staging.join("governance.did"))
            .map_err(|err| format!("failed to copy reviewed Governance DID: {err}"))?;
    }
    fs::write(staging.join("manifest.toml"), &resolved_text)
        .map_err(|err| format!("failed to write resolved manifest: {err}"))?;
    let provenance = format!(
        "[variant]\nsource = \"official\"\nscope = \"governance\"\nofficial_lock_sha256 = \"{}\"\nofficial_baseline = \"{}\"\nsource_tree_clean = true\nlocal_only = false\nexportable = true\n",
        sha256(lock_text.as_bytes()),
        required_value(lock, "metadata", "version")?
    );
    fs::write(staging.join("provenance.toml"), provenance)
        .map_err(|err| format!("failed to write provenance: {err}"))?;
    write_sha256sums(&staging)?;
    publish_staging(staging, destination)
}

fn resolve_local_overlay(
    io_root: &Path,
    official_lock: &Path,
    options: &Options,
) -> Result<ResolvedBundle, String> {
    let default_ic = io_root
        .parent()
        .ok_or_else(|| "IO repository has no sibling parent".to_string())?
        .join("ic");
    let requested = options.ic_repo.clone().unwrap_or(default_ic);
    let ic_root = fs::canonicalize(&requested).map_err(|err| {
        format!(
            "failed to resolve IC repository {}: {err}",
            requested.display()
        )
    })?;
    let cache = cache_root(options)?;
    let bazel_rc = prepare_local_bazelrc(&ic_root, &cache)?;
    let provenance = inspect_local_ic(&ic_root, &bazel_rc, options.bazel_version.as_deref())?;
    if options.reject_dirty && !provenance.clean {
        return Err(format!(
            "dirty IC checkout rejected; commit/stash outside this runner or omit --reject-dirty for local development (diff SHA-256 {})",
            provenance.diff_hash
        ));
    }
    if !provenance.clean {
        eprintln!(
            "WARNING: building dirty IC checkout; the local-only, non-exportable bundle ID binds tracked changes by SHA-256 {}",
            provenance.diff_hash
        );
    }
    let official = resolve_official_bundle(io_root, official_lock, options)?;
    let lock = SnsManifest::read(&official.manifest)?;
    let baseline = official.official_baseline.clone();
    let staging = cache.join(format!(".building-local-{}", options.run_id));
    ensure_new_staging(&staging)?;
    let wasms = staging.join("wasms");
    fs::create_dir(&wasms).map_err(|err| format!("failed to create {}: {err}", wasms.display()))?;

    copy_manifest_artifacts(&lock, &official.wasm_dir, &wasms)?;
    let official_governance_name = required_artifact(&lock, "sns_governance", "wasm")?;
    fs::copy(
        official.wasm_dir.join(official_governance_name),
        wasms.join("official_sns_governance.wasm"),
    )
    .map_err(|err| format!("failed to retain official Governance baseline: {err}"))?;

    let mut targets = vec![("sns_governance", GOVERNANCE_TARGET)];
    if options.scope == Scope::GovernanceRoot {
        targets.push(("sns_root", ROOT_TARGET));
    }
    let outputs = build_local_targets(
        &ic_root,
        &bazel_rc,
        options.bazel_version.as_deref(),
        &targets,
    )?;
    let mut resolved = lock.clone();
    let mut overrides = Vec::new();
    for (component, target) in &targets {
        let gz = outputs
            .get(*component)
            .ok_or_else(|| format!("missing cquery output for {component}"))?;
        let filename = format!("{component}.wasm");
        let raw = wasms.join(&filename);
        decompress_gzip(gz, &raw)?;
        let gz_name = format!("{component}.wasm.gz");
        if *component != "sns_governance" {
            let official_source = lock.source_artifact_name(component)?;
            if official_source != gz_name {
                fs::remove_file(wasms.join(official_source)).map_err(|err| {
                    format!("failed to remove overridden {component} source artifact: {err}")
                })?;
            }
        }
        fs::copy(gz, wasms.join(&gz_name))
            .map_err(|err| format!("failed to copy {}: {err}", gz.display()))?;
        let raw_hash = sha256_file(&raw)?;
        let gz_hash = sha256_file(gz)?;
        resolved.set_artifact(component, "wasm", filename)?;
        resolved.set_artifact(component, "sha256", raw_hash)?;
        resolved.set_artifact(component, "source_filename", gz_name)?;
        resolved.set_artifact(component, "source_sha256", gz_hash)?;
        resolved.set_artifact(component, "source_kind", "local_ic_bazel")?;
        resolved.set_artifact(
            component,
            "source_url",
            format!("local://ic/{}/{component}", provenance.head),
        )?;
        resolved.set_artifact(component, "source_commit", &provenance.head)?;
        resolved.set_artifact(component, "build_target", *target)?;
        resolved.set_artifact(component, "upstream_rev", &provenance.head)?;
        overrides.push((*component).to_string());
    }

    let did_source = ic_root.join(GOVERNANCE_DID);
    let did_text = fs::read_to_string(&did_source).map_err(|err| {
        format!(
            "failed to read candidate DID {}: {err}",
            did_source.display()
        )
    })?;
    let capability = did_has_reward_participation(&did_text);
    if capability {
        verify_candidate_did(&did_text)?;
    }
    fs::write(staging.join("governance.did"), &did_text)
        .map_err(|err| format!("failed to write candidate DID: {err}"))?;
    resolved.set_bool("capabilities", CAPABILITY_FIELD, capability)?;
    resolved.set_value(
        "contract",
        "governance_did_sha256",
        sha256(did_text.as_bytes()),
    )?;
    let root_did_hash = if options.scope == Scope::GovernanceRoot {
        let root_did = fs::read_to_string(ic_root.join(ROOT_DID))
            .map_err(|err| format!("failed to read candidate Root DID: {err}"))?;
        fs::write(staging.join("root.did"), &root_did)
            .map_err(|err| format!("failed to write candidate Root DID: {err}"))?;
        let hash = sha256(root_did.as_bytes());
        resolved.set_value("contract", "root_did_sha256", &hash)?;
        Some(hash)
    } else {
        None
    };
    resolved.set_value(
        "baseline",
        "sns_governance_wasm",
        "official_sns_governance.wasm",
    )?;
    resolved.set_value(
        "baseline",
        "sns_governance_sha256",
        required_artifact(&lock, "sns_governance", "sha256")?,
    )?;
    resolved.set_value(
        "baseline",
        "sns_governance_source_wasm",
        lock.source_artifact_name("sns_governance")?,
    )?;
    resolved.set_value(
        "baseline",
        "sns_governance_source_sha256",
        required_artifact(&lock, "sns_governance", "source_sha256")?,
    )?;
    resolved.set_value(
        "baseline",
        "sns_governance_revision",
        required_artifact(&lock, "sns_governance", "upstream_rev")?,
    )?;
    for (key, value) in [
        ("source", "local".to_string()),
        ("scope", options.scope.as_str().to_string()),
        ("official_baseline", baseline.clone()),
        ("ic_commit", provenance.head.clone()),
        ("ic_branch", provenance.branch.clone()),
        ("ic_merge_base", provenance.merge_base.clone()),
        ("source_diff_sha256", provenance.diff_hash.clone()),
        ("bazel_version", provenance.bazel_version.clone()),
        ("governance_did_sha256", sha256(did_text.as_bytes())),
        (
            "build_targets",
            targets
                .iter()
                .map(|(_, target)| *target)
                .collect::<Vec<_>>()
                .join(","),
        ),
        ("component_overrides", overrides.join(",")),
    ] {
        resolved.set_value("variant", key, value)?;
    }
    resolved.set_bool("variant", "source_tree_clean", provenance.clean)?;
    resolved.set_bool("variant", "local_only", !provenance.clean)?;
    resolved.set_bool("variant", "exportable", provenance.clean)?;
    let resolved_text = resolved.to_toml()?;
    fs::write(staging.join("manifest.toml"), &resolved_text)
        .map_err(|err| format!("failed to write resolved manifest: {err}"))?;
    let mut provenance_text = format!(
            "[variant]\nsource = \"local\"\nscope = \"{}\"\nofficial_baseline = \"{}\"\nic_commit = \"{}\"\nic_branch = \"{}\"\nic_merge_base = \"{}\"\nsource_tree_clean = {}\nsource_diff_sha256 = \"{}\"\nbazel_version = \"{}\"\ngovernance_did_sha256 = \"{}\"\nlocal_only = {}\nexportable = {}\n",
            options.scope.as_str(),
            escape_toml(&baseline),
            provenance.head,
            escape_toml(&provenance.branch),
            provenance.merge_base,
            provenance.clean,
            provenance.diff_hash,
            escape_toml(&provenance.bazel_version),
            sha256(did_text.as_bytes()),
            !provenance.clean,
            provenance.clean,
        );
    if let Some(hash) = root_did_hash {
        provenance_text.push_str(&format!("root_did_sha256 = \"{hash}\"\n"));
    }
    fs::write(staging.join("provenance.toml"), provenance_text)
        .map_err(|err| format!("failed to write provenance: {err}"))?;
    write_sha256sums(&staging)?;
    let manifest_hash = sha256_file(&staging.join("manifest.toml"))?;
    let dirty_id = if provenance.clean {
        "clean".to_string()
    } else {
        provenance.diff_hash[..16].to_string()
    };
    let id = format!(
        "local-{}-{}-{}-{}-{}",
        &provenance.head[..12],
        dirty_id,
        options.scope.as_str(),
        safe_id(&baseline),
        &manifest_hash[..16]
    );
    let root = cache.join(id);
    publish_staging(staging, &root)?;
    validate_bundle(&root)?;
    bundle_from_root(
        root,
        Source::Local,
        options.scope,
        options.profile,
        baseline,
        provenance.head,
        Some(provenance.clean),
        provenance.diff_hash,
        overrides,
    )
}

fn resolve_existing_bundle(options: &Options) -> Result<ResolvedBundle, String> {
    let requested = options
        .bundle
        .as_ref()
        .ok_or_else(|| "bundle path is required".to_string())?;
    if !requested.is_absolute() {
        return Err("--bundle must be an absolute path".into());
    }
    let root = fs::canonicalize(requested)
        .map_err(|err| format!("failed to resolve bundle {}: {err}", requested.display()))?;
    validate_bundle(&root)?;
    let manifest = SnsManifest::read(&root.join("manifest.toml"))?;
    let source = Source::Bundle;
    let scope = Scope::parse(
        manifest
            .value("variant", "scope")
            .unwrap_or(options.scope.as_str()),
    )?;
    let baseline = manifest
        .value("variant", "official_baseline")
        .or_else(|| manifest.value("metadata", "version"))
        .unwrap_or("unknown")
        .to_string();
    let clean = manifest.bool_value("variant", "source_tree_clean");
    let overrides = manifest
        .value("variant", "component_overrides")
        .unwrap_or("")
        .split(',')
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect();
    bundle_from_root(
        root,
        source,
        scope,
        options.profile,
        baseline,
        manifest.value("variant", "ic_commit").unwrap_or("").into(),
        clean,
        manifest
            .value("variant", "source_diff_sha256")
            .unwrap_or("")
            .into(),
        overrides,
    )
}

#[allow(clippy::too_many_arguments)]
fn bundle_from_root(
    root: PathBuf,
    source: Source,
    scope: Scope,
    profile: Profile,
    official_baseline: String,
    ic_commit: String,
    clean: Option<bool>,
    diff_hash: String,
    overrides: Vec<String>,
) -> Result<ResolvedBundle, String> {
    let manifest_path = root.join("manifest.toml");
    let manifest = SnsManifest::read(&manifest_path)?;
    let governance_name = required_artifact(&manifest, "sns_governance", "wasm")?;
    let governance_hash = required_artifact(&manifest, "sns_governance", "sha256")?.to_string();
    verify_hash(&root.join("wasms").join(governance_name), &governance_hash)?;
    let declared_capability = manifest
        .bool_value("capabilities", CAPABILITY_FIELD)
        .unwrap_or(false);
    let did_path = root.join("governance.did");
    let governance_did_hash = if did_path.is_file() {
        let text = fs::read_to_string(&did_path)
            .map_err(|err| format!("failed to read {}: {err}", did_path.display()))?;
        let detected = did_has_reward_participation(&text);
        if detected {
            verify_candidate_did(&text)?;
        }
        if detected != declared_capability {
            return Err(format!(
                "Governance DID capability {detected} disagrees with manifest metadata {declared_capability}"
            ));
        }
        sha256(text.as_bytes())
    } else {
        String::new()
    };
    if declared_capability && governance_did_hash.is_empty() {
        return Err("capability is true but governance.did is missing".into());
    }
    if declared_capability
        && manifest.value("contract", "governance_did_sha256") != Some(governance_did_hash.as_str())
    {
        return Err("capability-bearing Governance DID is not hash-bound by the manifest".into());
    }
    Ok(ResolvedBundle {
        wasm_dir: root.join("wasms"),
        manifest: manifest_path,
        source,
        scope,
        profile,
        official_baseline,
        ic_commit,
        clean,
        diff_hash,
        overrides,
        governance_hash,
        governance_did_hash,
        capability: declared_capability,
    })
}

fn inspect_local_ic(
    path: &Path,
    bazel_rc: &Path,
    bazel_version_override: Option<&str>,
) -> Result<LocalProvenance, String> {
    for expected in [
        ".git",
        GOVERNANCE_DID,
        "rs/sns/governance/BUILD.bazel",
        ROOT_DID,
        "rs/sns/root/BUILD.bazel",
    ] {
        if !path.join(expected).exists() {
            return Err(format!(
                "IC checkout {} is missing expected {}",
                path.display(),
                expected
            ));
        }
    }
    let git = |args: &[&str]| -> Result<String, String> {
        command_output(Command::new("git").arg("-C").arg(path).args(args))
    };
    let head = git(&["rev-parse", "HEAD"])?.trim().to_string();
    if head.len() != 40 || !head.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("IC HEAD is not a full 40-hex commit".into());
    }
    let branch = git(&["symbolic-ref", "--quiet", "--short", "HEAD"])
        .map(|value| value.trim().to_string())
        .unwrap_or_else(|_| "detached".into());
    let untracked = command_bytes(Command::new("git").arg("-C").arg(path).args([
        "ls-files",
        "--others",
        "--exclude-standard",
        "-z",
    ]))?;
    if !untracked.is_empty() {
        return Err(
            "refusing local IC checkout with untracked build inputs; the runner will not inspect or bundle them"
                .into(),
        );
    }
    let diff_status = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(["diff", "--quiet", "HEAD", "--"])
        .status()
        .map_err(|error| format!("failed to inspect local IC diff: {error}"))?;
    let clean = match diff_status.code() {
        Some(0) => true,
        Some(1) => false,
        _ => {
            return Err(format!(
                "local IC diff inspection failed with {diff_status}"
            ))
        }
    };
    let diff_hash = if clean {
        "none".into()
    } else {
        let mut diff_command = Command::new("git");
        diff_command
            .arg("-C")
            .arg(path)
            .args(["diff", "--binary", "HEAD", "--"]);
        command_sha256(&mut diff_command)?
    };
    let upstream = git(&[
        "rev-parse",
        "--abbrev-ref",
        "--symbolic-full-name",
        "@{upstream}",
    ])
    .map(|value| value.trim().to_string())
    .unwrap_or_default();
    let merge_base = if upstream.is_empty() {
        "unavailable".into()
    } else {
        git(&["merge-base", "HEAD", &upstream])
            .map(|value| value.trim().to_string())
            .unwrap_or_else(|_| "unavailable".into())
    };
    let bazel_version =
        command_output(bazel_command(path, bazel_rc, bazel_version_override).arg("version"))?
            .lines()
            .find(|line| line.starts_with("Build label:") || line.starts_with("bazel "))
            .unwrap_or("unknown")
            .trim()
            .to_string();
    Ok(LocalProvenance {
        head,
        branch,
        merge_base,
        clean,
        diff_hash,
        bazel_version,
    })
}

fn build_local_targets(
    ic_root: &Path,
    bazel_rc: &Path,
    bazel_version_override: Option<&str>,
    targets: &[(&str, &str)],
) -> Result<BTreeMap<String, PathBuf>, String> {
    let bazel_jobs = positive_env_or_default("IO_SNS_BAZEL_JOBS", 2)?;
    for (_, target) in targets {
        let query = command_output(
            bazel_command(ic_root, bazel_rc, bazel_version_override).args([
                "cquery",
                target,
                "--output=files",
            ]),
        )?;
        if query.lines().all(|line| !line.trim().ends_with(".wasm.gz")) {
            return Err(format!(
                "canonical target {target} did not resolve to a .wasm.gz output"
            ));
        }
    }
    let status = bazel_command(ic_root, bazel_rc, bazel_version_override)
        .arg("build")
        .arg(format!("--jobs={bazel_jobs}"))
        .args(targets.iter().map(|(_, target)| target))
        .status()
        .map_err(|err| format!("failed to invoke Bazel: {err}"))?;
    if !status.success() {
        return Err(format!("local IC Bazel build failed with {status}"));
    }
    let mut outputs = BTreeMap::new();
    for (component, target) in targets {
        let query = command_output(
            bazel_command(ic_root, bazel_rc, bazel_version_override).args([
                "cquery",
                target,
                "--output=files",
            ]),
        )?;
        let relative = query
            .lines()
            .map(str::trim)
            .find(|line| line.ends_with(".wasm.gz"))
            .ok_or_else(|| format!("no .wasm.gz cquery output for {target}"))?;
        let path = ic_root.join(relative);
        if !path.is_file() {
            return Err(format!("Bazel output is missing: {}", path.display()));
        }
        outputs.insert((*component).to_string(), path);
    }
    Ok(outputs)
}

fn prepare_local_bazelrc(ic_root: &Path, cache: &Path) -> Result<PathBuf, String> {
    let source = ic_root.join("bazel/conf/.bazelrc.build");
    let source_text = fs::read_to_string(&source)
        .map_err(|err| format!("failed to read {}: {err}", source.display()))?;
    let bazel_cache = cache.join("bazel");
    let zig_cache = bazel_cache.join("zig-cache");
    fs::create_dir_all(&zig_cache)
        .map_err(|err| format!("failed to create {}: {err}", zig_cache.display()))?;
    let mut filtered = source_text
        .lines()
        .filter(|line| !line.contains("/tmp/zig-cache"))
        .collect::<Vec<_>>()
        .join("\n");
    filtered.push_str(&format!(
        "\nbuild --repo_env=HERMETIC_CC_TOOLCHAIN_CACHE_PREFIX={}\nbuild --sandbox_add_mount_pair={}\nbuild --sandbox_writable_path={}\n",
        zig_cache.display(),
        zig_cache.display(),
        zig_cache.display()
    ));
    let hash = sha256(filtered.as_bytes());
    let path = bazel_cache.join(format!("ic-local-{hash}.bazelrc"));
    if !path.exists() {
        fs::write(&path, filtered)
            .map_err(|err| format!("failed to write {}: {err}", path.display()))?;
    }
    Ok(path)
}

fn bazel_command(ic_root: &Path, bazel_rc: &Path, bazel_version_override: Option<&str>) -> Command {
    let mut command = Command::new("bazelisk");
    command
        .current_dir(ic_root)
        .args([
            "--batch",
            "--nosystem_rc",
            "--nohome_rc",
            "--noworkspace_rc",
        ])
        .arg(format!("--bazelrc={}", bazel_rc.display()));
    if let Some(version) = bazel_version_override {
        command.env("USE_BAZEL_VERSION", version);
    }
    command
}

fn positive_env_or_default(name: &str, default: usize) -> Result<usize, String> {
    match env::var(name) {
        Ok(value) => value
            .parse::<usize>()
            .ok()
            .filter(|value| *value > 0)
            .ok_or_else(|| format!("{name} must be a positive integer")),
        Err(env::VarError::NotPresent) => Ok(default),
        Err(env::VarError::NotUnicode(_)) => Err(format!("{name} is not valid Unicode")),
    }
}

struct ProfileRun {
    id: String,
    lifecycle_root: PathBuf,
    pocket_ic_bin: Option<PathBuf>,
    pocket_ic_pid: Option<PathBuf>,
}

impl ProfileRun {
    fn new(cache: PathBuf, id: &str) -> Result<Self, String> {
        let run_root = cache.join("runs");
        fs::create_dir_all(&run_root)
            .map_err(|error| format!("failed to create {}: {error}", run_root.display()))?;
        let mut pocket_ic_pid = None;
        let pocket_ic_bin = env::var_os("POCKET_IC_BIN")
            .map(PathBuf::from)
            .map(|requested| {
                let source = fs::canonicalize(&requested).map_err(|error| {
                    format!(
                        "failed to resolve POCKET_IC_BIN {}: {error}",
                        requested.display()
                    )
                })?;
                let run_bin = run_root.join(format!("pocket-ic-server-{id}"));
                let pid_file = run_root.join(format!("pocket-ic-server-{id}.pids"));
                if run_bin.exists() {
                    return Err(format!(
                        "run-owned PocketIC path already exists: {}",
                        run_bin.display()
                    ));
                }
                let wrapper = pocket_ic_wrapper(&pid_file, &source);
                fs::write(&run_bin, wrapper)
                    .map_err(|error| format!("failed to write {}: {error}", run_bin.display()))?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    fs::set_permissions(&run_bin, fs::Permissions::from_mode(0o700)).map_err(
                        |error| format!("failed to make {} executable: {error}", run_bin.display()),
                    )?;
                }
                #[cfg(not(unix))]
                return Err("SNS profile runner requires Unix process identity semantics".into());
                pocket_ic_pid = Some(pid_file);
                Ok(run_bin)
            })
            .transpose()?;
        Ok(Self {
            id: id.to_string(),
            lifecycle_root: run_root.join(format!("lifecycle-{id}")),
            pocket_ic_bin,
            pocket_ic_pid,
        })
    }

    fn cleanup(&self) {
        let Some(run_bin) = &self.pocket_ic_bin else {
            return;
        };
        if let Some(pid_file) = &self.pocket_ic_pid {
            if let Ok(pids) = fs::read_to_string(pid_file) {
                for pid in pids
                    .lines()
                    .filter(|pid| !pid.is_empty() && pid.bytes().all(|byte| byte.is_ascii_digit()))
                {
                    let group = format!("-{pid}");
                    let _ = Command::new("kill")
                        .args(["-TERM", "--", &group])
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .status();
                    std::thread::sleep(Duration::from_millis(100));
                    let still_running = Command::new("kill")
                        .args(["-0", "--", &group])
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .status()
                        .is_ok_and(|status| status.success());
                    if still_running {
                        let _ = Command::new("kill")
                            .args(["-KILL", "--", &group])
                            .stdout(Stdio::null())
                            .stderr(Stdio::null())
                            .status();
                    }
                }
            }
            let _ = fs::remove_file(pid_file);
        }
        let _ = fs::remove_file(run_bin);
    }
}

fn pocket_ic_wrapper(pid_file: &Path, source: &Path) -> String {
    format!(
        "#!/bin/sh\nexport IO_SNS_RUN_PID_FILE={}\nexport IO_SNS_RUN_POCKET_IC={}\nexec setsid sh -c 'printf \"%s\\n\" \"$$\" >> \"$IO_SNS_RUN_PID_FILE\"\nexec \"$IO_SNS_RUN_POCKET_IC\" \"$@\"' sh \"$@\"\n",
        shell_quote(&pid_file.to_string_lossy()),
        shell_quote(&source.to_string_lossy()),
    )
}

impl Drop for ProfileRun {
    fn drop(&mut self) {
        self.cleanup();
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProfileStep {
    RewardBoundary,
    DtoCompatibility,
    CandidateContract,
    IoIntegration,
    Upgrade,
    Lifecycle,
}

fn shared_profile_plan(profile: Profile, capability: bool) -> Result<Vec<ProfileStep>, String> {
    let contract = || {
        let mut steps = vec![ProfileStep::RewardBoundary, ProfileStep::DtoCompatibility];
        if capability {
            steps.push(ProfileStep::CandidateContract);
        }
        Ok(steps)
    };
    match profile {
        Profile::Contract => contract(),
        Profile::Io => {
            if !capability {
                return Err(format!(
                    "profile io requires capability {CAPABILITY_FIELD}; use contract for compatibility-only Governance"
                ));
            }
            let mut steps = contract()?;
            steps.push(ProfileStep::IoIntegration);
            Ok(steps)
        }
        Profile::Upgrade => {
            if !capability {
                return Err(format!(
                    "profile upgrade requires capability {CAPABILITY_FIELD}"
                ));
            }
            Ok(vec![ProfileStep::Upgrade])
        }
        Profile::Lifecycle => {
            if !capability {
                return Err(format!(
                    "profile lifecycle requires capability {CAPABILITY_FIELD}"
                ));
            }
            Ok(vec![ProfileStep::Lifecycle])
        }
    }
}

fn dispatch_profile(
    io_root: &Path,
    bundle: &ResolvedBundle,
    profile_run: &ProfileRun,
) -> Result<(), String> {
    if !bundle.capability {
        eprintln!(
            "SNS capability {CAPABILITY_FIELD}=false: compatibility tests run; feature allocation is unsupported and no ballot fallback is used"
        );
    }
    for step in shared_profile_plan(bundle.profile, bundle.capability)? {
        match step {
            ProfileStep::RewardBoundary => {
                run_cargo(
                    io_root,
                    bundle,
                    profile_run,
                    &["test", "-p", "io-sns-reward-boundary"],
                )?;
            }
            ProfileStep::DtoCompatibility => run_exact_unit(
                io_root,
                bundle,
                profile_run,
                "io-governance-types",
                "tests::sns_reward_shares_decode_additively_and_convert_exactly",
            )?,
            ProfileStep::CandidateContract => run_exact_ignored(
                io_root,
                bundle,
                profile_run,
                "candidate_latest_reward_event_participation_contract",
            )?,
            ProfileStep::IoIntegration => {
                build_io_profile_wasms(io_root, bundle, profile_run)?;
                run_exact_ignored(
                    io_root,
                    bundle,
                    profile_run,
                    "candidate_reward_shares_drive_io_rewards",
                )?
            }
            ProfileStep::Upgrade => {
                build_io_profile_wasms(io_root, bundle, profile_run)?;
                run_exact_ignored(
                    io_root,
                    bundle,
                    profile_run,
                    "official_to_candidate_reward_participation_upgrade",
                )?
            }
            ProfileStep::Lifecycle => run_lifecycle_profile(io_root, bundle, profile_run)?,
        }
    }
    if !bundle.capability && bundle.profile == Profile::Contract {
        eprintln!("Compatibility-only contract completed; the resolved Governance DID has no {CAPABILITY_FIELD} field");
    }
    Ok(())
}

const LIFECYCLE_PHASES: &[&str] = &[
    "bootstrap-official-network",
    "build-local-io-canisters",
    "deploy-local-dapps",
    "propose-and-finalize-sns",
    "discover-sns-canisters",
    "exercise-ledger",
    "exercise-index-and-archives",
    "exercise-governance-and-controllers",
    "exercise-ledger",
    "exercise-index-and-archives",
    "observe-one-day-reward",
    "exercise-account-semantic-protocol",
    "package-evidence",
];

fn topology_allocation_ids(
    topology: &serde_json::Value,
    subnet_kind: &str,
    count: usize,
) -> Result<Vec<String>, String> {
    let configs = topology
        .get("subnet_configs")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "fresh lifecycle topology omits subnet_configs".to_string())?;
    let matching = configs
        .iter()
        .filter(|config| {
            config
                .get("subnet_kind")
                .and_then(serde_json::Value::as_str)
                == Some(subnet_kind)
        })
        .collect::<Vec<_>>();
    if matching.len() != 1 {
        return Err(format!(
            "fresh lifecycle topology has {} {subnet_kind} subnets; expected exactly one",
            matching.len()
        ));
    }
    let start = matching[0]
        .pointer("/alloc_range/start")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| format!("fresh lifecycle {subnet_kind} subnet omits alloc_range.start"))?;
    let principal = Principal::from_text(start)
        .map_err(|error| format!("invalid {subnet_kind} allocation start {start}: {error}"))?;
    let mut bytes = principal.as_slice().to_vec();
    if bytes.len() < 4 || bytes[bytes.len() - 2..] != [1, 1] {
        return Err(format!(
            "unsupported {subnet_kind} allocation principal shape: {start}"
        ));
    }
    let counter_index = bytes.len() - 4;
    let initial = u16::from_be_bytes([bytes[counter_index], bytes[counter_index + 1]]);
    (0..count)
        .map(|offset| {
            let offset = u16::try_from(offset)
                .map_err(|_| "lifecycle allocation offset exceeds u16".to_string())?;
            let counter = initial
                .checked_add(offset)
                .ok_or_else(|| "lifecycle allocation range overflow".to_string())?;
            let encoded = counter.to_be_bytes();
            bytes[counter_index] = encoded[0];
            bytes[counter_index + 1] = encoded[1];
            Ok(Principal::from_slice(&bytes).to_text())
        })
        .collect()
}

fn quoted_assignment(text: &str, key: &str) -> Result<String, String> {
    let prefix = format!("{key} = \"");
    text.lines()
        .find_map(|line| {
            line.trim()
                .strip_prefix(&prefix)
                .and_then(|value| value.strip_suffix('"'))
                .map(str::to_string)
        })
        .ok_or_else(|| format!("fresh lifecycle input omits {key}"))
}

fn replace_single_assignment_line(
    text: &str,
    key: &str,
    replacement: &str,
) -> Result<String, String> {
    let mut matches = 0_usize;
    let mut rendered = String::with_capacity(text.len() + replacement.len());
    for line in text.lines() {
        if line.trim_start().starts_with(&format!("{key} =")) {
            matches += 1;
            let indentation = &line[..line.len() - line.trim_start().len()];
            rendered.push_str(indentation);
            rendered.push_str(replacement);
        } else {
            rendered.push_str(line);
        }
        rendered.push('\n');
    }
    if matches != 1 {
        return Err(format!(
            "fresh lifecycle input must contain exactly one {key} assignment, found {matches}"
        ));
    }
    Ok(rendered)
}

fn rewrite_lifecycle_allocations(inputs: &Path, topology_path: &Path) -> Result<(), String> {
    if !topology_path.is_absolute() {
        return Err("IO_LOCAL_SNS_TOPOLOGY_FILE must be absolute".to_string());
    }
    let topology_text = fs::read_to_string(topology_path).map_err(|error| {
        format!(
            "failed to read fresh lifecycle topology {}: {error}",
            topology_path.display()
        )
    })?;
    let topology: serde_json::Value = serde_json::from_str(&topology_text).map_err(|error| {
        format!(
            "failed to parse fresh lifecycle topology {}: {error}",
            topology_path.display()
        )
    })?;
    let dapp_ids = topology_allocation_ids(&topology, "NNS", 4)?;
    let sns_ids = topology_allocation_ids(&topology, "SNS", 5)?;
    let local_vars = fs::read_to_string(inputs.join("local-vars.toml"))
        .map_err(|error| format!("failed to read lifecycle local-vars.toml: {error}"))?;
    let runtime = fs::read_to_string(inputs.join("runtime.local.toml"))
        .map_err(|error| format!("failed to read lifecycle runtime.local.toml: {error}"))?;
    let replacements = [
        (
            quoted_assignment(&local_vars, "io_stream_manager_canister")?,
            dapp_ids[0].clone(),
        ),
        (
            quoted_assignment(&local_vars, "io_nns_neuron_manager_canister")?,
            dapp_ids[1].clone(),
        ),
        (
            quoted_assignment(&local_vars, "io_historian_canister")?,
            dapp_ids[2].clone(),
        ),
        (
            quoted_assignment(&local_vars, "frontend_canister")?,
            dapp_ids[3].clone(),
        ),
        (quoted_assignment(&runtime, "root")?, sns_ids[0].clone()),
        (
            quoted_assignment(&runtime, "governance")?,
            sns_ids[1].clone(),
        ),
        (quoted_assignment(&runtime, "ledger")?, sns_ids[2].clone()),
        (quoted_assignment(&runtime, "swap")?, sns_ids[3].clone()),
        (quoted_assignment(&runtime, "index")?, sns_ids[4].clone()),
    ];
    let governance = Principal::from_text(&sns_ids[1])
        .map_err(|error| format!("invalid allocated SNS Governance principal: {error}"))?;
    let treasury_subaccount = crate::sns_distribution_subaccount(governance, 0);
    for relative in [
        "local-vars.toml",
        "runtime.local.toml",
        "sns_init.local.yaml",
        "io_stream_manager.did",
        "io_nns_neuron_manager.did",
        "canister-ids.local.toml",
    ] {
        let path = inputs.join(relative);
        let mut text = fs::read_to_string(&path).map_err(|error| {
            format!("failed to read lifecycle input {}: {error}", path.display())
        })?;
        for (planned, allocated) in &replacements {
            text = text.replace(planned, allocated);
        }
        if relative == "io_stream_manager.did" {
            text = replace_single_assignment_line(
                &text,
                "nonredeemable_governance_io_accounts",
                &format!(
                    "nonredeemable_governance_io_accounts = vec {{ record {{ owner = principal \"{}\"; subaccount = opt blob \"{}\" }} }};",
                    sns_ids[1],
                    treasury_subaccount
                        .as_bytes()
                        .chunks_exact(2)
                        .map(|pair| format!("\\{}", std::str::from_utf8(pair).expect("hex is ASCII")))
                        .collect::<String>()
                ),
            )?;
        }
        fs::write(&path, text).map_err(|error| {
            format!(
                "failed to rewrite lifecycle input {}: {error}",
                path.display()
            )
        })?;
    }
    Ok(())
}

fn run_lifecycle_profile(
    io_root: &Path,
    bundle: &ResolvedBundle,
    profile_run: &ProfileRun,
) -> Result<(), String> {
    if env::var("IO_LOCAL_SNS_FRESH_TOPOLOGY_ACK").as_deref() != Ok("fresh-owned") {
        return Err(
            "lifecycle requires IO_LOCAL_SNS_FRESH_TOPOLOGY_ACK=fresh-owned and a newly started, uniquely owned loopback topology"
                .into(),
        );
    }
    let server_url = env::var("IO_LOCAL_POCKET_IC_SERVER_URL").map_err(|_| {
        "lifecycle fresh-topology preflight requires IO_LOCAL_POCKET_IC_SERVER_URL".to_string()
    })?;
    let instance_id = env::var("IO_LOCAL_POCKET_IC_INSTANCE_ID").map_err(|_| {
        "lifecycle fresh-topology preflight requires IO_LOCAL_POCKET_IC_INSTANCE_ID".to_string()
    })?;
    let topology_path = env::var("IO_LOCAL_SNS_TOPOLOGY_FILE").map(PathBuf::from).map_err(|_| {
        "lifecycle requires IO_LOCAL_SNS_TOPOLOGY_FILE from the uniquely owned sns-testing-init state"
            .to_string()
    })?;
    let official_checkout = env::var("IO_LOCAL_SNS_IC_CHECKOUT")
        .map(PathBuf::from)
        .map_err(|_| {
            "lifecycle requires IO_LOCAL_SNS_IC_CHECKOUT naming the isolated clean pinned checkout"
                .to_string()
        })?;
    if !official_checkout.is_absolute() {
        return Err("IO_LOCAL_SNS_IC_CHECKOUT must be absolute".into());
    }
    let preflight = Command::new("cargo")
        .current_dir(io_root)
        .args([
            "run",
            "-p",
            "e2e-real-canisters",
            "--bin",
            "observe_existing_reward",
        ])
        .env("IO_LOCAL_ASSERT_FRESH_HOST_TIME_ONLY", "1")
        .env("IO_POCKET_IC_SERVER_URL", server_url)
        .env("IO_POCKET_IC_INSTANCE_ID", instance_id)
        .status()
        .map_err(|err| format!("failed to inspect fresh lifecycle topology time: {err}"))?;
    if !preflight.success() {
        return Err(format!(
            "lifecycle fresh-topology time preflight failed with {preflight}"
        ));
    }
    let bundle_dir = bundle
        .manifest
        .parent()
        .ok_or_else(|| "resolved SNS bundle has no parent directory".to_string())?;
    let runbook = io_root.join("deploy/local-sns-rehearsal/runbook.sh");
    if profile_run.lifecycle_root.exists() {
        return Err(format!(
            "lifecycle run root already exists; refusing a reused topology state: {}",
            profile_run.lifecycle_root.display()
        ));
    }
    let inputs = profile_run.lifecycle_root.join("inputs");
    let generated = profile_run.lifecycle_root.join("generated");
    fs::create_dir_all(inputs.join("assets")).map_err(|error| {
        format!(
            "failed to create fresh lifecycle inputs {}: {error}",
            inputs.display()
        )
    })?;
    fs::create_dir_all(&generated).map_err(|error| {
        format!(
            "failed to create fresh lifecycle output {}: {error}",
            generated.display()
        )
    })?;
    let rehearsal = io_root.join("deploy/local-sns-rehearsal");
    let copied_inputs = [
        ("local-vars.toml", "local-vars.toml"),
        ("runtime.local.toml", "runtime.local.toml"),
        ("sns_init.local.yaml", "sns_init.local.yaml"),
        ("assets/io-local-logo.svg", "assets/io-local-logo.svg"),
        (
            "assets/io-local-token-logo.svg",
            "assets/io-local-token-logo.svg",
        ),
        (
            "install-args.local/io_stream_manager.did",
            "io_stream_manager.did",
        ),
        (
            "install-args.local/io_nns_neuron_manager.did",
            "io_nns_neuron_manager.did",
        ),
        ("canister-ids.local.toml", "canister-ids.local.toml"),
    ];
    for (source, destination) in copied_inputs {
        let source = rehearsal.join(source);
        let destination = inputs.join(destination);
        fs::copy(&source, &destination).map_err(|error| {
            format!(
                "fresh lifecycle requires reviewed local input {}: {error}",
                source.display()
            )
        })?;
    }
    rewrite_lifecycle_allocations(&inputs, &topology_path)?;
    eprintln!(
        "Fresh lifecycle run directory: {}",
        profile_run.lifecycle_root.display()
    );
    for phase in LIFECYCLE_PHASES {
        let status = Command::new(&runbook)
            .current_dir(io_root)
            .arg(phase)
            .env("IO_LOCAL_SNS_REHEARSAL_ACK", "local-only")
            .env("IO_LOCAL_SNS_BUNDLE_DIR", bundle_dir)
            .env("IO_LOCAL_SNS_GENERATED_DIR", &generated)
            .env(
                "IO_LOCAL_SNS_LOCAL_VARS_FILE",
                inputs.join("local-vars.toml"),
            )
            .env(
                "IO_LOCAL_SNS_RUNTIME_FILE",
                inputs.join("runtime.local.toml"),
            )
            .env("IO_LOCAL_SNS_INIT_FILE", inputs.join("sns_init.local.yaml"))
            .env(
                "IO_LOCAL_SNS_STREAM_ARGS_FILE",
                inputs.join("io_stream_manager.did"),
            )
            .env(
                "IO_LOCAL_SNS_NNS_ARGS_FILE",
                inputs.join("io_nns_neuron_manager.did"),
            )
            .env(
                "IO_LOCAL_SNS_CANISTER_EVIDENCE_FILE",
                inputs.join("canister-ids.local.toml"),
            )
            .env("IO_LOCAL_SNS_IC_CHECKOUT", &official_checkout)
            .status()
            .map_err(|err| format!("failed to run lifecycle phase {phase}: {err}"))?;
        if !status.success() {
            return Err(format!(
                "official local rehearsal lifecycle phase {phase} failed with {status}"
            ));
        }
    }
    Ok(())
}

fn run_exact_unit(
    io_root: &Path,
    bundle: &ResolvedBundle,
    profile_run: &ProfileRun,
    package: &str,
    test: &str,
) -> Result<(), String> {
    let listed = cargo_list_output(
        io_root,
        bundle,
        profile_run,
        &["test", "-p", package, "--", "--list"],
    )?;
    require_exact_test_match(&listed, test)?;
    run_cargo(
        io_root,
        bundle,
        profile_run,
        &["test", "-p", package, test, "--", "--exact", "--nocapture"],
    )
}

fn build_io_profile_wasms(
    io_root: &Path,
    bundle: &ResolvedBundle,
    profile_run: &ProfileRun,
) -> Result<(), String> {
    run_cargo(
        io_root,
        bundle,
        profile_run,
        &[
            "build",
            "-p",
            "io-stream-manager",
            "-p",
            "mock-nns-governance",
            "--target",
            "wasm32-unknown-unknown",
        ],
    )
}

fn run_exact_ignored(
    io_root: &Path,
    bundle: &ResolvedBundle,
    profile_run: &ProfileRun,
    test: &str,
) -> Result<(), String> {
    if profile_run.pocket_ic_bin.is_none() {
        return Err(
            "profile requires POCKET_IC_BIN; expected /home/codexdev/.local/bin/pocket-ic-server for local IO validation"
                .to_string(),
        );
    }
    let qualified = format!("tests::{test}");
    for command in exact_profile_commands(&qualified) {
        let args = command.args.iter().map(String::as_str).collect::<Vec<_>>();
        match command.output {
            ProfileOutput::CaptureSmall => {
                let listed = cargo_list_output(io_root, bundle, profile_run, &args)?;
                require_exact_test_match(&listed, &qualified)?;
            }
            ProfileOutput::Stream => run_cargo(io_root, bundle, profile_run, &args)?,
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProfileOutput {
    CaptureSmall,
    Stream,
}

#[derive(Debug, Eq, PartialEq)]
struct ExactProfileCommand {
    output: ProfileOutput,
    args: Vec<String>,
}

fn exact_profile_commands(qualified: &str) -> [ExactProfileCommand; 2] {
    [
        ExactProfileCommand {
            output: ProfileOutput::CaptureSmall,
            args: [
                "test",
                "-p",
                "e2e-real-canisters",
                "--",
                "--ignored",
                "--list",
            ]
            .into_iter()
            .map(str::to_string)
            .collect(),
        },
        ExactProfileCommand {
            output: ProfileOutput::Stream,
            args: [
                "test",
                "-p",
                "e2e-real-canisters",
                qualified,
                "--",
                "--ignored",
                "--exact",
                "--nocapture",
            ]
            .into_iter()
            .map(str::to_string)
            .collect(),
        },
    ]
}

fn require_exact_test_match(list_output: &str, qualified: &str) -> Result<(), String> {
    let matches = list_output
        .lines()
        .filter(|line| line.trim() == format!("{qualified}: test"))
        .count();
    if matches == 1 {
        Ok(())
    } else {
        Err(format!(
            "profile exact test {qualified:?} matched {matches} tests; expected exactly one"
        ))
    }
}

fn cargo_list_output(
    io_root: &Path,
    bundle: &ResolvedBundle,
    profile_run: &ProfileRun,
    args: &[&str],
) -> Result<String, String> {
    let mut command = cargo_command(io_root, bundle, profile_run, args);
    let output = command
        .output()
        .map_err(|err| format!("failed to run cargo {}: {err}", args.join(" ")))?;
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    print!("{combined}");
    if output.status.success() {
        Ok(combined)
    } else {
        Err(format!(
            "cargo {} failed with {}",
            args.join(" "),
            output.status
        ))
    }
}

fn run_cargo(
    io_root: &Path,
    bundle: &ResolvedBundle,
    profile_run: &ProfileRun,
    args: &[&str],
) -> Result<(), String> {
    let status = cargo_command(io_root, bundle, profile_run, args)
        .status()
        .map_err(|err| format!("failed to run cargo {}: {err}", args.join(" ")))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("cargo {} failed with {status}", args.join(" ")))
    }
}

fn cargo_command(
    io_root: &Path,
    bundle: &ResolvedBundle,
    profile_run: &ProfileRun,
    args: &[&str],
) -> Command {
    let mut command = Command::new("cargo");
    command
        .current_dir(io_root)
        .args(args)
        .env("IO_REAL_SNS_WASM_DIR", &bundle.wasm_dir)
        .env("IO_REAL_SNS_WASM_MANIFEST", &bundle.manifest)
        .env("IO_POCKETIC_RUN_ID", &profile_run.id);
    if env::var_os("CARGO_BUILD_JOBS").is_none() {
        command.env("CARGO_BUILD_JOBS", "2");
    }
    if env::var_os("RUST_TEST_THREADS").is_none() {
        command.env("RUST_TEST_THREADS", "1");
    }
    if let Some(pocket_ic_bin) = &profile_run.pocket_ic_bin {
        command.env("POCKET_IC_BIN", pocket_ic_bin);
    }
    command
}

fn print_summary(bundle: &ResolvedBundle) {
    let overrides = if bundle.overrides.is_empty() {
        "none".to_string()
    } else {
        bundle.overrides.join(", ")
    };
    println!("SNS source: {}", bundle.source.as_str());
    if !bundle.ic_commit.is_empty() {
        println!("IC commit: {}", bundle.ic_commit);
        match bundle.clean {
            Some(true) => println!("IC worktree: clean"),
            Some(false) => println!("IC worktree: dirty (diff SHA-256 {})", bundle.diff_hash),
            None => {}
        }
    }
    println!("Scope: {}", bundle.scope.as_str());
    println!("Official base: {}", bundle.official_baseline);
    println!("Component overrides: {overrides}");
    println!("Governance Wasm SHA-256: {}", bundle.governance_hash);
    if !bundle.governance_did_hash.is_empty() {
        println!("Governance DID SHA-256: {}", bundle.governance_did_hash);
    }
    if let Ok(manifest) = SnsManifest::read(&bundle.manifest) {
        for component in ALL_ARTIFACTS {
            if let (Some(raw), Some(source)) = (
                manifest.artifact(component, "sha256"),
                manifest.artifact(component, "source_sha256"),
            ) {
                println!("{component} raw/source SHA-256: {raw} / {source}");
            }
        }
        if let Some(root_did) = manifest.value("contract", "root_did_sha256") {
            println!("Root DID SHA-256: {root_did}");
        }
    }
    println!("Capability {}: {}", CAPABILITY_FIELD, bundle.capability);
    println!("Resolved manifest: {}", bundle.manifest.display());
    println!("Profile: {}", bundle.profile.as_str());
}

fn validate_official_lock(lock: &SnsManifest) -> Result<(), String> {
    let baseline = required_value(lock, "metadata", "version")?;
    if baseline.trim().is_empty() || baseline.contains("latest") {
        return Err("official baseline must be a non-moving reviewed identifier".into());
    }
    for component in ALL_ARTIFACTS {
        let filename = required_artifact(lock, component, "wasm")?;
        validate_artifact_filename(filename)?;
        validate_sha(required_artifact(lock, component, "sha256")?)?;
        validate_sha(required_artifact(lock, component, "source_sha256")?)?;
        let url = required_artifact(lock, component, "source_url")?;
        if !url.starts_with("https://download.dfinity.systems/")
            && !url.starts_with("https://github.com/")
        {
            return Err(format!(
                "official {component} has unapproved source URL {url}"
            ));
        }
        let revision = required_artifact(lock, component, "upstream_rev")?;
        if revision.len() != 40 || !revision.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(format!("official {component} revision is not full 40-hex"));
        }
    }
    match lock.bool_value("capabilities", CAPABILITY_FIELD) {
        Some(true) => {
            let path = required_value(lock, "contract", "governance_did")?;
            reject_relative_unsafe(Path::new(path))?;
            validate_sha(required_value(lock, "contract", "governance_did_sha256")?)
        }
        Some(false) => Ok(()),
        _ => Err(format!(
            "official lock must declare [capabilities].{CAPABILITY_FIELD} as a boolean"
        )),
    }
}

fn validate_bundle(root: &Path) -> Result<(), String> {
    if !root.is_dir() {
        return Err(format!("bundle is not a directory: {}", root.display()));
    }
    let mut allowed_top = BTreeSet::from([
        "SHA256SUMS",
        "governance.did",
        "manifest.toml",
        "provenance.toml",
        "root.did",
        "wasms",
    ]);
    for entry in fs::read_dir(root)
        .map_err(|err| format!("failed to read bundle {}: {err}", root.display()))?
    {
        let entry = entry.map_err(|err| format!("failed to read bundle entry: {err}"))?;
        let name = entry.file_name();
        let name = name
            .to_str()
            .ok_or_else(|| "bundle contains a non-UTF-8 filename".to_string())?;
        if !allowed_top.remove(name) {
            return Err(format!(
                "bundle contains unexpected top-level entry {name:?}"
            ));
        }
        let metadata = fs::symlink_metadata(entry.path())
            .map_err(|err| format!("failed to inspect bundle entry: {err}"))?;
        if metadata.file_type().is_symlink() {
            return Err(format!("bundle symlink is forbidden: {name}"));
        }
    }
    for required in ["SHA256SUMS", "manifest.toml", "provenance.toml", "wasms"] {
        if !root.join(required).exists() {
            return Err(format!("bundle is missing required {required}"));
        }
    }
    let wasms = root.join("wasms");
    if !wasms.is_dir() {
        return Err("bundle wasms entry is not a directory".into());
    }
    for entry in
        fs::read_dir(&wasms).map_err(|err| format!("failed to read {}: {err}", wasms.display()))?
    {
        let entry = entry.map_err(|err| format!("failed to read Wasm entry: {err}"))?;
        let metadata = fs::symlink_metadata(entry.path())
            .map_err(|err| format!("failed to inspect Wasm entry: {err}"))?;
        let name = entry.file_name();
        let name = name
            .to_str()
            .ok_or_else(|| "bundle contains a non-UTF-8 Wasm filename".to_string())?;
        validate_artifact_filename(name)?;
        if !metadata.file_type().is_file() {
            return Err(format!("bundle Wasm must be a regular file: {name}"));
        }
    }
    let manifest = SnsManifest::read(&root.join("manifest.toml"))?;
    if let Some(expected) = manifest.value("contract", "root_did_sha256") {
        verify_hash(&root.join("root.did"), expected)?;
    } else if root.join("root.did").is_file() {
        return Err("root.did is not hash-bound by the manifest".into());
    }
    if manifest.value("variant", "ic_repository").is_some()
        || manifest.value("variant", "ic_remotes").is_some()
    {
        return Err("bundle provenance must not contain local paths or raw remotes".into());
    }
    let local_only = manifest
        .bool_value("variant", "local_only")
        .unwrap_or(false);
    let exportable = manifest.bool_value("variant", "exportable").unwrap_or(true);
    if local_only == exportable {
        return Err("bundle local_only/exportable provenance is incoherent".into());
    }
    let mut expected_wasms = BTreeSet::new();
    for component in ALL_ARTIFACTS {
        let filename = required_artifact(&manifest, component, "wasm")?;
        validate_artifact_filename(filename)?;
        expected_wasms.insert(filename.to_string());
        verify_hash(
            &wasms.join(filename),
            required_artifact(&manifest, component, "sha256")?,
        )?;
        let source_name = manifest
            .artifact(component, "source_filename")
            .or_else(|| {
                manifest
                    .artifact(component, "source_url")
                    .and_then(|url| url.rsplit('/').next())
            })
            .ok_or_else(|| format!("manifest is missing {component} source filename"))?;
        validate_artifact_filename(source_name)?;
        expected_wasms.insert(source_name.to_string());
        verify_hash(
            &wasms.join(source_name),
            required_artifact(&manifest, component, "source_sha256")?,
        )?;
    }
    if let Some(baseline_name) = manifest.value("baseline", "sns_governance_wasm") {
        validate_artifact_filename(baseline_name)?;
        expected_wasms.insert(baseline_name.to_string());
        verify_hash(
            &wasms.join(baseline_name),
            required_value(&manifest, "baseline", "sns_governance_sha256")?,
        )?;
        let source_name = required_value(&manifest, "baseline", "sns_governance_source_wasm")?;
        validate_artifact_filename(source_name)?;
        expected_wasms.insert(source_name.to_string());
        verify_hash(
            &wasms.join(source_name),
            required_value(&manifest, "baseline", "sns_governance_source_sha256")?,
        )?;
    }
    let actual_wasms = fs::read_dir(&wasms)
        .map_err(|err| format!("failed to read {}: {err}", wasms.display()))?
        .map(|entry| {
            entry
                .map_err(|err| format!("failed to read Wasm entry: {err}"))?
                .file_name()
                .into_string()
                .map_err(|_| "bundle contains a non-UTF-8 Wasm filename".to_string())
        })
        .collect::<Result<BTreeSet<_>, _>>()?;
    if actual_wasms != expected_wasms {
        return Err(format!(
            "bundle Wasm file set mismatch; expected={expected_wasms:?}, actual={actual_wasms:?}"
        ));
    }
    verify_sha256sums(root)
}

fn verify_candidate_did(text: &str) -> Result<(), String> {
    for required in [
        "type Uint128 = record",
        "high : nat64",
        "low : nat64",
        "type RewardEventParticipation = record",
        "reward_event_end_timestamp_seconds : nat64",
        "reward_shares : opt Uint128",
        "latest_reward_event_participation : opt RewardEventParticipation",
        "get_latest_reward_event : () -> (RewardEvent) query",
        "list_neurons : (ListNeurons) -> (ListNeuronsResponse) query",
    ] {
        if !text.contains(required) {
            return Err(format!(
                "candidate Governance DID does not prove required additive contract: missing {required:?}"
            ));
        }
    }
    Ok(())
}

fn did_has_reward_participation(text: &str) -> bool {
    text.contains("latest_reward_event_participation : opt RewardEventParticipation")
}

fn official_artifacts_available(manifest: &SnsManifest, directory: &Path) -> Result<bool, String> {
    if !directory.is_dir() {
        return Ok(false);
    }
    for component in ALL_ARTIFACTS {
        let filename = required_artifact(manifest, component, "wasm")?;
        let path = directory.join(filename);
        if !path.is_file()
            || verify_hash(&path, required_artifact(manifest, component, "sha256")?).is_err()
        {
            return Ok(false);
        }
        let source_name = manifest.source_artifact_name(component)?;
        let source_path = directory.join(source_name);
        if !source_path.is_file()
            || verify_hash(
                &source_path,
                required_artifact(manifest, component, "source_sha256")?,
            )
            .is_err()
        {
            return Ok(false);
        }
    }
    Ok(true)
}

fn copy_manifest_artifacts(
    manifest: &SnsManifest,
    source: &Path,
    destination: &Path,
) -> Result<(), String> {
    for component in ALL_ARTIFACTS {
        let filename = required_artifact(manifest, component, "wasm")?;
        let from = source.join(filename);
        verify_hash(&from, required_artifact(manifest, component, "sha256")?)?;
        fs::copy(&from, destination.join(filename))
            .map_err(|err| format!("failed to copy {}: {err}", from.display()))?;
        let source_name = manifest.source_artifact_name(component)?;
        let source_path = source.join(&source_name);
        verify_hash(
            &source_path,
            required_artifact(manifest, component, "source_sha256")?,
        )?;
        fs::copy(&source_path, destination.join(&source_name)).map_err(|err| {
            format!(
                "failed to copy source artifact {}: {err}",
                source_path.display()
            )
        })?;
    }
    Ok(())
}

fn ensure_new_staging(path: &Path) -> Result<(), String> {
    if path.exists() {
        return Err(format!(
            "stale resolver staging path exists: {}; inspect and remove it manually",
            path.display()
        ));
    }
    fs::create_dir(path)
        .map_err(|err| format!("failed to create staging {}: {err}", path.display()))
}

fn publish_staging(staging: PathBuf, destination: &Path) -> Result<(), String> {
    if destination.exists() {
        validate_bundle(destination)?;
        let staged_manifest = sha256_file(&staging.join("manifest.toml"))?;
        let published_manifest = sha256_file(&destination.join("manifest.toml"))?;
        if staged_manifest != published_manifest {
            return Err(format!(
                "bundle ID collision at {}; staged manifest {staged_manifest}, published manifest {published_manifest}",
                destination.display()
            ));
        }
        fs::remove_dir_all(&staging).map_err(|err| {
            format!(
                "failed to remove resolver-owned duplicate staging {}: {err}",
                staging.display()
            )
        })?;
        return Ok(());
    }
    fs::rename(&staging, destination).map_err(|err| {
        format!(
            "failed to publish immutable bundle {} -> {}: {err}",
            staging.display(),
            destination.display()
        )
    })
}

fn write_sha256sums(root: &Path) -> Result<(), String> {
    let mut files = Vec::new();
    collect_regular_files(root, root, &mut files)?;
    files.retain(|path| path != Path::new("SHA256SUMS"));
    files.sort();
    let mut text = String::new();
    for relative in files {
        let hash = sha256_file(&root.join(&relative))?;
        text.push_str(&format!("{hash}  {}\n", relative.display()));
    }
    fs::write(root.join("SHA256SUMS"), text)
        .map_err(|err| format!("failed to write SHA256SUMS: {err}"))
}

fn verify_sha256sums(root: &Path) -> Result<(), String> {
    let text = fs::read_to_string(root.join("SHA256SUMS"))
        .map_err(|err| format!("failed to read SHA256SUMS: {err}"))?;
    let mut listed = BTreeSet::new();
    for (line_no, line) in text.lines().enumerate() {
        let (hash, path) = line
            .split_once("  ")
            .ok_or_else(|| format!("SHA256SUMS line {} is malformed", line_no + 1))?;
        validate_sha(hash)?;
        let relative = Path::new(path);
        reject_relative_unsafe(relative)?;
        if relative.is_absolute() || !listed.insert(relative.to_path_buf()) {
            return Err(format!("invalid or duplicate SHA256SUMS path {path:?}"));
        }
        verify_hash(&root.join(relative), hash)?;
    }
    let mut actual = Vec::new();
    collect_regular_files(root, root, &mut actual)?;
    actual.retain(|path| path != Path::new("SHA256SUMS"));
    let actual = actual.into_iter().collect::<BTreeSet<_>>();
    if actual != listed {
        return Err(format!(
            "SHA256SUMS file set mismatch; listed={listed:?}, actual={actual:?}"
        ));
    }
    Ok(())
}

fn collect_regular_files(
    root: &Path,
    current: &Path,
    out: &mut Vec<PathBuf>,
) -> Result<(), String> {
    for entry in fs::read_dir(current)
        .map_err(|err| format!("failed to read {}: {err}", current.display()))?
    {
        let entry = entry.map_err(|err| format!("failed to read directory entry: {err}"))?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)
            .map_err(|err| format!("failed to inspect {}: {err}", path.display()))?;
        if metadata.file_type().is_symlink() {
            return Err(format!("bundle symlink is forbidden: {}", path.display()));
        }
        if metadata.is_dir() {
            collect_regular_files(root, &path, out)?;
        } else if metadata.is_file() {
            out.push(
                path.strip_prefix(root)
                    .map_err(|_| "bundle traversal escaped root".to_string())?
                    .to_path_buf(),
            );
        } else {
            return Err(format!("bundle entry is not regular: {}", path.display()));
        }
    }
    Ok(())
}

fn required_value<'a>(
    manifest: &'a SnsManifest,
    section: &str,
    key: &str,
) -> Result<&'a str, String> {
    manifest
        .value(section, key)
        .ok_or_else(|| format!("manifest is missing [{section}] {key}"))
}

fn required_artifact<'a>(
    manifest: &'a SnsManifest,
    component: &str,
    field: &str,
) -> Result<&'a str, String> {
    manifest
        .artifact(component, field)
        .ok_or_else(|| format!("manifest is missing artifacts.{component}_{field}"))
}

fn validate_artifact_filename(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.starts_with('.')
        || value.contains('/')
        || value.contains('\\')
        || value.contains("..")
        || value.to_ascii_lowercase().contains("secret")
        || value.to_ascii_lowercase().contains("identity")
        || value.to_ascii_lowercase().contains("token")
        || value.ends_with(".pem")
        || value == ".env"
    {
        return Err(format!("unsafe artifact filename {value:?}"));
    }
    Ok(())
}

fn reject_relative_unsafe(path: &Path) -> Result<(), String> {
    for component in path.components() {
        if matches!(component, Component::ParentDir) {
            return Err(format!("path traversal is forbidden: {}", path.display()));
        }
    }
    Ok(())
}

fn validate_sha(value: &str) -> Result<(), String> {
    if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(format!("invalid SHA-256 {value:?}"))
    }
}

fn verify_hash(path: &Path, expected: &str) -> Result<(), String> {
    validate_sha(expected)?;
    let actual = sha256_file(path)?;
    if actual.eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        Err(format!(
            "{} SHA-256 mismatch: expected {expected}, got {actual}",
            path.display()
        ))
    }
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file =
        fs::File::open(path).map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(hex::encode(digest.finalize()))
}

fn sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn command_output(command: &mut Command) -> Result<String, String> {
    let description = format!("{command:?}");
    let output = command
        .output()
        .map_err(|err| format!("failed to run {description}: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "command {description} failed with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    String::from_utf8(output.stdout)
        .map_err(|err| format!("command {description} returned non-UTF-8 output: {err}"))
}

fn command_bytes(command: &mut Command) -> Result<Vec<u8>, String> {
    let description = format!("{command:?}");
    let output = command
        .output()
        .map_err(|err| format!("failed to run {description}: {err}"))?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(format!(
            "command {description} failed with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

fn command_sha256(command: &mut Command) -> Result<String, String> {
    let description = format!("{command:?}");
    let mut child = command
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|error| format!("failed to run {description}: {error}"))?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| format!("command {description} has no stdout pipe"))?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = stdout
            .read(&mut buffer)
            .map_err(|error| format!("failed to read {description}: {error}"))?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    let status = child
        .wait()
        .map_err(|error| format!("failed to wait for {description}: {error}"))?;
    if status.success() {
        Ok(hex::encode(digest.finalize()))
    } else {
        Err(format!("command {description} failed with {status}"))
    }
}

fn decompress_gzip(source: &Path, destination: &Path) -> Result<(), String> {
    let output = fs::File::create(destination)
        .map_err(|err| format!("failed to create {}: {err}", destination.display()))?;
    let status = Command::new("gzip")
        .args([OsStr::new("-dc"), source.as_os_str()])
        .stdout(Stdio::from(output))
        .status()
        .map_err(|err| format!("failed to invoke gzip: {err}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "gzip failed for {} with {status}",
            source.display()
        ))
    }
}

fn escape_toml(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn safe_id(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_override_environment_defaults() {
        let parsed = parse_options(&[
            "--source".into(),
            "bundle".into(),
            "--bundle".into(),
            "/tmp/example".into(),
            "--profile".into(),
            "upgrade".into(),
        ])
        .unwrap();
        assert_eq!(parsed.source, Source::Bundle);
        assert_eq!(parsed.profile, Profile::Upgrade);
    }

    #[test]
    fn only_implemented_scope_and_profiles_are_advertised() {
        assert!(Scope::parse("sns-suite").is_err());
        assert_eq!(
            Scope::parse("governance-root").unwrap(),
            Scope::GovernanceRoot
        );
        assert!(Profile::parse("all").is_err());
        assert_eq!(
            shared_profile_plan(Profile::Lifecycle, true).unwrap(),
            vec![ProfileStep::Lifecycle]
        );
        assert!(shared_profile_plan(Profile::Lifecycle, false).is_err());
    }

    #[test]
    fn lifecycle_orders_every_host_signed_phase_before_reward_time_advance() {
        assert_eq!(LIFECYCLE_PHASES.last(), Some(&"package-evidence"));
        assert_eq!(
            &LIFECYCLE_PHASES[LIFECYCLE_PHASES.len() - 3..],
            &[
                "observe-one-day-reward",
                "exercise-account-semantic-protocol",
                "package-evidence"
            ]
        );
        for phase in [
            "bootstrap-official-network",
            "deploy-local-dapps",
            "propose-and-finalize-sns",
            "discover-sns-canisters",
            "exercise-ledger",
            "exercise-index-and-archives",
            "exercise-governance-and-controllers",
        ] {
            assert!(
                LIFECYCLE_PHASES
                    .iter()
                    .position(|candidate| candidate == &phase)
                    < LIFECYCLE_PHASES
                        .iter()
                        .position(|candidate| candidate == &"observe-one-day-reward")
            );
        }
    }

    #[test]
    fn lifecycle_derives_fresh_sns_ids_from_the_owned_topology() {
        let topology: serde_json::Value = serde_json::from_str(
            r#"{"subnet_configs":[{"subnet_kind":"SNS","alloc_range":{"start":"dllsh-pd777-77776-qaaaa-cai"}}]}"#,
        )
        .unwrap();
        assert_eq!(
            topology_allocation_ids(&topology, "SNS", 5).unwrap(),
            [
                "dllsh-pd777-77776-qaaaa-cai",
                "dmkut-c3777-77776-qaaaq-cai",
                "dfj7p-ut777-77776-qaaba-cai",
                "dciz3-zl777-77776-qaabq-cai",
                "dxpiw-yd777-77776-qaaca-cai",
            ]
        );
    }

    #[test]
    fn candidate_did_requires_the_additive_field_and_queries() {
        let did = "type Uint128 = record { high : nat64; low : nat64 };\n\
            type RewardEventParticipation = record { reward_event_end_timestamp_seconds : nat64; reward_shares : opt Uint128 };\n\
            type RewardEvent = record {}; type ListNeurons = record {}; type ListNeuronsResponse = record {};\n\
            type Neuron = record { latest_reward_event_participation : opt RewardEventParticipation };\n\
            service : { get_latest_reward_event : () -> (RewardEvent) query; list_neurons : (ListNeurons) -> (ListNeuronsResponse) query }";
        verify_candidate_did(did).unwrap();
        assert!(verify_candidate_did(&did.replace("reward_shares", "old_shares")).is_err());
    }

    #[test]
    fn artifact_names_reject_traversal_and_secret_material() {
        for name in ["../x.wasm", "nested/x.wasm", "identity.pem", ".env"] {
            assert!(validate_artifact_filename(name).is_err(), "{name}");
        }
        validate_artifact_filename("sns_governance.wasm").unwrap();
    }

    #[test]
    fn official_local_and_bundle_sources_share_one_profile_implementation() {
        let expected = shared_profile_plan(Profile::Io, true).unwrap();
        for source in [Source::Official, Source::Local, Source::Bundle] {
            let resolved_manifest_is_the_only_source_input = source;
            assert_eq!(
                shared_profile_plan(Profile::Io, true).unwrap(),
                expected,
                "{resolved_manifest_is_the_only_source_input:?} selected a different test implementation"
            );
        }
        assert!(expected.contains(&ProfileStep::RewardBoundary));
        assert!(expected.contains(&ProfileStep::DtoCompatibility));
        assert!(expected.contains(&ProfileStep::CandidateContract));
        assert!(expected.contains(&ProfileStep::IoIntegration));
    }

    #[test]
    fn misspelled_exact_profile_test_is_rejected() {
        let listed = "tests::candidate_reward_shares_drive_io_rewards: test\n";
        assert!(
            require_exact_test_match(listed, "tests::candidate_reward_share_drive_io_rewards")
                .is_err()
        );
        assert!(require_exact_test_match(
            listed,
            "tests::candidate_reward_shares_drive_io_rewards"
        )
        .is_ok());
    }

    #[test]
    fn exact_test_discovery_is_captured_but_execution_is_streamed() {
        let commands = exact_profile_commands("tests::candidate_contract");
        assert_eq!(commands[0].output, ProfileOutput::CaptureSmall);
        assert!(commands[0].args.iter().any(|arg| arg == "--list"));
        assert_eq!(commands[1].output, ProfileOutput::Stream);
        assert!(commands[1].args.iter().any(|arg| arg == "--nocapture"));
        assert!(!commands[1].args.iter().any(|arg| arg == "--list"));
    }

    #[test]
    fn pocket_ic_wrapper_records_the_new_session_leader() {
        let wrapper = pocket_ic_wrapper(
            Path::new("/tmp/io sns run.pid"),
            Path::new("/tmp/pocket ic server"),
        );
        let setsid = wrapper.find("exec setsid sh -c").unwrap();
        let write_pid = wrapper.find("printf \"%s\\n\" \"$$\"").unwrap();
        assert!(setsid < write_pid);
        assert!(wrapper.contains(">> \"$IO_SNS_RUN_PID_FILE\""));
        assert!(wrapper.contains("exec \"$IO_SNS_RUN_POCKET_IC\" \"$@\""));
        assert!(wrapper.contains("'/tmp/io sns run.pid'"));
        assert!(wrapper.contains("'/tmp/pocket ic server'"));
    }
}
