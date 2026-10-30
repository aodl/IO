//! Direct standard SNS genesis and complete IO acceptance.
//! Historical feasibility test names are retained; the active launch has no sale.
#![cfg(test)]

use crate::{artifacts, icrc, pocketic_env, sns_governance_setup as gov, sns_lifecycle as swap};
use candid::{decode_one, encode_one, CandidType, Nat, Principal};
use ic_http_certification::{HttpRequest, HttpResponse, StatusCode};
use io_ledger_types::{
    Account, IcpTokens, IcpTransferArgs, IcpTransferError, IcrcAccount, Subaccount,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

#[derive(CandidType)]
struct LedgerTipRequest {
    start: u64,
    length: u64,
}
#[derive(CandidType, Deserialize)]
struct LedgerTip {
    chain_length: u64,
}

fn approved_sns_baseline() -> Value {
    serde_json::from_str(include_str!(
        "../../../deploy/acceptance/sns-source-baseline.json"
    ))
    .unwrap()
}

fn approved_sns_manifest() -> io_sns_manifest::SnsManifest {
    io_sns_manifest::SnsManifest::parse(include_str!("../wasms.example.toml")).unwrap()
}

fn assert_approved_sns_bundle(manifest: &io_sns_manifest::SnsManifest) {
    let baseline = approved_sns_baseline();
    let approved = approved_sns_manifest();
    assert_eq!(
        manifest.value("metadata", "version"),
        approved.value("metadata", "version")
    );
    assert_eq!(
        manifest.value("metadata", "latest_version_proposal_id"),
        approved.value("metadata", "latest_version_proposal_id")
    );
    assert_eq!(
        manifest.bool_value("capabilities", "latest_reward_event_participation"),
        Some(true)
    );
    for component in ["governance", "root", "ledger", "index", "swap", "archive"] {
        let artifact = format!("sns_{component}");
        for manifest_field in ["upstream_rev", "sha256", "source_sha256"] {
            assert_eq!(
                manifest.artifact(&artifact, manifest_field),
                approved.artifact(&artifact, manifest_field),
                "{component} {manifest_field}"
            );
        }
        assert_eq!(
            manifest.artifact(&artifact, "source_kind"),
            approved.artifact(&artifact, "source_kind")
        );
        assert_eq!(
            manifest.artifact(&artifact, "nns_proposal_id"),
            approved.artifact(&artifact, "nns_proposal_id"),
            "{component} NNS proposal"
        );
        assert_eq!(
            manifest.value("contract", &format!("{component}_did_sha256")),
            approved.value("contract", &format!("{component}_did_sha256")),
            "{component} DID"
        );
    }
    assert_eq!(
        baseline["framework_id"].as_str(),
        approved.value("metadata", "version")
    );
}
const TOTAL: u64 = 100_000_000_000_000_000;
const OPERATOR_MEMO: u64 = 0x494f_474f_5630_3031;
const JUPITER_MEMO: u64 = 0x494f_4a55_5030_3031;
const RESERVE_MEMO: u64 = 0x494f_5245_5330_3031;

#[path = "../../../tools/acceptance/nns_parent.rs"]
mod nns_parent;

#[path = "../../../tools/acceptance/frontend_assets.rs"]
mod frontend_assets;

#[path = "acceptance_maintained.rs"]
mod maintained;

// The C boundary performs offline artifact/payload checks without touching PIC.
// Use the existing harness's owned-server pattern. Offline checks can be
// delayed by host build contention; wall-clock lifetime is not simulated time.
struct AcceptanceServer {
    child: std::process::Child,
    port_file: PathBuf,
}
impl Drop for AcceptanceServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_file(&self.port_file);
    }
}
impl AcceptanceServer {
    fn start() -> (Self, String) {
        let port_file = workspace().join("target").join(format!(
            "acceptance-server-{}-{}.port",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let child = Command::new(std::env::var_os("POCKET_IC_BIN").expect("PocketIC required"))
            .args(["--ttl", "3600", "--hard-ttl", "7200", "--port-file"])
            .arg(&port_file)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("start acceptance PocketIC server");
        let mut server = Self { child, port_file };
        let started = std::time::Instant::now();
        loop {
            if let Ok(value) = fs::read_to_string(&server.port_file) {
                if let Ok(port) = value.trim().parse::<u16>() {
                    return (server, format!("http://127.0.0.1:{port}/"));
                }
            }
            assert!(
                server.child.try_wait().unwrap().is_none(),
                "acceptance server exited during startup"
            );
            assert!(
                started.elapsed() < Duration::from_secs(30),
                "acceptance server startup timeout"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

struct Sale {
    pic: pocket_ic::PocketIc,
    operator: Principal,
    buyer: Principal,
    governance_voter: Principal,
    governance: Principal,
    ledger: Principal,
    root: Principal,
    swap: Principal,
    stream: Principal,
    icp: Principal,
    evidence: PathBuf,
    _server: Option<AcceptanceServer>,
}

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn local_fixture_logo(dir: &Path) -> PathBuf {
    // Valid 32x32 RGB PNG, a solid local-test colour. Never deployment branding.
    let path = dir.join("local-fixture-logo.png");
    fs::write(&path, hex::decode("89504e470d0a1a0a0000000d4948445200000020000000200802000000fc18eda30000002a49444154789cedcd410900000804b08b68444318d0143e84c1fe4bd79c8a402010080402814020f8122c1105905bd7ba11e60000000049454e44ae426082").unwrap()).unwrap();
    path
}

fn prepare_io_artifacts(evidence: &Path) {
    use sha2::{Digest, Sha256};
    let source = PathBuf::from(
        std::env::var_os("IO_ACCEPTANCE_IO_WASM_DIR").expect("build current-source IO Wasms first"),
    );
    let destination = evidence.join("io-wasms");
    fs::create_dir(&destination).unwrap();
    let mut modules = serde_json::Map::new();
    for (role, name) in [
        ("stream", "io_stream_manager"),
        ("nns_manager", "io_nns_neuron_manager"),
        ("historian", "io_historian"),
        ("frontend", "io_frontend"),
    ] {
        let frontend_build =
            std::env::var_os("IO_ACCEPTANCE_FRONTEND_BUILD_DIR").map(PathBuf::from);
        let wasm_path = if role == "frontend" {
            frontend_build
                .as_ref()
                .expect("build isolated acceptance frontend first")
                .join("io_frontend.wasm")
        } else {
            source.join(format!("{name}.wasm"))
        };
        let bytes = fs::read(wasm_path).unwrap();
        let hash = hex::encode(Sha256::digest(&bytes));
        let path = destination.join(format!("{name}.wasm"));
        fs::write(&path, bytes).unwrap();
        let sidecar = format!("{name}.wasm.sha256");
        fs::write(destination.join(&sidecar), format!("{hash}  {name}.wasm\n")).unwrap();
        modules.insert(
            role.into(),
            json!({"path":path,"raw_wasm_sha256":hash,"sha256_sidecar":sidecar}),
        );
        if role == "frontend" {
            let public = frontend_build.unwrap().join("public");
            let bytes = fs::read(&path).unwrap();
            let evidence =
                frontend_assets::validate(&public, &bytes, &frontend_assets::acceptance_config())
                    .unwrap();
            modules.get_mut(role).unwrap()["browser_assets"] = json!(public);
            write_json(&destination.join("frontend-assets.json"), &evidence);
        }
    }
    write_json(
        &evidence.join("io-artifacts.json"),
        &Value::Object(modules.clone()),
    );
}

impl Sale {
    fn new() -> Self {
        Self::new_with_signed_ingress(false)
    }

    fn new_with_signed_ingress(signed_ingress: bool) -> Self {
        let artifacts::ArtifactStatus::Ready(artifacts) =
            artifacts::resolve_from_env(true).unwrap()
        else {
            panic!("official bundle required")
        };
        assert_approved_sns_bundle(&artifacts.manifest);
        let parser = PathBuf::from(
            std::env::var_os("IO_ACCEPTANCE_SNS_PAYLOAD_TOOL")
                .expect("build typed payload tool first"),
        );
        let (server, url) = AcceptanceServer::start();
        let pic = pocketic_env::new_pic_with_nns_governance_server(&url);
        let operator =
            Principal::from_text("zkkkd-i34qc-367ln-e2u7o-ezznu-dkfqh-gtfvz-cviph-6qa4v-evtfs-wqe")
                .unwrap();
        let buyer = Principal::from_slice(&[102; 29]);
        let create = |framework| {
            let subnet = if framework {
                pic.topology().get_sns().unwrap()
            } else {
                pic.topology().get_app_subnets()[0]
            };
            let id = pic.create_canister_on_subnet(Some(operator), None, subnet);
            pic.add_cycles(id, 20_000_000_000_000);
            id
        };
        let governance = create(true);
        let root = create(true);
        let ledger = create(true);
        let index = create(true);
        let swap = create(true);
        let stream = create(false);
        let nns_manager = create(false);
        let historian = create(false);
        let frontend = create(false);
        let jupiter = operator;
        let jupiter_source = create(false);
        let evidence_parent = std::env::var_os("IO_ACCEPTANCE_EVIDENCE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| workspace().join("target"));
        assert!(evidence_parent.is_absolute());
        fs::create_dir_all(&evidence_parent).unwrap();
        let evidence = evidence_parent.join(format!(
            "acceptance-genesis-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&evidence).unwrap();
        let context = json!({"local_fixture":true,"root_install":true,"operator":operator.to_text(),"governance_voting_principal":jupiter.to_text(),"jupiter_source":jupiter_source.to_text(),
            "stream":stream.to_text(),"nns_manager":nns_manager.to_text(),"historian":historian.to_text(),"frontend":frontend.to_text(),"governance":governance.to_text(),"root":root.to_text(),"ledger":ledger.to_text(),"index":index.to_text(),"swap":swap.to_text(),
            "logo_path":local_fixture_logo(&evidence),"bundle_path":artifacts.manifest_path.as_ref().unwrap().parent().unwrap(),"parser_tool":parser,"permanent_observation_route":"local-created-neuron"});
        write_json(&evidence.join("context.json"), &context);
        write_json(
            &evidence.join("authority-plan.json"),
            &json!({"initial_framework_installer":operator.to_text(),"initial_io_installer":root.to_text(),"route":"direct-Normal-genesis-root-install",
            "initial_registered_dapps":[frontend.to_text(),historian.to_text(),nns_manager.to_text(),stream.to_text()],"io_install_before_controller_handoff":false,"registration_caller":operator.to_text(),"registration_method":"Root init payload","chunk_store":"each target on its own subnet","final_io_controller":root.to_text(),"deployment_parameters_approved":false}),
        );
        let mut fixture = Self {
            pic,
            operator,
            buyer,
            governance_voter: jupiter,
            governance,
            ledger,
            root,
            swap,
            stream,
            icp: Principal::from_text(io_production_wiring::ICP_LEDGER_PRINCIPAL).unwrap(),
            evidence,
            _server: Some(server),
        };
        if signed_ingress {
            fixture.signed_local_ingress();
        }
        fixture.prepare_permanent();
        let mut context: Value =
            serde_json::from_slice(&fs::read(fixture.evidence.join("context.json")).unwrap())
                .unwrap();
        context["genesis_timestamp_seconds"] =
            json!(fixture.pic.get_time().as_nanos_since_unix_epoch() / 1_000_000_000);
        write_json(&fixture.evidence.join("context.json"), &context);
        for (command, output) in [
            ("acceptance_local_accounts", "accounts.json"),
            ("acceptance_local_config", "sns.yaml"),
        ] {
            assert!(Command::new(workspace().join("target/debug/xtask"))
                .current_dir(workspace())
                .arg(command)
                .arg(fixture.evidence.join("context.json"))
                .arg(fixture.evidence.join(output))
                .status()
                .unwrap()
                .success());
        }
        let payloads = fixture.evidence.join("payloads");
        assert!(Command::new(&parser)
            .current_dir(workspace())
            .arg(fixture.evidence.join("sns.yaml"))
            .arg(workspace().join("deploy/acceptance/canonical-sns-economics.json"))
            .arg(fixture.evidence.join("context.json"))
            .arg(&payloads)
            .status()
            .unwrap()
            .success());
        let mut baseline = serde_json::Map::new();
        for (name, id) in [
            ("governance", governance),
            ("root", root),
            ("ledger", ledger),
            ("index", index),
            ("swap", swap),
            ("stream", stream),
            ("nns_manager", nns_manager),
            ("historian", historian),
            ("frontend", frontend),
        ] {
            let status = fixture.pic.canister_status(id, Some(operator)).unwrap();
            baseline.insert(name.into(),json!({"canister_id":id.to_text(),"subnet_id":fixture.pic.get_subnet(id).unwrap().to_text(),"controllers":status.settings.controllers.iter().map(|p|p.to_text()).collect::<Vec<_>>(),"module_hash":status.module_hash.map(hex::encode),"cycles":u64::try_from(status.cycles.0).unwrap()}));
        }
        write_json(
            &fixture.evidence.join("baseline-canisters.json"),
            &Value::Object(baseline),
        );
        prepare_io_artifacts(&fixture.evidence);
        checkpoint(&fixture.evidence, "A");
        for (id, role) in [
            (ledger, "ledger"),
            (index, "index"),
            (root, "root"),
            (swap, "swap"),
            (governance, "governance"),
        ] {
            fixture.pic.install_canister(
                id,
                artifacts
                    .load_required_source_wasm_gz(&format!("sns_{role}"))
                    .unwrap(),
                fs::read(payloads.join(format!("{role}.bin"))).unwrap(),
                Some(operator),
            );
            assert_eq!(
                fixture
                    .pic
                    .canister_status(id, Some(operator))
                    .unwrap()
                    .module_hash,
                Some(
                    hex::decode(
                        artifacts
                            .manifest
                            .source_sha256(&format!("sns_{role}"))
                            .unwrap()
                    )
                    .unwrap()
                )
            );
        }
        let modules: Value =
            serde_json::from_slice(&fs::read(fixture.evidence.join("io-artifacts.json")).unwrap())
                .unwrap();
        let mut chunks = serde_json::Map::new();
        for (role, target) in [
            ("stream", stream),
            ("nns_manager", nns_manager),
            ("historian", historian),
            ("frontend", frontend),
        ] {
            let bytes = fs::read(modules[role]["path"].as_str().unwrap()).unwrap();
            let hashes = bytes
                .chunks(1_000_000)
                .map(|chunk| {
                    hex::encode(
                        fixture
                            .pic
                            .upload_chunk(target, Some(operator), chunk.to_vec())
                            .unwrap(),
                    )
                })
                .collect::<Vec<_>>();
            chunks.insert(role.into(),json!({"store_canister_id":target.to_text(),"raw_wasm_sha256":modules[role]["raw_wasm_sha256"],"chunk_hashes":hashes}));
        }
        write_json(
            &fixture.evidence.join("preuploaded-chunks.json"),
            &Value::Object(chunks),
        );
        for (id, controller) in [
            (governance, root),
            (root, governance),
            (ledger, root),
            (index, root),
            (swap, root),
            (stream, root),
            (nns_manager, root),
            (historian, root),
            (frontend, root),
        ] {
            fixture
                .pic
                .set_controllers(id, Some(operator), vec![controller])
                .unwrap();
        }
        fixture.observe_genesis();
        fixture
    }

    fn transfer_icp(&self, caller: Principal, to: Account, amount: u64) {
        let result: Result<u64, IcpTransferError> = icrc::update_one(
            &self.pic,
            self.icp,
            caller,
            "transfer",
            IcpTransferArgs {
                memo: 0,
                amount: IcpTokens { e8s: amount },
                fee: IcpTokens { e8s: 10_000 },
                from_subaccount: None,
                to: to.icp_account_identifier_bytes().to_vec(),
                created_at_time: None,
            },
        );
        result.unwrap();
    }

    fn raw_query(&self, id: Principal, method: &str, args: Vec<u8>, name: &str) -> Vec<u8> {
        let reply = self
            .pic
            .query_call(id, self.operator, method, args)
            .unwrap();
        fs::write(self.evidence.join(format!("{name}.bin")), &reply).unwrap();
        reply
    }
    fn ledger_observation(&self, name: &str, owner: Principal, sub: Option<Vec<u8>>) -> u64 {
        let account = icrc::account(owner, sub.map(|s| s.try_into().unwrap()));
        let reply = self.raw_query(
            self.ledger,
            "icrc1_balance_of",
            encode_one(account).unwrap(),
            name,
        );
        decode_one::<Nat>(&reply).unwrap().0.try_into().unwrap()
    }
    fn observe_swap(&self, phase: &str) {
        for (method, label) in [
            ("get_state", "state"),
            ("get_lifecycle", "lifecycle"),
            ("get_derived_state", "derived"),
            ("get_timers", "timers"),
        ] {
            self.raw_query(
                self.swap,
                method,
                encode_one(gov::EmptyRecord {}).unwrap(),
                &format!("swap-{label}-{phase}"),
            );
        }
        let buyers = self
            .pic
            .update_call(
                self.swap,
                self.operator,
                "get_buyers_total",
                encode_one(gov::EmptyRecord {}).unwrap(),
            )
            .unwrap();
        fs::write(
            self.evidence.join(format!("swap-buyers-{phase}.bin")),
            buyers,
        )
        .unwrap();
        #[derive(CandidType)]
        struct RecipeRequest {
            offset: Option<u64>,
            limit: Option<u32>,
        }
        self.raw_query(
            self.swap,
            "list_sns_neuron_recipes",
            encode_one(RecipeRequest {
                offset: Some(0),
                limit: Some(1000),
            })
            .unwrap(),
            &format!("swap-recipes-{phase}"),
        );
        self.ledger_observation(&format!("swap-io-{phase}"), self.swap, None);
        self.raw_query(
            self.icp,
            "account_balance",
            encode_one(IcpBalance {
                account: Account::new(self.swap, None)
                    .icp_account_identifier_bytes()
                    .to_vec(),
            })
            .unwrap(),
            &format!("swap-icp-{phase}"),
        );
        assert_parked_swap(self, phase == "after");
    }
    fn observe_genesis(&self) {
        let supply = self.raw_query(
            self.ledger,
            "icrc1_total_supply",
            candid::encode_args(()).unwrap(),
            "genesis-total-supply",
        );
        assert_eq!(decode_one::<Nat>(&supply).unwrap(), Nat::from(TOTAL));
        for (name, owner, memo, stake) in [
            ("operator", self.operator, OPERATOR_MEMO, 3_000_000_000u64),
            (
                "jupiter",
                self.governance_voter,
                JUPITER_MEMO,
                10_000_000_000_000_000,
            ),
            (
                "reserve",
                self.operator,
                RESERVE_MEMO,
                TOTAL - 10_000_000_000_000_000 - 3_000_000_000,
            ),
        ] {
            assert_eq!(
                self.ledger_observation(
                    &format!("genesis-{name}-balance"),
                    self.governance,
                    Some(gov::compute_neuron_staking_subaccount(owner, memo).to_vec())
                ),
                stake
            );
        }
        assert_eq!(
            self.ledger_observation("genesis-swap-balance", self.swap, None),
            0
        );
        assert_eq!(
            self.ledger_observation("genesis-minting-balance", self.governance, None),
            0
        );
        let accounts: Value =
            serde_json::from_slice(&fs::read(self.evidence.join("accounts.json")).unwrap())
                .unwrap();
        for (key, name) in [
            ("sns_token_treasury", "genesis-treasury-balance"),
            ("future_swap", "genesis-future-swap-balance"),
        ] {
            let sub = hex::decode(accounts[key]["subaccount_hex"].as_str().unwrap()).unwrap();
            assert_eq!(self.ledger_observation(name, self.governance, Some(sub)), 0);
        }
        for (method, name) in [
            ("icrc1_name", "ledger-name"),
            ("icrc1_symbol", "ledger-symbol"),
        ] {
            assert_eq!(
                decode_one::<String>(&self.raw_query(
                    self.ledger,
                    method,
                    candid::encode_args(()).unwrap(),
                    name
                ))
                .unwrap(),
                "IO"
            );
        }
        self.raw_query(
            self.governance,
            "get_mode",
            encode_one(gov::EmptyRecord {}).unwrap(),
            "governance-mode",
        );
        self.raw_query(
            self.governance,
            "get_running_sns_version",
            encode_one(gov::EmptyRecord {}).unwrap(),
            "genesis-running-version",
        );
        self.raw_query(
            self.governance,
            "get_nervous_system_parameters",
            candid::encode_args(()).unwrap(),
            "genesis-governance-parameters",
        );
        self.raw_query(
            self.governance,
            "list_neurons",
            encode_one(gov::ListNeurons {
                of_principal: None,
                limit: 100,
                start_page_at: None,
            })
            .unwrap(),
            "genesis-neurons",
        );
        let root = self
            .pic
            .update_call(
                self.root,
                self.operator,
                "get_sns_canisters_summary",
                encode_one(RootSummaryRequest {
                    update_canister_list: Some(false),
                })
                .unwrap(),
            )
            .unwrap();
        fs::write(self.evidence.join("genesis-root-summary.bin"), root).unwrap();
        self.observe_swap("before");
        self.pic.advance_time(Duration::from_secs(61));
        for _ in 0..5 {
            self.pic.tick();
        }
        self.observe_swap("after");
        unauthorized(self.manage(
            self.buyer,
            gov::compute_neuron_staking_subaccount(self.governance_voter, JUPITER_MEMO).to_vec(),
            gov::Command::RegisterVote(gov::RegisterVote {
                proposal: Some(gov::ProposalId { id: 1 }),
                vote: 1,
            }),
        ));
        for caller in [self.operator, self.buyer] {
            unauthorized(
                self.manage(
                    caller,
                    gov::compute_neuron_staking_subaccount(self.governance_voter, JUPITER_MEMO)
                        .to_vec(),
                    gov::Command::AddNeuronPermissions(gov::AddNeuronPermissions {
                        principal_id: Some(self.operator),
                        permissions_to_add: Some(gov::NeuronPermissionList {
                            permissions: vec![2, 3, 10],
                        }),
                    }),
                ),
            );
        }
        assert_eq!(self.operator, self.governance_voter);
        let proposer = self.neuron(self.operator, OPERATOR_MEMO);
        let voting = self.neuron(self.operator, JUPITER_MEMO);
        assert_ne!(proposer.id, voting.id);
        assert_eq!(voting.permissions.len(), 1);
        assert_eq!(voting.permissions[0].principal, Some(self.operator));
        assert_eq!(voting.permissions[0].permission_type, vec![4]);
        assert!(voting.followees.is_empty());
        assert!(voting.topic_followees.is_none());
        assert_eq!(proposer.permissions[0].principal, Some(self.operator));
        assert_eq!(
            proposer.permissions[0].permission_type,
            (1..=10).collect::<Vec<_>>()
        );
    }

    fn neurons(&self) -> Vec<gov::SnsNeuronRecord> {
        let response: gov::ListNeuronsResponse = icrc::query_one(
            &self.pic,
            self.governance,
            "list_neurons",
            gov::ListNeurons {
                of_principal: None,
                limit: 100,
                start_page_at: None,
            },
        );
        response.neurons
    }
    fn manage(
        &self,
        caller: Principal,
        subaccount: Vec<u8>,
        command: gov::Command,
    ) -> gov::ManageNeuronResponse {
        icrc::update_one(
            &self.pic,
            self.governance,
            caller,
            "manage_neuron",
            gov::ManageNeuron {
                subaccount,
                command: Some(command),
            },
        )
    }
    fn balance(&self, owner: Principal, subaccount: Option<Vec<u8>>) -> u64 {
        let amount: Nat = icrc::query_one(
            &self.pic,
            self.ledger,
            "icrc1_balance_of",
            IcrcAccount { owner, subaccount },
        );
        amount.0.try_into().unwrap()
    }
    fn migrate_reserve(&self) {
        let reserve = gov::compute_neuron_staking_subaccount(self.operator, RESERVE_MEMO);
        let destination =
            hex::decode("781a5fc2d878692d1fa555359c190f96d61f85b392cf11b80f4666169d789027")
                .unwrap();
        let response = self.manage(
            self.operator,
            reserve.to_vec(),
            gov::Command::Disburse(gov::Disburse {
                amount: None,
                to_account: Some(gov::Account {
                    owner: Some(self.stream),
                    subaccount: Some(gov::Subaccount {
                        subaccount: destination.clone(),
                    }),
                }),
            }),
        );
        assert!(
            matches!(response.command, Some(gov::CommandResponse::Disburse(_))),
            "{response:?}"
        );
        fs::write(
            self.evidence.join("reserve-disbursement-response.bin"),
            encode_one(response).unwrap(),
        )
        .unwrap();
        let supply = self.raw_query(
            self.ledger,
            "icrc1_total_supply",
            candid::encode_args(()).unwrap(),
            "b-total-supply",
        );
        let supply: u64 = decode_one::<Nat>(&supply).unwrap().0.try_into().unwrap();
        let reserve_balance =
            self.ledger_observation("b-stream-reserve", self.stream, Some(destination));
        let excluded_reserve =
            self.ledger_observation("b-reserve-neuron", self.governance, Some(reserve.to_vec()));
        let excluded_jupiter = self.ledger_observation(
            "b-jupiter-neuron",
            self.governance,
            Some(
                gov::compute_neuron_staking_subaccount(self.governance_voter, JUPITER_MEMO)
                    .to_vec(),
            ),
        );
        let operator = self.ledger_observation(
            "b-operator-neuron",
            self.governance,
            Some(gov::compute_neuron_staking_subaccount(self.operator, OPERATOR_MEMO).to_vec()),
        );
        assert_eq!(supply, TOTAL - 10_000);
        assert_eq!(excluded_reserve, 0);
        assert_eq!(excluded_jupiter, 10_000_000_000_000_000);
        assert_eq!(operator, 3_000_000_000);
        let c0 = supply
            .checked_sub(reserve_balance)
            .unwrap()
            .checked_sub(excluded_jupiter)
            .unwrap();
        assert_eq!(c0, 3_000_000_000);
        self.observe_swap("after");
        write_json(
            &self.evidence.join("observation-b.json"),
            &json!({"schema":"io.acceptance.observation-b.v1","operator_stake_e8s":operator,"total_supply_e8s":supply,"stream_reserve_e8s":reserve_balance,"excluded_reserve_e8s":excluded_reserve,"excluded_jupiter_e8s":excluded_jupiter,"reserve_disbursement_fee_e8s":10_000}),
        );
        let plan = checkpoint(&self.evidence, "B");
        assert_eq!(plan["c0_e8s"], 3_000_000_000u64);
        assert_eq!(plan["permanent_top_up_e8s"], 2_000_000_000u64);
    }
}
#[derive(CandidType, Deserialize)]
struct IcpBalance {
    account: Vec<u8>,
}

#[test]
#[ignore = "requires reviewed official SNS-W bundle, parser tool and PocketIC"]
fn self_deployed_genesis_migrates_reserve() {
    let sale = Sale::new();
    sale.migrate_reserve();
    println!("real direct-genesis evidence: {}", sale.evidence.display());
}

impl Sale {
    fn neuron(&self, owner: Principal, memo: u64) -> gov::SnsNeuronRecord {
        let id = gov::compute_neuron_staking_subaccount(owner, memo);
        self.neurons()
            .into_iter()
            .find(|n| n.id.as_ref().unwrap().id == id)
            .unwrap()
    }
    fn jupiter_manage(&self, command: gov::Command) -> gov::ManageNeuronResponse {
        self.manage(
            self.operator,
            gov::compute_neuron_staking_subaccount(self.operator, JUPITER_MEMO).to_vec(),
            command,
        )
    }
    fn propose(&self, title: &str, action: gov::Action) -> gov::ManageNeuronResponse {
        self.manage(
            self.operator,
            gov::compute_neuron_staking_subaccount(self.operator, OPERATOR_MEMO).to_vec(),
            gov::Command::MakeProposal(gov::Proposal {
                title: title.into(),
                summary: title.into(),
                url: String::new(),
                action: Some(action),
            }),
        )
    }
    fn motion(&self, title: &str) -> gov::ManageNeuronResponse {
        self.propose(
            title,
            gov::Action::Motion(gov::Motion {
                motion_text: title.into(),
            }),
        )
    }
    fn proposal(&self, id: u64) -> gov::SnsProposalRecord {
        let response: gov::ListProposalsResponse = icrc::query_one(
            &self.pic,
            self.governance,
            "list_proposals",
            gov::ListProposals {
                include_reward_status: vec![],
                before_proposal: None,
                limit: 100,
                exclude_type: vec![],
                include_status: vec![],
                include_topics: None,
            },
        );
        response
            .proposals
            .into_iter()
            .find(|p| p.id.as_ref().unwrap().id == id)
            .unwrap()
    }
    fn jupiter_vote(&self, id: u64, vote: i32) {
        let response = self.jupiter_manage(gov::Command::RegisterVote(gov::RegisterVote {
            proposal: Some(gov::ProposalId { id }),
            vote,
        }));
        assert!(
            matches!(
                response.command,
                Some(gov::CommandResponse::RegisterVote(_))
            ),
            "{response:?}"
        );
        for _ in 0..20 {
            self.pic.tick();
        }
    }
}
fn proposal_id(response: gov::ManageNeuronResponse) -> u64 {
    let Some(gov::CommandResponse::MakeProposal(result)) = response.command else {
        panic!("proposal rejected: {response:?}")
    };
    result.proposal_id.unwrap().id
}
fn unauthorized(response: gov::ManageNeuronResponse) {
    let Some(gov::CommandResponse::Error(error)) = response.command else {
        panic!("restricted operation succeeded: {response:?}")
    };
    assert_eq!(
        error.error_type, 2,
        "must fail authorization, not a malformed or inapplicable request: {error:?}"
    );
}

#[test]
#[ignore = "direct-genesis feasibility proof; reviewed official bundle and PocketIC required"]
fn real_normal_genesis_without_sale() {
    normal_genesis_without_sale(false);
}

#[test]
#[ignore = "parked official Swap and real blessed framework upgrade; official bundle and PocketIC required"]
fn real_parked_swap_official_framework_upgrade_without_sale() {
    normal_genesis_without_sale(true);
}

fn normal_genesis_without_sale(complete: bool) {
    use sha2::Digest;
    let artifacts::ArtifactStatus::Ready(artifacts) = artifacts::resolve_from_env(true).unwrap()
    else {
        panic!("required official artifacts missing")
    };
    assert_approved_sns_bundle(&artifacts.manifest);
    let (server, url) = AcceptanceServer::start();
    let pic = pocketic_env::new_pic_with_nns_governance_server(&url);
    let operator =
        Principal::from_text("zkkkd-i34qc-367ln-e2u7o-ezznu-dkfqh-gtfvz-cviph-6qa4v-evtfs-wqe")
            .unwrap();
    let create = |framework| {
        let id = pic.create_canister_on_subnet(
            Some(operator),
            None,
            if complete && framework {
                pic.topology().get_sns().unwrap()
            } else {
                pic.topology().get_app_subnets()[0]
            },
        );
        pic.add_cycles(id, 20_000_000_000_000);
        id
    };
    let governance = create(true);
    let root = create(true);
    let ledger = create(true);
    let index = create(true);
    // Required constructor identity, not a fabricated sale or Swap state.
    let swap = create(true);
    let stream = create(false);
    let jupiter = operator;
    let actor = fs::read(
        workspace().join("target/wasm32-unknown-unknown/debug/mock_nns_candidate_actor.wasm"),
    )
    .expect("existing local caller Wasm required");
    let evidence = workspace().join("target").join(format!(
        "acceptance-no-sale-{}-{}",
        std::process::id(),
        pic.get_time().as_nanos_since_unix_epoch()
    ));
    fs::create_dir(&evidence).unwrap();
    let logo = Command::new("base64")
        .args(["-w", "0"])
        .arg(local_fixture_logo(&evidence))
        .output()
        .unwrap();
    assert!(logo.status.success());
    let economics: Value = serde_json::from_str(include_str!(
        "../../../deploy/acceptance/canonical-sns-economics.json"
    ))
    .unwrap();
    let now = pic.get_time().as_nanos_since_unix_epoch() / 1_000_000_000;
    let allocations = [
        (
            operator,
            OPERATOR_MEMO,
            3_000_000_000,
            1_296_060,
            vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
        ),
        (
            jupiter,
            JUPITER_MEMO,
            10_000_000_000_000_000,
            1_296_060,
            vec![4],
        ),
        (
            operator,
            RESERVE_MEMO,
            TOTAL - 10_000_000_000_000_000 - 3_000_000_000,
            0,
            vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
        ),
    ];
    let mut init: gov::Governance =
        decode_one(&gov::governance_init_arg(Some(ledger), Some(root))).unwrap();
    init.swap_canister_id = Some(swap);
    if complete {
        init.deployed_version = Some(reviewed_framework_version(&artifacts, true));
    }
    init.genesis_timestamp_seconds = now;
    init.sns_initialization_parameters =
        "Direct no-sale IO genesis; no SNS-W creation or swap".into();
    init.sns_metadata = Some(gov::ManageSnsMetadata {
        name: Some("IO Protocol".into()),
        description: Some("Acceptance-only direct IO governance genesis".into()),
        url: Some("https://ghrkr-eyaaa-aaaas-amxvq-cai.icp0.io/".into()),
        logo: Some(format!(
            "data:image/png;base64,{}",
            String::from_utf8(logo.stdout).unwrap()
        )),
    });
    let parameters = init.parameters.as_mut().unwrap();
    parameters.reject_cost_e8s = economics["proposal_reject_cost_e8s"].as_u64();
    parameters.max_dissolve_delay_seconds = economics["maximum_dissolve_delay_seconds"].as_u64();
    parameters.wait_for_quiet_deadline_increase_seconds =
        economics["wait_for_quiet_deadline_increase_seconds"].as_u64();
    parameters
        .voting_rewards_parameters
        .as_mut()
        .unwrap()
        .reward_rate_transition_duration_seconds =
        economics["reward_rate_transition_seconds"].as_u64();
    init.neurons = allocations
        .iter()
        .map(|(owner, memo, stake, delay, permissions)| {
            let id = gov::compute_neuron_staking_subaccount(*owner, *memo).to_vec();
            (
                hex::encode(&id),
                gov::SnsNeuronRecord {
                    id: Some(gov::NeuronId { id }),
                    permissions: vec![gov::NeuronPermission {
                        principal: Some(*owner),
                        permission_type: permissions.clone(),
                    }],
                    cached_neuron_stake_e8s: *stake,
                    neuron_fees_e8s: 0,
                    created_timestamp_seconds: now,
                    aging_since_timestamp_seconds: now,
                    dissolve_state: Some(gov::DissolveState::DissolveDelaySeconds(*delay)),
                    voting_power_percentage_multiplier: 100,
                    maturity_e8s_equivalent: 0,
                    staked_maturity_e8s_equivalent: None,
                    source_nns_neuron_id: None,
                    auto_stake_maturity: Some(false),
                    vesting_period_seconds: Some(0),
                    disburse_maturity_in_progress: vec![],
                    followees: vec![],
                    topic_followees: None,
                    latest_reward_event_participation: None,
                },
            )
        })
        .collect();
    let balances = allocations
        .iter()
        .map(|(owner, memo, stake, _, _)| {
            (
                icrc::account(
                    governance,
                    Some(gov::compute_neuron_staking_subaccount(*owner, *memo)),
                ),
                *stake,
            )
        })
        .collect();
    let icrc::LedgerArgument::Init(mut ledger_init) = decode_one(&icrc::ledger_init_arg(
        root,
        icrc::account(governance, None),
        balances,
    ))
    .unwrap() else {
        unreachable!()
    };
    ledger_init.token_name = "IO".into();
    ledger_init.token_symbol = "IO".into();
    ledger_init.metadata.push((
        "icrc1:logo".into(),
        icrc::MetadataValue::Text(init.sns_metadata.as_ref().unwrap().logo.clone().unwrap()),
    ));
    ledger_init.index_principal = Some(index);
    let mut payloads = vec![
        (governance, "sns_governance", encode_one(&init).unwrap()),
        (
            ledger,
            "sns_ledger",
            encode_one(icrc::LedgerArgument::Init(ledger_init)).unwrap(),
        ),
        (index, "sns_index", icrc::index_init_arg(ledger)),
        (
            root,
            "sns_root",
            encode_one(crate::sns_root_setup::SnsRootCanister {
                governance_canister_id: Some(governance),
                ledger_canister_id: Some(ledger),
                swap_canister_id: Some(swap),
                index_canister_id: Some(index),
                dapp_canister_ids: vec![stream],
                archive_canister_ids: vec![],
                extensions: None,
                testflight: false,
                timers: None,
            })
            .unwrap(),
        ),
    ];
    if complete {
        // This is pinned constructor configuration, not a test scheduling shortcut:
        // canister time is a u64 nanosecond count, so this opening second is unreachable.
        payloads.push((
            swap,
            "sns_swap",
            encode_one(NoSaleSwapInit {
                nns_governance_canister_id: io_production_wiring::NNS_GOVERNANCE_PRINCIPAL.into(),
                sns_governance_canister_id: governance.to_text(),
                sns_ledger_canister_id: ledger.to_text(),
                icp_ledger_canister_id: io_production_wiring::ICP_LEDGER_PRINCIPAL.into(),
                sns_root_canister_id: root.to_text(),
                fallback_controller_principal_ids: vec![operator.to_text()],
                transaction_fee_e8s: Some(10_000),
                neuron_minimum_stake_e8s: Some(100_000_000),
                should_auto_finalize: Some(false),
                sns_token_e8s: Some(0),
                min_participants: Some(1),
                min_direct_participation_icp_e8s: Some(100_000_000),
                max_direct_participation_icp_e8s: Some(100_000_000),
                min_participant_icp_e8s: Some(100_000_000),
                max_participant_icp_e8s: Some(100_000_000),
                swap_start_timestamp_seconds: Some(u64::MAX),
                swap_due_timestamp_seconds: Some(u64::MAX),
                neuron_basket_construction_parameters: Some(
                    swap::NeuronBasketConstructionParameters {
                        count: 2,
                        dissolve_delay_interval_seconds: 86_400,
                    },
                ),
                // Manual-constructor sentinel, never a claimed NNS creation proposal or SNS-W record.
                nns_proposal_id: Some(0),
            })
            .unwrap(),
        ));
        // Install Ledger, Index, Root and Swap before Governance records their hashes.
        payloads.rotate_left(1);
    }
    for (id, key, bytes) in payloads {
        fs::write(evidence.join(format!("{key}-init.bin")), &bytes).unwrap();
        pic.install_canister(
            id,
            if complete {
                artifacts
                    .load_required_source_wasm_gz(if key == "sns_governance" {
                        "sns_governance_baseline"
                    } else {
                        key
                    })
                    .unwrap()
            } else {
                artifacts.load_required(key).unwrap()
            },
            bytes,
            Some(operator),
        );
        assert_eq!(
            pic.canister_status(id, Some(operator)).unwrap().module_hash,
            Some(if complete {
                hex::decode(
                    artifacts
                        .manifest
                        .source_sha256(if key == "sns_governance" {
                            "sns_governance_baseline"
                        } else {
                            key
                        })
                        .unwrap(),
                )
                .unwrap()
            } else {
                hex::decode(artifacts.manifest.require_hash(key).unwrap()).unwrap()
            })
        );
    }
    let chunk_store = if complete { stream } else { governance };
    assert_eq!(pic.get_subnet(chunk_store), pic.get_subnet(stream));
    let chunked = gov::ChunkedCanisterWasm {
        wasm_module_hash: sha2::Sha256::digest(&actor).to_vec(),
        store_canister_id: Some(chunk_store),
        chunk_hashes_list: actor
            .chunks(1_000_000)
            .map(|chunk| {
                pic.upload_chunk(chunk_store, Some(operator), chunk.to_vec())
                    .unwrap()
            })
            .collect(),
    };
    for (id, controller) in [
        (governance, root),
        (root, governance),
        (ledger, root),
        (index, root),
        (swap, root),
        (stream, root),
    ] {
        pic.set_controllers(id, Some(operator), vec![controller])
            .unwrap();
        assert_eq!(
            pic.canister_status(id, Some(controller))
                .unwrap()
                .settings
                .controllers,
            vec![controller]
        );
    }
    let fixture = Sale {
        pic,
        operator,
        buyer: Principal::from_slice(&[102; 29]),
        governance_voter: jupiter,
        governance,
        root,
        ledger,
        swap,
        stream,
        icp: Principal::from_text(io_production_wiring::ICP_LEDGER_PRINCIPAL).unwrap(),
        evidence,
        _server: Some(server),
    };
    let supply: Nat = icrc::query_one(&fixture.pic, ledger, "icrc1_total_supply", ());
    assert_eq!(supply, Nat::from(TOTAL));
    assert_eq!(fixture.balance(governance, None), 0);
    for method in ["icrc1_name", "icrc1_symbol"] {
        let text: String = icrc::query_one(&fixture.pic, ledger, method, ());
        assert_eq!(text, "IO");
    }
    let metadata: Vec<(String, icrc::MetadataValue)> =
        icrc::query_one(&fixture.pic, ledger, "icrc1_metadata", ());
    for key in ["icrc1:name", "icrc1:symbol"] {
        assert!(metadata.iter().any(
            |(k, v)| k == key && matches!(v, icrc::MetadataValue::Text(value) if value == "IO")
        ));
    }
    assert!(metadata.iter().any(|(key, value)| key == "icrc1:logo"
        && matches!(value, icrc::MetadataValue::Text(logo) if logo.starts_with("data:image/png;base64,"))));
    let parameter_bytes = fixture
        .pic
        .query_call(
            governance,
            Principal::anonymous(),
            "get_nervous_system_parameters",
            candid::encode_args(()).unwrap(),
        )
        .unwrap();
    let parameters: gov::NervousSystemParameters = decode_one(&parameter_bytes).unwrap();
    assert_eq!(parameters.automatically_advance_target_version, Some(false));
    assert_eq!(parameters.max_number_of_neurons, Some(1_000));
    assert_eq!(parameters.reject_cost_e8s, Some(1_000_000_000));
    fs::write(
        fixture.evidence.join("effective-governance-parameters.bin"),
        parameter_bytes,
    )
    .unwrap();
    let mode: GetModeResponse =
        icrc::query_one(&fixture.pic, governance, "get_mode", swap::EmptyRecord {});
    assert_eq!(mode.mode, Some(1));
    assert_eq!(fixture.neurons().len(), 3);
    assert_eq!(
        fixture.neuron(jupiter, JUPITER_MEMO).permissions,
        vec![gov::NeuronPermission {
            principal: Some(jupiter),
            permission_type: vec![4]
        }]
    );
    assert_eq!(
        fixture
            .neuron(jupiter, JUPITER_MEMO)
            .voting_power_percentage_multiplier,
        100
    );
    for (owner, memo, stake, _, _) in &allocations {
        assert_eq!(
            fixture.neuron(*owner, *memo).cached_neuron_stake_e8s,
            *stake
        );
        assert_eq!(
            fixture.balance(
                governance,
                Some(gov::compute_neuron_staking_subaccount(*owner, *memo).to_vec())
            ),
            *stake
        );
    }
    assert_eq!(fixture.balance(swap, None), 0);
    write_json(
        &fixture.evidence.join("no-sale-genesis.json"),
        &json!({
            "mode":mode.mode,"total_supply_e8s":TOTAL,"token_name":"IO","token_symbol":"IO",
            "operator_stake_e8s":3_000_000_000u64,"jupiter_stake_e8s":10_000_000_000_000_000u64,
            "reserve_stake_e8s":TOTAL-10_000_000_000_000_000u64-3_000_000_000u64,
            "swap_balance_e8s":0,"swap_installed":complete,"sns_w_deployment_registered":false,
        }),
    );
    unauthorized(fixture.manage(
        fixture.buyer,
        gov::compute_neuron_staking_subaccount(jupiter, JUPITER_MEMO).to_vec(),
        gov::Command::RegisterVote(gov::RegisterVote {
            proposal: Some(gov::ProposalId { id: 1 }),
            vote: 1,
        }),
    ));
    let proposal = proposal_id(fixture.propose(
        "Direct-genesis Root installation",
        gov::Action::UpgradeSnsControlledCanister(gov::UpgradeSnsControlledCanister {
            canister_id: Some(stream),
            new_canister_wasm: vec![],
            mode: Some(1),
            canister_upgrade_arg: Some(vec![]),
            chunked_canister_wasm: Some(chunked.clone()),
        }),
    ));
    assert_eq!(
        fixture.proposal(proposal).executed_timestamp_seconds,
        0,
        "operator alone must not adopt"
    );
    let tally = fixture.proposal(proposal).latest_tally.unwrap();
    assert_eq!(
        tally.total.checked_sub(tally.yes).unwrap(),
        10_000_000_000_000_000
    );
    fixture.jupiter_vote(proposal, 1);
    assert!(fixture.proposal(proposal).executed_timestamp_seconds > 0);
    // Root acknowledges change_canister before its asynchronous install finishes.
    let actor_hash = sha2::Sha256::digest(&actor).to_vec();
    for _ in 0..60 {
        if fixture
            .pic
            .canister_status(stream, Some(root))
            .unwrap()
            .module_hash
            == Some(actor_hash.clone())
        {
            break;
        }
        fixture.pic.advance_time(Duration::from_secs(1));
        for _ in 0..3 {
            fixture.pic.tick();
        }
    }
    fs::write(
        fixture.evidence.join("dapp-install-proposal.txt"),
        format!("{:#?}\n", fixture.proposal(proposal)),
    )
    .unwrap();
    if fixture
        .pic
        .canister_status(stream, Some(root))
        .unwrap()
        .module_hash
        .is_none()
    {
        let logs = fixture.pic.fetch_canister_logs(root, governance);
        fs::write(
            fixture.evidence.join("root-dapp-install-logs.txt"),
            format!("{logs:#?}\n"),
        )
        .unwrap();
    }
    assert_eq!(
        fixture
            .pic
            .canister_status(stream, Some(root))
            .unwrap()
            .module_hash,
        Some(sha2::Sha256::digest(&actor).to_vec())
    );
    let upgrade = proposal_id(fixture.propose(
        "Direct-genesis Root upgrade",
        gov::Action::UpgradeSnsControlledCanister(gov::UpgradeSnsControlledCanister {
            canister_id: Some(stream),
            new_canister_wasm: vec![],
            mode: Some(3),
            canister_upgrade_arg: Some(vec![]),
            chunked_canister_wasm: Some(chunked),
        }),
    ));
    fixture.jupiter_vote(upgrade, 1);
    assert!(fixture.proposal(upgrade).executed_timestamp_seconds > 0);
    fs::write(
        fixture.evidence.join("root-actions.txt"),
        format!(
            "install={:#?}\nupgrade={:#?}\n",
            fixture.proposal(proposal),
            fixture.proposal(upgrade)
        ),
    )
    .unwrap();
    if complete {
        prove_parked_standard_upgrade(&fixture, &artifacts);
    } else {
        let framework_upgrade = fixture.propose(
            "Probe standard framework upgrade without a Swap module",
            gov::Action::UpgradeSnsToNextVersion(gov::EmptyRecord {}),
        );
        let Some(gov::CommandResponse::Error(error)) = framework_upgrade.command else {
            panic!("genesis without deployed_version unexpectedly supports a framework upgrade: {framework_upgrade:?}");
        };
        assert_eq!(error.error_type, 15);
        assert_eq!(
            error.error_message,
            "1 defects in Proposal:\nGovernanceProto.deployed_version is not set.",
            "unexpected framework upgrade blocker: {error:?}"
        );
        fs::write(
            fixture.evidence.join("framework-upgrade-blocker.txt"),
            format!("{error:?}\n"),
        )
        .unwrap();
    }
    let summary_bytes = fixture
        .pic
        .update_call(
            root,
            Principal::anonymous(),
            "get_sns_canisters_summary",
            encode_one(RootSummaryRequest {
                update_canister_list: Some(false),
            })
            .unwrap(),
        )
        .unwrap();
    let summary: RootSummary = decode_one(&summary_bytes).unwrap();
    if complete {
        assert_eq!(
            summary.swap.unwrap().status.unwrap().module_hash,
            Some(hex::decode(artifacts.manifest.source_sha256("sns_swap").unwrap()).unwrap())
        );
    } else {
        assert!(summary.swap.unwrap().status.is_none());
    }
    assert_eq!(summary.dapps.len(), 1);
    assert_eq!(
        summary.dapps[0].status.as_ref().unwrap().module_hash,
        Some(sha2::Sha256::digest(&actor).to_vec())
    );
    fs::write(fixture.evidence.join("root-summary.bin"), summary_bytes).unwrap();
    for method in ["get_running_sns_version", "get_upgrade_journal"] {
        let bytes = fixture
            .pic
            .query_call(
                governance,
                Principal::anonymous(),
                method,
                encode_one(gov::EmptyRecord {}).unwrap(),
            )
            .unwrap();
        fs::write(fixture.evidence.join(format!("{method}-before.bin")), bytes).unwrap();
    }
    fixture
        .pic
        .advance_time(Duration::from_secs(2 * 86_400 + 43_200 + 300));
    for _ in 0..80 {
        fixture.pic.tick();
    }
    let event: io_governance_types::SnsRewardEvent =
        icrc::query_one(&fixture.pic, governance, "get_latest_reward_event", ());
    assert_eq!(event.distributed_e8s_equivalent, 0);
    if !complete {
        assert!(event.settled_proposals.iter().any(|id| id.id == proposal));
        assert_eq!(event.distributed_e8s_equivalent, 0);
        let jupiter_neuron = fixture.neuron(jupiter, JUPITER_MEMO);
        let participation = jupiter_neuron.latest_reward_event_participation.unwrap();
        assert_eq!(
            participation.reward_event_end_timestamp_seconds,
            event.end_timestamp_seconds
        );
        assert!(participation.reward_shares.unwrap().0 > 0u8.into());
    }
    // No contribution or finalization method was called.
    assert_eq!(
        fixture
            .pic
            .canister_status(swap, Some(root))
            .unwrap()
            .module_hash,
        if complete {
            Some(hex::decode(artifacts.manifest.source_sha256("sns_swap").unwrap()).unwrap())
        } else {
            None
        }
    );
    assert_eq!(fixture.balance(swap, None), 0);
    let final_parameters: gov::NervousSystemParameters = icrc::query_one(
        &fixture.pic,
        governance,
        "get_nervous_system_parameters",
        (),
    );
    assert_eq!(
        final_parameters.automatically_advance_target_version,
        Some(false)
    );
    for method in ["get_running_sns_version", "get_upgrade_journal"] {
        let bytes = fixture
            .pic
            .query_call(
                governance,
                Principal::anonymous(),
                method,
                encode_one(gov::EmptyRecord {}).unwrap(),
            )
            .unwrap();
        fs::write(fixture.evidence.join(format!("{method}-after.bin")), bytes).unwrap();
    }
    let metrics = fixture
        .pic
        .update_call(
            governance,
            Principal::anonymous(),
            "get_metrics_replicated",
            encode_one(MetricsRequest {
                time_window_seconds: Some(86_400),
            })
            .unwrap(),
        )
        .unwrap();
    let decoded_metrics: MetricsResponse = decode_one(&metrics).unwrap();
    assert!(matches!(decoded_metrics.get_metrics_result, Some(Ok(_))));
    fs::write(fixture.evidence.join("governance-metrics.bin"), metrics).unwrap();
    if complete {
        assert_parked_swap(&fixture, true);
        let supply: Nat = icrc::query_one(&fixture.pic, ledger, "icrc1_total_supply", ());
        assert_eq!(supply, Nat::from(TOTAL));
        for (owner, memo, stake, _, _) in allocations {
            assert_eq!(
                fixture.balance(
                    governance,
                    Some(gov::compute_neuron_staking_subaccount(owner, memo).to_vec())
                ),
                stake
            );
        }
        println!("parked Adopted Swap, exact no-sale allocations and IO/IO; real official predecessor-to-successor Governance framework upgrade; no sale/finalization/ICP participation; constructor acceptance is separate from supported sale Params validation; evidence={}",fixture.evidence.display());
        return;
    }
    let reserve = gov::compute_neuron_staking_subaccount(operator, RESERVE_MEMO);
    let stream_reserve =
        hex::decode("781a5fc2d878692d1fa555359c190f96d61f85b392cf11b80f4666169d789027").unwrap();
    let response = fixture.manage(
        operator,
        reserve.to_vec(),
        gov::Command::Disburse(gov::Disburse {
            amount: None,
            to_account: Some(gov::Account {
                owner: Some(stream),
                subaccount: Some(gov::Subaccount {
                    subaccount: stream_reserve.clone(),
                }),
            }),
        }),
    );
    assert!(
        matches!(response.command, Some(gov::CommandResponse::Disburse(_))),
        "{response:?}"
    );
    let supply: Nat = icrc::query_one(&fixture.pic, ledger, "icrc1_total_supply", ());
    let supply: u64 = supply.0.try_into().unwrap();
    let excluded = fixture.balance(
        governance,
        Some(gov::compute_neuron_staking_subaccount(jupiter, JUPITER_MEMO).to_vec()),
    );
    let reserve_balance = fixture.balance(stream, Some(stream_reserve));
    let c0 = supply
        .checked_sub(reserve_balance)
        .unwrap()
        .checked_sub(excluded)
        .unwrap();
    assert_eq!(supply, TOTAL - 10_000);
    assert_eq!(fixture.balance(governance, Some(reserve.to_vec())), 0);
    assert_eq!(c0, 3_000_000_000);
    write_json(
        &fixture.evidence.join("no-sale-capitalization.json"),
        &json!({
            "canonical_supply_e8s": supply, "stream_reserve_e8s":reserve_balance,
            "excluded_jupiter_e8s":excluded, "c0_e8s":c0, "b0_e8s":c0,
            "additional_permanent_capital_e8s":(u128::from(c0)*2).div_ceil(3),
            "independent_anchor_e8s":1_000_000_000, "swap_balance_e8s":0,
            "capitalization_transfers_executed":false,
        }),
    );
    println!("direct Normal genesis without sale: exact 1B IO; IO/IO branding; sole DAO controllers; Vote-only Jupiter caller adopted Root install+upgrade; settled zero-native-reward event with exact shares; Swap reserved but never installed; evidence={}", fixture.evidence.display());
}

#[derive(CandidType, Default)]
struct NoSaleSwapInit {
    nns_governance_canister_id: String,
    sns_governance_canister_id: String,
    sns_ledger_canister_id: String,
    icp_ledger_canister_id: String,
    sns_root_canister_id: String,
    fallback_controller_principal_ids: Vec<String>,
    transaction_fee_e8s: Option<u64>,
    neuron_minimum_stake_e8s: Option<u64>,
    should_auto_finalize: Option<bool>,
    sns_token_e8s: Option<u64>,
    min_participants: Option<u32>,
    min_direct_participation_icp_e8s: Option<u64>,
    max_direct_participation_icp_e8s: Option<u64>,
    min_participant_icp_e8s: Option<u64>,
    max_participant_icp_e8s: Option<u64>,
    swap_start_timestamp_seconds: Option<u64>,
    swap_due_timestamp_seconds: Option<u64>,
    neuron_basket_construction_parameters: Option<swap::NeuronBasketConstructionParameters>,
    nns_proposal_id: Option<u64>,
}

fn reviewed_framework_version(artifacts: &artifacts::ArtifactSet, prior: bool) -> gov::Version {
    let approved = approved_sns_manifest();
    for key in ["sns_wasm", "sns_governance_baseline"] {
        for field in ["sha256", "source_sha256", "upstream_rev", "nns_proposal_id"] {
            assert_eq!(
                artifacts.manifest.artifact(key, field),
                approved.artifact(key, field)
            );
        }
    }
    let hash = |key| hex::decode(artifacts.manifest.source_sha256(key).unwrap()).unwrap();
    gov::Version {
        root_wasm_hash: hash("sns_root"),
        governance_wasm_hash: hash(if prior {
            "sns_governance_baseline"
        } else {
            "sns_governance"
        }),
        ledger_wasm_hash: hash("sns_ledger"),
        swap_wasm_hash: hash("sns_swap"),
        // The blessed Archive template is recorded even with zero running Archives.
        archive_wasm_hash: hash("sns_archive"),
        index_wasm_hash: hash("sns_index"),
    }
}

fn assert_parked_swap(fixture: &Sale, timer_ran: bool) {
    #[derive(CandidType, Deserialize)]
    struct GetTimersResponse {
        timers: Option<gov::Timers>,
    }
    let lifecycle: swap::GetLifecycleResponse = icrc::query_one(
        &fixture.pic,
        fixture.swap,
        "get_lifecycle",
        gov::EmptyRecord {},
    );
    assert_eq!(lifecycle.lifecycle, Some(5)); // Adopted, never Committed/Aborted.
    assert!(
        lifecycle
            .decentralization_sale_open_timestamp_seconds
            .unwrap()
            > u64::MAX / 1_000_000_000
    );
    assert_eq!(
        lifecycle.decentralization_sale_open_timestamp_seconds,
        Some(u64::MAX)
    );
    assert_eq!(
        lifecycle.decentralization_swap_termination_timestamp_seconds,
        None
    );
    let timers: GetTimersResponse = icrc::query_one(
        &fixture.pic,
        fixture.swap,
        "get_timers",
        gov::EmptyRecord {},
    );
    let timers = timers.timers.unwrap();
    assert_eq!(timers.requires_periodic_tasks, Some(true));
    if timer_ran {
        assert!(timers.last_spawned_timestamp_seconds.is_some());
    }
    let params: swap::GetSaleParametersResponse = icrc::query_one(
        &fixture.pic,
        fixture.swap,
        "get_sale_parameters",
        gov::EmptyRecord {},
    );
    assert_eq!(params.params.as_ref().unwrap().sns_token_e8s, 0);
    let derived: swap::GetDerivedStateResponse = icrc::query_one(
        &fixture.pic,
        fixture.swap,
        "get_derived_state",
        gov::EmptyRecord {},
    );
    assert_eq!(derived.buyer_total_icp_e8s, Some(0));
    assert_eq!(derived.sns_tokens_per_icp, Some(0.0));
    assert_eq!(derived.direct_participant_count, Some(0));
    let io = fixture.balance(fixture.swap, None);
    let icp: IcpTokens = icrc::query_one(
        &fixture.pic,
        fixture.icp,
        "account_balance",
        IcpBalance {
            account: Account::new(fixture.swap, None)
                .icp_account_identifier_bytes()
                .to_vec(),
        },
    );
    fs::write(fixture.evidence.join(if timer_ran {"parked-swap-after.txt"} else {"parked-swap-before.txt"}),format!("lifecycle={lifecycle:#?}\ntimers={timers:#?}\nparams={params:#?}\nderived={derived:#?}\nquarantined_default_account_icp_e8s={}\nclaim_bearing_default_account_io_e8s={}\n",icp.e8s,io)).unwrap();
}

fn prove_parked_standard_upgrade(fixture: &Sale, artifacts: &artifacts::ArtifactSet) {
    #[derive(CandidType, Deserialize)]
    struct RunningVersion {
        deployed_version: Option<gov::Version>,
        pending_version: Option<gov::EmptyRecord>,
    }
    #[derive(CandidType, Deserialize)]
    struct Journal {
        entries: Vec<JournalEntry>,
    }
    #[derive(CandidType, Deserialize)]
    struct JournalEntry {
        event: Option<JournalEvent>,
    }
    #[derive(CandidType, Deserialize)]
    enum JournalEvent {
        UpgradeStepsRefreshed(gov::EmptyRecord),
        UpgradeStepsReset(gov::EmptyRecord),
        TargetVersionSet(gov::EmptyRecord),
        TargetVersionReset(gov::EmptyRecord),
        UpgradeStarted(gov::EmptyRecord),
        UpgradeOutcome(UpgradeOutcome),
    }
    #[derive(CandidType, Deserialize)]
    struct UpgradeOutcome {
        status: Option<UpgradeStatus>,
    }
    #[derive(CandidType, Deserialize)]
    enum UpgradeStatus {
        Success(gov::EmptyRecord),
        Timeout(gov::EmptyRecord),
        InvalidState(gov::EmptyRecord),
        ExternalFailure(gov::EmptyRecord),
    }
    #[derive(CandidType, Deserialize)]
    struct Versions {
        versions: Vec<gov::Version>,
    }
    #[derive(CandidType, Deserialize)]
    struct JournalResponse {
        deployed_version: Option<gov::Version>,
        upgrade_journal: Option<Journal>,
        upgrade_steps: Option<Versions>,
        target_version: Option<gov::Version>,
    }
    let parameters: gov::NervousSystemParameters = icrc::query_one(
        &fixture.pic,
        fixture.governance,
        "get_nervous_system_parameters",
        (),
    );
    assert_eq!(parameters.automatically_advance_target_version, Some(false));
    assert!(parameters.default_followees.unwrap().followees.is_empty());
    let check_permissions = || {
        let neuron = fixture.neuron(fixture.governance_voter, JUPITER_MEMO);
        assert_eq!(
            neuron.permissions,
            vec![gov::NeuronPermission {
                principal: Some(fixture.governance_voter),
                permission_type: vec![4]
            }]
        );
        assert!(neuron.followees.is_empty());
        assert!(neuron.topic_followees.is_none());
        for caller in [fixture.operator, fixture.buyer] {
            let command = gov::Command::AddNeuronPermissions(gov::AddNeuronPermissions {
                principal_id: Some(fixture.operator),
                permissions_to_add: Some(gov::NeuronPermissionList {
                    permissions: vec![2],
                }),
            });
            let response = if caller == fixture.governance_voter {
                fixture.jupiter_manage(command)
            } else {
                fixture.manage(
                    caller,
                    gov::compute_neuron_staking_subaccount(fixture.governance_voter, JUPITER_MEMO)
                        .to_vec(),
                    command,
                )
            };
            unauthorized(response);
        }
    };
    check_permissions();
    assert_parked_swap(fixture, false);
    let (sns_wasm, predecessor, successor) = exercise_official_upgrade_discovery(
        &fixture.pic,
        artifacts,
        fixture.governance,
        &fixture.evidence,
    );
    let physical = |phase: &str, version: &gov::Version| {
        let bytes = fixture
            .pic
            .update_call(
                fixture.root,
                Principal::anonymous(),
                "get_sns_canisters_summary",
                encode_one(RootSummaryRequest {
                    update_canister_list: Some(false),
                })
                .unwrap(),
            )
            .unwrap();
        let summary: RootSummary = decode_one(&bytes).unwrap();
        assert!(summary.archives.is_empty());
        let listed: crate::sns_root_setup::ListSnsCanistersResponse = icrc::query_one(
            &fixture.pic,
            fixture.root,
            "list_sns_canisters",
            gov::EmptyRecord {},
        );
        for (record, id, hash, controller) in [
            (
                summary.root,
                fixture.root,
                &version.root_wasm_hash,
                fixture.governance,
            ),
            (
                summary.governance,
                fixture.governance,
                &version.governance_wasm_hash,
                fixture.root,
            ),
            (
                summary.ledger,
                fixture.ledger,
                &version.ledger_wasm_hash,
                fixture.root,
            ),
            (
                summary.index,
                listed.index.unwrap(),
                &version.index_wasm_hash,
                fixture.root,
            ),
            (
                summary.swap,
                fixture.swap,
                &version.swap_wasm_hash,
                fixture.root,
            ),
        ] {
            let record = record.unwrap();
            assert_eq!(record.canister_id, Some(id));
            let status = record.status.unwrap();
            assert_eq!(status.module_hash, Some(hash.clone()));
            assert_eq!(status.settings.controllers, vec![controller]);
        }
        fs::write(
            fixture.evidence.join(format!("framework-root-{phase}.bin")),
            bytes,
        )
        .unwrap();
    };
    physical("before", &predecessor);
    let before = fixture
        .pic
        .query_call(
            fixture.governance,
            Principal::anonymous(),
            "get_running_sns_version",
            encode_one(gov::EmptyRecord {}).unwrap(),
        )
        .unwrap();
    let before_version: RunningVersion = decode_one(&before).unwrap();
    assert_eq!(before_version.deployed_version, Some(predecessor.clone()));
    assert_eq!(before_version.pending_version, None);
    fs::write(
        fixture.evidence.join("framework-version-before.bin"),
        before,
    )
    .unwrap();
    // The pinned Governance refresh backoff is one hour. Let Governance itself
    // discover the available successor, while automatic advancement is disabled.
    fixture.pic.advance_time(Duration::from_secs(3_601));
    let mut discovered = false;
    for _ in 0..20 {
        for _ in 0..5 {
            fixture.pic.tick();
        }
        let bytes = fixture
            .pic
            .query_call(
                fixture.governance,
                Principal::anonymous(),
                "get_upgrade_journal",
                encode_one(gov::EmptyRecord {}).unwrap(),
            )
            .unwrap();
        let journal: JournalResponse = decode_one(&bytes).unwrap();
        assert_eq!(journal.deployed_version, Some(predecessor.clone()));
        assert_eq!(journal.target_version, None);
        if journal
            .upgrade_steps
            .is_some_and(|steps| steps.versions == vec![predecessor.clone(), successor.clone()])
        {
            fs::write(
                fixture
                    .evidence
                    .join("framework-upgrade-journal-before.bin"),
                bytes,
            )
            .unwrap();
            discovered = true;
            break;
        }
        fixture.pic.advance_time(Duration::from_secs(60));
    }
    assert!(
        discovered,
        "Governance did not cache the default upgrade steps"
    );
    physical("before-proposal", &predecessor);
    let proposal = proposal_id(fixture.propose(
        "Reviewed official framework Governance successor",
        gov::Action::UpgradeSnsToNextVersion(gov::EmptyRecord {}),
    ));
    assert_eq!(fixture.proposal(proposal).executed_timestamp_seconds, 0);
    fixture.jupiter_vote(proposal, 1);
    let mut completed = false;
    for _ in 0..30 {
        fixture.pic.advance_time(Duration::from_secs(60));
        for _ in 0..5 {
            fixture.pic.tick();
        }
        if let Ok(bytes) = fixture.pic.query_call(
            fixture.governance,
            Principal::anonymous(),
            "get_running_sns_version",
            encode_one(gov::EmptyRecord {}).unwrap(),
        ) {
            let version: RunningVersion = decode_one(&bytes).unwrap();
            if version.deployed_version == Some(successor.clone())
                && version.pending_version.is_none()
            {
                fs::write(fixture.evidence.join("framework-version-after.bin"), bytes).unwrap();
                completed = true;
                break;
            }
        }
    }
    assert!(
        completed,
        "framework upgrade did not commit: {:?}",
        fixture.proposal(proposal)
    );
    assert!(fixture.proposal(proposal).executed_timestamp_seconds > 0);
    physical("after", &successor);
    check_permissions();
    let journal = fixture
        .pic
        .query_call(
            fixture.governance,
            Principal::anonymous(),
            "get_upgrade_journal",
            encode_one(gov::EmptyRecord {}).unwrap(),
        )
        .unwrap();
    let decoded: JournalResponse = decode_one(&journal).unwrap();
    assert_eq!(decoded.deployed_version, Some(successor));
    assert_eq!(decoded.target_version, None);
    let entries = decoded.upgrade_journal.unwrap().entries;
    assert!(entries
        .iter()
        .any(|entry| matches!(entry.event, Some(JournalEvent::UpgradeStarted(_)))));
    assert!(entries.iter().any(|entry| matches!(
        entry.event,
        Some(JournalEvent::UpgradeOutcome(UpgradeOutcome {
            status: Some(UpgradeStatus::Success(_))
        }))
    )));
    assert!(!entries.iter().any(|entry| matches!(
        entry.event,
        Some(JournalEvent::UpgradeOutcome(UpgradeOutcome {
            status: Some(
                UpgradeStatus::Timeout(_)
                    | UpgradeStatus::InvalidState(_)
                    | UpgradeStatus::ExternalFailure(_)
            )
        }))
    )));
    fs::write(
        fixture.evidence.join("framework-upgrade-journal.bin"),
        journal,
    )
    .unwrap();
    fs::write(
        fixture.evidence.join("framework-upgrade-proposal.txt"),
        format!("{:#?}\n", fixture.proposal(proposal)),
    )
    .unwrap();
    let registry: crate::sns_wasm_setup::ListDeployedSnsesResponse = icrc::query_one(
        &fixture.pic,
        sns_wasm,
        "list_deployed_snses",
        gov::EmptyRecord {},
    );
    assert!(registry.instances.is_empty());
    assert_parked_swap(fixture, true);
}

#[test]
#[ignore = "reviewed standard Swap constructor rejection; official bundle and PocketIC required"]
fn real_swap_rejects_missing_sale_fields() {
    let artifacts::ArtifactStatus::Ready(artifacts) = artifacts::resolve_from_env(true).unwrap()
    else {
        panic!("required official artifacts missing")
    };
    assert_approved_sns_bundle(&artifacts.manifest);
    let (server, url) = AcceptanceServer::start();
    let pic = pocket_ic::PocketIcBuilder::new()
        .with_server_url(url.parse().unwrap())
        .with_sns_subnet()
        .build();
    let subnet = pic.topology().get_sns().unwrap();
    let operator = Principal::from_slice(&[91; 29]);
    let root = pic.create_canister_on_subnet(Some(operator), None, subnet);
    let governance = pic.create_canister_on_subnet(Some(operator), None, subnet);
    let ledger = pic.create_canister_on_subnet(Some(operator), None, subnet);
    let wasm = artifacts.load_required_source_wasm_gz("sns_swap").unwrap();
    let hash = hex::decode(artifacts.manifest.source_sha256("sns_swap").unwrap()).unwrap();
    let evidence = workspace()
        .join("target")
        .join(format!("no-sale-swap-rejection-{}", std::process::id()));
    fs::create_dir(&evidence).unwrap();
    for allocation in [None, Some(0)] {
        // Each rejected install consumes the canister's install-code allowance.
        // Keep independent invalid constructors on separate fresh canisters.
        let swap = pic.create_canister_on_subnet(
            Some(operator),
            Some(pocket_ic::CanisterSettings {
                controllers: Some(vec![root]),
                ..Default::default()
            }),
            subnet,
        );
        pic.add_cycles(swap, 20_000_000_000_000);
        let chunks = wasm
            .chunks(1_000_000)
            .map(|chunk| pic.upload_chunk(swap, Some(root), chunk.to_vec()).unwrap())
            .collect::<Vec<_>>();
        let args = encode_one(NoSaleSwapInit {
            nns_governance_canister_id: io_production_wiring::NNS_GOVERNANCE_PRINCIPAL.into(),
            sns_governance_canister_id: governance.to_text(),
            sns_ledger_canister_id: ledger.to_text(),
            icp_ledger_canister_id: io_production_wiring::ICP_LEDGER_PRINCIPAL.into(),
            sns_root_canister_id: root.to_text(),
            fallback_controller_principal_ids: vec![operator.to_text()],
            transaction_fee_e8s: Some(10_000),
            neuron_minimum_stake_e8s: Some(100_000_000),
            should_auto_finalize: Some(false),
            sns_token_e8s: allocation,
            ..Default::default()
        })
        .unwrap();
        let label = if allocation.is_some() {
            "zero"
        } else {
            "absent"
        };
        fs::write(evidence.join(format!("{label}-init.bin")), &args).unwrap();
        let result = pic.install_chunked_canister(
            swap,
            Some(root),
            pocket_ic::CanisterInstallMode::Install,
            swap,
            chunks.clone(),
            hash.clone(),
            args,
        );
        fs::write(
            evidence.join(format!("{label}-install-result.txt")),
            format!("{result:#?}\n"),
        )
        .unwrap();
        let rejected = result.expect_err("unexpected Swap install: stop without advancing time");
        assert!(rejected
            .reject_message
            .contains("Mandatory fields must be specified:"));
        for field in [
            "min_participants",
            "min_direct_participation_icp_e8s",
            "max_direct_participation_icp_e8s",
            "min_participant_icp_e8s",
            "max_participant_icp_e8s",
            "swap_due_timestamp_seconds",
            "neuron_basket_construction_parameters",
            "nns_proposal_id",
        ] {
            assert!(rejected.reject_message.contains(field), "{rejected:?}");
        }
        let status = pic.canister_status(swap, Some(root)).unwrap();
        assert_eq!(status.module_hash, None);
        assert_eq!(status.settings.controllers, vec![root]);
        fs::write(
            evidence.join(format!("{label}-status.txt")),
            format!("{status:#?}\n"),
        )
        .unwrap();
        for method in ["get_lifecycle", "get_timers"] {
            let reply = pic.query_call(
                swap,
                Principal::anonymous(),
                method,
                encode_one(gov::EmptyRecord {}).unwrap(),
            );
            assert!(
                reply.is_err(),
                "failed install must expose no lifecycle or timers"
            );
            fs::write(
                evidence.join(format!("{label}-{method}.txt")),
                format!("{reply:#?}\n"),
            )
            .unwrap();
        }
    }
    println!("standard Swap rejects omitted sale-opening fields with absent/zero allocation; this does not reject the separately tested fully specified parked constructor; evidence={}", evidence.display());
    drop(pic);
    drop(server);
}

#[test]
#[ignore = "controlled official SNS-W default path; reviewed predecessor/successor and PocketIC required"]
fn real_unregistered_sns_w_official_upgrade_discovery() {
    let artifacts::ArtifactStatus::Ready(artifacts) = artifacts::resolve_from_env(true).unwrap()
    else {
        panic!("required official artifacts missing")
    };
    assert_approved_sns_bundle(&artifacts.manifest);
    let approved = approved_sns_manifest();
    for key in ["sns_wasm", "sns_governance_baseline"] {
        for field in ["sha256", "source_sha256", "upstream_rev", "nns_proposal_id"] {
            assert_eq!(
                artifacts.manifest.artifact(key, field),
                approved.artifact(key, field),
                "unreviewed {key} {field}"
            );
        }
    }
    let (server, url) = AcceptanceServer::start();
    let pic = pocket_ic::PocketIcBuilder::new()
        .with_server_url(url.parse().unwrap())
        .with_nns_subnet()
        .with_sns_subnet()
        .build();
    let observer = pic.create_canister_on_subnet(None, None, pic.topology().get_sns().unwrap());
    let evidence = workspace().join("target").join(format!(
        "unregistered-sns-w-discovery-{}",
        std::process::id()
    ));
    fs::create_dir(&evidence).unwrap();
    exercise_official_upgrade_discovery(&pic, &artifacts, observer, &evidence);
    drop(pic);
    drop(server);
}

fn exercise_official_upgrade_discovery(
    pic: &pocket_ic::PocketIc,
    artifacts: &artifacts::ArtifactSet,
    observer: Principal,
    evidence: &Path,
) -> (Principal, gov::Version, gov::Version) {
    use crate::sns_wasm_setup as sns_w;
    #[derive(CandidType)]
    struct NextRequest {
        current_version: Option<gov::Version>,
        governance_canister_id: Option<Principal>,
    }
    #[derive(CandidType, Deserialize)]
    struct NextResponse {
        next_version: Option<gov::Version>,
    }
    #[derive(CandidType)]
    struct StepsRequest {
        starting_at: Option<gov::Version>,
        sns_governance_canister_id: Option<Principal>,
        limit: u32,
    }
    #[derive(CandidType, Deserialize)]
    struct Step {
        version: Option<gov::Version>,
    }
    #[derive(CandidType, Deserialize)]
    struct StepsResponse {
        steps: Vec<Step>,
    }
    let sns_wasm = crate::nns_setup::install_sns_wasm_on_existing_pic(
        pic,
        artifacts,
        crate::nns_setup::SnsWasmCanisterInitPayload {
            allowed_principals: vec![],
            access_controls_enabled: false,
            sns_subnet_ids: vec![pic.topology().get_sns().unwrap()],
        },
    )
    .unwrap();
    // Controlled local mirror of pinned official publications, not NNS proposal proof.
    // The preceding Governance and the selected successor are both reviewed artifacts.
    let governance_plan = *sns_w::SNS_WASM_PUBLICATION_PLAN
        .iter()
        .find(|plan| plan.canister_type == sns_w::SnsCanisterType::Governance)
        .unwrap();
    for (plan, predecessor) in sns_w::SNS_WASM_PUBLICATION_PLAN
        .iter()
        .map(|plan| (*plan, true))
        .chain(std::iter::once((governance_plan, false)))
    {
        let key = if predecessor && plan.artifact_key == "sns_governance" {
            "sns_governance_baseline"
        } else {
            plan.artifact_key
        };
        let wasm = artifacts.load_required_source_wasm_gz(key).unwrap();
        let hash = hex::decode(artifacts.manifest.source_sha256(key).unwrap()).unwrap();
        let publication = artifacts
            .manifest
            .artifact(key, "nns_proposal_id")
            .unwrap()
            .parse()
            .unwrap();
        let result: sns_w::AddWasmResponse = icrc::update_one(
            pic,
            sns_wasm,
            Principal::anonymous(),
            "add_wasm",
            sns_w::AddWasmRequest {
                hash: hash.clone(),
                wasm: Some(sns_w::SnsWasm {
                    wasm,
                    proposal_id: Some(publication),
                    canister_type: sns_w::sns_canister_type_id(plan.canister_type),
                }),
                skip_update_latest_version: Some(false),
            },
        );
        assert_eq!(result.result, Some(sns_w::AddWasmResult::Hash(hash)));
    }
    let successor = reviewed_framework_version(artifacts, false);
    let predecessor = reviewed_framework_version(artifacts, true);
    let next = pic
        .query_call(
            sns_wasm,
            observer,
            "get_next_sns_version",
            encode_one(NextRequest {
                current_version: Some(predecessor.clone()),
                governance_canister_id: None,
            })
            .unwrap(),
        )
        .unwrap();
    assert_eq!(
        decode_one::<NextResponse>(&next).unwrap().next_version,
        Some(successor.clone())
    );
    fs::write(evidence.join("next-version.bin"), next).unwrap();
    let steps = pic
        .query_call(
            sns_wasm,
            observer,
            "list_upgrade_steps",
            encode_one(StepsRequest {
                starting_at: Some(predecessor.clone()),
                sns_governance_canister_id: Some(observer),
                limit: 10,
            })
            .unwrap(),
        )
        .unwrap();
    let decoded: StepsResponse = decode_one(&steps).unwrap();
    assert_eq!(
        decoded
            .steps
            .into_iter()
            .map(|step| step.version.unwrap())
            .collect::<Vec<_>>(),
        vec![predecessor.clone(), successor.clone()]
    );
    fs::write(evidence.join("upgrade-steps.bin"), steps).unwrap();
    let fetched: sns_w::GetWasmResponse = icrc::query_one(
        pic,
        sns_wasm,
        "get_wasm",
        sns_w::GetWasmRequest {
            hash: successor.governance_wasm_hash.clone(),
        },
    );
    let fetched = fetched.wasm.unwrap();
    assert_eq!(
        fetched.canister_type,
        sns_w::sns_canister_type_id(sns_w::SnsCanisterType::Governance)
    );
    assert_eq!(
        fetched.wasm,
        artifacts
            .load_required_source_wasm_gz("sns_governance")
            .unwrap()
    );
    assert_eq!(fetched.proposal_id, Some(144202));
    let deployed: sns_w::ListDeployedSnsesResponse =
        icrc::query_one(pic, sns_wasm, "list_deployed_snses", gov::EmptyRecord {});
    assert!(deployed.instances.is_empty());
    // An absent Swap is not this complete blessed tuple. This is only a lookup
    // argument; it is never installed as Governance state or presented as a deployment.
    let mut incomplete = predecessor.clone();
    incomplete.swap_wasm_hash.clear();
    let missing: NextResponse = icrc::query_one(
        pic,
        sns_wasm,
        "get_next_sns_version",
        NextRequest {
            current_version: Some(incomplete),
            governance_canister_id: Some(observer),
        },
    );
    assert_eq!(missing.next_version, None);
    fs::write(evidence.join("discovery.txt"),format!("predecessor={predecessor:#?}\nsuccessor={successor:#?}\nregistry_empty=true\nsuccessor_publication={}\nretrieved_exact_pinned_gzip=true\nproposal_validation_executed=false\nframework_upgrade_executed=false\n",fetched.proposal_id.unwrap())).unwrap();
    println!("unregistered caller discovers exact reviewed predecessor/successor and retrieves official Governance gzip through default SNS-W path; this helper performs no SNS deployment, proposal or upgrade; evidence={}",evidence.display());
    (sns_wasm, predecessor, successor)
}

#[test]
#[ignore = "requires reviewed official SNS-W bundle, parser tool and PocketIC"]
fn real_operator_30_io_proposer_fees() {
    let sale = Sale::new();
    sale.migrate_reserve();
    assert_eq!(
        sale.neuron(sale.operator, OPERATOR_MEMO)
            .cached_neuron_stake_e8s,
        3_000_000_000
    );
    let adopted = proposal_id(sale.motion("Local operator fee release"));
    assert_eq!(
        sale.neuron(sale.operator, OPERATOR_MEMO).neuron_fees_e8s,
        1_000_000_000
    );
    assert_eq!(sale.proposal(adopted).decided_timestamp_seconds, 0);
    sale.jupiter_vote(adopted, 1);
    assert!(sale.proposal(adopted).executed_timestamp_seconds > 0);
    assert_eq!(sale.neuron(sale.operator, OPERATOR_MEMO).neuron_fees_e8s, 0);
    let parameters: gov::NervousSystemParameters = icrc::query_one(
        &sale.pic,
        sale.governance,
        "get_nervous_system_parameters",
        (),
    );
    assert_eq!(parameters.max_number_of_neurons, Some(1_000));
    assert_eq!(parameters.automatically_advance_target_version, Some(false));
    for count in 1..=3 {
        let id = proposal_id(sale.motion(&format!("Local rejected proposal {count}")));
        assert_eq!(
            sale.neuron(sale.operator, OPERATOR_MEMO).neuron_fees_e8s,
            count * 1_000_000_000
        );
        if count == 1 {
            // The proposer vote cannot adopt while the governance-only neuron abstains.
            sale.pic.advance_time(Duration::from_secs(2 * 86_400 + 1));
            for _ in 0..30 {
                sale.pic.tick();
            }
        } else {
            sale.jupiter_vote(id, 2);
        }
        let rejected = sale.proposal(id);
        assert!(rejected.decided_timestamp_seconds > 0);
        assert_eq!(rejected.executed_timestamp_seconds, 0);
        assert_eq!(
            sale.neuron(sale.operator, OPERATOR_MEMO).neuron_fees_e8s,
            count * 1_000_000_000
        );
    }
    let response = sale.motion("Local insufficient effective stake");
    let Some(gov::CommandResponse::Error(error)) = response.command else {
        panic!("{response:?}")
    };
    assert_eq!(error.error_type, 10);
    assert!(error.error_message.contains("enough stake"), "{error:?}");
    fs::write(sale.evidence.join("proposer-fees.txt"), "stake=3000000000\nrejection_cost=1000000000\nadoption_released_fee=true\noperator_alone_rejected=true\nthree_rejections_retained=3000000000\nfourth_submission=insufficient_stake\n").unwrap();
}

#[test]
#[ignore = "requires reviewed official SNS-W bundle, parser tool and PocketIC"]
fn real_acceptance_operator_exact_vote_only_permissions() {
    let sale = Sale::new();
    sale.migrate_reserve();
    let initial = sale.neuron(sale.governance_voter, JUPITER_MEMO);
    assert_eq!(initial.permissions.len(), 1);
    assert_eq!(
        initial.permissions[0].principal,
        Some(sale.governance_voter)
    );
    assert_eq!(initial.permissions[0].permission_type, vec![4]);
    assert!(initial.followees.is_empty());
    assert!(initial.topic_followees.is_none());
    let hard = initial;
    unauthorized(sale.manage(
        sale.buyer,
        gov::compute_neuron_staking_subaccount(sale.governance_voter, JUPITER_MEMO).to_vec(),
        gov::Command::RegisterVote(gov::RegisterVote {
            proposal: Some(gov::ProposalId { id: 1 }),
            vote: 1,
        }),
    ));
    let id = proposal_id(sale.motion("Local Vote-only affirmative vote"));
    sale.jupiter_vote(id, 1);
    assert!(sale.proposal(id).executed_timestamp_seconds > 0);
    unauthorized(
        sale.jupiter_manage(gov::Command::MakeProposal(gov::Proposal {
            title: "Valid local motion".into(),
            summary: "Authorization probe".into(),
            url: String::new(),
            action: Some(gov::Action::Motion(gov::Motion {
                motion_text: "Valid local motion".into(),
            })),
        })),
    );
    unauthorized(sale.jupiter_manage(gov::Command::Split(gov::Split {
        amount_e8s: 200_000_000,
        memo: 42,
    })));
    unauthorized(sale.jupiter_manage(gov::Command::Configure(gov::Configure {
        operation: Some(gov::Operation::StopDissolving(gov::EmptyRecord {})),
    })));
    unauthorized(sale.jupiter_manage(gov::Command::AddNeuronPermissions(
        gov::AddNeuronPermissions {
            principal_id: Some(sale.buyer),
            permissions_to_add: Some(gov::NeuronPermissionList {
                permissions: vec![4],
            }),
        },
    )));
    unauthorized(sale.jupiter_manage(gov::Command::AddNeuronPermissions(
        gov::AddNeuronPermissions {
            principal_id: Some(sale.operator),
            permissions_to_add: Some(gov::NeuronPermissionList {
                permissions: vec![2, 3, 10],
            }),
        },
    )));
    unauthorized(sale.jupiter_manage(gov::Command::RemoveNeuronPermissions(
        gov::RemoveNeuronPermissions {
            principal_id: Some(sale.operator),
            permissions_to_remove: Some(gov::NeuronPermissionList {
                permissions: vec![4],
            }),
        },
    )));
    sale.pic.advance_time(Duration::from_secs(1_296_061));
    for _ in 0..30 {
        sale.pic.tick();
    }
    unauthorized(sale.jupiter_manage(gov::Command::Disburse(gov::Disburse {
        amount: Some(gov::Tokens { e8s: 100_000_000 }),
        to_account: Some(gov::Account {
            owner: Some(sale.buyer),
            subaccount: None,
        }),
    })));
    assert_eq!(
        sale.neuron(sale.governance_voter, JUPITER_MEMO).permissions,
        hard.permissions
    );
    fs::write(sale.evidence.join("vote-only.txt"), "caller=reviewed_operator\nremaining_permissions=Vote\nremaining_principals=1\nManageVotingPermission=never_granted\nvote=succeeded\npropose_split_configure_grant_disburse=NotAuthorized\n").unwrap();
}

fn write_json(path: &Path, value: &Value) {
    fs::write(
        path,
        format!("{}\n", serde_json::to_string_pretty(value).unwrap()),
    )
    .unwrap();
}
fn checkpoint(dir: &Path, mode: &str) -> Value {
    let output = Command::new(workspace().join("target/debug/xtask"))
        .current_dir(workspace())
        .arg("acceptance_checkpoint")
        .arg(mode)
        .arg(dir)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "checkpoint {mode}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result = serde_json::from_slice(&output.stdout).expect("checkpoint JSON");
    write_json(&dir.join(format!("checkpoint-{mode}.json")), &result);
    result
}

impl Sale {
    fn nns_manage(
        &self,
        caller: Principal,
        id: Option<u64>,
        command: io_governance_types::NnsManageNeuronCommandRequest,
    ) -> io_governance_types::NnsProductionManageNeuronResponse {
        use io_governance_types::*;
        icrc::update_one(
            &self.pic,
            Principal::from_text(io_production_wiring::NNS_GOVERNANCE_PRINCIPAL).unwrap(),
            caller,
            "manage_neuron",
            NnsProductionManageNeuronRequest {
                id: None,
                neuron_id_or_subaccount: id
                    .map(|id| NnsNeuronIdOrSubaccount::NeuronId(NnsNeuronIdRecord { id })),
                command: Some(command),
            },
        )
    }
    fn nns_info(&self, id: u64) -> io_governance_types::NnsNeuronInfoRecord {
        let nns = Principal::from_text(io_production_wiring::NNS_GOVERNANCE_PRINCIPAL).unwrap();
        let info: Result<
            io_governance_types::NnsNeuronInfoRecord,
            io_governance_types::NnsGovernanceErrorRecord,
        > = icrc::query_one(&self.pic, nns, "get_neuron_info", id);
        info.unwrap()
    }
    fn prepare_permanent(&self) {
        use io_governance_types::*;
        use sha2::{Digest, Sha256};
        let mut context: Value =
            serde_json::from_slice(&fs::read(self.evidence.join("context.json")).unwrap()).unwrap();
        let manager = Principal::from_text(context["nns_manager"].as_str().unwrap()).unwrap();
        let nns = Principal::from_text(io_production_wiring::NNS_GOVERNANCE_PRINCIPAL).unwrap();
        // Create a real, operator-controlled baseline parent using public calls.
        // It is separate from both launch monetary funding legs.
        self.transfer_icp(
            Principal::anonymous(),
            Account::new(self.operator, None),
            1_000_000_000,
        );
        let stake = gov::compute_neuron_staking_subaccount(self.operator, 71);
        self.transfer_icp(
            self.operator,
            Account::new(nns, Some(Subaccount(stake))),
            300_000_000,
        );
        let claim = self.nns_manage(
            self.operator,
            None,
            NnsManageNeuronCommandRequest::ClaimOrRefresh(NnsClaimOrRefresh {
                by: Some(NnsClaimOrRefreshBy::MemoAndController(
                    NnsClaimOrRefreshNeuronFromAccount {
                        controller: Some(self.operator),
                        memo: 71,
                    },
                )),
            }),
        );
        let Some(NnsManageNeuronResponseCommandRecord::ClaimOrRefresh(claim)) = claim.command
        else {
            panic!("{claim:?}")
        };
        let parent = claim.refreshed_neuron_id.unwrap().id;
        let delay = self.nns_info(parent).dissolve_delay_seconds;
        let started = self.nns_manage(
            self.operator,
            Some(parent),
            NnsManageNeuronCommandRequest::Configure(NnsProductionConfigure {
                operation: Some(NnsProductionConfigureOperation::StartDissolving(
                    EmptyRecord {},
                )),
            }),
        );
        assert!(
            matches!(
                started.command,
                Some(NnsManageNeuronResponseCommandRecord::Configure(_))
            ),
            "{started:?}"
        );
        self.pic.advance_time(Duration::from_secs(delay + 1));
        for _ in 0..20 {
            self.pic.tick();
        }
        let request = NnsDisburseToNeuron {
            dissolve_delay_seconds: 63_115_200,
            kyc_verified: true,
            amount_e8s: 100_010_000,
            new_controller: Some(manager),
            nonce: 72,
        };
        fs::write(
            self.evidence.join("permanent-creation-request.bin"),
            encode_one(&request).unwrap(),
        )
        .unwrap();
        let created = self.nns_manage(
            self.operator,
            Some(parent),
            NnsManageNeuronCommandRequest::DisburseToNeuron(request),
        );
        fs::write(
            self.evidence.join("permanent-creation-response.bin"),
            encode_one(&created).unwrap(),
        )
        .unwrap();
        let Some(NnsManageNeuronResponseCommandRecord::DisburseToNeuron(created)) = created.command
        else {
            panic!("supported permanent creation failed: {created:?}")
        };
        let permanent = created.created_neuron_id.unwrap().id;
        let before = self.nns_info(permanent);
        fs::write(
            self.evidence.join("permanent-info-before.bin"),
            encode_one(&before).unwrap(),
        )
        .unwrap();
        assert_eq!(before.stake_e8s, 100_000_000);
        assert_eq!(before.dissolve_delay_seconds, 63_115_200);
        let mut hash = Sha256::new();
        hash.update([12]);
        hash.update(b"neuron-split");
        hash.update(manager.as_slice());
        hash.update(72u64.to_be_bytes());
        let permanent_sub: [u8; 32] = hash.finalize().into();
        context["local_permanent_neuron_id"] = json!(permanent);
        context["local_permanent_staking_subaccount_hex"] = json!(hex::encode(permanent_sub));
        write_json(&self.evidence.join("context.json"), &context);
    }

    fn capitalize(&self) {
        use io_governance_types::*;
        let context: Value =
            serde_json::from_slice(&fs::read(self.evidence.join("context.json")).unwrap()).unwrap();
        let manager = Principal::from_text(context["nns_manager"].as_str().unwrap()).unwrap();
        let nns = Principal::from_text(io_production_wiring::NNS_GOVERNANCE_PRINCIPAL).unwrap();
        let plan = checkpoint(&self.evidence, "B");
        let b0 = plan["b0_e8s"].as_u64().unwrap();
        let delta = plan["permanent_top_up_e8s"].as_u64().unwrap();
        self.transfer_icp(
            Principal::anonymous(),
            Account::new(self.operator, None),
            b0 + delta + 2_000_000_000,
        );
        let permanent = context["local_permanent_neuron_id"].as_u64().unwrap();
        let permanent_sub: [u8; 32] = hex::decode(
            context["local_permanent_staking_subaccount_hex"]
                .as_str()
                .unwrap(),
        )
        .unwrap()
        .try_into()
        .unwrap();
        let before = self.nns_info(permanent);
        assert_eq!(before.stake_e8s, 100_000_000);
        assert_eq!(before.dissolve_delay_seconds, 63_115_200);
        self.transfer_icp(
            self.operator,
            Account::new(nns, Some(Subaccount(permanent_sub))),
            delta,
        );
        assert_eq!(
            self.nns_info(permanent).stake_e8s,
            before.stake_e8s,
            "top-up has not refreshed cached stake yet"
        );
        let refresh = self.nns_manage(
            self.operator,
            Some(permanent),
            NnsManageNeuronCommandRequest::ClaimOrRefresh(NnsClaimOrRefresh {
                by: Some(NnsClaimOrRefreshBy::NeuronIdOrSubaccount(EmptyRecord {})),
            }),
        );
        assert!(
            matches!(
                refresh.command,
                Some(NnsManageNeuronResponseCommandRecord::ClaimOrRefresh(_))
            ),
            "non-controller refresh failed: {refresh:?}"
        );
        fs::write(
            self.evidence.join("permanent-refresh-response.bin"),
            encode_one(&refresh).unwrap(),
        )
        .unwrap();
        let after = self.nns_info(permanent);
        fs::write(
            self.evidence.join("permanent-info-after.bin"),
            encode_one(&after).unwrap(),
        )
        .unwrap();
        assert_eq!(after.stake_e8s, before.stake_e8s + delta);
        assert_eq!(after.dissolve_delay_seconds, before.dissolve_delay_seconds);
        let forbidden = self.nns_manage(
            self.operator,
            Some(permanent),
            NnsManageNeuronCommandRequest::Configure(NnsProductionConfigure {
                operation: Some(NnsProductionConfigureOperation::IncreaseDissolveDelay(
                    NnsIncreaseDissolveDelay {
                        additional_dissolve_delay_seconds: 1,
                    },
                )),
            }),
        );
        let Some(NnsManageNeuronResponseCommandRecord::Error(error)) = forbidden.command else {
            panic!("non-controller configured permanent: {forbidden:?}")
        };
        assert_eq!(error.error_type, 3, "NNS NotAuthorized required: {error:?}");
        let liquid: [u8; 32] =
            hex::decode("ccdbb6def992deafdfc3a890cc431818631d7b42f94df6b2b3e1270459be8a78")
                .unwrap()
                .try_into()
                .unwrap();
        self.transfer_icp(
            self.operator,
            Account::new(self.stream, Some(Subaccount(liquid))),
            b0,
        );
        let parent_memo = nns_parent::memo(&workspace()).unwrap();
        let anchor = gov::compute_neuron_staking_subaccount(manager, parent_memo);
        assert_ne!(anchor.as_slice(), permanent_sub.as_slice());
        self.transfer_icp(
            self.operator,
            Account::new(nns, Some(Subaccount(anchor))),
            1_000_000_000,
        );
        assert_eq!(
            self.neuron(self.governance_voter, JUPITER_MEMO).permissions,
            vec![gov::NeuronPermission {
                principal: Some(self.governance_voter),
                permission_type: vec![4]
            }]
        );
        let parameters: gov::NervousSystemParameters = icrc::query_one(
            &self.pic,
            self.governance,
            "get_nervous_system_parameters",
            (),
        );
        fs::write(
            self.evidence.join("effective-governance-parameters.bin"),
            encode_one(&parameters).unwrap(),
        )
        .unwrap();
        let p = &parameters;
        assert_eq!(p.automatically_advance_target_version, Some(false));
        let r = p.voting_rewards_parameters.as_ref().unwrap();
        let params = json!({"max_number_of_neurons":p.max_number_of_neurons,"reject_cost_e8s":p.reject_cost_e8s,
            "neuron_minimum_stake_e8s":p.neuron_minimum_stake_e8s,"transaction_fee_e8s":p.transaction_fee_e8s,
            "max_dissolve_delay_seconds":p.max_dissolve_delay_seconds,"neuron_minimum_dissolve_delay_to_vote_seconds":p.neuron_minimum_dissolve_delay_to_vote_seconds,
            "max_dissolve_delay_bonus_percentage":p.max_dissolve_delay_bonus_percentage,"max_age_bonus_percentage":p.max_age_bonus_percentage,
            "max_neuron_age_for_age_bonus":p.max_neuron_age_for_age_bonus,"initial_voting_period_seconds":p.initial_voting_period_seconds,
            "wait_for_quiet_deadline_increase_seconds":p.wait_for_quiet_deadline_increase_seconds,
            "automatically_advance_target_version":p.automatically_advance_target_version,"voting_rewards_parameters":{
                "round_duration_seconds":r.round_duration_seconds,"reward_rate_transition_duration_seconds":r.reward_rate_transition_duration_seconds,
                "initial_reward_rate_basis_points":r.initial_reward_rate_basis_points,"final_reward_rate_basis_points":r.final_reward_rate_basis_points}});
        let balance = |account: Account| -> u64 {
            let b: IcpTokens = icrc::query_one(
                &self.pic,
                self.icp,
                "account_balance",
                IcpBalance {
                    account: account.icp_account_identifier_bytes().to_vec(),
                },
            );
            b.e8s
        };
        let supply: Nat = decode_one(
            &self
                .pic
                .query_call(
                    self.ledger,
                    Principal::anonymous(),
                    "icrc1_total_supply",
                    candid::encode_args(()).unwrap(),
                )
                .unwrap(),
        )
        .unwrap();
        let before_b: Value =
            serde_json::from_slice(&fs::read(self.evidence.join("observation-b.json")).unwrap())
                .unwrap();
        let raw = self
            .pic
            .update_call(
                self.root,
                self.operator,
                "get_sns_canisters_summary",
                encode_one(RootSummaryRequest {
                    update_canister_list: Some(false),
                })
                .unwrap(),
            )
            .unwrap();
        fs::write(
            self.evidence.join("remaining-authority-root-summary.bin"),
            &raw,
        )
        .unwrap();
        let root_summary: RootSummary = decode_one(&raw).unwrap();
        let mut controllers = serde_json::Map::new();
        for role in ["stream", "nns_manager", "historian", "frontend"] {
            let target = Principal::from_text(context[role].as_str().unwrap()).unwrap();
            let status = root_summary
                .dapps
                .iter()
                .find(|d| d.canister_id == Some(target))
                .unwrap()
                .status
                .as_ref()
                .unwrap();
            assert!(
                status.module_hash.is_none(),
                "IO remains uninstalled before C"
            );
            assert!(
                self.pic
                    .canister_status(target, Some(self.operator))
                    .is_err(),
                "operator must not retain IO controller authority"
            );
            assert_eq!(status.settings.controllers, vec![self.root]);
            controllers.insert(
                role.into(),
                json!(status
                    .settings
                    .controllers
                    .iter()
                    .map(|p| p.to_text())
                    .collect::<Vec<_>>()),
            );
        }
        let tip_bytes = self
            .pic
            .query_call(
                self.icp,
                self.operator,
                "query_blocks",
                encode_one(LedgerTipRequest {
                    start: 0,
                    length: 0,
                })
                .unwrap(),
            )
            .unwrap();
        let tip: LedgerTip = decode_one(&tip_bytes).unwrap();
        assert!(tip.chain_length > 0);
        fs::write(self.evidence.join("activation-ledger-tip.bin"), tip_bytes).unwrap();
        self.observe_swap("after");
        write_json(
            &self.evidence.join("observation-c.json"),
            &json!({"schema":"io.acceptance.observation-c.v1",
            "permanent_neuron_id":permanent,"permanent_principal_before_top_up_e8s":before.stake_e8s,
            "permanent_cached_stake_after_refresh_e8s":after.stake_e8s,"permanent_ledger_stake_e8s":balance(Account::new(nns,Some(Subaccount(permanent_sub)))),
            "permanent_staking_owner":nns.to_text(),"permanent_staking_subaccount_hex":hex::encode(permanent_sub),
            "permanent_controller":manager.to_text(),"permanent_controller_evidence":"successful DisburseToNeuron creation request and rejection of operator Configure",
            "permanent_dissolve_delay_seconds":after.dissolve_delay_seconds,"permanent_auto_stake_maturity":false,
            "refresh_caller":self.operator.to_text(),"refresh_succeeded":true,
            "liquid_backing_e8s":balance(Account::new(self.stream,Some(Subaccount(liquid)))),
            "dynamic_anchor_seed_e8s":balance(Account::new(nns,Some(Subaccount(anchor)))),
            "io_total_supply_e8s":u64::try_from(supply.0).unwrap(),
            "io_total_supply_before_funding_e8s":before_b["total_supply_e8s"],"jupiter_activation_block_floor":tip.chain_length,
            "remaining_install_controllers":controllers,"effective_governance_parameters":params}),
        );
    }
}

impl Sale {
    fn install_io(&self, mutations_only: bool) {
        let context: Value =
            serde_json::from_slice(&fs::read(self.evidence.join("context.json")).unwrap()).unwrap();
        let modules: Value =
            serde_json::from_slice(&fs::read(self.evidence.join("io-artifacts.json")).unwrap())
                .unwrap();
        let output = Command::new(workspace().join("target/debug/xtask"))
            .current_dir(workspace())
            .arg("acceptance_constructors")
            .arg(&self.evidence)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "constructors: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        checkpoint(&self.evidence, "C");
        if mutations_only {
            // Excess initial funding must leave every generated monetary
            // constructor unchanged. C compares their exact existing bytes.
            let path = self.evidence.join("observation-c.json");
            let original = fs::read(&path).unwrap();
            let mut excess: Value = serde_json::from_slice(&original).unwrap();
            for seed in [1_000_000_001_u64, 6_000_000_000] {
                excess["dynamic_anchor_seed_e8s"] = json!(seed);
                write_json(&path, &excess);
                checkpoint(&self.evidence, "C");
            }
            fs::write(path, original).unwrap();
            for (pointer,value) in [("/permanent_dissolve_delay_seconds",json!(63_115_199)),
            ("/permanent_cached_stake_after_refresh_e8s",json!(1)),("/permanent_controller",json!(self.operator.to_text())),
            ("/effective_governance_parameters/voting_rewards_parameters/initial_reward_rate_basis_points",json!(1)),
            ("/effective_governance_parameters/max_number_of_neurons",json!(200_000)),("/dynamic_anchor_seed_e8s",json!(999_999_999))] {
            mutation(&self.evidence,"observation-c.json",pointer,value,"C");
        }
            let path = self.evidence.join("constructors/stream.bin");
            let original = fs::read(&path).unwrap();
            fs::write(&path, b"DIDL malformed").unwrap();
            rejects_checkpoint(&self.evidence, "C");
            fs::write(path, original).unwrap();
            let path = PathBuf::from(modules["stream"]["path"].as_str().unwrap());
            let original = fs::read(&path).unwrap();
            fs::write(&path, b"malformed Wasm").unwrap();
            rejects_checkpoint(&self.evidence, "C");
            fs::write(path, original).unwrap();
            checkpoint(&self.evidence, "C");
            return;
        }
        let dapps: Vec<Principal> = ["stream", "nns_manager", "historian", "frontend"]
            .iter()
            .map(|role| Principal::from_text(context[role].as_str().unwrap()).unwrap())
            .collect();
        {
            let chunks: Value = serde_json::from_slice(
                &fs::read(self.evidence.join("preuploaded-chunks.json")).unwrap(),
            )
            .unwrap();
            let mut proposals = Vec::new();
            for role in ["nns_manager", "stream", "historian", "frontend"] {
                let target = Principal::from_text(context[role].as_str().unwrap()).unwrap();
                let fees_before = self.neuron(self.operator, OPERATOR_MEMO).neuron_fees_e8s;
                let id = proposal_id(
                    self.propose(
                        &format!("Local install {role} through Governance and Root"),
                        gov::Action::UpgradeSnsControlledCanister(
                            gov::UpgradeSnsControlledCanister {
                                new_canister_wasm: vec![],
                                mode: Some(1),
                                canister_id: Some(target),
                                canister_upgrade_arg: Some(
                                    fs::read(
                                        self.evidence
                                            .join("constructors")
                                            .join(format!("{role}.bin")),
                                    )
                                    .unwrap(),
                                ),
                                chunked_canister_wasm: Some(gov::ChunkedCanisterWasm {
                                    wasm_module_hash: hex::decode(
                                        modules[role]["raw_wasm_sha256"].as_str().unwrap(),
                                    )
                                    .unwrap(),
                                    store_canister_id: Some(target),
                                    chunk_hashes_list: chunks[role]["chunk_hashes"]
                                        .as_array()
                                        .unwrap()
                                        .iter()
                                        .map(|h| hex::decode(h.as_str().unwrap()).unwrap())
                                        .collect(),
                                }),
                            },
                        ),
                    ),
                );
                let pending = self.proposal(id);
                assert_eq!(pending.decided_timestamp_seconds, 0);
                assert_eq!(pending.executed_timestamp_seconds, 0);
                assert_eq!(
                    self.neuron(self.operator, OPERATOR_MEMO).neuron_fees_e8s,
                    fees_before + 1_000_000_000
                );
                unauthorized(self.manage(
                    self.buyer,
                    gov::compute_neuron_staking_subaccount(self.operator, JUPITER_MEMO).to_vec(),
                    gov::Command::RegisterVote(gov::RegisterVote {
                        proposal: Some(gov::ProposalId { id }),
                        vote: 1,
                    }),
                ));
                fs::write(
                    self.evidence
                        .join(format!("install-{role}-before-vote.bin")),
                    encode_one(pending).unwrap(),
                )
                .unwrap();
                self.jupiter_vote(id, 1);
                assert!(
                    self.proposal(id).executed_timestamp_seconds > 0,
                    "Governance/Root install proposal failed: {:?}",
                    self.proposal(id)
                );
                assert_eq!(
                    self.neuron(self.operator, OPERATOR_MEMO).neuron_fees_e8s,
                    fees_before
                );
                let voter = self.neuron(self.operator, JUPITER_MEMO);
                assert_eq!(voter.permissions.len(), 1);
                assert_eq!(voter.permissions[0].principal, Some(self.operator));
                assert_eq!(voter.permissions[0].permission_type, vec![4]);
                assert!(voter.followees.is_empty());
                assert!(voter.topic_followees.is_none());
                fs::write(
                    self.evidence.join(format!("install-{role}-after-vote.bin")),
                    encode_one(self.proposal(id)).unwrap(),
                )
                .unwrap();
                let expected =
                    hex::decode(modules[role]["raw_wasm_sha256"].as_str().unwrap()).unwrap();
                let mut installed = false;
                for _ in 0..120 {
                    self.pic.advance_time(Duration::from_secs(1));
                    self.pic.tick();
                    let observed: RootSummary = icrc::update_one(
                        &self.pic,
                        self.root,
                        self.operator,
                        "get_sns_canisters_summary",
                        RootSummaryRequest {
                            update_canister_list: Some(false),
                        },
                    );
                    installed = observed
                        .dapps
                        .iter()
                        .find(|d| d.canister_id == Some(target))
                        .and_then(|d| d.status.as_ref())
                        .is_some_and(|s| {
                            s.module_hash.as_ref() == Some(&expected)
                                && s.settings.controllers == vec![self.root]
                        });
                    if installed {
                        break;
                    }
                }
                assert!(installed, "Root did not install the exact raw {role} Wasm");
                proposals.push(json!({"role":role,"proposal_id":id,"mode":"Install","actual_installer":self.root.to_text(),"chunk_store":target.to_text(),"module_hash":hex::encode(expected)}));
            }
            write_json(
                &self.evidence.join("root-install-proposals.json"),
                &json!(proposals),
            );
        }
        let summary: RootSummary = icrc::update_one(
            &self.pic,
            self.root,
            self.operator,
            "get_sns_canisters_summary",
            RootSummaryRequest {
                update_canister_list: Some(false),
            },
        );
        assert_eq!(summary.dapps.len(), dapps.len());
        for dapp in &summary.dapps {
            assert!(dapps.contains(&dapp.canister_id.unwrap()));
            assert_eq!(
                dapp.status.as_ref().unwrap().settings.controllers,
                vec![self.root]
            );
        }
        fs::write(
            self.evidence.join("registered-root-summary.bin"),
            encode_one(&summary).unwrap(),
        )
        .unwrap();
        let frontend = Principal::from_text(context["frontend"].as_str().unwrap()).unwrap();
        let public = Path::new(modules["frontend"]["browser_assets"].as_str().unwrap());
        let checked = frontend_assets::validate(
            public,
            &fs::read(modules["frontend"]["path"].as_str().unwrap()).unwrap(),
            &frontend_assets::acceptance_config(),
        )
        .unwrap();
        let get = |url: &str| -> HttpResponse<'static> {
            let request =
                encode_one(HttpRequest::get(url).with_certificate_version(2).build()).unwrap();
            assert_eq!(
                decode_one::<HttpRequest<'_>>(&request)
                    .unwrap()
                    .certificate_version(),
                Some(2)
            );
            decode_one(
                &self
                    .pic
                    .query_call(frontend, Principal::anonymous(), "http_request", request)
                    .unwrap(),
            )
            .unwrap()
        };
        let html = get("/");
        assert_eq!(html.status_code(), StatusCode::OK);
        assert_eq!(html.body(), fs::read(public.join("index.html")).unwrap());
        let html_text = std::str::from_utf8(html.body()).unwrap();
        let start = html_text.find("/generated/app.").unwrap();
        let end = start + html_text[start..].find(".js").unwrap() + 3;
        let bundle_path = &html_text[start..end];
        assert_eq!(checked["bundle"], bundle_path);
        let bundle = get(bundle_path);
        assert_eq!(bundle.status_code(), StatusCode::OK);
        assert_eq!(
            bundle.body(),
            fs::read(public.join(&bundle_path[1..])).unwrap()
        );
        assert_eq!(
            get("/generated/frontend-bundle.json").status_code(),
            StatusCode::NOT_FOUND
        );
        write_json(
            &self.evidence.join("installed-frontend-assets.json"),
            &checked,
        );
        let manager = dapps[1];
        let mut ready = false;
        for _ in 0..180 {
            self.pic.advance_time(Duration::from_secs(10));
            self.pic.tick();
            let stream: io_stream_manager::Status = decode_one(
                &self
                    .pic
                    .query_call(
                        self.stream,
                        self.operator,
                        "get_status",
                        candid::encode_args(()).unwrap(),
                    )
                    .unwrap(),
            )
            .unwrap();
            let nns: io_nns_neuron_manager::Status = decode_one(
                &self
                    .pic
                    .query_call(
                        manager,
                        self.operator,
                        "get_status",
                        candid::encode_args(()).unwrap(),
                    )
                    .unwrap(),
            )
            .unwrap();
            if stream.lifecycle == io_stream_manager::Lifecycle::Ready
                && nns.lifecycle == io_nns_neuron_manager::Lifecycle::Ready
            {
                ready = true;
                break;
            }
        }
        assert!(ready, "IO did not become autonomously Ready");
        self.autonomous_top_up(manager);
        // Complete outstanding cross-subnet callbacks before any day-sized jump.
        for _ in 0..200 {
            self.pic.tick();
        }
    }
}

impl Sale {
    fn autonomous_top_up(&self, manager: Principal) {
        let stream_status = || -> io_stream_manager::Status {
            decode_one(
                &self
                    .pic
                    .query_call(
                        self.stream,
                        self.operator,
                        "get_status",
                        candid::encode_args(()).unwrap(),
                    )
                    .unwrap(),
            )
            .unwrap()
        };
        let nns_status = || -> io_nns_neuron_manager::Status {
            decode_one(
                &self
                    .pic
                    .query_call(
                        manager,
                        self.operator,
                        "get_status",
                        candid::encode_args(()).unwrap(),
                    )
                    .unwrap(),
            )
            .unwrap()
        };
        let started = self.pic.get_time().as_nanos_since_unix_epoch();
        let mut completed = None;
        // Existing 60-second retry opportunities; no shortened production cadence
        // and no update/keeper calls anywhere inside this observation window.
        for _ in 0..120 {
            self.pic.advance_time(Duration::from_secs(60));
            self.pic.tick();
            let stream = stream_status();
            let nns = nns_status();
            if let Some(checkpoint) = stream.latest_reconciliation_checkpoint.as_ref() {
                if checkpoint.pooled_target_e8s > 0
                    && checkpoint.observed_pooled_e8s == checkpoint.pooled_target_e8s
                    && stream.operation_kind.is_none()
                    && nns.active_operation.is_none()
                    && nns.latest_pooled_target.as_ref().is_some_and(|target| {
                        target.target_e8s == checkpoint.pooled_target_e8s
                            && target.status == io_nns_neuron_manager::PooledTargetStatus::AtTarget
                    })
                {
                    completed = Some(stream);
                    break;
                }
            }
        }
        let structural = completed.unwrap_or_else(|| {
            panic!(
                "timer-only TopUp did not finalize: stream={:?}; nns={:?}",
                stream_status(),
                nns_status()
            )
        });
        let checkpoint = structural
            .latest_reconciliation_checkpoint
            .as_ref()
            .unwrap();
        assert_eq!(checkpoint.active_backing_io_e8s, 3_000_000_000);
        assert_eq!(checkpoint.pooled_backing_e8s, 3_000_000_000);
        assert_eq!(checkpoint.total_claim_backing_e8s, 3_000_000_000);
        assert_eq!(checkpoint.claim_supply_e8s, 3_000_000_000);
        let physical: IcpTokens = icrc::query_one(
            &self.pic,
            self.icp,
            "account_balance",
            IcpBalance {
                account: Account::new(
                    Principal::from_text(io_production_wiring::NNS_GOVERNANCE_PRINCIPAL).unwrap(),
                    Some(Subaccount(gov::compute_neuron_staking_subaccount(
                        manager, 0,
                    ))),
                )
                .icp_account_identifier_bytes()
                .to_vec(),
            },
        );
        assert_eq!(
            u128::from(physical.e8s),
            checkpoint.pooled_backing_e8s + 1_000_000_000 - 10_000,
            "the single initial TopUp fee must consume excluded anchor, not claim backing"
        );
        write_json(
            &self.evidence.join("timer-only-top-up.json"),
            &json!({"started_at_nanos":started,"ended_at_nanos":self.pic.get_time().as_nanos_since_unix_epoch(),"keeper_calls":0,"generation":checkpoint.generation,"pooled_target_e8s":checkpoint.pooled_target_e8s,"pooled_backing_e8s":checkpoint.pooled_backing_e8s,"claim_supply_e8s":checkpoint.claim_supply_e8s,"backing_e8s":checkpoint.total_claim_backing_e8s,"dynamic_physical_principal_e8s":physical.e8s,"anchor_fee_e8s":10_000}),
        );
    }
}

#[test]
#[ignore = "requires the same real acceptance inputs as real_launch_checkpoints_a_b_c"]
fn real_launch_timer_only_top_up_and_reward() {
    let sale = Sale::new();
    sale.migrate_reserve();
    sale.capitalize();
    sale.install_io(false);
    sale.eligible_reward_event("timer-only-reward");
}

#[test]
#[ignore = "fresh owned official sale-free SNS; pinned CLI, current IO Wasms and typed payload tool required"]
fn real_maintained_sale_free_lifecycle() {
    maintained::run();
}

#[derive(CandidType, Deserialize)]
struct RootSummaryRequest {
    update_canister_list: Option<bool>,
}
#[derive(CandidType)]
struct MetricsRequest {
    time_window_seconds: Option<u64>,
}
#[derive(CandidType, Deserialize)]
struct MetricsResponse {
    get_metrics_result: Option<Result<gov::EmptyRecord, gov::GovernanceError>>,
}
#[derive(CandidType, Deserialize)]
struct RootSummary {
    dapps: Vec<RootCanisterSummary>,
    swap: Option<RootCanisterSummary>,
    root: Option<RootCanisterSummary>,
    governance: Option<RootCanisterSummary>,
    ledger: Option<RootCanisterSummary>,
    index: Option<RootCanisterSummary>,
    archives: Vec<RootCanisterSummary>,
}
#[derive(CandidType, Deserialize)]
struct RootCanisterSummary {
    canister_id: Option<Principal>,
    status: Option<RootCanisterStatus>,
}
#[derive(CandidType, Deserialize)]
struct RootCanisterStatus {
    module_hash: Option<Vec<u8>>,
    settings: RootSettings,
}
#[derive(CandidType, Deserialize)]
struct RootSettings {
    controllers: Vec<Principal>,
}

#[test]
#[ignore = "requires current-source IO Wasms, reviewed official SNS-W bundle, parser tool and PocketIC"]
fn real_launch_checkpoints_a_b_c() {
    let sale = Sale::new();
    sale.migrate_reserve();
    // Real unsolicited ICP cannot create accepted participation in Adopted.
    // Positive no-DoS regression: all A/B/C and monetary checks must still pass.
    sale.transfer_icp(sale.operator, Account::new(sale.buyer, None), 100_010_000);
    sale.transfer_icp(sale.buyer, Account::new(sale.swap, None), 100_000_000);
    sale.pic.advance_time(Duration::from_secs(61));
    for _ in 0..5 {
        sale.pic.tick();
    }
    let refresh = sale.pic.update_call(
        sale.swap,
        sale.buyer,
        "refresh_buyer_tokens",
        encode_one(swap::RefreshBuyerTokensRequest {
            buyer: sale.buyer.to_text(),
            confirmation_text: None,
        })
        .unwrap(),
    );
    assert!(refresh.is_err(), "parked Swap accepted a buyer refresh");
    let rejection = format!("{refresh:?}");
    assert!(
        rejection.contains("Participation is possible only") && rejection.contains("Adopted"),
        "unexpected rejection: {rejection}"
    );
    fs::write(
        sale.evidence.join("unsolicited-refresh-rejection.txt"),
        format!("{refresh:?}\n"),
    )
    .unwrap();
    sale.raw_query(
        sale.icp,
        "account_balance",
        encode_one(IcpBalance {
            account: Account::new(sale.swap, None)
                .icp_account_identifier_bytes()
                .to_vec(),
        })
        .unwrap(),
        "swap-icp-after",
    );
    let lifecycle: swap::GetLifecycleResponse =
        icrc::query_one(&sale.pic, sale.swap, "get_lifecycle", gov::EmptyRecord {});
    assert_eq!(lifecycle.lifecycle, Some(5));
    let derived: swap::GetDerivedStateResponse = decode_one(&sale.raw_query(
        sale.swap,
        "get_derived_state",
        encode_one(gov::EmptyRecord {}).unwrap(),
        "unsolicited-derived-state",
    ))
    .unwrap();
    assert_eq!(derived.buyer_total_icp_e8s, Some(0));
    assert_eq!(derived.direct_participant_count, Some(0));
    let buyers: swap::GetBuyersTotalResponse = icrc::update_one(
        &sale.pic,
        sale.swap,
        sale.operator,
        "get_buyers_total",
        gov::EmptyRecord {},
    );
    assert_eq!(buyers.buyers_total, 0);
    sale.observe_swap("after");
    let plan = checkpoint(&sale.evidence, "B");
    assert_eq!(
        plan["swap_default_accounts"]["after"]["quarantined_icp_e8s"],
        100_000_000u64
    );
    assert_eq!(
        plan["swap_default_accounts"]["after"]["claim_bearing_io_e8s"],
        0
    );
    assert_eq!(plan["c0_e8s"], 3_000_000_000u64);
    assert_eq!(plan["b0_e8s"], 3_000_000_000u64);
    assert_eq!(plan["permanent_top_up_e8s"], 2_000_000_000u64);
    sale.capitalize();
    sale.install_io(false);
    sale.redemption_and_reward();
    let final_icp: IcpTokens =
        decode_one(&fs::read(sale.evidence.join("swap-icp-after.bin")).unwrap()).unwrap();
    assert_eq!(final_icp.e8s, 100_000_000);
    write_json(
        &sale.evidence.join("dust-tolerant-launch.json"),
        &json!({"checkpoint_b":plan,"final_quarantined_default_account_icp_e8s":final_icp.e8s,"swap_icp_counted_as_backing_or_funding_e8s":0,"all_swap_subaccounts_inventoried":false}),
    );
    println!("real launch A/B/C evidence: {}", sale.evidence.display());
}

#[derive(CandidType, Deserialize)]
struct GetModeResponse {
    mode: Option<i32>,
}

fn rejects_checkpoint(dir: &Path, mode: &str) {
    let output = Command::new(workspace().join("target/debug/xtask"))
        .current_dir(workspace())
        .arg("acceptance_checkpoint")
        .arg(mode)
        .arg(dir)
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "mutated checkpoint {mode} unexpectedly passed"
    );
}
fn mutation(dir: &Path, file: &str, pointer: &str, replacement: Value, mode: &str) {
    let path = dir.join(file);
    let original = fs::read(&path).unwrap();
    let mut changed: Value = serde_json::from_slice(&original).unwrap();
    *changed.pointer_mut(pointer).unwrap() = replacement;
    write_json(&path, &changed);
    rejects_checkpoint(dir, mode);
    fs::write(path, original).unwrap();
}

#[test]
#[ignore = "requires reviewed official SNS-W bundle, parser tool and PocketIC"]
fn real_checkpoint_c_negative_cases() {
    let sale = Sale::new();
    sale.migrate_reserve();
    sale.capitalize();
    sale.install_io(true);
    println!(
        "real checkpoint C mutations evidence: {}",
        sale.evidence.display()
    );
}

#[test]
#[ignore = "requires reviewed official SNS-W bundle, parser tool and PocketIC"]
fn real_checkpoint_a_b_negative_cases() {
    let sale = Sale::new();
    let d = &sale.evidence;
    assert!(!d.join("observation-b.json").exists());
    assert!(!d.join("observation-c.json").exists());
    // Mutate only private copies of reviewed build inputs; never the shared
    // verified cache. Real byte/DID corruption must fail before payload use.
    let context_path = d.join("context.json");
    let original_context = fs::read(&context_path).unwrap();
    let mut context: Value = serde_json::from_slice(&original_context).unwrap();
    let source = Path::new(context["bundle_path"].as_str().unwrap());
    let copy = d.join("sns-bundle-negative");
    fs::create_dir(&copy).unwrap();
    fs::create_dir(copy.join("wasms")).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_file() {
            fs::copy(entry.path(), copy.join(entry.file_name())).unwrap();
        }
    }
    for entry in fs::read_dir(source.join("wasms")).unwrap() {
        let entry = entry.unwrap();
        fs::copy(entry.path(), copy.join("wasms").join(entry.file_name())).unwrap();
    }
    context["bundle_path"] = json!(copy);
    write_json(&context_path, &context);
    checkpoint(d, "A");
    for relative in ["wasms/sns_governance.wasm", "root.did", "governance.did"] {
        let path = copy.join(relative);
        let original = fs::read(&path).unwrap();
        let mut corrupt = original.clone();
        corrupt.push(b' ');
        fs::write(&path, corrupt).unwrap();
        rejects_checkpoint(d, "A");
        fs::write(path, original).unwrap();
    }
    fs::write(&context_path, original_context).unwrap();
    for (file, pointer, value) in [
        (
            "accounts.json",
            "/io_reserve/owner",
            json!(sale.buyer.to_text()),
        ),
        (
            "accounts.json",
            "/io_reserve/subaccount_hex",
            json!(hex::encode([1u8; 32])),
        ),
        (
            "accounts.json",
            "/liquid_icp/owner",
            json!(sale.buyer.to_text()),
        ),
        (
            "accounts.json",
            "/liquid_icp/subaccount_hex",
            json!(hex::encode([2u8; 32])),
        ),
        (
            "context.json",
            "/governance_voting_principal",
            json!("alk7f-5aaaa-aaaar-qb4ra-cai"),
        ),
        ("context.json", "/nns_manager", json!(sale.stream.to_text())),
        (
            "context.json",
            "/jupiter_source",
            json!("TBD_REQUIRED_SOURCE"),
        ),
        ("accounts.json", "/excluded_from_claims", json!(false)),
        (
            "io-artifacts.json",
            "/stream/raw_wasm_sha256",
            json!("0".repeat(64)),
        ),
        (
            "accounts.json",
            "/jupiter_excluded_from_rewards",
            Value::Null,
        ),
        (
            "authority-plan.json",
            "/initial_framework_installer",
            json!(sale.buyer.to_text()),
        ),
    ] {
        mutation(d, file, pointer, value, "A");
    }
    let yaml = d.join("sns.yaml");
    let original = fs::read(&yaml).unwrap();
    let mut stale = original.clone();
    stale.extend_from_slice(b"\n# stale generated output\n");
    fs::write(&yaml, stale).unwrap();
    rejects_checkpoint(d, "A");
    fs::write(&yaml, original).unwrap();
    let payload = d.join("payloads/governance.bin");
    let original = fs::read(&payload).unwrap();
    fs::write(&payload, b"DIDL malformed").unwrap();
    rejects_checkpoint(d, "A");
    fs::write(payload, original).unwrap();
    checkpoint(d, "A");
    sale.migrate_reserve();
    for (pointer, value) in [
        ("/operator_stake_e8s", json!(3_000_000_001u64)),
        ("/total_supply_e8s", json!(TOTAL)),
        ("/stream_reserve_e8s", json!(0)),
        ("/excluded_reserve_e8s", json!(1)),
        ("/excluded_jupiter_e8s", json!(0)),
    ] {
        mutation(d, "observation-b.json", pointer, value, "B");
    }
    // Mutate typed physical observations and restore the original raw replies.
    let original = fs::read(d.join("swap-lifecycle-after.bin")).unwrap();
    let lifecycle: swap::GetLifecycleResponse = decode_one(&original).unwrap();
    for opened in [true, false] {
        let mut changed = lifecycle.clone();
        if opened {
            changed.lifecycle = Some(2);
        } else {
            changed.decentralization_sale_open_timestamp_seconds = Some(1);
        }
        fs::write(
            d.join("swap-lifecycle-after.bin"),
            encode_one(changed).unwrap(),
        )
        .unwrap();
        rejects_checkpoint(d, "B");
    }
    fs::write(d.join("swap-lifecycle-after.bin"), original).unwrap();
    let original = fs::read(d.join("swap-buyers-after.bin")).unwrap();
    fs::write(
        d.join("swap-buyers-after.bin"),
        encode_one(swap::GetBuyersTotalResponse { buyers_total: 1 }).unwrap(),
    )
    .unwrap();
    rejects_checkpoint(d, "B");
    fs::write(d.join("swap-buyers-after.bin"), original).unwrap();
    for phase in ["before", "after"] {
        let path = d.join(format!("swap-icp-{phase}.bin"));
        let original = fs::read(&path).unwrap();
        fs::write(&path, encode_one(IcpTokens { e8s: 1 }).unwrap()).unwrap();
        let plan = checkpoint(d, "B");
        assert_eq!(
            plan["swap_default_accounts"][phase]["quarantined_icp_e8s"],
            1
        );
        assert_eq!(plan["c0_e8s"], 3_000_000_000u64);
        fs::write(path, original).unwrap();
    }
    // An extra IO balance alongside the unchanged 30 IO operator stake would
    // contradict the independently reconciled 30 IO claim supply.
    let path = d.join("swap-io-after.bin");
    let original = fs::read(&path).unwrap();
    fs::write(&path, encode_one(Nat::from(1u64)).unwrap()).unwrap();
    rejects_checkpoint(d, "B");
    fs::write(path, original).unwrap();
    checkpoint(d, "B");
    for args in [
        vec!["acceptance_checkpoint", "D"],
        vec!["acceptance_preflight", "seald"],
        vec!["acceptance_checkpoint", "A", "extra", "argument"],
    ] {
        assert!(!Command::new(workspace().join("target/debug/xtask"))
            .current_dir(workspace())
            .args(args)
            .output()
            .unwrap()
            .status
            .success());
    }
    println!("real checkpoint mutations evidence: {}", d.display());
}

#[test]
#[ignore = "requires pinned parser; parser component test independent of PocketIC/artifacts"]
fn parsed_acceptance_production_economic_equivalence() {
    let evidence = workspace().join("target").join(format!(
        "acceptance-parser-component-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&evidence).unwrap();
    let parser = PathBuf::from(
        std::env::var_os("IO_ACCEPTANCE_SNS_PAYLOAD_TOOL").expect("build pinned parser first"),
    );
    // Component-only identities and time; these are never installation evidence.
    let mut context = json!({"local_fixture":true,"root_install":true,
        "logo_path":local_fixture_logo(&evidence),"genesis_timestamp_seconds":1_800_000_000u64});
    for (i, role) in [
        "operator",
        "governance_voting_principal",
        "jupiter_source",
        "stream",
        "nns_manager",
        "historian",
        "frontend",
        "governance",
        "root",
        "ledger",
        "index",
        "swap",
    ]
    .iter()
    .enumerate()
    {
        context[*role] = json!(Principal::from_slice(&[200 + i as u8; 29]).to_text());
    }
    let mut projections = Vec::new();
    for environment in ["acceptance", "production"] {
        context["governance_voting_principal"] = if environment == "acceptance" {
            context["operator"].clone()
        } else {
            json!("alk7f-5aaaa-aaaar-qb4ra-cai")
        };
        context["fixture_environment"] = json!(environment);
        let path = evidence.join(format!("{environment}-local-context.json"));
        write_json(&path, &context);
        let yaml = evidence.join(format!("{environment}-local.yaml"));
        assert!(Command::new(workspace().join("target/debug/xtask"))
            .current_dir(workspace())
            .arg("acceptance_local_config")
            .arg(&path)
            .arg(&yaml)
            .status()
            .unwrap()
            .success());
        let output = evidence.join(format!("{environment}-local-payloads"));
        assert!(Command::new(&parser)
            .arg(&yaml)
            .arg(workspace().join("deploy/acceptance/canonical-sns-economics.json"))
            .arg(&path)
            .arg(&output)
            .status()
            .unwrap()
            .success());
        let text = fs::read_to_string(yaml).unwrap();
        let config: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(config["token_name"], "IO");
        assert_eq!(config["token_symbol"], "IO");
        assert_eq!(config["dapp_canisters"].as_array().unwrap().len(), 4);
        assert_eq!(config["parked_swap"]["sns_token_e8s"], 0);
        projections.push(fs::read(output.join("parsed-economics.json")).unwrap());
    }
    assert_eq!(
        projections[0], projections[1],
        "parsed economic parameters differ across environments"
    );
    let original: Value =
        serde_json::from_slice(&fs::read(evidence.join("acceptance-local.yaml")).unwrap()).unwrap();
    for (name, pointer, value) in [
        (
            "operator-stake",
            "/economics/governance_neuron/stake_e8s",
            json!(2_900_000_000u64),
        ),
        (
            "rejection-fee",
            "/economics/proposal_reject_cost_e8s",
            json!(900_000_000u64),
        ),
        (
            "rewards",
            "/economics/initial_reward_rate_basis_points",
            json!(1),
        ),
        (
            "opening-time",
            "/parked_swap/swap_start_timestamp_seconds",
            json!(1_800_000_001u64),
        ),
        (
            "auto-finalization",
            "/parked_swap/should_auto_finalize",
            json!(true),
        ),
        (
            "swap-allocation",
            "/parked_swap/sns_token_e8s",
            json!(300_000_000u64),
        ),
        (
            "proposal-sentinel",
            "/parked_swap/nns_proposal_id",
            json!(123456),
        ),
        (
            "participation-limit",
            "/parked_swap/max_direct_participation_icp_e8s",
            json!(200_000_000u64),
        ),
        (
            "missing-basket",
            "/parked_swap/neuron_basket_construction_parameters",
            Value::Null,
        ),
        ("token-symbol", "/token_symbol", json!("IOLT")),
        (
            "acceptance-voter",
            "/governance_voting_principal",
            json!("alk7f-5aaaa-aaaar-qb4ra-cai"),
        ),
    ] {
        let mut invalid = original.clone();
        *invalid.pointer_mut(pointer).unwrap() = value;
        let yaml = evidence.join(format!("invalid-{name}.yaml"));
        write_json(&yaml, &invalid);
        let result = Command::new(&parser)
            .arg(yaml)
            .arg(workspace().join("deploy/acceptance/canonical-sns-economics.json"))
            .arg(evidence.join("acceptance-local-context.json"))
            .arg(evidence.join(format!("invalid-{name}-payloads")))
            .output()
            .unwrap();
        assert!(
            !result.status.success(),
            "typed initializer accepted {name}"
        );
        fs::write(
            evidence.join(format!("invalid-{name}.stderr")),
            result.stderr,
        )
        .unwrap();
    }
    // A self-consistent human voter must still be refused by the production policy.
    let mut production: Value =
        serde_json::from_slice(&fs::read(evidence.join("production-local.yaml")).unwrap()).unwrap();
    let mut production_context: Value =
        serde_json::from_slice(&fs::read(evidence.join("production-local-context.json")).unwrap())
            .unwrap();
    production["governance_voting_principal"] = production["operator"].clone();
    production_context["governance_voting_principal"] = production_context["operator"].clone();
    let yaml = evidence.join("invalid-production-human-voter.yaml");
    let context = evidence.join("invalid-production-human-voter-context.json");
    write_json(&yaml, &production);
    write_json(&context, &production_context);
    let rejected = Command::new(&parser)
        .arg(yaml)
        .arg(workspace().join("deploy/acceptance/canonical-sns-economics.json"))
        .arg(context)
        .arg(evidence.join("invalid-production-human-voter-payloads"))
        .output()
        .unwrap();
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr)
        .contains("genesis authority differs from context"));
    fs::write(
        evidence.join("invalid-production-human-voter.stderr"),
        rejected.stderr,
    )
    .unwrap();
    println!("pinned-parser component evidence: {}", evidence.display());
}

impl Sale {
    fn stream_status(&self) -> io_stream_manager::Status {
        decode_one(
            &self
                .pic
                .query_call(
                    self.stream,
                    self.operator,
                    "get_status",
                    candid::encode_args(()).unwrap(),
                )
                .unwrap(),
        )
        .unwrap()
    }
    fn record_timer_failure(&self, label: &str) {
        let context: Value =
            serde_json::from_slice(&fs::read(self.evidence.join("context.json")).unwrap()).unwrap();
        write_json(
            &self.evidence.join(format!("{label}-time.json")),
            &json!({"now_nanos":self.pic.get_time().as_nanos_since_unix_epoch()}),
        );
        for role in ["governance", "stream", "nns_manager"] {
            let id = Principal::from_text(context[role].as_str().unwrap()).unwrap();
            fs::write(
                self.evidence.join(format!("{label}-{role}-status.txt")),
                format!("{:#?}\n", self.pic.canister_status(id, Some(self.root))),
            )
            .unwrap();
            fs::write(
                self.evidence.join(format!("{label}-{role}-logs.txt")),
                format!("{:#?}\n", self.pic.fetch_canister_logs(id, self.root)),
            )
            .unwrap();
        }
        for (method, args) in [
            ("get_latest_reward_event", candid::encode_args(()).unwrap()),
            ("get_timers", encode_one(gov::EmptyRecord {}).unwrap()),
        ] {
            match self
                .pic
                .query_call(self.governance, self.operator, method, args)
            {
                Ok(bytes) => {
                    fs::write(self.evidence.join(format!("{label}-{method}.bin")), bytes).unwrap()
                }
                Err(error) => fs::write(
                    self.evidence.join(format!("{label}-{method}-error.txt")),
                    format!("{error:?}\n"),
                )
                .unwrap(),
            }
        }
    }
    fn eligible_reward_event(&self, label: &str) {
        // Re-entry is eligible from the event after full backing is restored.
        // Complete that timer-driven admission before submitting this motion.
        let operator_stake = self
            .neuron(self.operator, OPERATOR_MEMO)
            .cached_neuron_stake_e8s;
        let mut backed = false;
        for attempt in 0..120 {
            let status = self.stream_status();
            if let Some(checkpoint) = status.latest_reconciliation_checkpoint.as_ref() {
                if status.operation_kind.is_none()
                    && checkpoint.active_backing_io_e8s == u128::from(operator_stake)
                    && checkpoint.pooled_target_e8s > 0
                    && checkpoint.observed_pooled_e8s == checkpoint.pooled_target_e8s
                {
                    write_json(
                        &self.evidence.join(format!("{label}-backed-admission.json")),
                        &json!({"keeper_calls":0,"active_backing_io_e8s":checkpoint.active_backing_io_e8s,
                        "pooled_target_e8s":checkpoint.pooled_target_e8s,"observed_pooled_e8s":checkpoint.observed_pooled_e8s,
                        "event_round":status.latest_processed_reward_event.map(|event|event.round)}),
                    );
                    backed = true;
                    break;
                }
            }
            self.pic.advance_time(Duration::from_secs(if attempt == 0 {
                86_400 + 301
            } else {
                60
            }));
            for _ in 0..200 {
                self.pic.tick();
            }
        }
        assert!(
            backed,
            "operator did not regain full backing before the reward proposal"
        );
        let before = self.stream_status();
        let proposal = proposal_id(self.motion("Sale-free launch canonical reward shares"));
        self.jupiter_vote(proposal, 1);
        assert!(self.proposal(proposal).executed_timestamp_seconds > 0);
        let mut observed = None;
        for _ in 0..3 {
            self.pic.advance_time(Duration::from_secs(86_400 + 301));
            for _ in 0..120 {
                for _ in 0..200 {
                    self.pic.tick();
                }
                let status = self.stream_status();
                if status.accumulated_eligible_credit > before.accumulated_eligible_credit
                    && status.operation_kind.is_none()
                    && status.latest_reward_event_classification
                        == Some(
                            io_stream_manager::state::RewardEventClassification::ProposalBearing,
                        )
                {
                    observed = Some(status);
                    break;
                }
                self.pic.advance_time(Duration::from_secs(60));
            }
            if observed.is_some() {
                break;
            }
        }
        let after = observed.unwrap_or_else(|| {
            self.record_timer_failure(label);
            panic!(
                "timer-only eligible proposal event not consumed: {:?}",
                self.stream_status()
            )
        });
        let event = decode_one::<io_governance_types::SnsRewardEvent>(&self.raw_query(
            self.governance,
            "get_latest_reward_event",
            candid::encode_args(()).unwrap(),
            &format!("{label}-governance-event"),
        ))
        .unwrap();
        assert_eq!(event.distributed_e8s_equivalent, 0);
        assert!(event.settled_proposals.iter().any(|p| p.id == proposal));
        self.raw_query(
            self.governance,
            "list_neurons",
            encode_one(gov::ListNeurons {
                of_principal: None,
                limit: 100,
                start_page_at: None,
            })
            .unwrap(),
            &format!("{label}-governance-neurons"),
        );
        let operator = self
            .neuron(self.operator, OPERATOR_MEMO)
            .latest_reward_event_participation
            .unwrap();
        let jupiter = self
            .neuron(self.governance_voter, JUPITER_MEMO)
            .latest_reward_event_participation
            .unwrap();
        assert_eq!(
            operator.reward_event_end_timestamp_seconds,
            event.end_timestamp_seconds
        );
        assert_eq!(
            jupiter.reward_event_end_timestamp_seconds,
            event.end_timestamp_seconds
        );
        assert!(operator.reward_shares.as_ref().unwrap().0 > 0u8.into());
        assert!(jupiter.reward_shares.as_ref().unwrap().0 > 0u8.into());
        assert_eq!(
            after.accumulated_eligible_credit - before.accumulated_eligible_credit,
            io_reward_policy::DAILY_EVENT_CREDIT
        );
        assert_eq!(
            after.accumulated_policy_credit - before.accumulated_policy_credit,
            io_reward_policy::DAILY_EVENT_CREDIT
        );
        let id = gov::compute_neuron_staking_subaccount(self.operator, OPERATOR_MEMO);
        assert!(
            after
                .accumulated_entitlements
                .iter()
                .all(|e| e.sns_neuron_id == id),
            "Jupiter must have no entry or denominator weight"
        );
        assert_eq!(
            after.accumulated_entitlements.len(),
            1,
            "exactly the operator is reward eligible"
        );
        let prior_credit = before
            .accumulated_entitlements
            .iter()
            .find(|e| e.sns_neuron_id == id)
            .map_or(0, |e| e.accumulated_eligible_credit);
        let entitlement = &after.accumulated_entitlements[0];
        assert_eq!(
            entitlement.accumulated_eligible_credit - prior_credit,
            io_reward_policy::DAILY_EVENT_CREDIT
        );
        assert_eq!(entitlement.destination.owner, self.governance);
        assert_eq!(entitlement.destination.subaccount, Some(id.to_vec()));
        assert!(!after.reward_processing_paused);
        write_json(
            &self.evidence.join(format!("{label}.json")),
            &json!({"keeper_calls":0,"proposal_id":proposal,"reward_events_before":before.processed_reward_event_count,"reward_events_after":after.processed_reward_event_count,
            "eligible_credit_delta":after.accumulated_eligible_credit-before.accumulated_eligible_credit,"policy_credit_delta":after.accumulated_policy_credit-before.accumulated_policy_credit,
            "operator_reward_shares":operator.reward_shares.unwrap().to_string(),"jupiter_reward_shares":jupiter.reward_shares.unwrap().to_string(),"jupiter_monetary_excluded":true}),
        );
    }
    fn redemption_and_reward(&self) {
        let context: Value =
            serde_json::from_slice(&fs::read(self.evidence.join("context.json")).unwrap()).unwrap();
        let manager = Principal::from_text(context["nns_manager"].as_str().unwrap()).unwrap();
        let operator_sub = gov::compute_neuron_staking_subaccount(self.operator, OPERATOR_MEMO);
        let started = self.manage(
            self.operator,
            operator_sub.to_vec(),
            gov::Command::Configure(gov::Configure {
                operation: Some(gov::Operation::StartDissolving(gov::EmptyRecord {})),
            }),
        );
        assert!(matches!(
            started.command,
            Some(gov::CommandResponse::Configure(_))
        ));
        // Actual user unlock. Timers alone reconcile the no-longer-active stake
        // and return the real 14-day NNS child principal while this delay elapses.
        let mut remaining = 1_296_061;
        while remaining > 0 {
            let advance = remaining.min(86_400 + 301);
            self.pic.advance_time(Duration::from_secs(advance));
            for _ in 0..200 {
                self.pic.tick();
            }
            remaining -= advance;
        }
        let disbursed = self.manage(
            self.operator,
            operator_sub.to_vec(),
            gov::Command::Disburse(gov::Disburse {
                amount: Some(gov::Tokens { e8s: 200_000_000 }),
                to_account: Some(gov::Account {
                    owner: Some(self.buyer),
                    subaccount: None,
                }),
            }),
        );
        assert!(
            matches!(disbursed.command, Some(gov::CommandResponse::Disburse(_))),
            "{disbursed:?}"
        );
        assert_eq!(self.balance(self.buyer, None), 199_990_000);
        let accounts: Value =
            serde_json::from_slice(&fs::read(self.evidence.join("accounts.json")).unwrap())
                .unwrap();
        let liquid_sub: [u8; 32] =
            hex::decode(accounts["liquid_icp"]["subaccount_hex"].as_str().unwrap())
                .unwrap()
                .try_into()
                .unwrap();
        let icp_balance = |account: Account| -> u64 {
            let b: IcpTokens = icrc::query_one(
                &self.pic,
                self.icp,
                "account_balance",
                IcpBalance {
                    account: account.icp_account_identifier_bytes().to_vec(),
                },
            );
            b.e8s
        };
        let liquid_account = Account::new(self.stream, Some(Subaccount(liquid_sub)));
        let mut stable = None;
        for _ in 0..40 {
            for _ in 0..200 {
                self.pic.tick();
            }
            let stream = self.stream_status();
            let nns: io_nns_neuron_manager::Status =
                icrc::query_one(&self.pic, manager, "get_status", ());
            if let Some(checkpoint) = &stream.latest_reconciliation_checkpoint {
                if icp_balance(liquid_account.clone()) >= 100_000_000
                    && stream.operation_kind.is_none()
                    && nns.active_operation.is_none()
                    && checkpoint.unwinding_backing_e8s == 0
                    && checkpoint.transit_backing_e8s == 0
                {
                    stable = Some(checkpoint.clone());
                    break;
                }
            }
            self.pic.advance_time(Duration::from_secs(86_400 + 301));
        }
        let checkpoint = stable.unwrap_or_else(|| {
            self.record_timer_failure("nns-return");
            panic!(
                "timer-only NNS return did not make redemption liquid: {:?}",
                self.stream_status()
            )
        });
        let io_before = self.balance(self.buyer, None);
        let icp_before = icp_balance(Account::new(self.buyer, None));
        let reserve_sub =
            hex::decode(accounts["io_reserve"]["subaccount_hex"].as_str().unwrap()).unwrap();
        let reserve_before = self.balance(self.stream, Some(reserve_sub.clone()));
        let staging = io_accounts::redemption_staging(self.stream);
        let sent: Result<Nat, io_ledger_types::IcrcTransferError> = icrc::update_one(
            &self.pic,
            self.ledger,
            self.buyer,
            "icrc1_transfer",
            icrc::transfer_arg(
                None,
                IcrcAccount {
                    owner: staging.owner,
                    subaccount: staging.subaccount.clone(),
                },
                50_000_000,
                Some(10_000),
                None,
                None,
            ),
        );
        let block = sent.unwrap();
        let supply: Nat = icrc::query_one(&self.pic, self.ledger, "icrc1_total_supply", ());
        let supply: u128 = supply.0.try_into().unwrap();
        let excluded = accounts["excluded_staking_accounts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| {
                u128::from(self.balance(
                    Principal::from_text(a["owner"].as_str().unwrap()).unwrap(),
                    Some(hex::decode(a["subaccount_hex"].as_str().unwrap()).unwrap()),
                ))
            })
            .sum::<u128>();
        let claims = supply - u128::from(reserve_before) - excluded;
        let liquid = icp_balance(liquid_account);
        let state = io_core_model::EconomicState {
            backing: io_core_model::Backing {
                liquid: u128::from(liquid),
                pooled: checkpoint.pooled_backing_e8s,
                unwinding: 0,
                transit: 0,
            },
            claims,
            active_backing: checkpoint.active_backing_io_e8s,
            active_reward: checkpoint.active_reward_io_e8s,
        };
        let quote = io_core_model::redemption_quote(state, 50_000_000, 10_000).unwrap();
        for _ in 0..180 {
            self.pic.advance_time(Duration::from_secs(60));
            for _ in 0..200 {
                self.pic.tick();
            }
            if icp_balance(Account::new(self.buyer, None)) > icp_before
                && self.stream_status().operation_kind.is_none()
            {
                break;
            }
        }
        let credited = icp_balance(Account::new(self.buyer, None)) - icp_before;
        assert_eq!(
            u128::from(credited),
            quote.net_icp,
            "exact canonical pre-dispatch redemption quote"
        );
        assert_eq!(
            self.balance(self.buyer, None),
            io_before - 50_000_000 - 10_000
        );
        assert_eq!(self.balance(staging.owner, staging.subaccount), 0);
        assert_eq!(
            self.balance(self.stream, Some(reserve_sub)),
            reserve_before + 50_000_000 - 10_000
        );
        let supply_after: Nat = icrc::query_one(&self.pic, self.ledger, "icrc1_total_supply", ());
        assert_eq!(u128::try_from(supply_after.0).unwrap(), supply - 10_000);
        write_json(
            &self.evidence.join("redemption-reward.json"),
            &json!({"keeper_calls":0,"redemption_block":block.to_string(),"redemption_io_e8s":50_000_000,"claim_supply_e8s":claims,"backing_e8s":state.backing.liquid+state.backing.pooled,"gross_icp_e8s":quote.gross_icp,"net_icp_e8s":quote.net_icp,"credited_icp_e8s":credited,"icp_payout_fee_e8s":10_000,"io_ingress_fee_e8s":10_000,"io_reserve_sweep_fee_e8s":10_000,"legitimate_operator_disbursement_e8s":200_000_000}),
        );
        let restored = self.manage(
            self.operator,
            operator_sub.to_vec(),
            gov::Command::Configure(gov::Configure {
                operation: Some(gov::Operation::IncreaseDissolveDelay(
                    gov::IncreaseDissolveDelay {
                        additional_dissolve_delay_seconds: 1_296_060,
                    },
                )),
            }),
        );
        assert!(
            matches!(restored.command, Some(gov::CommandResponse::Configure(_))),
            "{restored:?}"
        );
        self.eligible_reward_event("post-redemption-reward");
        self.observe_swap("after");
        let final_parameters: gov::NervousSystemParameters = decode_one(&self.raw_query(
            self.governance,
            "get_nervous_system_parameters",
            candid::encode_args(()).unwrap(),
            "final-governance-parameters",
        ))
        .unwrap();
        assert_eq!(
            final_parameters.automatically_advance_target_version,
            Some(false)
        );
        assert_eq!(final_parameters.max_number_of_neurons, Some(1000));
        assert!(final_parameters
            .default_followees
            .unwrap()
            .followees
            .is_empty());
        let jupiter = self.neuron(self.governance_voter, JUPITER_MEMO);
        assert_eq!(jupiter.cached_neuron_stake_e8s, 10_000_000_000_000_000);
        assert_eq!(
            self.balance(
                self.governance,
                Some(
                    gov::compute_neuron_staking_subaccount(self.governance_voter, JUPITER_MEMO)
                        .to_vec()
                )
            ),
            10_000_000_000_000_000
        );
        assert_eq!(
            jupiter.permissions,
            vec![gov::NeuronPermission {
                principal: Some(self.governance_voter),
                permission_type: vec![4]
            }]
        );
        assert!(jupiter.followees.is_empty() && jupiter.topic_followees.is_none());
        #[derive(CandidType, Deserialize)]
        struct Running {
            deployed_version: Option<gov::Version>,
            pending_version: Option<gov::EmptyRecord>,
        }
        let running: Running = decode_one(&self.raw_query(
            self.governance,
            "get_running_sns_version",
            encode_one(gov::EmptyRecord {}).unwrap(),
            "final-running-version",
        ))
        .unwrap();
        let manifest = approved_sns_manifest();
        let hash = |role: &str| {
            hex::decode(manifest.source_sha256(&format!("sns_{role}")).unwrap()).unwrap()
        };
        assert_eq!(
            running.deployed_version,
            Some(gov::Version {
                governance_wasm_hash: hash("governance"),
                root_wasm_hash: hash("root"),
                ledger_wasm_hash: hash("ledger"),
                index_wasm_hash: hash("index"),
                swap_wasm_hash: hash("swap"),
                archive_wasm_hash: hash("archive")
            })
        );
        assert!(running.pending_version.is_none());
        let raw = self
            .pic
            .update_call(
                self.root,
                self.operator,
                "get_sns_canisters_summary",
                encode_one(RootSummaryRequest {
                    update_canister_list: Some(false),
                })
                .unwrap(),
            )
            .unwrap();
        let summary: RootSummary = decode_one(&raw).unwrap();
        assert!(summary.archives.is_empty());
        for (role, record, controller) in [
            ("governance", summary.governance, self.root),
            ("root", summary.root, self.governance),
            ("ledger", summary.ledger, self.root),
            ("index", summary.index, self.root),
            ("swap", summary.swap, self.root),
        ] {
            let record = record.unwrap();
            assert_eq!(
                record.canister_id,
                Some(Principal::from_text(context[role].as_str().unwrap()).unwrap())
            );
            let status = record.status.unwrap();
            assert_eq!(status.module_hash, Some(hash(role)));
            assert_eq!(status.settings.controllers, vec![controller]);
        }
        let modules: Value =
            serde_json::from_slice(&fs::read(self.evidence.join("io-artifacts.json")).unwrap())
                .unwrap();
        assert_eq!(summary.dapps.len(), 4);
        for role in ["stream", "nns_manager", "historian", "frontend"] {
            let id = Principal::from_text(context[role].as_str().unwrap()).unwrap();
            let records = summary
                .dapps
                .iter()
                .filter(|d| d.canister_id == Some(id))
                .collect::<Vec<_>>();
            assert_eq!(records.len(), 1);
            let status = records[0].status.as_ref().unwrap();
            assert_eq!(
                status.module_hash,
                Some(hex::decode(modules[role]["raw_wasm_sha256"].as_str().unwrap()).unwrap())
            );
            assert_eq!(status.settings.controllers, vec![self.root]);
        }
        fs::write(self.evidence.join("final-root-summary.bin"), raw).unwrap();
    }
}

#[test]
#[ignore = "real NNS PocketIC public-call authority proof; independent of SNS sale artifacts"]
fn real_nns_existing_id_noncontroller_refresh() {
    use io_governance_types::*;
    let pic = pocketic_env::new_pic_with_nns_governance_features();
    // Query senders are simulated by PocketIC. The observer controls neither
    // the neuron nor its controller canister; no mainnet visibility is inferred.
    let stranger = Principal::from_slice(&[84; 29]);
    let owner = pic.create_canister_on_subnet(None, None, pic.topology().get_app_subnets()[0]);
    let nns = Principal::from_text(io_production_wiring::NNS_GOVERNANCE_PRINCIPAL).unwrap();
    let ledger = Principal::from_text(io_production_wiring::ICP_LEDGER_PRINCIPAL).unwrap();
    let metadata: String = decode_one(
        &pic.query_call(
            nns,
            owner,
            "get_build_metadata",
            candid::encode_args(()).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    println!("PocketIC bootstrap NNS Governance build metadata: {metadata}");
    let sub = gov::compute_neuron_staking_subaccount(owner, 83);
    let fund = |amount| {
        let result: Result<u64, IcpTransferError> = icrc::update_one(
            &pic,
            ledger,
            Principal::anonymous(),
            "transfer",
            IcpTransferArgs {
                memo: 0,
                amount: IcpTokens { e8s: amount },
                fee: IcpTokens { e8s: 10_000 },
                from_subaccount: None,
                to: Account::new(nns, Some(Subaccount(sub)))
                    .icp_account_identifier_bytes()
                    .to_vec(),
                created_at_time: None,
            },
        );
        result.unwrap();
    };
    let manage = |caller, id: Option<u64>, command| -> NnsProductionManageNeuronResponse {
        icrc::update_one(
            &pic,
            nns,
            caller,
            "manage_neuron",
            NnsProductionManageNeuronRequest {
                id: None,
                neuron_id_or_subaccount: id
                    .map(|id| NnsNeuronIdOrSubaccount::NeuronId(NnsNeuronIdRecord { id })),
                command: Some(command),
            },
        )
    };
    fund(200_000_000);
    let claimed = manage(
        owner,
        None,
        NnsManageNeuronCommandRequest::ClaimOrRefresh(NnsClaimOrRefresh {
            by: Some(NnsClaimOrRefreshBy::MemoAndController(
                NnsClaimOrRefreshNeuronFromAccount {
                    memo: 83,
                    controller: Some(owner),
                },
            )),
        }),
    );
    let Some(NnsManageNeuronResponseCommandRecord::ClaimOrRefresh(claimed)) = claimed.command
    else {
        panic!("{claimed:?}")
    };
    let id = claimed.refreshed_neuron_id.unwrap().id;
    let read = || -> NnsNeuronRecord {
        let result: Result<NnsNeuronRecord, NnsGovernanceErrorRecord> = decode_one(
            &pic.query_call(nns, owner, "get_full_neuron", encode_one(id).unwrap())
                .unwrap(),
        )
        .unwrap();
        result.unwrap()
    };
    let before = read();
    assert_eq!(before.controller, Some(owner));
    assert_eq!(before.cached_neuron_stake_e8s, 200_000_000);
    fund(100_000_000);
    assert_eq!(read().cached_neuron_stake_e8s, 200_000_000);
    let refreshed = manage(
        stranger,
        Some(id),
        NnsManageNeuronCommandRequest::ClaimOrRefresh(NnsClaimOrRefresh {
            by: Some(NnsClaimOrRefreshBy::NeuronIdOrSubaccount(EmptyRecord {})),
        }),
    );
    let Some(NnsManageNeuronResponseCommandRecord::ClaimOrRefresh(refreshed)) = refreshed.command
    else {
        panic!("{refreshed:?}")
    };
    assert_eq!(refreshed.refreshed_neuron_id.unwrap().id, id);
    let after = read();
    assert_eq!(after.cached_neuron_stake_e8s, 300_000_000);
    assert_eq!(after.controller, before.controller);
    assert_eq!(after.dissolve_state, before.dissolve_state);
    assert_eq!(after.followees, before.followees);
    assert_eq!(after.hot_keys, before.hot_keys);
    assert_eq!(after.auto_stake_maturity, before.auto_stake_maturity);
    for operation in [
        NnsProductionConfigureOperation::IncreaseDissolveDelay(NnsIncreaseDissolveDelay {
            additional_dissolve_delay_seconds: 1,
        }),
        NnsProductionConfigureOperation::AddHotKey(NnsAddHotKey {
            new_hot_key: Some(stranger),
        }),
        NnsProductionConfigureOperation::ChangeAutoStakeMaturity(NnsChangeAutoStakeMaturity {
            requested_setting_for_auto_stake_maturity: true,
        }),
    ] {
        let denied = manage(
            stranger,
            Some(id),
            NnsManageNeuronCommandRequest::Configure(NnsProductionConfigure {
                operation: Some(operation),
            }),
        );
        let Some(NnsManageNeuronResponseCommandRecord::Error(error)) = denied.command else {
            panic!("{denied:?}")
        };
        assert_eq!(error.error_type, 3, "NNS NotAuthorized required: {error:?}");
    }
    assert_eq!(
        read(),
        after,
        "unauthorized operations changed neuron state"
    );
    let query = |caller| -> Result<NnsNeuronRecord, NnsGovernanceErrorRecord> {
        decode_one(
            &pic.query_call(nns, caller, "get_full_neuron", encode_one(id).unwrap())
                .unwrap(),
        )
        .unwrap()
    };
    let before_reads = read();
    let denied = query(stranger).unwrap_err();
    assert_eq!(
        denied.error_type, 3,
        "unrelated principal must be NotAuthorized"
    );
    assert_eq!(query(owner).unwrap(), before_reads);
    assert_eq!(
        read(),
        before_reads,
        "full-neuron reads changed canonical state"
    );
    assert_eq!(before_reads.id.as_ref().unwrap().id, id);
    assert_eq!(before_reads.account, sub);
    assert_eq!(before_reads.controller, Some(owner));
    assert_eq!(before_reads.cached_neuron_stake_e8s, 300_000_000);
    assert!(before_reads.dissolve_state.is_some());
    assert!(before_reads.hot_keys.is_empty());
    assert_eq!(
        before_reads.maturity_disbursements_in_progress.as_deref(),
        Some(&[][..])
    );
    let public_list = |include_public| -> (Vec<u8>, NnsProductionListNeuronsResponse) {
        let bytes = pic
            .query_call(
                nns,
                stranger,
                "list_neurons",
                encode_one(NnsProductionListNeuronsRequest {
                    neuron_ids: vec![id],
                    include_neurons_readable_by_caller: false,
                    include_empty_neurons_readable_by_caller: Some(false),
                    include_public_neurons_in_full_neurons: Some(include_public),
                    page_number: Some(0),
                    page_size: Some(10),
                    neuron_subaccounts: None,
                })
                .unwrap(),
            )
            .unwrap();
        let reply = decode_one(&bytes).unwrap();
        (bytes, reply)
    };
    assert!(
        public_list(true).1.full_neurons.is_empty(),
        "private neuron must not leak"
    );
    let response = manage(
        owner,
        Some(id),
        NnsManageNeuronCommandRequest::Configure(NnsProductionConfigure {
            operation: Some(NnsProductionConfigureOperation::SetVisibility(
                NnsSetVisibility {
                    visibility: Some(2),
                },
            )),
        }),
    );
    assert!(
        matches!(
            response.command,
            Some(NnsManageNeuronResponseCommandRecord::Configure(_))
        ),
        "{response:?}"
    );
    let public_before = read();
    assert_eq!(public_before.visibility, Some(2));
    assert!(public_before.hot_keys.is_empty());
    assert!(public_list(false).1.full_neurons.is_empty());
    let (before_bytes, before_list) = public_list(true);
    assert_eq!(before_list.full_neurons, vec![public_before.clone()]);
    assert_eq!(public_before.id.as_ref().unwrap().id, id);
    assert_eq!(public_before.controller, Some(owner));
    assert_eq!(public_before.account, sub);
    assert_eq!(query(stranger).unwrap_err().error_type, 3);
    assert_eq!(
        read(),
        public_before,
        "public observations mutated neuron state"
    );
    fund(100_000_000);
    let refresh = manage(
        stranger,
        Some(id),
        NnsManageNeuronCommandRequest::ClaimOrRefresh(NnsClaimOrRefresh {
            by: Some(NnsClaimOrRefreshBy::NeuronIdOrSubaccount(EmptyRecord {})),
        }),
    );
    assert!(matches!(
        refresh.command,
        Some(NnsManageNeuronResponseCommandRecord::ClaimOrRefresh(_))
    ));
    let public_after = read();
    assert_eq!(public_after.visibility, Some(2));
    let (after_bytes, after_list) = public_list(true);
    assert_eq!(after_list.full_neurons, vec![public_after.clone()]);
    assert_eq!(query(stranger).unwrap_err().error_type, 3);
    assert_eq!(
        read(),
        public_after,
        "post-refresh public observation mutated state"
    );
    assert_eq!(
        public_after.cached_neuron_stake_e8s,
        public_before.cached_neuron_stake_e8s + 100_000_000
    );
    assert_eq!(public_after.id, public_before.id);
    assert_eq!(public_after.account, public_before.account);
    assert_eq!(public_after.controller, public_before.controller);
    assert_eq!(public_after.dissolve_state, public_before.dissolve_state);
    assert_eq!(public_after.hot_keys, public_before.hot_keys);
    assert_eq!(public_after.followees, public_before.followees);
    assert_eq!(
        public_after.auto_stake_maturity,
        public_before.auto_stake_maturity
    );
    assert_eq!(
        public_after.maturity_e8s_equivalent,
        public_before.maturity_e8s_equivalent
    );
    assert_eq!(
        public_after.staked_maturity_e8s_equivalent,
        public_before.staked_maturity_e8s_equivalent
    );
    assert_eq!(
        public_after.maturity_disbursements_in_progress,
        public_before.maturity_disbursements_in_progress
    );
    let evidence = workspace()
        .join("target")
        .join(format!("public-neuron-observation-{}", std::process::id()));
    fs::create_dir(&evidence).unwrap();
    fs::write(evidence.join("list-neurons-before.bin"), before_bytes).unwrap();
    fs::write(evidence.join("list-neurons-after.bin"), after_bytes).unwrap();
    fs::write(
        evidence.join("public-neuron-records.txt"),
        format!(
            "caller={}\nneuron_id={id}\nNNS build metadata={metadata}\nbefore={public_before:#?}\nafter={public_after:#?}\n",
            stranger.to_text(),
        ),
    )
    .unwrap();
    println!("public full-neuron observation: unrelated caller, no hotkeys, authentic ListNeuronsResponse before/after 300000000->400000000; strict identity/configuration/maturity comparisons; evidence={}", evidence.display());
    println!("full-neuron authorization: unrelated caller rejected by get_full_neuron even after Public visibility; no hotkeys added; public list replies preserve the canonical typed record; no mainnet permission inferred");
    println!("existing-ID non-controller refresh: 200000000 -> 300000000 e8s; controller, dissolve policy, followees, hot keys and maturity policy unchanged; Configure denied");
}
