use super::*;
use io_sns_lifecycle::{
    verify_manifest_entry_paths, verify_upgrade_proposal_against_manifest, UpgradeProposalRequest,
};

#[test]
fn simplicity_normative_guard_rejects_accidental_execution_promises() {
    for phrase in [
        "At most one external effect per invocation.",
        "One update submits at most one effect.",
        "Voting-power staleness pauses monetary work.",
        "Replay makes zero Governance calls.",
    ] {
        assert!(
            stale_normative_phrase(phrase).is_some(),
            "guard missed {phrase:?}"
        );
    }
    assert!(stale_normative_phrase(
        "Ambiguity stops dependent effects; voting-power refresh is best-effort housekeeping."
    )
    .is_none());
}

#[test]
fn pooled_claim_topology_requires_pinned_policy_and_shared_unresolved_accounts() {
    let stream = format!(
        "icp_ledger = principal \"{ICP_LEDGER_PRINCIPAL}\"\n\
         nns_manager = principal \"{PRODUCTION_IO_NNS_NEURON_MANAGER_CANISTER_ID}\"\n\
         jupiter_io_account = record {{ owner = principal \"{JUPITER_FAUCET_CANISTER_ID}\"; subaccount = opt TODO_JUPITER_IO_SUBACCOUNT }}\n\
         io_reserve = record {{ owner = principal \"{PRODUCTION_IO_STREAM_MANAGER_CANISTER_ID}\"; subaccount = opt TODO_IO_RESERVE_SUBACCOUNT }}\n\
         liquid_icp = record {{ owner = principal \"{PRODUCTION_IO_STREAM_MANAGER_CANISTER_ID}\"; subaccount = opt TODO_STREAM_LIQUID_SUBACCOUNT }}\n\
         expected_icp_fee_e8s = {ICP_TRANSFER_FEE_E8S} : nat"
    );
    let nns = format!(
        "stream_manager = principal \"{PRODUCTION_IO_STREAM_MANAGER_CANISTER_ID}\"\n\
         jupiter = principal \"{JUPITER_FAUCET_CANISTER_ID}\"\n\
         icp_ledger = principal \"{ICP_LEDGER_PRINCIPAL}\"\n\
         nns_governance = principal \"{NNS_GOVERNANCE_PRINCIPAL}\"\n\
         two_year_neuron_id = {PROTECTED_IO_NNS_NEURON_ID} : nat64\n\
         pooled_parent_memo = {PRODUCTION_POOLED_PARENT_MEMO} : nat64\n\
         pooled_parent_followee_id = {PROTECTED_IO_NNS_NEURON_ID} : nat64\n\
         jupiter_account = record {{ owner = principal \"{JUPITER_FAUCET_CANISTER_ID}\"; subaccount = null }}\n\
         jupiter_staging = record {{ owner = principal \"{PRODUCTION_IO_NNS_NEURON_MANAGER_CANISTER_ID}\"; subaccount = null }}\n\
         stream_liquid_account = record {{ owner = principal \"{PRODUCTION_IO_STREAM_MANAGER_CANISTER_ID}\"; subaccount = opt TODO_STREAM_LIQUID_SUBACCOUNT }}\n\
         expected_icp_fee_e8s = {ICP_TRANSFER_FEE_E8S} : nat\n\
         jupiter_activation_block_floor = TODO_JUPITER_ACTIVATION_BLOCK_FLOOR\n\
         audited_permanent_principal_e8s = TODO_AUDITED_PERMANENT_PRINCIPAL_E8S"
    );
    validate_pooled_claim_topology(&stream, &nns).unwrap();
    assert!(validate_pooled_claim_topology(
        &stream,
        &nns.replace("subaccount = null", "subaccount = opt TODO_WRONG"),
    )
    .is_err());
    assert!(validate_pooled_claim_topology(
        &stream.replace(PRODUCTION_IO_NNS_NEURON_MANAGER_CANISTER_ID, "aaaaa-aa"),
        &nns,
    )
    .is_err());
    assert!(validate_pooled_claim_topology(
        &stream,
        &nns.replace("pooled_parent_memo = 0", "pooled_parent_memo = 1"),
    )
    .is_err());
    assert!(validate_pooled_claim_topology(
        &stream,
        &nns.replace(
            &format!("pooled_parent_followee_id = {PROTECTED_IO_NNS_NEURON_ID}"),
            "pooled_parent_followee_id = 1",
        ),
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
    assert!(Command::new("git")
        .current_dir(&root)
        .args(["init", "--quiet"])
        .status()
        .unwrap()
        .success());
    write(&root, "source.txt", "first source tree\n");
    for args in [
        vec!["add", "source.txt"],
        vec![
            "-c",
            "user.name=IO xtask test",
            "-c",
            "user.email=io-xtask@example.invalid",
            "commit",
            "--quiet",
            "-m",
            "first source",
        ],
    ] {
        assert!(Command::new("git")
            .current_dir(&root)
            .args(args)
            .status()
            .unwrap()
            .success());
    }
    write(&root, "source.txt", "second source tree\n");
    assert!(Command::new("git")
        .current_dir(&root)
        .args(["add", "source.txt"])
        .status()
        .unwrap()
        .success());
    assert!(Command::new("git")
        .current_dir(&root)
        .args([
            "-c",
            "user.name=IO xtask test",
            "-c",
            "user.email=io-xtask@example.invalid",
            "commit",
            "--quiet",
            "-m",
            "second source",
        ])
        .status()
        .unwrap()
        .success());
    root
}

#[test]
fn obsolete_economics_guard_rejects_old_terms_from_active_root_files() {
    for (index, term) in [
        concat!("reward_backing", "_neuron_id"),
        concat!("seeded_two_week", "_principal_e8s"),
        concat!("two_week", "_receipt_source"),
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
        concat!("source_operation", "_id"),
        concat!("stream_receipt", "_fingerprint"),
        concat!("maturity_staging", " : Account"),
        concat!("maturity Mint", " block proof"),
        concat!("proved Mint", " determines the backed IO pool"),
        concat!("actual maturity", " Mint"),
        concat!("staging_balance", "_before_e8s"),
    ]
    .into_iter()
    .enumerate()
    {
        let root = temp_root(&format!("obsolete-active-{index}"));
        write(&root, "deploy/local-sns-rehearsal/active.toml", term);
        let error = check_obsolete_economics_guard_at(&root).unwrap_err();
        assert!(error.contains("obsolete active assumption"), "{error}");
        let _ = fs::remove_dir_all(root);
    }
}

#[test]
fn obsolete_economics_guard_preserves_immutable_dated_history() {
    let root = temp_root("obsolete-history");
    write(
        &root,
        "deploy/local-sns-rehearsal/evidence/2026-01-01/history.toml",
        concat!("reward_backing", "_neuron_id = 1\n"),
    );
    assert_eq!(check_obsolete_economics_guard_at(&root), Ok(()));
    let _ = fs::remove_dir_all(root);
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

fn selector_text(
    package: &str,
    source_commit: &str,
    artifact_commit: &str,
    release_manifest_sha256: &str,
    package_manifest_sha256: &str,
    package_sha256s_sha256: &str,
) -> String {
    format!(
            "[schema]\nversion = 1\n\n[current]\npackage = \"{package}\"\nio_release_source_commit = \"{source_commit}\"\nio_artifact_recording_commit = \"{artifact_commit}\"\nrelease_manifest_sha256 = \"{release_manifest_sha256}\"\npackage_manifest_sha256 = \"{package_manifest_sha256}\"\npackage_sha256s_sha256 = \"{package_sha256s_sha256}\"\n"
        )
}

fn dummy_selector_text(package: &str) -> String {
    selector_text(
        package,
        &"1".repeat(40),
        &"2".repeat(40),
        &"3".repeat(64),
        &"4".repeat(64),
        &"5".repeat(64),
    )
}

fn write_selector(root: &Path, package: &str) {
    write(
        root,
        CURRENT_CANONICAL_SELECTOR,
        &dummy_selector_text(package),
    );
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

fn copy_release_artifact_set(from: &Path, to: &Path) {
    fs::create_dir_all(to.join("release-artifacts")).unwrap();
    for name in expected_release_artifact_names() {
        fs::copy(
            from.join("release-artifacts").join(&name),
            to.join("release-artifacts").join(&name),
        )
        .unwrap();
    }
}

fn write_artifact_manifest(root: &Path, manifest: &ArtifactManifest) {
    let text = serde_json::to_string_pretty(manifest).unwrap();
    write(root, MANIFEST_PATH, &format!("{text}\n"));
}

fn read_artifact_manifest(root: &Path) -> ArtifactManifest {
    read_manifest(root).unwrap()
}

fn create_unreachable_commit(root: &Path) -> String {
    let tree = Command::new("git")
        .current_dir(root)
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
        .current_dir(root)
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
            "IO runs SNS-shaped mock/PocketIC tests, pinned real-canister profiles, and an optional maintained source-built local SNS-W rehearsal.\nWe do not currently run the official SNS launch locally in required CI.\nOfficial SNS testing is optional and heavier.\nThe current official ICP/DFINITY SNS testing documentation is the source of truth.\nThe historical standalone `dfinity/sns-testing` repository is deprecated.\nThe maintained official local SNS flow uses the source-built `sns` CLI; this is not part of required IO workflows.\nSNS testflight remains a separately authorized mainnet rehearsal.\nIO's canonical IO ledger should be the SNS ledger; any IO_TEST ledger is non-canonical.\nNNS Manager execution canister oae4c-3iaaa-aaaar-qb5qq-cai and two-year protected NNS neuron 10292412127977304661 are not touched by these tests.\nLayer 1\nLayer 2\nLayer 3\nLayer 4\n",
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
            "[source_open]\nstatus = \"incomplete\"\n[reproducible_builds]\nstatus = \"incomplete\"\n[security_review]\nstatus = \"incomplete\"\n[sns_config_validated]\nstatus = \"incomplete\"\n[local_sns_testing_rehearsal]\nstatus = \"incomplete\"\nevidence = \"same-source candidate Governance/Root compatibility; upstream non-blocking tooling defect\"\n[mainnet_testflight]\nstatus = \"incomplete\"\n[app_canisters_stable_on_mainnet]\nstatus = \"incomplete\"\n[nns_root_co_controller_step_planned]\nstatus = \"incomplete\"\n[fallback_controllers_defined]\nstatus = \"incomplete\"\n[dapp_canisters_listed]\nstatus = \"incomplete\"\n[sns_controlled_dapp_upgrade_path_proved]\nstatus = \"incomplete\"\n[official_reward_share_release]\nstatus = \"incomplete\"\nevidence = \"official reviewed SNS Governance release containing the capability\"\n[frontend_sns_integration_tested]\nstatus = \"incomplete\"\n[cycles_management_strategy]\nstatus = \"incomplete\"\n[custom_domain_frontend_plan]\nstatus = \"incomplete\"\n[audit_package]\nstatus = \"incomplete\"\n",
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
        ".gitignore",
        "deploy/local-sns-rehearsal/sns_init.local.yaml\n",
    );
    write(
            root,
            "deploy/local-sns-rehearsal/README.md",
            "local-only real SNS-created IO ledger/index/governance/root stack not final tokenomics not a mainnet SNS proposal not required CI Do not use `--network ic` protocol reserve reserve-to-user transfer prepared user-to-reserve redemption push validate_local_sns_rehearsal validate_local_sns_ledger validate_local_sns_scripts Human-readable local evidence-derived wiring Not accepted by production wiring validators Do not use as install args\n",
        );
    write(
            root,
            "deploy/local-sns-rehearsal/sns_init.local.template.yaml",
            "Local-only\nNot final tokenomics\nNot a mainnet SNS proposal\nfallback_controller_principals\n{{fallback_controller_principal}}\ndapp_canisters\nToken:\nsymbol: \"IOLO\"\ntransaction_fee\nDistribution:\ntreasury: \"800_000 tokens\"\nswap: \"100_000 tokens\"\nSwap:\n  start_time: null\nNnsProposal:\nTODO_LOCAL\n",
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
            "network = \"local\"\nsource = \"official-local-sns-rehearsal\"\nofficial_tooling = \"manual-local-only\"\n[toolchain_provenance]\nofficial_ic_source_commit = \"0123456789abcdef0123456789abcdef01234567\"\nsns_testing_source_path = \"rs/sns/testing\"\noperator_identity_principal = \"bd3sg-teaaa-aaaaa-qaaba-cai\"\nlocal_network_url = \"http://127.0.0.1:8080\"\nsns_cli_sha256 = \"TODO\"\nsns_testing_init_sha256 = \"TODO\"\nsns_testing_cli_sha256 = \"TODO\"\n[sns_canisters]\nroot = \"TODO\"\ngovernance = \"TODO\"\nledger = \"TODO\"\nindex = \"TODO\"\nswap = \"TODO\"\narchive = \"TODO\"\n[expected_local_sns_config]\ntoken_symbol = \"IOLO\"\ntransaction_fee_e8s = 10_000\ntotal_supply_e8s = 1\nprotocol_reserve_funding_amount_e8s = 1\n[ledger_evidence]\ntransaction_fee_e8s = 10_000\ntotal_supply_e8s = 1\nprotocol_reserve_balance_e8s = 1\nreserve_transfer_amount_e8s = 1\nredemption_push_amount_e8s = 1\nbad_fee_error_observed = true\ninsufficient_funds_error_observed = true\nduplicate_tested_transfer = \"transfer_reserve_to_user\"\nindex_account_history_observed = true\n[reserve_funding_transfer]\nsns_proposal_id = 1\nproposal_adopted = true\nproposal_executed = true\ncreated_at_time_nanos = \"none\"\nmemo_hex = \"none\"\nproof_source = \"SnsLedgerBlock\"\nproof_source_canister = \"TODO\"\nproof_method = \"Icrc3GetBlocks\"\narchive_canister = \"none\"\n[transfer_reserve_to_user]\nfrom_owner = \"TODO\"\nfrom_subaccount_hex = \"none\"\nto_owner = \"TODO\"\nto_subaccount_hex = \"none\"\nfee_disposition = \"burned\"\nsender_balance_before_e8s = 1\nrecipient_balance_after_e8s = 1\ntotal_supply_before_e8s = 1\ntotal_supply_after_e8s = 1\nproof_source = \"SnsIndexAccountHistory\"\nproof_source_canister = \"TODO\"\nproof_method = \"IcrcIndexGetAccountTransactions\"\narchive_canister = \"none\"\n[transfer_user_to_reserve]\nfrom_owner = \"TODO\"\nfrom_subaccount_hex = \"none\"\nto_owner = \"TODO\"\nto_subaccount_hex = \"none\"\nfee_disposition = \"burned\"\nsender_balance_before_e8s = 1\nrecipient_balance_after_e8s = 1\ntotal_supply_before_e8s = 1\ntotal_supply_after_e8s = 1\nproof_source = \"SnsIndexAccountHistory\"\nproof_source_canister = \"TODO\"\nproof_method = \"IcrcIndexGetAccountTransactions\"\narchive_canister = \"none\"\n[duplicate_test]\n[issuance_model]\nresolved_as = \"protocol_reserve_transfer\"\nminting_assumed = false\ntreasury_transfer_assumed = true\nfee_disposition_mode = \"burned\"\ntotal_supply_changes_explained = true\n",
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
        "deploy/local-sns-rehearsal/scripts/12-provision-local-nns-readiness.sh",
        "deploy/local-sns-rehearsal/scripts/13-propose-and-finalize-sns.sh",
        "deploy/local-sns-rehearsal/scripts/14-discover-sns-canisters.sh",
        "deploy/local-sns-rehearsal/scripts/15-exercise-ledger.sh",
        "deploy/local-sns-rehearsal/scripts/16-exercise-index-and-archives.sh",
        "deploy/local-sns-rehearsal/scripts/17-exercise-governance-and-controllers.sh",
        "deploy/local-sns-rehearsal/scripts/17-observe-one-day-reward.sh",
        "deploy/local-sns-rehearsal/scripts/18-exercise-account-semantic-protocol.sh",
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
            "deploy/local-sns-rehearsal/nns-governance-test.did",
            "// Local sns-testing\nservice : { update_neuron : (record {}) -> (opt record { error_message : text; error_type : int32 }) };\n",
        );
    write(
            root,
            "deploy/local-sns-rehearsal/scripts/12-provision-local-nns-readiness.sh",
            "#!/usr/bin/env bash\n# local-only optional\n# Requires IO_LOCAL_SNS_REHEARSAL_ACK=local-only.\nrequire_local_script_guard \"$@\"\n: \"${IO_LOCAL_SNS_REHEARSAL_ACK:?local-only}\"\n# rs/ledger_suite/icp/ledger.did query_blocks chain_length = \\([0-9_][0-9_]*\\) icrc1_transfer claim_or_refresh_neuron_from_account update_neuron 63115200 auto_stake_maturity = opt false maturity_disbursements_in_progress = opt vec {} two_year_neuron_id pooled_parent_memo pooled_parent_followee_id dynamic_anchor_target hostile_dust_e8s Dynamic staking subaccount dynamic_parent=seeded-unclaimed\n",
        );
    write(
            root,
            "deploy/local-sns-rehearsal/scripts/15-exercise-ledger.sh",
            "#!/usr/bin/env bash\n# local-only optional\n# Requires IO_LOCAL_SNS_REHEARSAL_ACK=local-only.\nrequire_local_script_guard \"$@\"\n: \"${IO_LOCAL_SNS_REHEARSAL_ACK:?local-only}\"\nprepare_response=\"$(dfx canister call \"$stream\" prepare_redemption \"$redeem_args\")\"\n# Err = variant { Busy }\ndfx canister call --candid io_stream_manager.did \"$stream\" resume '()'\ndfx canister call --candid io_stream_manager.did \"$stream\" resume_reward_backing '()'\ndfx canister call --candid io_nns_neuron_manager.did \"$nns_manager\" resume '()'\n# prepared redemption remained Busy after bounded production reconciliation recovery\n",
        );
    write(
            root,
            "deploy/local-sns-rehearsal/scripts/17-exercise-governance-and-controllers.sh",
            "#!/usr/bin/env bash\n# local-only optional\n# Requires IO_LOCAL_SNS_REHEARSAL_ACK=local-only.\nrequire_local_script_guard \"$@\"\n: \"${IO_LOCAL_SNS_REHEARSAL_ACK:?local-only}\"\n# upgrade-sns-controlled-canister submit_inline_sns_upgrade AddGenericNervousSystemFunction validate_set_paused ExecuteGenericNervousSystemFunction sns_governance_source_sha256 sns_root_source_sha256 sns_ledger_source_sha256 sns_index_source_sha256 sns_swap_source_sha256 same_release=true get_public_status configured = true gz_wasm_path gz_wasm_sha256 transport=gzip latest_pooled_target = null dynamic_parent=present excluded_dynamic_surplus_e8s observe_dynamic_backing_status\nhistorian_nns_manager_expected_hash=\"$(manifest_artifact_value io_nns_neuron_manager gz_wasm_sha256)\"\nhex_blob_literal \"$historian_nns_manager_expected_hash\"\nif ! phase_is_done 17-nns-activated; then :; fi\nif ! phase_is_done 17-stream-activated; then :; fi\n",
        );
    write(
            root,
            "deploy/local-sns-rehearsal/scripts/17-observe-one-day-reward.sh",
            "#!/usr/bin/env bash\n# local-only optional\n# Requires IO_LOCAL_SNS_REHEARSAL_ACK=local-only.\nrequire_local_script_guard \"$@\"\n: \"${IO_LOCAL_SNS_REHEARSAL_ACK:?local-only}\"\n# IO_LOCAL_REWARD_ADVANCE_SECONDS=86400 IO_LOCAL_REWARD_CANONICAL_TWO_EVENT=1 canonical_reward_observation_margin_seconds=300 pre_margin_resume_reward_work=Err( warmup_reward_margin_wait_seconds= warmup_reward_scheduler_epsilon_seconds=1 warmup_reward_observation_deadline_seconds= stream_status_after_warmup_margin= runtime_value accounts operator_principal require_hex_32_bytes neuron_state= IncreaseDissolveDelay DissolveDelaySeconds = 1209600 resume_reward_work ProposalBearing processed_reward_event_count: 2 accumulated_policy_credit: 2000000000000000000\n",
        );
    write(
            root,
            "deploy/local-sns-rehearsal/scripts/11-build-local-io-canisters.sh",
            "#!/usr/bin/env bash\n# local-only optional\n# Requires IO_LOCAL_SNS_REHEARSAL_ACK=local-only.\nrequire_local_script_guard \"$@\"\n# git -C \"$REPO_ROOT\" diff --quiet; artifact_commit=; git -C \"$REPO_ROOT\" show; tracked_clean=true\n",
        );
    write(
            root,
            "deploy/local-sns-rehearsal/scripts/18-exercise-account-semantic-protocol.sh",
            "#!/usr/bin/env bash\n# local-only optional\n# Requires IO_LOCAL_SNS_REHEARSAL_ACK=local-only.\nrequire_local_script_guard \"$@\"\n# semantic_staging_carries_late_value_into_the_next_cycle_for_both_roles controlled_jupiter_uses_real_nns_and_exact_production_receipts controlled_two_year_compounds_real_maturity_without_io_issuance exact_post_m70_upgrade_rewards_fourteen_day_boundary account_semantic_carry_forward kind=TwoWeek account_semantic_carry_forward kind=TwoYear obsolete_maturity_api\n",
        );
    write(
            root,
            "deploy/local-sns-rehearsal/scripts/18-package-evidence.sh",
            "#!/usr/bin/env bash\n# local-only optional\n# Requires IO_LOCAL_SNS_REHEARSAL_ACK=local-only.\nrequire_local_script_guard \"$@\"\nstage=$(mktemp -d)\n# validate_local_sns_evidence_package validate_local_sns_committed_evidence current-canonical.toml\n# mv \"$selector_temporary\" \"$selector_path\"\n# preceding selector restored and candidate removed\n# account_semantic_economics = true phase-inventory.toml source-built-tools.toml sha256sum -c SHA256SUMS\n",
        );
    write(
            root,
            "deploy/local-sns-rehearsal/scripts/13-propose-and-finalize-sns.sh",
            "#!/usr/bin/env bash\n# local-only optional\n# Requires IO_LOCAL_SNS_REHEARSAL_ACK=local-only.\nrequire_local_script_guard \"$@\"\n: \"${IO_LOCAL_SNS_REHEARSAL_ACK:?local-only}\"\n# publish_sns_wasm_via_nns sns_governance_source_sha256 sns_root_source_sha256 Governance Root get_metadata\n",
        );
    write(
            root,
            "deploy/local-sns-rehearsal/scripts/12-deploy-local-dapps.sh",
            "#!/usr/bin/env bash\n# local-only optional\n# Requires IO_LOCAL_SNS_REHEARSAL_ACK=local-only.\nrequire_local_script_guard \"$@\"\n: \"${IO_LOCAL_SNS_REHEARSAL_ACK:?local-only}\"\n# dfx canister id; allocated ID differs from planned; isolated lifecycle inputs; sns_governance_source_sha256 governance_sed_blob\n",
        );
    write(
            root,
            "deploy/local-sns-rehearsal/scripts/14-discover-sns-canisters.sh",
            "#!/usr/bin/env bash\n# local-only optional\n# Requires IO_LOCAL_SNS_REHEARSAL_ACK=local-only.\nrequire_local_script_guard \"$@\"\n: \"${IO_LOCAL_SNS_REHEARSAL_ACK:?local-only}\"\n# ManageNervousSystemParameters max_number_of_neurons 1_000\n",
        );
    write(
            root,
            "deploy/local-sns-rehearsal/scripts/lib-local-sns.sh",
            "#!/usr/bin/env bash\n# local-only optional\n# Requires IO_LOCAL_SNS_REHEARSAL_ACK=local-only.\nrequire_local_script_guard \"$@\"\n: \"${IO_LOCAL_SNS_REHEARSAL_ACK:?local-only}\"\n# nns_function = 30 manage_neuron get_proposal_info get_latest_sns_version_pretty executed_timestamp_seconds extract_proposal_id already-published get_proposal e8s_to_decimal_tokens https://forum.dfinity.org/t/io-local-rehearsal/0\n",
        );
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
    let err = parse_local_sns_evidence("deploy/local-sns-rehearsal/canister-ids.local.toml", &text)
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
            "type InitArgs = record {};\nservice : (InitArgs) -> {\n  prepare_redemption : () -> ();\n  settle_redemption : () -> ();\n  resume_redemption : () -> ();\n  prepare_claim_backing_receipt : () -> ();\n  prove_claim_backing_receipt : () -> ();\n  resume : () -> ();\n  prove_active_transfer : () -> ();\n  set_paused : () -> ();\n  validate_set_paused : (bool) -> (variant { Ok : text; Err : text }) query;\n  get_status : () -> () query;\n}\n",
        );
    write(
            root,
            "canisters/io_nns_neuron_manager/io_nns_neuron_manager.did",
            "type InitArgs = record {};\nservice : (InitArgs) -> {\n  notify_jupiter_deposit : () -> ();\n  prepare_pool_reconciliation : () -> ();\n  observe_claim_assets : () -> ();\n  observe_dynamic_backing_status : () -> ();\n  observe_pool_policy : () -> ();\n  prepare_two_week_maturity : () -> ();\n  start_maturity : () -> ();\n  resume : () -> ();\n  prove_active_transfer : () -> ();\n  set_paused : () -> ();\n  validate_set_paused : (bool) -> (variant { Ok : text; Err : text }) query;\n  get_status : () -> () query;\n}\n",
        );
    write(
            root,
            "canisters/io_historian/io_historian.did",
            "type ObservationConfig = record {};\nservice : (opt ObservationConfig) -> {\n  get_dashboard_state : () -> (text) query;\n  get_protocol_snapshot : () -> (text) query;\n  get_public_status : () -> (text) query;\n  get_claim_rate : () -> (text) query;\n  version : () -> (text) query;\n}\n",
        );
    write(
            root,
            "canisters/frontend/web/declarations/io_historian/io_historian.did.js",
            "export const idlFactory = ({ IDL }) => IDL.Service({\n  get_dashboard_state: IDL.Func([], [], [\"query\"]),\n  get_protocol_snapshot: IDL.Func([], [], [\"query\"]),\n  get_public_status: IDL.Func([], [], [\"query\"]),\n  get_claim_rate: IDL.Func([], [], [\"query\"]),\n  version: IDL.Func([], [], [\"query\"]),\n});\n",
        );
    write(
        root,
        "canisters/frontend/web/declarations/io_historian/index.js",
        "import { idlFactory } from \"./io_historian.did.js\";\nexport { idlFactory };\n",
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
io_nns_neuron_id = 10_292_412_127_977_304_661

[deployment_targets]
io_stream_manager = "thset-pqaaa-aaaar-qb7wa-cai"
io_nns_neuron_manager = "oae4c-3iaaa-aaaar-qb5qq-cai"
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
io_historian = "tjqj3-uaaaa-aaaar-qb7xa-cai"
frontend = "torpp-zyaaa-aaaar-qb7xq-cai"

[notes]
description = "Production fiduciary-subnet canisters are reserved placeholders only. They are not live protocol deployments."
"#
}

fn production_mapping_doc() -> &'static str {
    r#"
io_stream_manager thset-pqaaa-aaaar-qb7wa-cai
io_nns_neuron_manager oae4c-3iaaa-aaaar-qb5qq-cai
io_historian tjqj3-uaaaa-aaaar-qb7xa-cai
frontend torpp-zyaaa-aaaar-qb7xq-cai
"#
}

fn production_canister_roles_doc() -> &'static str {
    r#"
## io_nns_neuron_manager
Production execution identity: existing protected controller `oae4c-3iaaa-aaaar-qb5qq-cai`.

## io_stream_manager
Production fiduciary status: reserved as `thset-pqaaa-aaaar-qb7wa-cai`, `ReservedNotLive`.

## io_historian
Production fiduciary status: reserved as `tjqj3-uaaaa-aaaar-qb7xa-cai`, `ReservedNotLive`.

## frontend
Production fiduciary status: reserved as `torpp-zyaaa-aaaar-qb7xq-cai`, `ReservedNotLive`.
"#
}

fn write_production_wiring_fixture(root: &Path) {
    write_did_surface_fixture(root);
    write(
        root,
        "tools/scripts/required-check",
        "#!/usr/bin/env bash\ncargo test\n",
    );
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
10292412127977304661
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
io_nns_neuron_manager oae4c-3iaaa-aaaar-qb5qq-cai
io_historian tjqj3-uaaaa-aaaar-qb7xa-cai
frontend torpp-zyaaa-aaaar-qb7xq-cai
thset-pqaaa-aaaar-qb7wa-cai
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
fn release_artifact_comparison_accepts_identical_complete_sets() {
    let first = temp_root("artifact-compare-identical-first");
    let second = temp_root("artifact-compare-identical-second");
    write_artifact_set(&first);
    copy_release_artifact_set(&first, &second);
    compare_release_artifact_dirs(
        &first.join("release-artifacts"),
        &second.join("release-artifacts"),
    )
    .unwrap();
    let _ = fs::remove_dir_all(first);
    let _ = fs::remove_dir_all(second);
}

#[test]
fn release_artifact_comparison_rejects_modified_checked_in_wasm_gzip_sidecar_and_manifest() {
    for (case, path) in [
        ("wasm", "io_stream_manager.wasm"),
        ("gzip", "io_stream_manager.wasm.gz"),
        ("sidecar", "io_stream_manager.wasm.sha256"),
        ("manifest", "manifest.json"),
    ] {
        let checked_in = temp_root(&format!("artifact-compare-{case}-checked-in"));
        let rebuilt = temp_root(&format!("artifact-compare-{case}-rebuilt"));
        write_artifact_set(&checked_in);
        copy_release_artifact_set(&checked_in, &rebuilt);
        write(
            &checked_in,
            &format!("release-artifacts/{path}"),
            &format!("deliberately modified checked-in {case}\n"),
        );
        assert!(compare_release_artifact_dirs(
            &checked_in.join("release-artifacts"),
            &rebuilt.join("release-artifacts"),
        )
        .unwrap_err()
        .contains("mismatch"));
        let _ = fs::remove_dir_all(checked_in);
        let _ = fs::remove_dir_all(rebuilt);
    }
}

#[test]
fn release_artifact_comparison_rejects_missing_or_unexpected_files() {
    let first = temp_root("artifact-compare-file-set-first");
    let second = temp_root("artifact-compare-file-set-second");
    write_artifact_set(&first);
    copy_release_artifact_set(&first, &second);
    write(&second, "release-artifacts/unexpected.wasm", "unexpected");
    assert!(compare_release_artifact_dirs(
        &first.join("release-artifacts"),
        &second.join("release-artifacts"),
    )
    .unwrap_err()
    .contains("file set mismatch"));
    fs::remove_file(second.join("release-artifacts/unexpected.wasm")).unwrap();
    fs::remove_file(second.join("release-artifacts/io_frontend.wasm.gz")).unwrap();
    assert!(compare_release_artifact_dirs(
        &first.join("release-artifacts"),
        &second.join("release-artifacts"),
    )
    .unwrap_err()
    .contains("file set mismatch"));
    let _ = fs::remove_dir_all(first);
    let _ = fs::remove_dir_all(second);
}

#[test]
fn executable_release_and_ci_scripts_are_portable() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for path in [
        "tools/scripts/build-release-from-source",
        "tools/scripts/verify-release-from-source",
        "tools/scripts/release-build-temp-root",
        "tools/scripts/provision-pocket-ic",
        "tools/scripts/provision-icp-cli",
        "tools/scripts/provision-security-tools",
        ".github/workflows/test.yml",
        ".github/workflows/security.yml",
        ".github/workflows/reproducible-build.yml",
    ] {
        let contents = fs::read_to_string(root.join(path)).unwrap();
        assert!(
            !contents.contains("/home/codexdev"),
            "developer-specific path returned in {path}"
        );
    }
}

#[test]
fn release_reproducibility_varies_paths_and_remaps_cargo_home() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let builder = fs::read_to_string(root.join("tools/scripts/build-canister")).unwrap();
    for required in [
        "CARGO_ENCODED_RUSTFLAGS",
        "--remap-path-prefix=${release_cargo_home}=/io/cargo-home",
        "unset RUSTFLAGS",
    ] {
        assert!(
            builder.contains(required),
            "release builder is missing path-remapping guardrail: {required}"
        );
    }

    let verifier =
        fs::read_to_string(root.join("tools/scripts/verify-release-from-source")).unwrap();
    for required in [
        "release-source-root-with-intentionally-different-absolute-path-length",
        "cargo-home-with-intentionally-different-absolute-path-length",
        "CARGO_HOME=\"${alternate_cargo_home}\"",
        "IO_RELEASE_BUILD_TMPDIR=\"${long_source_root}\"",
    ] {
        assert!(
            verifier.contains(required),
            "release verifier is missing unequal-path guardrail: {required}"
        );
    }
}

