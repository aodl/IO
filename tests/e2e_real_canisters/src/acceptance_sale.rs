//! Self-deployed, Fund-disabled sale. No SNS-W/NNS caller impersonation.
#![cfg(test)]

use crate::{artifacts, icrc, pocketic_env, sns_governance_setup as gov, sns_lifecycle as swap};
use candid::{decode_one, encode_one, CandidType, Nat, Principal};
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

const PIN: &str = "fcc281f7200a8d3192ec46c225b436b2096bbf58";
const SALE: u64 = 300_000_000;
const TOTAL: u64 = 100_000_000_000_000_000;
const OPERATOR_MEMO: u64 = 0x494f_474f_5630_3031;
const JUPITER_MEMO: u64 = 0x494f_4a55_5030_3031;
const RESERVE_MEMO: u64 = 0x494f_5245_5330_3031;

// The C boundary performs offline artifact/payload checks without touching PIC.
// Use the existing harness's owned-server pattern with a sufficient idle TTL.
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
            .args(["--ttl", "600", "--hard-ttl", "2400", "--port-file"])
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
    jupiter: Principal,
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
        let bytes = fs::read(source.join(format!("{name}.wasm"))).unwrap();
        let hash = hex::encode(Sha256::digest(&bytes));
        let path = destination.join(format!("{name}.wasm"));
        fs::write(&path, bytes).unwrap();
        let sidecar = format!("{name}.wasm.sha256");
        fs::write(destination.join(&sidecar), format!("{hash}  {name}.wasm\n")).unwrap();
        modules.insert(
            role.into(),
            json!({"path":path,"raw_wasm_sha256":hash,"sha256_sidecar":sidecar}),
        );
    }
    write_json(
        &evidence.join("io-artifacts.json"),
        &Value::Object(modules.clone()),
    );
}

impl Sale {
    fn new() -> Self {
        Self::with_initial_registration(false)
    }

