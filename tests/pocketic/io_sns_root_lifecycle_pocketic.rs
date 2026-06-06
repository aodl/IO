use candid::{decode_one, encode_one, Nat, Principal};
use io_core_model::E8S_PER_TOKEN;
use io_ledger_types::{
    map_icrc_transfer_result, Account, IcrcAccount, IcrcTransferArg, IcrcTransferError, Subaccount,
};
use io_sns_lifecycle::{
    read_manifest, resolve_manifest_entry, ExpectedModuleHashRequest, RegisterDappCanisterRequest,
    RootUpgradeAttempt, RootUpgradeAttemptStatus, RootUpgradeIntent, RootUpgradeOutcomeRequest,
    UpgradeProposal, UpgradeProposalRequest, UpgradeProposalStatus, UpgradeVote,
};
use pocket_ic::PocketIc;
use sha2::{Digest, Sha256};

const CYCLES: u128 = 2_000_000_000_000;

#[derive(Clone, Debug, PartialEq, Eq, candid::CandidType, serde::Deserialize)]
struct DebugMintArgs {
    to: String,
    amount_e8s: u128,
    memo: String,
}

#[derive(Clone, Debug, PartialEq, Eq, candid::CandidType, serde::Deserialize)]
struct DebugRejectAccountArgs {
    account: String,
}

#[derive(Clone, Debug, PartialEq, Eq, candid::CandidType, serde::Deserialize)]
struct LedgerTransaction {
    from: String,
    to: String,
    amount_e8s: u128,
    memo: String,
    block_index: u64,
    timestamp: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, candid::CandidType, serde::Deserialize)]
struct MockSnsNeuron {
    neuron_id: u64,
    staked_io_e8s: u128,
    eligible_seconds: u64,
    eligible_closed_proposals: u64,
    voted_closed_proposals: u64,
    is_genesis_governance_neuron: bool,
    is_protocol_owned: bool,
    is_dissolving: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, candid::CandidType, serde::Deserialize)]
struct IndexInitArgs {
    ledger_principal_text: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, candid::CandidType, serde::Deserialize)]
struct DebugTickOutcome {
    processed_authorized_streams: u64,
    processed_redemptions: u64,
    io_issued_e8s: u128,
    icp_paid_e8s: u128,
    errors: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, candid::CandidType, serde::Deserialize)]
struct NnsDebugTickOutcome {
    disbursed_two_year_maturity_e8s: u128,
    disbursed_two_week_maturity_e8s: u128,
    disbursed_unwind_principal_e8s: u128,
    planned_pool_rebalances: u64,
    errors: Vec<String>,
}

fn t(n: u128) -> u128 {
    n * E8S_PER_TOKEN
}

fn pocketic_available() -> bool {
    std::env::var_os("POCKET_IC_BIN").is_some()
}

fn required_wasm(path: &str) -> Option<Vec<u8>> {
    match std::fs::read(path) {
        Ok(bytes) => Some(bytes),
        Err(_) => {
            eprintln!("skipping SNS root lifecycle PocketIC test because {path} is missing");
            None
        }
    }
}