#[test]
fn frontend_generation_starts_from_an_empty_generated_directory() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let package = fs::read_to_string(root.join("package.json")).unwrap();
    assert!(package.contains("\"setup:frontend\": \"npm run clean:frontend-generated && npm ci\""));

    let cleaner =
        fs::read_to_string(root.join("canisters/frontend/web/clean-generated.mjs")).unwrap();
    for required in [
        "rmSync(directory, { recursive: true, force: true })",
        "cleanGeneratedDirectory();",
    ] {
        assert!(
            cleaner.contains(required),
            "frontend cleaner is missing stale-asset guardrail: {required}"
        );
    }

    let builder =
        fs::read_to_string(root.join("canisters/frontend/web/build-frontend.mjs")).unwrap();
    assert!(builder.contains("cleanGeneratedDirectory();"));
}

#[test]
fn test_workflow_provisions_pinned_runtime_tools_at_valid_basenames() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let workflow = fs::read_to_string(root.join(".github/workflows/test.yml")).unwrap();
    for required in [
        "${RUNNER_TEMP}/pocket-ic-14.0.0",
        "${pocket_ic_dir}/pocket-ic-server",
        "tools/scripts/provision-pocket-ic",
        "POCKET_IC_BIN=${pocket_ic_bin}",
        "${RUNNER_TEMP}/icp-cli-0.2.7",
        "${icp_dir}/icp",
        "tools/scripts/provision-icp-cli",
        "${GITHUB_PATH}",
    ] {
        assert!(
            workflow.contains(required),
            "missing CI guardrail: {required}"
        );
    }
    assert!(!workflow.contains("pocket-ic-server-14.0.0"));
}