    fn with_initial_registration(root_install: bool) -> Self {
        let artifacts::ArtifactStatus::Ready(artifacts) =
            artifacts::resolve_from_env(true).unwrap()
        else {
            panic!("required artifact set missing")
        };
        assert_eq!(
            artifacts.manifest.value("variant", "scope"),
            Some("same-source")
        );
        for key in [
            "sns_governance",
            "sns_root",
            "sns_ledger",
            "sns_index",
            "sns_swap",
            "sns_archive",
        ] {
            assert_eq!(artifacts.manifest.artifact(key, "source_commit"), Some(PIN));
            artifacts.load_required(key).unwrap();
            artifacts.load_required_source_wasm_gz(key).unwrap();
        }
        let parser = PathBuf::from(
            std::env::var_os("IO_ACCEPTANCE_SNS_PAYLOAD_TOOL")
                .expect("build pinned SNS payload tool first"),
        );
        let (server, pic) = if root_install {
            let (server, url) = AcceptanceServer::start();
            let pic = pocketic_env::new_pic_with_nns_governance_server(&url);
            (Some(server), pic)
        } else {
            (None, pocketic_env::new_pic_with_nns_governance_features())
        };
        let app = pic.topology().get_app_subnets()[0];
        let operator = Principal::from_slice(&[101; 29]);
        let buyer = Principal::from_slice(&[102; 29]);
        let create = || {
            let id = pic.create_canister_on_subnet(Some(operator), None, app);
            pic.add_cycles(id, 20_000_000_000_000);
            id
        };
        let governance = create();
        let root = create();
        let ledger = create();
        let index = create();
        let swap = create();
        let stream = create();
        let nns_manager = create();
        let historian = create();
        let frontend = create();
        let jupiter = create();
        let jupiter_source = create();
        let io = workspace();
        pic.install_canister(
            jupiter,
            fs::read(io.join("target/wasm32-unknown-unknown/debug/mock_nns_candidate_actor.wasm"))
                .expect("build the local caller Wasm first"),
            vec![],
            Some(operator),
        );
        let evidence = io.join("target").join(format!(
            "acceptance-sale-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&evidence).unwrap();
        let start = pic.get_time().as_nanos_since_unix_epoch() / 1_000_000_000 + 60;
        let context = json!({"local_fixture":true,"root_install":root_install, "operator":operator.to_text(), "jupiter":jupiter.to_text(),
            "jupiter_source":jupiter_source.to_text(), "stream":stream.to_text(), "nns_manager":nns_manager.to_text(),
            "historian":historian.to_text(), "frontend":frontend.to_text(), "governance":governance.to_text(),
            "root":root.to_text(), "ledger":ledger.to_text(), "index":index.to_text(), "swap":swap.to_text(),
            "logo_path":local_fixture_logo(&evidence),
            "bundle_path":artifacts.manifest_path.as_ref().unwrap().parent().unwrap(), "parser_tool":parser,
            "nns_proposal_id":0, "swap_start_timestamp_seconds":start,"swap_due_timestamp_seconds":start+86_400});
        let context_path = evidence.join("context.json");
        write_json(&context_path, &context);
        write_json(
            &evidence.join("authority-plan.json"),
            &json!({
            "initial_framework_installer":operator.to_text(), "initial_io_installer":if root_install {root.to_text()} else {operator.to_text()},
            "route":if root_install {"root-install-after-sale"} else {"local-deferred-registration-candidate"},
            "initial_registered_dapps":if root_install {vec![frontend.to_text(),historian.to_text(),nns_manager.to_text(),stream.to_text()]} else {vec![]},
            "io_install_before_controller_handoff":!root_install, "registration_caller":if root_install {operator.to_text()} else {governance.to_text()},
            "registration_method":if root_install {"Root init payload"} else {"Governance/RegisterDappCanisters"},
            "chunk_store":if root_install {Some(governance.to_text())} else {None},
            "final_io_controller":root.to_text(), "deployment_parameters_approved":false}),
        );
        assert!(Command::new(io.join("target/debug/xtask"))
            .current_dir(&io)
            .arg("acceptance_local_accounts")
            .arg(&context_path)
            .arg(evidence.join("accounts.json"))
            .status()
            .unwrap()
            .success());
        let yaml = evidence.join("sns.yaml");
        assert!(Command::new(io.join("target/debug/xtask"))
            .current_dir(&io)
            .arg("acceptance_local_config")
            .arg(&context_path)
            .arg(&yaml)
            .status()
            .unwrap()
            .success());
        let payloads = evidence.join("payloads");
        assert!(
            Command::new(parser)
                .arg(&yaml)
                .arg(io.join("deploy/acceptance/canonical-sns-economics.json"))
                .arg(&context_path)
                .arg(&payloads)
                .status()
                .unwrap()
                .success(),
            "pinned parser/builder rejected generated config"
        );
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
            let status = pic.canister_status(id, Some(operator)).unwrap();
            baseline.insert(name.into(),json!({"canister_id":id.to_text(),"subnet_id":pic.get_subnet(id).unwrap().to_text(),"controllers":status.settings.controllers.iter().map(|p|p.to_text()).collect::<Vec<_>>(),
                "module_hash":status.module_hash.map(hex::encode)}));
        }
        write_json(
            &evidence.join("baseline-canisters.json"),
            &Value::Object(baseline),
        );
        prepare_io_artifacts(&evidence);
        checkpoint(&evidence, "A");
        for (id, role) in [
            (governance, "governance"),
            (root, "root"),
            (ledger, "ledger"),
            (index, "index"),
            (swap, "swap"),
        ] {
            pic.install_canister(
                id,
                artifacts.load_required(&format!("sns_{role}")).unwrap(),
                fs::read(payloads.join(format!("{role}.bin"))).unwrap(),
                Some(operator),
            );
            assert_eq!(pic.get_subnet(id), Some(app));
            assert_eq!(
                pic.canister_status(id, Some(operator)).unwrap().module_hash,
                Some(
                    hex::decode(
                        artifacts
                            .manifest
                            .require_hash(&format!("sns_{role}"))
                            .unwrap()
                    )
                    .unwrap()
                ),
                "installed raw SNS module hash mismatch"
            );
        }
        if root_install {
            // Governance can inspect its own chunk store; Root is its controller
            // and can consume those chunks. All targets share this app subnet.
            let modules: Value =
                serde_json::from_slice(&fs::read(evidence.join("io-artifacts.json")).unwrap())
                    .unwrap();
            let mut chunks = serde_json::Map::new();
            for role in ["stream", "nns_manager", "historian", "frontend"] {
                let bytes = fs::read(modules[role]["path"].as_str().unwrap()).unwrap();
                let hashes: Vec<String> = bytes
                    .chunks(1_000_000)
                    .map(|chunk| {
                        hex::encode(
                            pic.upload_chunk(governance, Some(operator), chunk.to_vec())
                                .unwrap(),
                        )
                    })
                    .collect();
                chunks.insert(
                    role.into(),
                    json!({"store_canister_id":governance.to_text(),
                    "raw_wasm_sha256":modules[role]["raw_wasm_sha256"],"chunk_hashes":hashes}),
                );
            }
            write_json(
                &evidence.join("preuploaded-chunks.json"),
                &Value::Object(chunks),
            );
            for target in [stream, nns_manager, historian, frontend] {
                pic.set_controllers(target, Some(operator), vec![root])
                    .unwrap();
            }
        }
        for (id, controllers) in [
            (governance, vec![root]),
            (root, vec![governance]),
            (ledger, vec![root]),
            (index, vec![root]),
            (swap, vec![root]),
        ] {
            pic.set_controllers(id, Some(operator), controllers)
                .unwrap();
        }
        let icp = Principal::from_text(io_production_wiring::ICP_LEDGER_PRINCIPAL).unwrap();
        Self {
            pic,
            operator,
            buyer,
            jupiter,
            governance,
            ledger,
            root,
            swap,
            stream,
            icp,
            evidence,
            _server: server,
        }
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

    fn finish(&self) {
        self.pic.advance_time(Duration::from_secs(61));
        for _ in 0..20 {
            self.pic.tick();
        }
        let state: swap::GetLifecycleResponse =
            icrc::query_one(&self.pic, self.swap, "get_lifecycle", swap::EmptyRecord {});
        assert_eq!(state.lifecycle, Some(2));
        // PocketIC's local genesis Account provides test ICP. The buyer itself
        // makes the contribution; no privileged Governance/SNS-W call is used.
        self.transfer_icp(
            Principal::anonymous(),
            Account::new(self.buyer, None),
            SALE + 10_000,
        );
        let ticket: swap::NewSaleTicketResponse = icrc::update_one(
            &self.pic,
            self.swap,
            self.buyer,
            "new_sale_ticket",
            swap::NewSaleTicketRequest {
                subaccount: None,
                amount_icp_e8s: SALE,
            },
        );
        assert!(
            matches!(ticket.result, Some(swap::NewSaleTicketResult::Ok(_))),
            "{ticket:?}"
        );
        let mut sub = [0; 32];
        sub[0] = self.buyer.as_slice().len() as u8;
        sub[1..1 + self.buyer.as_slice().len()].copy_from_slice(self.buyer.as_slice());
        self.transfer_icp(
            self.buyer,
            Account::new(self.swap, Some(Subaccount(sub))),
            SALE,
        );
        let refresh: swap::RefreshBuyerTokensResponse = icrc::update_one(&self.pic, self.swap, self.buyer, "refresh_buyer_tokens", swap::RefreshBuyerTokensRequest {
            buyer: self.buyer.to_text(), confirmation_text: Some("IO local-fixture-not-for-deployment sale at a gross reference price of 1 ICP per IO.".into()),
        });
        assert_eq!(refresh.icp_accepted_participation_e8s, SALE);
        // Swap commits in its periodic task. Follow the existing lifecycle
        // harness with simulated-time progress and observe the actual transition.
        let mut committed = false;
        for _ in 0..300 {
            self.pic.tick();
            let lifecycle: swap::GetLifecycleResponse =
                icrc::query_one(&self.pic, self.swap, "get_lifecycle", swap::EmptyRecord {});
            if lifecycle.lifecycle == Some(3) {
                committed = true;
                break;
            }
            assert_ne!(lifecycle.lifecycle, Some(4), "sale aborted: {lifecycle:?}");
            self.pic.advance_time(Duration::from_secs(30));
        }
        assert!(committed, "funded Swap did not commit");
        let raw = self
            .pic
            .update_call(
                self.swap,
                self.buyer,
                "finalize_swap",
                encode_one(swap::EmptyRecord {}).unwrap(),
            )
            .unwrap();
        let response: swap::FinalizeSwapResponse = decode_one(&raw).unwrap();
        fs::write(self.evidence.join("finalization.bin"), &raw).unwrap();
        fs::write(
            self.evidence.join("finalization.txt"),
            format!("{response:#?}\n"),
        )
        .unwrap();
        assert!(response.error_message.is_none(), "{response:#?}");
        let fund: FundEnvelope = decode_one(&raw).unwrap();
        let Some(FundPossibility::Ok(fund)) = fund
            .settle_neurons_fund_participation_result
            .and_then(|r| r.possibility)
        else {
            panic!("Fund-disabled settlement did not succeed: {response:#?}")
        };
        assert_eq!(fund.neurons_fund_participation_icp_e8s, Some(0));
        assert_eq!(fund.neurons_fund_neurons_count, Some(0));
        fs::write(self.evidence.join("finalization.bin"), raw).unwrap();
        fs::write(
            self.evidence.join("finalization.txt"),
            format!("{response:#?}\n"),
        )
        .unwrap();
        assert!(response.error_message.is_none(), "{response:#?}");
        for result in [
            &response.sweep_icp_result,
            &response.create_sns_neuron_recipes_result,
            &response.sweep_sns_result,
            &response.claim_neuron_result,
        ] {
            let result = result.as_ref().expect("missing finalization step");
            assert_eq!(
                result.failure + result.invalid + result.global_failures,
                0,
                "{response:#?}"
            );
            assert!(result.success + result.skipped > 0, "{response:#?}");
        }
        assert!(matches!(
            response
                .set_mode_call_result
                .as_ref()
                .and_then(|r| r.possibility.as_ref()),
            Some(swap::SetModePossibility::Ok(_))
        ));
        let Some(swap::SetDappControllersPossibility::Ok(root_result)) = response
            .set_dapp_controllers_call_result
            .as_ref()
            .and_then(|r| r.possibility.as_ref())
        else {
            panic!("Root finalization failed: {response:?}")
        };
        assert!(root_result.failed_updates.is_empty());
        let mode: GetModeResponse =
            icrc::query_one(&self.pic, self.governance, "get_mode", swap::EmptyRecord {});
        assert_eq!(mode.mode, Some(1));
        fs::write(
            self.evidence.join("governance-mode.bin"),
            encode_one(&mode).unwrap(),
        )
        .unwrap();
        let buyer: swap::GetBuyerStateResponse = icrc::query_one(
            &self.pic,
            self.swap,
            "get_buyer_state",
            swap::GetBuyerStateRequest {
                principal_id: Some(self.buyer),
            },
        );
        let swept = buyer.buyer_state.unwrap().icp.unwrap();
        assert_eq!(swept.amount_e8s, SALE);
        assert_eq!(swept.amount_transferred_e8s, Some(SALE - 10_000));
        assert_eq!(swept.transfer_fee_paid_e8s, Some(10_000));
        assert!(swept.transfer_success_timestamp_seconds > 0);
        let counts = |result: &Option<swap::SweepResult>| {
            let r = result.as_ref().unwrap();
            json!({"failure":r.failure,"invalid":r.invalid,"global_failures":r.global_failures,"success":r.success,"skipped":r.skipped})
        };
        write_json(
            &self.evidence.join("finalization.json"),
            &json!({
            "sweep_icp":counts(&response.sweep_icp_result), "create_recipes":counts(&response.create_sns_neuron_recipes_result),
            "sweep_sns":counts(&response.sweep_sns_result), "claim_neurons":counts(&response.claim_neuron_result),
            "governance_normal":mode.mode==Some(1), "root_controller_changes_successful":root_result.failed_updates.is_empty(),
            "fund_settlement_external_call_required":false}),
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
        let jupiter = gov::compute_neuron_staking_subaccount(self.jupiter, JUPITER_MEMO);
        let reserve_balance = self.balance(self.stream, Some(destination));
        let excluded_reserve = self.balance(self.governance, Some(reserve.to_vec()));
        let excluded_jupiter = self.balance(self.governance, Some(jupiter.to_vec()));
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
        let supply: u64 = supply.0.try_into().unwrap();
        let neurons = self.neurons();
        let participants: Vec<_> = neurons
            .iter()
            .filter(|n| {
                n.permissions
                    .iter()
                    .any(|p| p.principal == Some(self.buyer))
            })
            .collect();
        assert_eq!(participants.len(), 2);
        let participant_balances:Vec<_>=participants.iter().map(|n| {
            let id=n.id.as_ref().unwrap().id.clone();let balance=self.balance(self.governance,Some(id.clone()));
            assert_eq!(balance,n.cached_neuron_stake_e8s,"claimed participant stake must match its exact Ledger Account");
            json!({"owner":self.governance.to_text(),"subaccount_hex":hex::encode(id),"balance_e8s":balance})
        }).collect();
        write_json(
            &self.evidence.join("participant-accounts.json"),
            &json!(participant_balances),
        );
        let participant_stake: u64 = participant_balances
            .iter()
            .map(|a| a["balance_e8s"].as_u64().unwrap())
            .sum();
        let operator_stake = self.balance(
            self.governance,
            Some(gov::compute_neuron_staking_subaccount(self.operator, OPERATOR_MEMO).to_vec()),
        );
        assert_eq!(operator_stake, 3_000_000_000);
        let residual = self.balance(self.swap, None);
        assert_eq!(participant_stake + 2 * 10_000 + residual, SALE);
        assert_eq!(supply, TOTAL - 3 * 10_000);
        assert_eq!(excluded_jupiter, 10_000_000_000_000_000);
        assert_eq!(excluded_reserve, 0);
        let c0 = supply
            .checked_sub(reserve_balance)
            .unwrap()
            .checked_sub(excluded_jupiter)
            .unwrap();
        assert_eq!(c0, 3_000_000_000 + participant_stake + residual);
        let treasury: IcpTokens = icrc::query_one(
            &self.pic,
            self.icp,
            "account_balance",
            IcpBalance {
                account: Account::new(self.governance, None)
                    .icp_account_identifier_bytes()
                    .to_vec(),
            },
        );
        assert_eq!(treasury.e8s, SALE - 10_000);
        let finalization: Value =
            serde_json::from_slice(&fs::read(self.evidence.join("finalization.json")).unwrap())
                .unwrap();
        write_json(
            &self.evidence.join("observation-b.json"),
            &json!({
            "schema":"io.acceptance.observation-b.v1", "finalization":finalization,
            "accepted_direct_icp_e8s":SALE, "accepted_fund_icp_e8s":0, "operator_stake_e8s":operator_stake,
            "participant_stake_e8s":participant_stake, "distribution_fees_e8s":20_000,
            "swap_residual_e8s":residual, "treasury_icp_e8s":treasury.e8s,"icp_sweep_fees_e8s":10_000,
            "total_supply_e8s":supply,"stream_reserve_e8s":reserve_balance,
            "excluded_reserve_e8s":excluded_reserve,"excluded_jupiter_e8s":excluded_jupiter,
            "reserve_disbursement_fee_e8s":10_000}),
        );
        checkpoint(&self.evidence, "B");
        fs::write(self.evidence.join("sale-economics.txt"), format!("total_supply_e8s={supply}\nstream_reserve_e8s={reserve_balance}\nexcluded_reserve_e8s={excluded_reserve}\nexcluded_jupiter_e8s={excluded_jupiter}\nparticipant_stake_e8s={participant_stake}\nswap_residual_e8s={residual}\ntreasury_icp_e8s={}\nc0_e8s={c0}\nb0_e8s={c0}\npermanent_top_up_e8s={}\n", treasury.e8s, (u128::from(c0)*2).div_ceil(3))).unwrap();
    }
}
#[derive(CandidType, Deserialize)]
struct IcpBalance {
    account: Vec<u8>,
}

#[test]
#[ignore = "requires pinned same-source SNS bundle, parser tool and PocketIC"]
fn self_deployed_sale_finalizes_and_migrates_reserve() {
    let sale = Sale::new();
    sale.finish();
    sale.migrate_reserve();
    println!(
        "real self-deployed sale evidence: {}",
        sale.evidence.display()
    );
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
        let request = gov::ManageNeuron {
            subaccount: gov::compute_neuron_staking_subaccount(self.jupiter, JUPITER_MEMO).to_vec(),
            command: Some(command),
        };
        let result: Result<Vec<u8>, String> = decode_one(
            &self
                .pic
                .update_call(
                    self.jupiter,
                    self.operator,
                    "call_governance",
                    candid::encode_args((self.governance, encode_one(request).unwrap())).unwrap(),
                )
                .unwrap(),
        )
        .unwrap();
        decode_one(&result.unwrap()).unwrap()
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
#[ignore = "requires pinned same-source SNS bundle, parser tool and PocketIC"]
fn real_operator_30_io_proposer_fees() {
    let sale = Sale::new();
    sale.finish();
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
    // Standard capacity update before IO install, never a fixture-only override.
    let capacity = proposal_id(sale.propose(
        "Local IO canonical neuron capacity",
        gov::Action::ManageNervousSystemParameters(Box::new(gov::NervousSystemParameters {
            max_number_of_neurons: Some(1_000),
            ..Default::default()
        })),
    ));
    sale.jupiter_vote(capacity, 1);
    assert!(sale.proposal(capacity).executed_timestamp_seconds > 0);
    let parameters: gov::NervousSystemParameters = icrc::query_one(
        &sale.pic,
        sale.governance,
        "get_nervous_system_parameters",
        (),
    );
    assert_eq!(parameters.max_number_of_neurons, Some(1_000));
    for count in 1..=3 {
        let id = proposal_id(sale.motion(&format!("Local rejected proposal {count}")));
        assert_eq!(
            sale.neuron(sale.operator, OPERATOR_MEMO).neuron_fees_e8s,
            count * 1_000_000_000
        );
        if count == 1 {
            // The operator cannot adopt alone while Jupiter abstains.
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
#[ignore = "requires pinned same-source SNS bundle, parser tool and PocketIC"]
fn real_jupiter_canister_exact_vote_only_permissions() {
    let sale = Sale::new();
    sale.finish();
    sale.migrate_reserve();
    let initial = sale.neuron(sale.jupiter, JUPITER_MEMO);
    assert_eq!(initial.permissions.len(), 1);
    assert_eq!(initial.permissions[0].principal, Some(sale.jupiter));
    assert!(initial.permissions[0].permission_type.contains(&10));
    // Arrange a valid later disbursement case before irreversible hardening.
    let started = sale.jupiter_manage(gov::Command::Configure(gov::Configure {
        operation: Some(gov::Operation::StartDissolving(gov::EmptyRecord {})),
    }));
    assert!(matches!(
        started.command,
        Some(gov::CommandResponse::Configure(_))
    ));
    let remove = initial.permissions[0]
        .permission_type
        .iter()
        .copied()
        .filter(|p| *p != 4)
        .collect();
    let response = sale.jupiter_manage(gov::Command::RemoveNeuronPermissions(
        gov::RemoveNeuronPermissions {
            principal_id: Some(sale.jupiter),
            permissions_to_remove: Some(gov::NeuronPermissionList {
                permissions: remove,
            }),
        },
    ));
    assert!(
        matches!(
            response.command,
            Some(gov::CommandResponse::RemoveNeuronPermission(_))
        ),
        "{response:?}"
    );
    let hard = sale.neuron(sale.jupiter, JUPITER_MEMO);
    assert_eq!(
        hard.permissions,
        vec![gov::NeuronPermission {
            principal: Some(sale.jupiter),
            permission_type: vec![4]
        }]
    );
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
        sale.neuron(sale.jupiter, JUPITER_MEMO).permissions,
        hard.permissions
    );
    fs::write(sale.evidence.join("vote-only.txt"), "caller=local_canister\nremaining_permissions=Vote\nremaining_principals=1\nManageVotingPermission=removed\nvote=succeeded\npropose_split_configure_grant_disburse=NotAuthorized\n").unwrap();
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
    fn capitalize(&self) {
        use io_governance_types::*;
        use sha2::{Digest, Sha256};
        let context: Value =
            serde_json::from_slice(&fs::read(self.evidence.join("context.json")).unwrap()).unwrap();
        let manager = Principal::from_text(context["nns_manager"].as_str().unwrap()).unwrap();
        let nns = Principal::from_text(io_production_wiring::NNS_GOVERNANCE_PRINCIPAL).unwrap();
        let plan = checkpoint(&self.evidence, "B");
        let b0 = plan["b0_e8s"].as_u64().unwrap();
        let delta = plan["permanent_top_up_e8s"].as_u64().unwrap();
        // Create a real, operator-controlled baseline parent using public calls.
        // It is separate from sale proceeds and from both monetary funding legs.
        self.transfer_icp(
            Principal::anonymous(),
            Account::new(self.operator, None),
            b0 + delta + 2_000_000_000,
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
        let anchor = gov::compute_neuron_staking_subaccount(manager, 0);
        self.transfer_icp(
            self.operator,
            Account::new(nns, Some(Subaccount(anchor))),
            1_000_000_000,
        );
        let genesis_permissions = self.neuron(self.jupiter, JUPITER_MEMO).permissions;
        assert_eq!(genesis_permissions.len(), 1);
        let hardened = self.jupiter_manage(gov::Command::RemoveNeuronPermissions(
            gov::RemoveNeuronPermissions {
                principal_id: Some(self.jupiter),
                permissions_to_remove: Some(gov::NeuronPermissionList {
                    permissions: genesis_permissions[0]
                        .permission_type
                        .iter()
                        .copied()
                        .filter(|p| *p != 4)
                        .collect(),
                }),
            },
        ));
        assert!(
            matches!(
                hardened.command,
                Some(gov::CommandResponse::RemoveNeuronPermission(_))
            ),
            "{hardened:?}"
        );
        assert_eq!(
            self.neuron(self.jupiter, JUPITER_MEMO).permissions,
            vec![gov::NeuronPermission {
                principal: Some(self.jupiter),
                permission_type: vec![4]
            }]
        );
        // Required capacity is applied through a standard, actually adopted proposal.
        let required: gov::NervousSystemParameters = decode_one(
            &fs::read(
                self.evidence
                    .join("payloads/required-governance-parameters.bin"),
            )
            .unwrap(),
        )
        .unwrap();
        let id = proposal_id(self.propose(
            "Local canonical Governance capacity",
            gov::Action::ManageNervousSystemParameters(Box::new(required)),
        ));
        self.jupiter_vote(id, 1);
        assert!(self.proposal(id).executed_timestamp_seconds > 0);
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
        let r = p.voting_rewards_parameters.as_ref().unwrap();
        let params = json!({"max_number_of_neurons":p.max_number_of_neurons,"reject_cost_e8s":p.reject_cost_e8s,
            "neuron_minimum_stake_e8s":p.neuron_minimum_stake_e8s,"transaction_fee_e8s":p.transaction_fee_e8s,
            "max_dissolve_delay_seconds":p.max_dissolve_delay_seconds,"neuron_minimum_dissolve_delay_to_vote_seconds":p.neuron_minimum_dissolve_delay_to_vote_seconds,
            "max_dissolve_delay_bonus_percentage":p.max_dissolve_delay_bonus_percentage,"max_age_bonus_percentage":p.max_age_bonus_percentage,
            "max_neuron_age_for_age_bonus":p.max_neuron_age_for_age_bonus,"initial_voting_period_seconds":p.initial_voting_period_seconds,
            "wait_for_quiet_deadline_increase_seconds":p.wait_for_quiet_deadline_increase_seconds,"voting_rewards_parameters":{
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
        let root_summary: Option<RootSummary> = if context["root_install"] == true {
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
            Some(decode_one(&raw).unwrap())
        } else {
            None
        };
        let mut controllers = serde_json::Map::new();
        for role in ["stream", "nns_manager", "historian", "frontend"] {
            let target = Principal::from_text(context[role].as_str().unwrap()).unwrap();
            let actual_controllers = if let Some(summary) = &root_summary {
                let status = summary
                    .dapps
                    .iter()
                    .find(|d| d.canister_id == Some(target))
                    .unwrap()
                    .status
                    .as_ref()
                    .unwrap();
                assert!(
                    status.module_hash.is_none(),
                    "IO must remain uninstalled before C"
                );
                assert!(
                    self.pic
                        .canister_status(target, Some(self.operator))
                        .is_err(),
                    "operator must not retain IO controller authority"
                );
                status.settings.controllers.clone()
            } else {
                self.pic
                    .canister_status(target, Some(self.operator))
                    .unwrap()
                    .settings
                    .controllers
            };
            controllers.insert(
                role.into(),
                json!(actual_controllers
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
            "sale_treasury_icp_e8s":balance(Account::new(self.governance,None)),"io_total_supply_e8s":u64::try_from(supply.0).unwrap(),
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
            for (pointer,value) in [("/permanent_dissolve_delay_seconds",json!(63_115_199)),
            ("/permanent_cached_stake_after_refresh_e8s",json!(1)),("/permanent_controller",json!(self.operator.to_text())),
            ("/effective_governance_parameters/voting_rewards_parameters/initial_reward_rate_basis_points",json!(1)),
            ("/effective_governance_parameters/max_number_of_neurons",json!(200_000)),("/dynamic_anchor_seed_e8s",json!(0))] {
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
        if context["root_install"] == true {
            let chunks: Value = serde_json::from_slice(
                &fs::read(self.evidence.join("preuploaded-chunks.json")).unwrap(),
            )
            .unwrap();
            let mut proposals = Vec::new();
            for role in ["nns_manager", "stream", "historian", "frontend"] {
                let target = Principal::from_text(context[role].as_str().unwrap()).unwrap();
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
                                    store_canister_id: Some(self.governance),
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
                self.jupiter_vote(id, 1);
                assert!(
                    self.proposal(id).executed_timestamp_seconds > 0,
                    "Governance/Root install proposal failed: {:?}",
                    self.proposal(id)
                );
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
                proposals.push(json!({"role":role,"proposal_id":id,"mode":"Install","actual_installer":self.root.to_text(),"chunk_store":self.governance.to_text(),"module_hash":hex::encode(expected)}));
            }
            write_json(
                &self.evidence.join("root-install-proposals.json"),
                &json!(proposals),
            );
        } else {
            for role in ["nns_manager", "stream", "historian", "frontend"] {
                let target = Principal::from_text(context[role].as_str().unwrap()).unwrap();
                self.pic.install_canister(
                    target,
                    fs::read(modules[role]["path"].as_str().unwrap()).unwrap(),
                    fs::read(
                        self.evidence
                            .join("constructors")
                            .join(format!("{role}.bin")),
                    )
                    .unwrap(),
                    Some(self.operator),
                );
            }
            for role in ["stream", "nns_manager", "historian", "frontend"] {
                let id = Principal::from_text(context[role].as_str().unwrap()).unwrap();
                assert_eq!(
                    self.pic
                        .canister_status(id, Some(self.operator))
                        .unwrap()
                        .module_hash,
                    Some(hex::decode(modules[role]["raw_wasm_sha256"].as_str().unwrap()).unwrap())
                );
            }
            for target in &dapps {
                self.pic
                    .set_controllers(*target, Some(self.operator), vec![self.root])
                    .unwrap();
            }
            let id = proposal_id(self.propose(
                "Local register installed IO canisters",
                gov::Action::RegisterDappCanisters(gov::RegisterDappCanisters {
                    canister_ids: dapps.clone(),
                }),
            ));
            self.jupiter_vote(id, 1);
            assert!(
                self.proposal(id).executed_timestamp_seconds > 0,
                "registration proposal failed: {:?}",
                self.proposal(id)
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
        // Public keepers continue the same production observers used by timers.
        let mut observation = None;
        for _ in 0..120 {
            let _: Result<io_stream_manager::RewardEventObservation, io_stream_manager::ApiError> =
                decode_one(
                    &self
                        .pic
                        .update_call(
                            self.stream,
                            self.operator,
                            "resume_reward_work",
                            candid::encode_args(()).unwrap(),
                        )
                        .unwrap(),
                )
                .unwrap();
            self.pic.advance_time(Duration::from_secs(10));
            self.pic.tick();
            let status: io_stream_manager::Status = decode_one(
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
            if let Some(c) = status.latest_reconciliation_checkpoint {
                observation = Some(c);
                break;
            }
        }
        let observed =
            observation.expect("production structural/reward observation did not complete");
        assert_eq!(
            observed.active_backing_io_e8s, 3_000_000_000,
            "only the exact-delay operator is structurally active; short sale baskets are not"
        );
        assert!(observed.active_backing_io_e8s < observed.claim_supply_e8s);
        let mut completed = None;
        for _ in 0..120 {
            let _: Result<io_stream_manager::StreamProgress, io_stream_manager::ApiError> =
                decode_one(
                    &self
                        .pic
                        .update_call(
                            self.stream,
                            self.operator,
                            "resume",
                            candid::encode_args(()).unwrap(),
                        )
                        .unwrap(),
                )
                .unwrap();
            let _: Result<
                io_nns_neuron_manager::api::NnsProgress,
                io_nns_neuron_manager::ApiError,
            > = decode_one(
                &self
                    .pic
                    .update_call(
                        manager,
                        self.operator,
                        "resume",
                        candid::encode_args(()).unwrap(),
                    )
                    .unwrap(),
            )
            .unwrap();
            self.pic.advance_time(Duration::from_secs(10));
            self.pic.tick();
            let dynamic: Result<
                io_nns_neuron_manager::api::DynamicBackingStatus,
                io_nns_neuron_manager::ApiError,
            > = decode_one(
                &self
                    .pic
                    .update_call(
                        manager,
                        self.operator,
                        "observe_dynamic_backing_status",
                        candid::encode_args(()).unwrap(),
                    )
                    .unwrap(),
            )
            .unwrap();
            if let Ok(dynamic) = dynamic {
                if dynamic.claim_bearing_dynamic_principal_e8s == observed.pooled_target_e8s {
                    completed = Some(dynamic);
                    break;
                }
            }
        }
        let dynamic =
            completed.expect("Dynamic reconciliation did not reach the production-observed target");
        assert!(observed.pooled_target_e8s > 0);
        assert_eq!(dynamic.anchor_target_e8s, 1_000_000_000);
        // Existing reconciliation charges its one transfer fee to excluded
        // anchor capacity; it preserves claim backing at the observed target.
        assert_eq!(dynamic.anchor_available_e8s, 1_000_000_000 - 10_000);
        assert_eq!(
            dynamic.parent.as_ref().unwrap().physical_principal_e8s,
            dynamic.claim_bearing_dynamic_principal_e8s
                + dynamic.anchor_available_e8s
                + dynamic.excluded_dynamic_surplus_e8s
        );
        write_json(
            &self.evidence.join("installed-economics.json"),
            &json!({"autonomous_ready":ready,
            "active_backing_io_e8s":observed.active_backing_io_e8s,"reward_active_io_e8s":observed.active_reward_io_e8s,
            "claims_e8s":observed.claim_supply_e8s,"dynamic_claim_principal_e8s":dynamic.claim_bearing_dynamic_principal_e8s,
            "dynamic_anchor_target_e8s":dynamic.anchor_target_e8s,"dynamic_anchor_available_e8s":dynamic.anchor_available_e8s,
            "dynamic_anchor_fee_consumed_e8s":dynamic.anchor_target_e8s-dynamic.anchor_available_e8s,"pooled_target_e8s":observed.pooled_target_e8s}),
        );
    }
}
#[derive(CandidType, Deserialize)]
struct RootSummaryRequest {
    update_canister_list: Option<bool>,
}
#[derive(CandidType, Deserialize)]
struct RootSummary {
    dapps: Vec<RootCanisterSummary>,
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
#[ignore = "requires current-source IO Wasms, pinned same-source SNS bundle, parser tool and PocketIC"]
fn real_launch_checkpoints_a_b_c() {
    let sale = Sale::with_initial_registration(true);
    sale.finish();
    sale.migrate_reserve();
    sale.capitalize();
    sale.install_io(false);
    sale.redemption_and_reward();
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
#[ignore = "requires pinned same-source SNS bundle, parser tool and PocketIC"]
fn real_checkpoint_c_negative_cases() {
    let sale = Sale::with_initial_registration(true);
    sale.finish();
    sale.migrate_reserve();
    sale.capitalize();
    sale.install_io(true);
    println!(
        "real checkpoint C mutations evidence: {}",
        sale.evidence.display()
    );
}

#[test]
#[ignore = "requires pinned same-source SNS bundle, parser tool and PocketIC"]
fn real_checkpoint_a_b_negative_cases() {
    let sale = Sale::new();
    let d = &sale.evidence;
    assert!(!d.join("observation-b.json").exists());
    assert!(!d.join("observation-c.json").exists());
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
            "/jupiter",
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
    sale.finish();
    sale.migrate_reserve();
    for (pointer, value) in [
        ("/finalization/sweep_icp/failure", json!(1)),
        ("/finalization/claim_neurons/global_failures", json!(1)),
        ("/accepted_direct_icp_e8s", json!(300_000_001)),
        ("/treasury_icp_e8s", json!(0)),
        ("/excluded_jupiter_e8s", json!(0)),
    ] {
        mutation(d, "observation-b.json", pointer, value, "B");
    }
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
    // Identities and execution timestamps are labelled parser-test inputs, not
    // installed-canister or future sale observations. Configuration still comes
    // exclusively from the canonical renderer used by the real sale driver.
    let mut context = json!({"local_fixture":true,
        "logo_path":local_fixture_logo(&evidence),
        "nns_proposal_id":0,"swap_start_timestamp_seconds":1_800_000_000u64,
        "swap_due_timestamp_seconds":1_800_086_400u64});
    for (i, role) in [
        "operator",
        "jupiter",
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
        assert!(text.contains("IO Local Fixture"));
        if environment == "production" {
            assert!(!text.contains("acceptance rehearsal"));
            assert!(text.contains("IO production"));
        }
        projections.push(fs::read(output.join("parsed-economics.json")).unwrap());
    }
    assert_eq!(
        projections[0], projections[1],
        "parsed economic parameters differ across environments"
    );
    let original = fs::read_to_string(evidence.join("acceptance-local.yaml")).unwrap();
    for (name, from, to) in [
        (
            "operator-stake",
            "stake: 3000000000 e8s",
            "stake: 2900000000 e8s",
        ),
        (
            "rejection-fee",
            "rejection_fee: 1000000000 e8s",
            "rejection_fee: 900000000 e8s",
        ),
        ("rewards", "initial: 0%", "initial: 1%"),
        (
            "sale-price",
            "maximum_direct_participation_icp: 300000000 e8s",
            "maximum_direct_participation_icp: 400000000 e8s",
        ),
        ("missing-basket", "    events: 2\n", ""),
    ] {
        assert!(original.contains(from), "mutation input absent: {name}");
        let yaml = evidence.join(format!("invalid-{name}.yaml"));
        fs::write(&yaml, original.replacen(from, to, 1)).unwrap();
        let result = Command::new(&parser)
            .arg(yaml)
            .arg(workspace().join("deploy/acceptance/canonical-sns-economics.json"))
            .arg(evidence.join("acceptance-local-context.json"))
            .arg(evidence.join(format!("invalid-{name}-payloads")))
            .output()
            .unwrap();
        assert!(!result.status.success(), "parser accepted {name}");
        fs::write(
            evidence.join(format!("invalid-{name}.stderr")),
            result.stderr,
        )
        .unwrap();
    }
    println!("pinned-parser component evidence: {}", evidence.display());
}

#[derive(CandidType, Deserialize)]
struct FundEnvelope {
    settle_neurons_fund_participation_result: Option<FundResult>,
}
#[derive(CandidType, Deserialize)]
struct FundResult {
    possibility: Option<FundPossibility>,
}
#[derive(CandidType, Deserialize)]
enum FundPossibility {
    Ok(FundOk),
    Err(FundError),
}
#[derive(CandidType, Deserialize)]
struct FundOk {
    neurons_fund_participation_icp_e8s: Option<u64>,
    neurons_fund_neurons_count: Option<u64>,
}
#[derive(CandidType, Deserialize)]
struct FundError {
    message: Option<String>,
}

impl Sale {
    fn redemption_and_reward(&self) {
        let read_status = || -> io_stream_manager::Status {
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
        let before = read_status();
        for _ in 0..3 {
            self.pic.advance_time(Duration::from_secs(86_400 + 301));
            for _ in 0..60 {
                self.pic.tick();
                let _: Result<
                    io_stream_manager::RewardEventObservation,
                    io_stream_manager::ApiError,
                > = decode_one(
                    &self
                        .pic
                        .update_call(
                            self.stream,
                            self.operator,
                            "resume_reward_work",
                            candid::encode_args(()).unwrap(),
                        )
                        .unwrap(),
                )
                .unwrap();
                self.pic.advance_time(Duration::from_secs(10));
                if read_status().processed_reward_event_count > before.processed_reward_event_count
                    && read_status().accumulated_eligible_credit
                        > before.accumulated_eligible_credit
                {
                    break;
                }
            }
            if read_status().accumulated_eligible_credit > before.accumulated_eligible_credit {
                break;
            }
        }
        let rewards = read_status();
        assert!(
            rewards.processed_reward_event_count > before.processed_reward_event_count,
            "real reward event not consumed"
        );
        assert!(
            rewards.accumulated_eligible_credit > before.accumulated_eligible_credit,
            "operator daily entitlement not recorded"
        );
        let operator_id = gov::compute_neuron_staking_subaccount(self.operator, OPERATOR_MEMO);
        assert!(
            rewards
                .accumulated_entitlements
                .iter()
                .all(|entry| entry.sns_neuron_id == operator_id),
            "short baskets and Jupiter must not acquire IO entitlements"
        );
        let now = self.pic.get_time().as_nanos_since_unix_epoch() / 1_000_000_000;
        let liquid_neuron = self
            .neurons()
            .into_iter()
            .find(|n| {
                n.permissions
                    .iter()
                    .any(|p| p.principal == Some(self.buyer))
                    && match n.dissolve_state {
                        Some(gov::DissolveState::DissolveDelaySeconds(0)) => true,
                        Some(gov::DissolveState::WhenDissolvedTimestampSeconds(t)) => t <= now,
                        _ => false,
                    }
            })
            .expect("zero-delay sale basket neuron");
        let disbursed = self.manage(
            self.buyer,
            liquid_neuron.id.unwrap().id,
            gov::Command::Disburse(gov::Disburse {
                amount: None,
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
        let icp_balance = || -> u64 {
            let b: IcpTokens = icrc::query_one(
                &self.pic,
                self.icp,
                "account_balance",
                IcpBalance {
                    account: Account::new(self.buyer, None)
                        .icp_account_identifier_bytes()
                        .to_vec(),
                },
            );
            b.e8s
        };
        let icp_before = icp_balance();
        let io_before = self.balance(self.buyer, None);
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
                    subaccount: staging.subaccount,
                },
                50_000_000,
                Some(10_000),
                None,
                None,
            ),
        );
        let block = sent.unwrap();
        let accounts: Value =
            serde_json::from_slice(&fs::read(self.evidence.join("accounts.json")).unwrap())
                .unwrap();
        let context: Value =
            serde_json::from_slice(&fs::read(self.evidence.join("context.json")).unwrap()).unwrap();
        let liquid_sub: [u8; 32] =
            hex::decode(accounts["liquid_icp"]["subaccount_hex"].as_str().unwrap())
                .unwrap()
                .try_into()
                .unwrap();
        let liquid: IcpTokens = icrc::query_one(
            &self.pic,
            self.icp,
            "account_balance",
            IcpBalance {
                account: Account::new(self.stream, Some(Subaccount(liquid_sub)))
                    .icp_account_identifier_bytes()
                    .to_vec(),
            },
        );
        let manager = Principal::from_text(context["nns_manager"].as_str().unwrap()).unwrap();
        let dynamic: Result<
            io_nns_neuron_manager::api::DynamicBackingStatus,
            io_nns_neuron_manager::ApiError,
        > = decode_one(
            &self
                .pic
                .update_call(
                    manager,
                    self.buyer,
                    "observe_dynamic_backing_status",
                    candid::encode_args(()).unwrap(),
                )
                .unwrap(),
        )
        .unwrap();
        let dynamic = dynamic.unwrap();
        let structural = read_status().latest_reconciliation_checkpoint.unwrap();
        assert_eq!(structural.live_cohort_count, 0);
        assert_eq!(structural.unwinding_backing_e8s, 0);
        assert_eq!(structural.transit_backing_e8s, 0);
        let supply: Nat = decode_one(
            &self
                .pic
                .query_call(
                    self.ledger,
                    self.buyer,
                    "icrc1_total_supply",
                    candid::encode_args(()).unwrap(),
                )
                .unwrap(),
        )
        .unwrap();
        let reserve = self.balance(
            self.stream,
            Some(hex::decode(accounts["io_reserve"]["subaccount_hex"].as_str().unwrap()).unwrap()),
        );
        let excluded: u64 = accounts["excluded_staking_accounts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| {
                self.balance(
                    Principal::from_text(a["owner"].as_str().unwrap()).unwrap(),
                    Some(hex::decode(a["subaccount_hex"].as_str().unwrap()).unwrap()),
                )
            })
            .sum();
        let claims = u128::try_from(supply.0)
            .unwrap()
            .checked_sub(u128::from(reserve))
            .unwrap()
            .checked_sub(u128::from(excluded))
            .unwrap();
        let backing = u128::from(liquid.e8s) + dynamic.claim_bearing_dynamic_principal_e8s;
        let quote = io_core_model::redemption_quote(
            io_core_model::EconomicState {
                backing: io_core_model::Backing {
                    liquid: u128::from(liquid.e8s),
                    pooled: dynamic.claim_bearing_dynamic_principal_e8s,
                    unwinding: 0,
                    transit: 0,
                },
                claims,
                active_backing: structural.active_backing_io_e8s,
                active_reward: structural.active_reward_io_e8s,
            },
            50_000_000,
            10_000,
        )
        .unwrap();
        let mut last_redemption_result = None;
        let mut last_continuations = String::new();
        for _ in 0..180 {
            // Ledger fee burns can create a new Dynamic TopUp. Public keepers
            // finish that existing operation before redemption can be admitted.
            let stream_result: Result<
                io_stream_manager::StreamProgress,
                io_stream_manager::ApiError,
            > = decode_one(
                &self
                    .pic
                    .update_call(
                        self.stream,
                        self.buyer,
                        "resume",
                        candid::encode_args(()).unwrap(),
                    )
                    .unwrap(),
            )
            .unwrap();
            let nns_result: Result<
                io_nns_neuron_manager::api::NnsProgress,
                io_nns_neuron_manager::ApiError,
            > = decode_one(
                &self
                    .pic
                    .update_call(
                        manager,
                        self.buyer,
                        "resume",
                        candid::encode_args(()).unwrap(),
                    )
                    .unwrap(),
            )
            .unwrap();
            let reward_result: Result<
                io_stream_manager::RewardEventObservation,
                io_stream_manager::ApiError,
            > = decode_one(
                &self
                    .pic
                    .update_call(
                        self.stream,
                        self.buyer,
                        "resume_reward_work",
                        candid::encode_args(()).unwrap(),
                    )
                    .unwrap(),
            )
            .unwrap();
            let result: Result<(), io_stream_manager::ApiError> = decode_one(
                &self
                    .pic
                    .update_call(
                        self.stream,
                        self.buyer,
                        "process_redemptions",
                        candid::encode_args(()).unwrap(),
                    )
                    .unwrap(),
            )
            .unwrap();
            last_continuations =
                format!("stream={stream_result:?}; nns={nns_result:?}; reward={reward_result:?}");
            last_redemption_result = Some(result);
            self.pic.advance_time(Duration::from_secs(10));
            self.pic.tick();
            if icp_balance() > icp_before && read_status().operation_kind.is_none() {
                break;
            }
        }
        fs::write(
            self.evidence.join("redemption-status.txt"),
            format!(
                "result={last_redemption_result:?}\ncontinuations={last_continuations}\nstatus={:#?}\n",
                read_status()
            ),
        )
        .unwrap();
        assert!(
            icp_balance() > icp_before,
            "real-ledger redemption did not pay ICP: result={last_redemption_result:?}; continuations={last_continuations}; status={:#?}", read_status()
        );
        assert_eq!(
            u128::from(icp_balance() - icp_before),
            quote.net_icp,
            "redemption must match the canonical pre-dispatch balances and production quote"
        );
        assert_eq!(
            self.balance(self.buyer, None),
            io_before - 50_000_000 - 10_000
        );
        let treasury: IcpTokens = icrc::query_one(
            &self.pic,
            self.icp,
            "account_balance",
            IcpBalance {
                account: Account::new(self.governance, None)
                    .icp_account_identifier_bytes()
                    .to_vec(),
            },
        );
        assert_eq!(treasury.e8s, SALE - 10_000);
        write_json(
            &self.evidence.join("redemption-reward.json"),
            &json!({"reward_events_before":before.processed_reward_event_count,
            "reward_events_after":rewards.processed_reward_event_count,"eligible_credit_before":before.accumulated_eligible_credit,
            "eligible_credit_after":rewards.accumulated_eligible_credit,"redemption_io_e8s":50_000_000,"redemption_block":block.to_string(),
            "redemption_claim_supply_e8s":claims,"redemption_backing_e8s":backing,
            "redemption_gross_icp_e8s":quote.gross_icp,"redemption_icp_fee_e8s":10_000,
            "participant_icp_credit_e8s":icp_balance()-icp_before,"sale_treasury_retained_icp_e8s":treasury.e8s}),
        );
    }
}

#[test]
#[ignore = "real NNS PocketIC public-call authority proof; independent of SNS sale artifacts"]
fn real_nns_existing_id_noncontroller_refresh() {
    use io_governance_types::*;
    let pic = pocketic_env::new_pic_with_nns_governance_features();
    let owner = Principal::from_slice(&[83; 29]);
    let stranger = Principal::from_slice(&[84; 29]);
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
    println!("existing-ID non-controller refresh: 200000000 -> 300000000 e8s; controller, dissolve policy, followees, hot keys and maturity policy unchanged; Configure denied");
}