fn create_canister(pic: &PocketIc, wasm: Vec<u8>, arg: Vec<u8>) -> Principal {
    let canister = pic.create_canister();
    pic.add_cycles(canister, CYCLES);
    pic.install_canister(canister, wasm, arg, None);
    canister
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

struct LifecycleFixture {
    pic: PocketIc,
    stream: Principal,
    nns_manager: Principal,
    root: Principal,
    governance: Principal,
    icp_ledger: Principal,
    io_ledger: Principal,
    stream_wasm: Vec<u8>,
    nns_wasm: Vec<u8>,
}

fn setup() -> Option<LifecycleFixture> {
    if !pocketic_available() {
        eprintln!("skipping SNS root lifecycle PocketIC test because POCKET_IC_BIN is not set");
        return None;
    }

    let stream_wasm = required_wasm("target/wasm32-unknown-unknown/debug/io_stream_manager.wasm")?;
    let nns_wasm = required_wasm("target/wasm32-unknown-unknown/debug/io_nns_neuron_manager.wasm")?;
    let icp_ledger_wasm =
        required_wasm("target/wasm32-unknown-unknown/debug/mock_icp_ledger.wasm")?;
    let io_ledger_wasm = required_wasm("target/wasm32-unknown-unknown/debug/mock_io_ledger.wasm")?;
    let icp_index_wasm = required_wasm("target/wasm32-unknown-unknown/debug/mock_icp_index.wasm")?;
    let io_index_wasm = required_wasm("target/wasm32-unknown-unknown/debug/mock_io_index.wasm")?;
    let governance_wasm =
        required_wasm("target/wasm32-unknown-unknown/debug/mock_sns_governance.wasm")?;
    let root_wasm = required_wasm("target/wasm32-unknown-unknown/debug/mock_sns_root.wasm")?;

    let pic = PocketIc::new();
    let icp_ledger = create_canister(&pic, icp_ledger_wasm, vec![]);
    let io_ledger = create_canister(&pic, io_ledger_wasm, vec![]);
    let icp_index = create_canister(
        &pic,
        icp_index_wasm,
        encode_one(IndexInitArgs {
            ledger_principal_text: Some(icp_ledger.to_text()),
        })
        .unwrap(),
    );
    let io_index = create_canister(
        &pic,
        io_index_wasm,
        encode_one(IndexInitArgs {
            ledger_principal_text: Some(io_ledger.to_text()),
        })
        .unwrap(),
    );
    let root = create_canister(&pic, root_wasm, vec![]);
    let governance = create_canister(&pic, governance_wasm, vec![]);

    set_governance_root(&pic, governance, root);
    set_root_governance(&pic, root, governance);

    let stream = create_canister(
        &pic,
        stream_wasm.clone(),
        encode_one(io_stream_manager::InitArgs {
            icp_ledger_principal_text: Some(icp_ledger.to_text()),
            icp_index_principal_text: Some(icp_index.to_text()),
            io_ledger_principal_text: Some(io_ledger.to_text()),
            io_index_principal_text: Some(io_index.to_text()),
            sns_governance_principal_text: Some(governance.to_text()),
            ..Default::default()
        })
        .unwrap(),
    );
    let nns_manager = create_canister(
        &pic,
        nns_wasm.clone(),
        encode_one(io_nns_neuron_manager::InitArgs {
            controller_canister_principal_text: root.to_text(),
            two_year_nns_neuron_id: 42,
            initial_two_year_principal_e8s: t(1_000),
            model_annual_bps: 12_000,
            icp_ledger_principal_text: Some(icp_ledger.to_text()),
            io_stream_manager_principal_text: Some(stream.to_text()),
            ..Default::default()
        })
        .unwrap(),
    );

    for (name, principal) in [
        ("io_stream_manager", stream),
        ("io_nns_neuron_manager", nns_manager),
    ] {
        register_dapp(&pic, root, name, principal);
    }

    pic.set_controllers(stream, None, vec![root])
        .expect("set stream root controller");
    pic.set_controllers(nns_manager, None, vec![root])
        .expect("set nns manager root controller");
    assert_eq!(pic.get_controllers(stream), vec![root]);
    assert_eq!(pic.get_controllers(nns_manager), vec![root]);

    Some(LifecycleFixture {
        pic,
        stream,
        nns_manager,
        root,
        governance,
        icp_ledger,
        io_ledger,
        stream_wasm,
        nns_wasm,
    })
}

fn set_governance_root(pic: &PocketIc, governance: Principal, root: Principal) {
    pic.update_call(
        governance,
        Principal::anonymous(),
        "debug_set_root_principal",
        encode_one(root).unwrap(),
    )
    .expect("set governance root");
}

fn set_root_governance(pic: &PocketIc, root: Principal, governance: Principal) {
    pic.update_call(
        root,
        Principal::anonymous(),
        "debug_set_governance_principal",
        encode_one(governance).unwrap(),
    )
    .expect("set root governance");
}

fn register_dapp(pic: &PocketIc, root: Principal, name: &str, principal: Principal) {
    pic.update_call(
        root,
        Principal::anonymous(),
        "debug_register_dapp_canister",
        encode_one(RegisterDappCanisterRequest {
            name: name.to_string(),
            principal,
        })
        .unwrap(),
    )
    .expect("register dapp");
}

fn record_expected_hash(pic: &PocketIc, root: Principal, target: Principal, hash: String) {
    pic.update_call(
        root,
        Principal::anonymous(),
        "debug_record_expected_module_hash",
        encode_one(ExpectedModuleHashRequest {
            target_canister: target,
            expected_module_hash: hash,
        })
        .unwrap(),
    )
    .expect("record expected hash");
}

fn proposal_request(
    canister: &str,
    target: Principal,
    expected_module_hash: String,
) -> UpgradeProposalRequest {
    let manifest = read_manifest("release-artifacts/manifest.json").expect("release manifest");
    let entry = resolve_manifest_entry(&manifest, canister).expect("manifest entry");
    UpgradeProposalRequest {
        target_canister: target,
        wasm_sha256: entry.raw_wasm_sha256.clone(),
        wasm_gz_sha256: entry.gz_wasm_sha256.clone(),
        artifact_name: canister.to_string(),
        artifact_path: entry.raw_wasm_path.clone(),
        expected_module_hash: Some(expected_module_hash),
    }
}

fn submit(
    pic: &PocketIc,
    governance: Principal,
    request: UpgradeProposalRequest,
) -> UpgradeProposal {
    let bytes = pic
        .update_call(
            governance,
            Principal::anonymous(),
            "debug_submit_upgrade_proposal",
            encode_one(request).unwrap(),
        )
        .expect("submit proposal");
    decode_one(&bytes).unwrap()
}

fn vote_yes(pic: &PocketIc, governance: Principal, proposal_id: u64) {
    pic.update_call(
        governance,
        Principal::anonymous(),
        "debug_vote_proposal",
        encode_one((proposal_id, UpgradeVote::Yes)).unwrap(),
    )
    .expect("vote proposal");
}

fn adopt(pic: &PocketIc, governance: Principal, proposal_id: u64) -> UpgradeProposal {
    let bytes = pic
        .update_call(
            governance,
            Principal::anonymous(),
            "debug_adopt_upgrade_proposal",
            encode_one(proposal_id).unwrap(),
        )
        .expect("adopt proposal");
    decode_one::<Result<UpgradeProposal, String>>(&bytes)
        .unwrap()
        .expect("adopt result")
}

fn reject(pic: &PocketIc, governance: Principal, proposal_id: u64) -> UpgradeProposal {
    let bytes = pic
        .update_call(
            governance,
            Principal::anonymous(),
            "debug_reject_upgrade_proposal",
            encode_one(proposal_id).unwrap(),
        )
        .expect("reject proposal");
    decode_one::<Result<UpgradeProposal, String>>(&bytes)
        .unwrap()
        .expect("reject result")
}

fn finalize(
    pic: &PocketIc,
    governance: Principal,
    proposal_id: u64,
) -> Result<RootUpgradeIntent, String> {
    let bytes = pic
        .update_call(
            governance,
            Principal::anonymous(),
            "debug_finalize_proposal",
            encode_one(proposal_id).unwrap(),
        )
        .expect("finalize proposal");
    decode_one(&bytes).unwrap()
}

fn record_outcome(pic: &PocketIc, root: Principal, attempt_id: u64, success: bool) {
    pic.update_call(
        root,
        Principal::anonymous(),
        "debug_record_upgrade_outcome",
        encode_one(RootUpgradeOutcomeRequest {
            attempt_id,
            success,
            failure_reason: (!success).then(|| "harness upgrade failed".to_string()),
        })
        .unwrap(),
    )
    .expect("record outcome");
}

fn history(pic: &PocketIc, root: Principal) -> Vec<RootUpgradeAttempt> {
    let bytes = pic
        .query_call(
            root,
            Principal::anonymous(),
            "debug_get_upgrade_history",
            encode_one(()).unwrap(),
        )
        .expect("history");
    decode_one(&bytes).unwrap()
}

fn stream_state(fixture: &LifecycleFixture) -> io_stream_manager::ApiState {
    let bytes = fixture
        .pic
        .query_call(
            fixture.stream,
            Principal::anonymous(),
            "debug_get_state",
            encode_one(()).unwrap(),
        )
        .expect("stream state");
    decode_one(&bytes).unwrap()
}

fn nns_state(fixture: &LifecycleFixture) -> io_nns_neuron_manager::ApiState {
    let bytes = fixture
        .pic
        .query_call(
            fixture.nns_manager,
            Principal::anonymous(),
            "debug_get_state",
            encode_one(()).unwrap(),
        )
        .expect("nns state");
    decode_one(&bytes).unwrap()
}

fn tick_stream(fixture: &LifecycleFixture) -> DebugTickOutcome {
    let bytes = fixture
        .pic
        .update_call(
            fixture.stream,
            Principal::anonymous(),
            "debug_tick",
            encode_one(()).unwrap(),
        )
        .expect("stream tick");
    decode_one(&bytes).unwrap()
}

fn tick_nns(fixture: &LifecycleFixture) -> NnsDebugTickOutcome {
    let bytes = fixture
        .pic
        .update_call(
            fixture.nns_manager,
            Principal::anonymous(),
            "debug_tick",
            encode_one(()).unwrap(),
        )
        .expect("nns tick");
    decode_one(&bytes).unwrap()
}

fn mint(pic: &PocketIc, ledger: Principal, to: &str, amount_e8s: u128, memo: &str) {
    pic.update_call(
        ledger,
        Principal::anonymous(),
        "debug_mint",
        encode_one(DebugMintArgs {
            to: to.to_string(),
            amount_e8s,
            memo: memo.to_string(),
        })
        .unwrap(),
    )
    .expect("mint");
}

fn transfer(pic: &PocketIc, ledger: Principal, from: &str, to: &str, amount_e8s: u128, memo: &str) {
    let bytes = pic
        .update_call(
            ledger,
            Principal::anonymous(),
            "icrc1_transfer",
            encode_one(IcrcTransferArg {
                from_subaccount: Some(mock_subaccount(from).0.to_vec()),
                to: IcrcAccount::from(mock_account(to)),
                amount: Nat::from(amount_e8s),
                fee: None,
                memo: Some(memo.as_bytes().to_vec()),
                created_at_time: None,
            })
            .unwrap(),
        )
        .expect("transfer");
    map_icrc_transfer_result(decode_one::<Result<Nat, IcrcTransferError>>(&bytes).unwrap())
        .expect("ledger transfer result");
}

fn mock_subaccount(label: &str) -> Subaccount {
    let bytes = label.as_bytes();
    let mut subaccount = [0; 32];
    let len = bytes.len().min(31);
    subaccount[0] = len as u8;
    subaccount[1..=len].copy_from_slice(&bytes[..len]);
    Subaccount(subaccount)
}

fn mock_account(label: &str) -> Account {
    Account::new(Principal::anonymous(), Some(mock_subaccount(label)))
}

fn reject_to(pic: &PocketIc, ledger: Principal, account: &str) {
    pic.update_call(
        ledger,
        Principal::anonymous(),
        "debug_reject_to",
        encode_one(DebugRejectAccountArgs {
            account: account.to_string(),
        })
        .unwrap(),
    )
    .expect("reject account");
}

fn clear_rejections(pic: &PocketIc, ledger: Principal) {
    pic.update_call(
        ledger,
        Principal::anonymous(),
        "debug_clear_rejections",
        encode_one(()).unwrap(),
    )
    .expect("clear rejections");
}

fn transactions(pic: &PocketIc, ledger: Principal) -> Vec<LedgerTransaction> {
    let bytes = pic
        .query_call(
            ledger,
            Principal::anonymous(),
            "debug_get_transactions",
            encode_one(()).unwrap(),
        )
        .expect("transactions");
    decode_one(&bytes).unwrap()
}

fn add_sns_neuron(pic: &PocketIc, governance: Principal, neuron: MockSnsNeuron) {
    pic.update_call(
        governance,
        Principal::anonymous(),
        "debug_add_neuron",
        encode_one(neuron).unwrap(),
    )
    .expect("add sns neuron");
}

fn sns_neuron(id: u64, stake: u128, voted: u64, total: u64) -> MockSnsNeuron {
    MockSnsNeuron {
        neuron_id: id,
        staked_io_e8s: stake,
        eligible_seconds: 100,
        eligible_closed_proposals: total,
        voted_closed_proposals: voted,
        is_genesis_governance_neuron: false,
        is_protocol_owned: false,
        is_dissolving: false,
    }
}

fn execute_stream_upgrade(
    fixture: &LifecycleFixture,
    expected_debug_hash: String,
) -> RootUpgradeIntent {
    record_expected_hash(
        &fixture.pic,
        fixture.root,
        fixture.stream,
        expected_debug_hash.clone(),
    );
    let proposal = submit(
        &fixture.pic,
        fixture.governance,
        proposal_request("io_stream_manager", fixture.stream, expected_debug_hash),
    );
    vote_yes(&fixture.pic, fixture.governance, proposal.proposal_id);
    assert_eq!(
        adopt(&fixture.pic, fixture.governance, proposal.proposal_id).status,
        UpgradeProposalStatus::Adopted
    );
    let intent = finalize(&fixture.pic, fixture.governance, proposal.proposal_id).unwrap();
    fixture
        .pic
        .upgrade_canister(
            fixture.stream,
            fixture.stream_wasm.clone(),
            vec![],
            Some(fixture.root),
        )
        .expect("root-controller stream upgrade");
    record_outcome(&fixture.pic, fixture.root, intent.attempt_id, true);
    intent
}

fn execute_nns_upgrade(
    fixture: &LifecycleFixture,
    expected_debug_hash: String,
) -> RootUpgradeIntent {
    record_expected_hash(
        &fixture.pic,
        fixture.root,
        fixture.nns_manager,
        expected_debug_hash.clone(),
    );
    let proposal = submit(
        &fixture.pic,
        fixture.governance,
        proposal_request(
            "io_nns_neuron_manager",
            fixture.nns_manager,
            expected_debug_hash,
        ),
    );
    vote_yes(&fixture.pic, fixture.governance, proposal.proposal_id);
    adopt(&fixture.pic, fixture.governance, proposal.proposal_id);
    let intent = finalize(&fixture.pic, fixture.governance, proposal.proposal_id).unwrap();
    fixture
        .pic
        .upgrade_canister(
            fixture.nns_manager,
            fixture.nns_wasm.clone(),
            vec![],
            Some(fixture.root),
        )
        .expect("root-controller nns upgrade");
    record_outcome(&fixture.pic, fixture.root, intent.attempt_id, true);
    intent
}

#[test]
fn pocketic_sns_root_style_stream_upgrade_preserves_pending_reward_state() {
    let Some(fixture) = setup() else {
        return;
    };
    let stream_hash = sha256_hex(&fixture.stream_wasm);
    add_sns_neuron(
        &fixture.pic,
        fixture.governance,
        sns_neuron(10, t(10), 2, 2),
    );
    add_sns_neuron(
        &fixture.pic,
        fixture.governance,
        sns_neuron(11, t(10), 1, 2),
    );
    reject_to(&fixture.pic, fixture.io_ledger, "sns_neuron_11");

    mint(
        &fixture.pic,
        fixture.icp_ledger,
        "io_nns_neuron_manager",
        t(100),
        "fund_nns_manager",
    );
    transfer(
        &fixture.pic,
        fixture.icp_ledger,
        "io_nns_neuron_manager",
        "stream_manager_deposit",
        t(100),
        "two_week_maturity",
    );

    let failed = tick_stream(&fixture);
    assert!(!failed.errors.is_empty());
    assert_eq!(stream_state(&fixture).processed_transaction_count, 0);

    assert!(fixture
        .pic
        .upgrade_canister(fixture.stream, fixture.stream_wasm.clone(), vec![], None)
        .is_err());
    let intent = execute_stream_upgrade(&fixture, stream_hash);
    assert_eq!(intent.target_canister, fixture.stream);

    clear_rejections(&fixture.pic, fixture.io_ledger);
    let retry = tick_stream(&fixture);
    assert!(retry.errors.is_empty(), "{:?}", retry.errors);
    assert_eq!(retry.processed_authorized_streams, 1);
    assert_eq!(
        transactions(&fixture.pic, fixture.io_ledger)
            .iter()
            .filter(|tx| tx.to == "sns_neuron_10")
            .count(),
        1
    );
    assert_eq!(
        history(&fixture.pic, fixture.root)
            .last()
            .expect("root history")
            .status,
        RootUpgradeAttemptStatus::Succeeded
    );

    let stream_did = std::fs::read_to_string("canisters/io_stream_manager/io_stream_manager.did")
        .expect("stream production DID");
    assert!(stream_did.contains("service : (InitArgs) -> {}"));
    assert!(!stream_did.contains("debug_"));
    assert!(!stream_did.contains(" get_state :"));
}

#[test]
fn pocketic_sns_root_style_nns_manager_upgrade_preserves_pending_maturity() {
    let Some(fixture) = setup() else {
        return;
    };
    let nns_hash = sha256_hex(&fixture.nns_wasm);
    reject_to(&fixture.pic, fixture.icp_ledger, "stream_manager_deposit");
    fixture
        .pic
        .advance_time(std::time::Duration::from_secs(30 * 86_400));
    fixture
        .pic
        .update_call(
            fixture.nns_manager,
            Principal::anonymous(),
            "debug_advance_model_time",
            encode_one(io_nns_neuron_manager::AdvanceModelTimeRequest {
                elapsed_seconds: 30 * 86_400,
                annual_bps: Some(12_000),
            })
            .unwrap(),
        )
        .expect("advance nns model time");
    assert!(nns_state(&fixture).two_year_neuron.maturity_e8s > 0);

    let failed = tick_nns(&fixture);
    assert!(!failed.errors.is_empty());
    let maturity_before = nns_state(&fixture).two_year_neuron.maturity_e8s;

    let intent = execute_nns_upgrade(&fixture, nns_hash);
    assert_eq!(intent.target_canister, fixture.nns_manager);
    clear_rejections(&fixture.pic, fixture.icp_ledger);
    let retry = tick_nns(&fixture);
    assert!(retry.errors.is_empty(), "{:?}", retry.errors);
    assert_eq!(retry.disbursed_two_year_maturity_e8s, maturity_before);
    assert_eq!(
        transactions(&fixture.pic, fixture.icp_ledger)
            .iter()
            .filter(|tx| tx.to == "stream_manager_deposit" && tx.memo == "two_year_maturity")
            .count(),
        1
    );

    let nns_did =
        std::fs::read_to_string("canisters/io_nns_neuron_manager/io_nns_neuron_manager.did")
            .expect("nns production DID");
    assert!(nns_did.contains("service : (InitArgs) -> {}"));
    assert!(!nns_did.contains("debug_"));
    assert!(!nns_did.contains(" get_state :"));
}

#[test]
fn pocketic_sns_root_lifecycle_rejects_bad_paths() {
    let Some(fixture) = setup() else {
        return;
    };
    let stream_hash = sha256_hex(&fixture.stream_wasm);
    record_expected_hash(
        &fixture.pic,
        fixture.root,
        fixture.stream,
        stream_hash.clone(),
    );

    let rejected = submit(
        &fixture.pic,
        fixture.governance,
        proposal_request("io_stream_manager", fixture.stream, stream_hash.clone()),
    );
    assert_eq!(
        reject(&fixture.pic, fixture.governance, rejected.proposal_id).status,
        UpgradeProposalStatus::Rejected
    );
    assert!(
        finalize(&fixture.pic, fixture.governance, rejected.proposal_id)
            .unwrap_err()
            .contains("rejected")
    );

    let open = submit(
        &fixture.pic,
        fixture.governance,
        proposal_request("io_stream_manager", fixture.stream, stream_hash.clone()),
    );
    assert!(finalize(&fixture.pic, fixture.governance, open.proposal_id)
        .unwrap_err()
        .contains("open"));

    let wrong_hash = submit(
        &fixture.pic,
        fixture.governance,
        proposal_request("io_stream_manager", fixture.stream, "wrong".to_string()),
    );
    vote_yes(&fixture.pic, fixture.governance, wrong_hash.proposal_id);
    adopt(&fixture.pic, fixture.governance, wrong_hash.proposal_id);
    assert!(
        finalize(&fixture.pic, fixture.governance, wrong_hash.proposal_id)
            .unwrap_err()
            .contains("hash mismatch")
    );

    let wrong_target = submit(
        &fixture.pic,
        fixture.governance,
        proposal_request(
            "io_stream_manager",
            Principal::from_slice(&[42]),
            stream_hash,
        ),
    );
    vote_yes(&fixture.pic, fixture.governance, wrong_target.proposal_id);
    adopt(&fixture.pic, fixture.governance, wrong_target.proposal_id);
    assert!(
        finalize(&fixture.pic, fixture.governance, wrong_target.proposal_id)
            .unwrap_err()
            .contains("unknown dapp canister")
    );

    let unauthorized = fixture.pic.update_call(
        fixture.root,
        Principal::anonymous(),
        "debug_upgrade_dapp_canister",
        encode_one(io_sns_lifecycle::RootUpgradeRequest {
            proposal_id: 99,
            target_canister: fixture.stream,
            wasm_sha256: "raw".to_string(),
            wasm_gz_sha256: "gz".to_string(),
            artifact_name: "io_stream_manager".to_string(),
            artifact_path: "release-artifacts/io_stream_manager.wasm".to_string(),
            expected_module_hash: None,
        })
        .unwrap(),
    );
    let bytes = unauthorized.expect("unauthorized call response");
    assert!(decode_one::<Result<RootUpgradeIntent, String>>(&bytes)
        .unwrap()
        .unwrap_err()
        .contains("unauthorized caller"));
}