#[test]
fn source_open_package_keeps_the_canonical_apache_license() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    assert_eq!(
        sha256_hex(&root.join("LICENSE")).unwrap(),
        "cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30"
    );
    let review = fs::read_to_string(root.join("docs/security/source-open-package.md")).unwrap();
    for required in [
        "Apache-2.0",
        "canonical Apache License 2.0",
        "No vendored third-party source",
        "not legal advice",
    ] {
        assert!(
            review.contains(required),
            "missing source-open review: {required}"
        );
    }
}

#[test]
fn nns_neuron_staking_subaccount_matches_canonical_domain_encoding() {
    assert_eq!(
        nns_neuron_staking_subaccount(Principal::anonymous(), 42),
        "51f24fa3c2cda819352861ad22661f640f8be4be81e77304e77fe6c9cb87d2de"
    );
}

#[test]
fn sns_treasury_subaccount_matches_pinned_dfinity_fixture() {
    let governance =
        Principal::from_text("dmkut-c3777-77776-qaaaq-cai").expect("valid fixture principal");
    assert_eq!(
        sns_distribution_subaccount(governance, 0),
        "1205b30afec9d6b8da3bf45dbfebc286fa341246b9878ca63229d2b9ed49dd6f"
    );
}

#[test]
fn corrected_fixture_redemption_economics_matches_independent_sanity_check() {
    let result = calculate_redemption_economics(
        99_999_999_940_000,
        10_000_000_000,
        &[79_989_899_980_000],
        100_000_000_000_000,
        20_000_000,
        10_000,
    )
    .unwrap();
    assert_eq!(result.redeemable_supply_e8s, 20_000_099_960_000);
    assert_eq!(result.gross_icp_e8s, 99_999_500);
    assert_eq!(result.net_icp_e8s, 99_989_500);
}

#[test]
fn index_transfer_block_finds_unique_memo_bound_treasury_transfer() {
    let history = r#"(variant { Ok = record { transactions = vec {
          record { id = 7 : nat; transaction = record { kind = "transfer";
            transfer = opt record { memo = opt blob "\00\00\00\00\00\00\05\de";
              amount = 100_000_000 : nat; from = record { owner = principal "aaaaa-aa"; subaccount = opt blob "\01"; }; }; }; };
          record { id = 6 : nat; transaction = record { kind = "transfer";
            transfer = opt record { memo = opt blob "\00\00\00\00\00\00\05\dd";
              amount = 10_000_000_000 : nat; from = record { owner = principal "aaaaa-aa"; subaccount = opt blob "\01"; }; }; }; };
        }; }; })"#;
    assert_eq!(
        index_transfer_block(history, 10_000_000_000, "00000000000005dd").unwrap(),
        6
    );
    assert_eq!(
        index_transfer_block(history, 100_000_000, "00000000000005de").unwrap(),
        7
    );
}

#[test]
fn artifact_manifest_accepts_artifact_commit_then_evidence_tail() {
    let root = temp_root("manifest-ancestor-source");
    let source_commit = current_git_commit(&root).unwrap();
    write_artifact_set(&root);
    let manifest = build_manifest_for_commit(&root, source_commit).unwrap();
    write_artifact_manifest(&root, &manifest);
    assert!(Command::new("git")
        .current_dir(&root)
        .args(["add", "release-artifacts"])
        .status()
        .unwrap()
        .success());
    assert!(Command::new("git")
        .current_dir(&root)
        .args([
            "-c",
            "user.name=IO xtask test",
            "-c",
            "user.email=io-xtask@example.invalid",
            "commit",
            "--quiet",
            "-m",
            "record artifacts",
        ])
        .status()
        .unwrap()
        .success());
    write(&root, "docs/evidence.md", "evidence\n");
    assert!(Command::new("git")
        .current_dir(&root)
        .args(["add", "docs/evidence.md"])
        .status()
        .unwrap()
        .success());
    assert!(Command::new("git")
        .current_dir(&root)
        .args([
            "-c",
            "user.name=IO xtask test",
            "-c",
            "user.email=io-xtask@example.invalid",
            "commit",
            "--quiet",
            "-m",
            "record evidence",
        ])
        .status()
        .unwrap()
        .success());
    verify_artifacts_at(&root).unwrap();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn release_tail_rejects_simulated_post_source_canister_and_build_inputs() {
    for path in [
        "Cargo.toml",
        "Cargo.lock",
        "rust-toolchain.toml",
        "canisters/io_stream_manager/src/lib.rs",
        "canisters/frontend/public/index.html",
        "crates/io_build_support/src/lib.rs",
        "tools/scripts/build-canister",
        "tools/xtask/src/main.rs",
    ] {
        let error =
            validate_release_commit_paths("simulated-evidence-tail", &[path.to_string()], false)
                .unwrap_err();
        assert!(error.contains(path), "unexpected error: {error}");
    }
}

#[test]
fn release_tail_allows_only_artifacts_then_narrow_evidence_paths() {
    validate_release_commit_paths(
        "simulated-artifact-recording",
        &[
            "release-artifacts/manifest.json".into(),
            "release-artifacts/io_stream_manager.wasm".into(),
        ],
        true,
    )
    .unwrap();
    for path in [
        "deploy/local-sns-rehearsal/evidence/2026-08-12-example/manifest.toml",
        "docs/operations/release-checklist.md",
        ".github/workflows/ci.yml",
        "tools/sns/launch-readiness.toml",
    ] {
        validate_release_commit_paths("simulated-tail", &[path.into()], false).unwrap();
    }
    assert!(validate_release_commit_paths(
        "simulated-artifact-recording",
        &["docs/operations/release-checklist.md".into()],
        true,
    )
    .is_err());
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
    let non_ancestor = create_unreachable_commit(&root);
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
    manifest.artifacts[0].git_commit = Some("0123456789abcdef0123456789abcdef01234567".to_string());
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

    let entry = io_sns_lifecycle::resolve_manifest_entry(&manifest, "io_stream_manager").unwrap();
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
    let entry = io_sns_lifecycle::resolve_manifest_entry(&manifest, "io_stream_manager").unwrap();
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
              two_year_nns_neuron_id = 10_292_412_127_977_304_661 : nat64;
              io_stream_manager_principal_text = null : opt text;
              nns_governance_principal_text = null : opt text;
              icp_ledger_principal_text = null : opt text;
            })"#,
        InstallArgsMode::Mainnet,
    )
    .unwrap();
}

#[test]
fn install_args_validation_rejects_obsolete_protected_neuron() {
    let obsolete_neuron = 6_345_890_886_899_317_000_u64 + 159;
    let args = format!(
        r#"(record {{
              controller_canister_principal_text = "oae4c-3iaaa-aaaar-qb5qq-cai";
              two_year_nns_neuron_id = {obsolete_neuron} : nat64;
              io_stream_manager_principal_text = null : opt text;
              nns_governance_principal_text = null : opt text;
              icp_ledger_principal_text = null : opt text;
            }})"#
    );
    let err = validate_nns_install_args_text(&args, InstallArgsMode::Mainnet).unwrap_err();
    assert!(err.contains(&KNOWN_TWO_YEAR_NNS_NEURON_ID.to_string()));
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
    assert_eq!(check_sns_launch_readiness_at(&root, false).unwrap(), 16);
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
fn local_sns_rehearsal_rejects_stream_activation_before_nns_readiness() {
    let root = temp_root("local-sns-rehearsal-stream-before-nns");
    write_local_sns_rehearsal_fixture(&root);
    let path =
        root.join("deploy/local-sns-rehearsal/scripts/17-exercise-governance-and-controllers.sh");
    let text = fs::read_to_string(&path).unwrap();
    let nns = "if ! phase_is_done 17-nns-activated; then :; fi";
    let stream = "if ! phase_is_done 17-stream-activated; then :; fi";
    fs::write(
        &path,
        text.replace(&format!("{nns}\n{stream}"), &format!("{stream}\n{nns}")),
    )
    .unwrap();
    assert!(check_local_sns_rehearsal_at(&root)
        .unwrap_err()
        .contains("establish NNS readiness before Stream readiness"));
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
fn current_canonical_selector_accepts_only_closed_versioned_shapes() {
    let good = dummy_selector_text("2026-08-14-4320fdf-canonical-economics");
    let parsed = parse_current_canonical_selector(CURRENT_CANONICAL_SELECTOR, &good).unwrap();
    assert_eq!(parsed.package, "2026-08-14-4320fdf-canonical-economics");
    assert_eq!(parsed.version, 1);
    assert_eq!(
        parse_current_canonical_selector(
            CURRENT_CANONICAL_SELECTOR,
            &good.replace("version = 1", "version = 2")
        )
        .unwrap()
        .version,
        2
    );

    for bad in [
        good.replace("version = 1", "version = 3"),
        format!("{good}\nunexpected = \"field\"\n"),
        good.replace("[current]", "[current]\nunexpected = \"field\""),
        format!("{good}\n[current]\n"),
    ] {
        assert!(parse_current_canonical_selector(CURRENT_CANONICAL_SELECTOR, &bad).is_err());
    }
}

#[test]
fn current_canonical_selector_rejects_traversal_and_absolute_packages() {
    for package in [
        "../historical",
        "nested/package",
        "nested\\package",
        "/absolute/package",
        ".",
        "..",
    ] {
        assert!(parse_current_canonical_selector(
            CURRENT_CANONICAL_SELECTOR,
            &dummy_selector_text(package),
        )
        .unwrap_err()
        .contains("leaf directory"));
    }
}

#[test]
fn current_selector_binding_checks_every_release_and_package_identity() {
    let root = temp_root("current-selector-binding");
    let package = "deploy/local-sns-rehearsal/evidence/current-package";
    write(&root, MANIFEST_PATH, "current release manifest\n");
    write(
        &root,
        &format!("{package}/manifest.toml"),
        "package manifest\n",
    );
    write(&root, &format!("{package}/SHA256SUMS"), "package sums\n");
    let source_commit = "1".repeat(40);
    let artifact_commit = "2".repeat(40);
    let validated = ValidatedEvidencePackage {
        complete: true,
        monitoring: true,
        canonical_economics: true,
        account_semantic: false,
        io_release_source_commit: Some(source_commit.clone()),
        io_artifact_recording_commit: Some(artifact_commit.clone()),
    };
    let selector = CurrentCanonicalSelector {
        version: 1,
        package: "current-package".into(),
        io_release_source_commit: source_commit,
        io_artifact_recording_commit: artifact_commit,
        release_manifest_sha256: hex_sha256(&fs::read(root.join(MANIFEST_PATH)).unwrap()),
        package_manifest_sha256: hex_sha256(
            &fs::read(root.join(package).join("manifest.toml")).unwrap(),
        ),
        package_sha256s_sha256: hex_sha256(
            &fs::read(root.join(package).join("SHA256SUMS")).unwrap(),
        ),
    };
    validate_current_selector_binding(&root, package, &validated, &selector).unwrap();
    let mut account_semantic_selector = selector.clone();
    account_semantic_selector.version = 2;
    assert!(validate_current_selector_binding(
        &root,
        package,
        &validated,
        &account_semantic_selector
    )
    .unwrap_err()
    .contains("requires current account-semantic evidence"));

    let mut wrong = selector.clone();
    wrong.io_release_source_commit = "a".repeat(40);
    assert!(
        validate_current_selector_binding(&root, package, &validated, &wrong)
            .unwrap_err()
            .contains("release source")
    );
    let mut wrong = selector.clone();
    wrong.io_artifact_recording_commit = "b".repeat(40);
    assert!(
        validate_current_selector_binding(&root, package, &validated, &wrong)
            .unwrap_err()
            .contains("artifact-recording")
    );
    for field in [
        "release_manifest_sha256",
        "package_manifest_sha256",
        "package_sha256s_sha256",
    ] {
        let mut wrong = selector.clone();
        match field {
            "release_manifest_sha256" => wrong.release_manifest_sha256 = "0".repeat(64),
            "package_manifest_sha256" => wrong.package_manifest_sha256 = "0".repeat(64),
            "package_sha256s_sha256" => wrong.package_sha256s_sha256 = "0".repeat(64),
            _ => unreachable!(),
        }
        assert!(
            validate_current_selector_binding(&root, package, &validated, &wrong)
                .unwrap_err()
                .contains(field)
        );
    }
    let _ = fs::remove_dir_all(root);
}

#[test]
fn immutable_historical_canonical_package_validates_intrinsically() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let package = "deploy/local-sns-rehearsal/evidence/2026-08-12-4320fdf-canonical-economics";
    let validated = validate_local_sns_evidence_package_at(&root, package, false).unwrap();
    assert!(validated.complete);
    assert!(validated.monitoring);
    assert!(validated.canonical_economics);
}

#[test]
fn obsolete_guard_package_remains_intrinsic_history_but_not_current() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let package = "deploy/local-sns-rehearsal/evidence/2026-08-14-4320fdf-canonical-economics";
    let historical_ids =
        fs::read_to_string(root.join(package).join("canister-ids.local.toml")).unwrap();
    assert!(historical_ids.contains("6345890886899317159"));
    assert!(!historical_ids.contains(&PROTECTED_IO_NNS_NEURON_ID.to_string()));
    validate_local_sns_evidence_package_at(&root, package, false).unwrap();
    let err = validate_local_sns_evidence_package_at(&root, package, true).unwrap_err();
    assert!(
        err.contains("selected current package artifact commit"),
        "expected stale-release selection error, got {err:?}"
    );
}

#[test]
fn selecting_historical_canonical_after_release_change_fails() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let package = "deploy/local-sns-rehearsal/evidence/2026-08-12-4320fdf-canonical-economics";
    assert!(validate_local_sns_evidence_package_at(&root, package, true)
        .unwrap_err()
        .contains("selected current package artifact commit"));
}

#[test]
fn current_selector_missing_or_unselected_package_fails_closed() {
    let root = temp_root("current-selector-missing");
    write_completed_evidence_package(&root);
    assert!(check_local_sns_committed_evidence_at(&root)
        .unwrap_err()
        .contains("required selector is missing"));
    write_selector(&root, "missing-package");
    assert!(check_local_sns_committed_evidence_at(&root)
        .unwrap_err()
        .contains("was not encountered exactly once"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn current_selector_rejects_incomplete_or_noncanonical_selection() {
    let root = temp_root("current-selector-shape");
    let incomplete = write_incomplete_evidence_package(&root);
    let incomplete_name = Path::new(&incomplete)
        .file_name()
        .unwrap()
        .to_string_lossy();
    write_selector(&root, &incomplete_name);
    assert!(check_local_sns_committed_evidence_at(&root)
        .unwrap_err()
        .contains("must be complete, monitoring, and canonical"));

    let completed = write_completed_evidence_package(&root);
    let completed_name = Path::new(&completed).file_name().unwrap().to_string_lossy();
    write_selector(&root, &completed_name);
    assert!(check_local_sns_committed_evidence_at(&root)
        .unwrap_err()
        .contains("must be complete, monitoring, and canonical"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn current_selector_rejects_a_second_designation_file() {
    let root = temp_root("current-selector-duplicate");
    write_selector(&root, "missing-package");
    write(
        &root,
        "deploy/local-sns-rehearsal/evidence/also-current.toml",
        &dummy_selector_text("missing-package"),
    );
    assert!(check_local_sns_committed_evidence_at(&root)
        .unwrap_err()
        .contains("exact selector or regular package directories"));
    let _ = fs::remove_dir_all(root);
}

#[cfg(unix)]
#[test]
fn current_selector_rejects_symlink_selector_and_package() {
    use std::os::unix::fs::symlink;

    let root = temp_root("current-selector-symlinks");
    write(
        &root,
        "deploy/local-sns-rehearsal/evidence/selector-target.toml",
        &dummy_selector_text("selected-package"),
    );
    symlink(
        root.join("deploy/local-sns-rehearsal/evidence/selector-target.toml"),
        root.join(CURRENT_CANONICAL_SELECTOR),
    )
    .unwrap();
    assert!(check_local_sns_committed_evidence_at(&root)
        .unwrap_err()
        .contains("regular non-symlink file"));

    fs::remove_file(root.join(CURRENT_CANONICAL_SELECTOR)).unwrap();
    fs::remove_file(root.join("deploy/local-sns-rehearsal/evidence/selector-target.toml")).unwrap();
    write_selector(&root, "selected-package");
    fs::create_dir_all(root.join("outside-package")).unwrap();
    symlink(
        root.join("outside-package"),
        root.join("deploy/local-sns-rehearsal/evidence/selected-package"),
    )
    .unwrap();
    assert!(check_local_sns_committed_evidence_at(&root)
        .unwrap_err()
        .contains("exact selector or regular package directories"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn exact_incomplete_inventory_is_valid_but_not_current_launch_ready() {
    let root = temp_root("local-sns-evidence-incomplete");
    let package = write_incomplete_evidence_package(&root);
    let validated = validate_local_sns_evidence_package_at(&root, &package, false).unwrap();
    assert!(!validated.complete);
    assert!(check_local_sns_committed_evidence_at(&root)
        .unwrap_err()
        .contains("required selector is missing"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn exact_historical_inventory_is_valid_but_not_current_launch_ready() {
    let root = temp_root("local-sns-evidence-completed");
    let package = write_completed_evidence_package(&root);
    let validated = validate_local_sns_evidence_package_at(&root, &package, false).unwrap();
    assert!(validated.complete);
    assert!(!validated.monitoring);
    assert!(check_local_sns_committed_evidence_at(&root)
        .unwrap_err()
        .contains("required selector is missing"));
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
    assert!(
        validate_local_sns_evidence_package_at(&root, &package, false)
            .unwrap_err()
            .contains("duplicate SHA256SUMS")
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn local_sns_committed_evidence_rejects_unexpected_file() {
    let root = temp_root("local-sns-evidence-unexpected");
    let package = write_incomplete_evidence_package(&root);
    write(&root, &format!("{package}/extra.txt"), "extra\n");
    assert!(
        validate_local_sns_evidence_package_at(&root, &package, false)
            .unwrap_err()
            .contains("inventory mismatch")
    );
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
    assert!(
        validate_local_sns_evidence_package_at(&root, &package, false)
            .unwrap_err()
            .contains("placeholder marker")
    );
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
    assert!(
        validate_local_sns_evidence_package_at(&root, &package, false)
            .unwrap_err()
            .contains("reject symlinks")
    );
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
fn pocket_ic_provisioning_rejects_forged_download() {
    use std::os::unix::fs::PermissionsExt;

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target")
        .join(format!(
            "xtask-pocket-ic-provision-forged-{}",
            std::process::id()
        ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let fake_bin = root.join("fake-bin");
    fs::create_dir_all(&fake_bin).unwrap();
    write(
            &root,
            "fake-bin/curl",
            "#!/bin/sh\noutput=\nwhile [ \"$#\" -gt 0 ]; do\n  if [ \"$1\" = --output ]; then shift; output=$1; fi\n  shift\ndone\nprintf forged-archive > \"$output\"\n",
        );
    write(
        &root,
        "fake-bin/gzip",
        "#!/bin/sh\nprintf forged-pocket-ic-binary\n",
    );
    for executable in [fake_bin.join("curl"), fake_bin.join("gzip")] {
        let mut permissions = fs::metadata(&executable).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(executable, permissions).unwrap();
    }

    let script =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tools/scripts/provision-pocket-ic");
    let script_text = fs::read_to_string(&script).unwrap();
    for required in [
            "version=\"14.0.0\"",
            "f5009e61bcbff297435a67a8ef9fc02178ebb9ab3ee1ec3ac81f4fc3d49319c4",
            "https://github.com/dfinity/pocketic/releases/download/${version}/pocket-ic-x86_64-linux.gz",
            "--proto '=https'",
            "--tlsv1.2",
        ] {
            assert!(script_text.contains(required), "missing pin: {required}");
        }

    let output_path = root.join("pocket-ic-server");
    let output = Command::new("bash")
        .arg(&script)
        .arg(&output_path)
        .env("PATH", format!("{}:/usr/bin:/bin", fake_bin.display()))
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("SHA-256 mismatch"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!output_path.exists());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn pocket_ic_provisioning_rejects_invalid_output_basename() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target")
        .join(format!(
            "xtask-pocket-ic-provision-invalid-basename-{}",
            std::process::id()
        ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let script =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tools/scripts/provision-pocket-ic");
    let output = Command::new("bash")
        .arg(&script)
        .arg(root.join("pocket-ic-server-14.0.0"))
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("output basename must be pocket-ic or pocket-ic-server"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn icp_cli_provisioning_rejects_forged_download() {
    use std::os::unix::fs::PermissionsExt;

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target")
        .join(format!(
            "xtask-icp-cli-provision-forged-{}",
            std::process::id()
        ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let fake_bin = root.join("fake-bin");
    fs::create_dir_all(&fake_bin).unwrap();
    write(
            &root,
            "fake-bin/curl",
            "#!/bin/sh\noutput=\nwhile [ \"$#\" -gt 0 ]; do\n  if [ \"$1\" = --output ]; then shift; output=$1; fi\n  shift\ndone\nprintf forged-archive > \"$output\"\n",
        );
    let curl = fake_bin.join("curl");
    let mut permissions = fs::metadata(&curl).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&curl, permissions).unwrap();

    let script =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tools/scripts/provision-icp-cli");
    let script_text = fs::read_to_string(&script).unwrap();
    for required in [
        "version=\"0.2.7\"",
        "90eb2fc76267422a8ed20681453f1c52b93fea01",
        "bc6272fc0004d17538c650cfc8bacedd464ae86527efe172ed3b499a3e0f7798",
        "99aaef26bd765ce197c1de525ddb437ad1d3e933e5d3ca2d720ed189c23b7667",
        "https://github.com/dfinity/icp-cli/releases/download/v${version}/${archive_name}",
        "--proto '=https'",
        "--tlsv1.2",
    ] {
        assert!(script_text.contains(required), "missing pin: {required}");
    }

    let output_path = root.join("icp");
    let output = Command::new("bash")
        .arg(&script)
        .arg(&output_path)
        .env("PATH", format!("{}:/usr/bin:/bin", fake_bin.display()))
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("archive SHA-256 mismatch"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!output_path.exists());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn icp_cli_provisioning_rejects_invalid_output_basename() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target")
        .join(format!(
            "xtask-icp-cli-provision-invalid-basename-{}",
            std::process::id()
        ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let script =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tools/scripts/provision-icp-cli");
    let output = Command::new("bash")
        .arg(&script)
        .arg(root.join("icp-0.2.7"))
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("output basename must be icp"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn local_sns_ledger_check_fails_without_selected_current_evidence() {
    let root = temp_root("local-sns-ledger-skip");
    write_local_sns_rehearsal_fixture(&root);
    assert!(check_local_sns_ledger_at(&root)
        .unwrap_err()
        .contains("required selector is missing"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn local_sns_ledger_check_rejects_old_completed_root_evidence() {
    let root = temp_root("local-sns-ledger-good");
    write_local_sns_rehearsal_fixture(&root);
    write_completed_local_sns_evidence(&root);
    let error = check_local_sns_ledger_at(&root).unwrap_err();
    assert!(error.contains("generated runtime evidence must not be treated as canonical"));
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
        .contains("generated runtime evidence must not be treated as canonical"));
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
                "index_history_order = \"10292412127977304661\"",
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
                "protocol_reserve_balance_e8s = 59999999980000",
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
                    "transaction_fee_e8s = 10000\ntotal_supply_e8s = 99999999970000\nprotocol_reserve_account_owner",
                    "transaction_fee_e8s = 10001\ntotal_supply_e8s = 99999999970000\nprotocol_reserve_account_owner",
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
                "index_synced_through_block_index = 12",
                "index_synced_through_block_index = 11",
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
                "observation_timestamp = \"2026-07-28T00:00:02Z\"",
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
fn local_sns_ledger_check_accepts_exact_user_to_reserve_push() {
    let evidence = completed_local_sns_evidence();
    let parsed = parse_local_sns_evidence("fixture", &evidence).unwrap();
    assert_eq!(
        parsed.user_to_reserve_transfer.to_account.owner,
        parsed.ledger.protocol_reserve_account_owner
    );
    assert_eq!(
        parsed.user_to_reserve_transfer.to_account.subaccount_hex,
        parsed.ledger.protocol_reserve_subaccount_hex
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
fn production_wiring_validation_accepts_fixture() {
    let root = temp_root("production-wiring-good");
    write_production_wiring_fixture(&root);
    check_production_wiring_at(&root).unwrap();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn production_wiring_validation_rejects_wrong_reserved_doc_mapping() {
    let root = temp_root("production-wiring-wrong-reserved-doc-mapping");
    write_production_wiring_fixture(&root);
    write(
            &root,
            "docs/architecture/canister-roles.md",
            &production_canister_roles_doc().replace(
                "io_stream_manager\nProduction fiduciary status: reserved as `thset-pqaaa-aaaar-qb7wa-cai`",
                "io_stream_manager\nProduction fiduciary status: reserved as `tjqj3-uaaaa-aaaar-qb7xa-cai`",
            ),
        );

    let err = check_production_wiring_at(&root).unwrap_err();
    assert!(
        err.contains("io_stream_manager") || err.contains(PRODUCTION_IO_STREAM_MANAGER_CANISTER_ID),
        "expected wrong mapping error, got {err:?}"
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
