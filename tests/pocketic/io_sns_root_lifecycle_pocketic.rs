use candid::{decode_one, encode_one, CandidType, Principal};
use io_sns_lifecycle::{
    ExpectedModuleHashRequest, RegisterDappCanisterRequest, RootUpgradeAttempt,
    RootUpgradeAttemptStatus, RootUpgradeIntent, RootUpgradeOutcomeRequest, RootUpgradeRequest,
    UpgradeProposal, UpgradeProposalRequest, UpgradeProposalStatus, UpgradeVote,
};
use pocket_ic::PocketIc;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::path::PathBuf;

const CYCLES: u128 = 2_000_000_000_000;

#[derive(Clone, Debug, CandidType, Deserialize)]
struct NnsInitArgs {
    config: NnsConfig,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct NnsConfig {
    sns_governance: Principal,
    stream_manager: Principal,
    jupiter: Principal,
    icp_ledger: Principal,
    nns_governance: Principal,
    two_year_neuron_id: u64,
    pooled_parent_memo: u64,
    pooled_parent_followee_id: u64,
    minimum_parent_stake_e8s: u128,
    jupiter_account: io_stream_manager::Account,
    jupiter_staging: io_stream_manager::Account,
    stream_liquid_account: io_stream_manager::Account,
    expected_io_fee_e8s: u128,
    expected_icp_fee_e8s: u128,
    jupiter_activation_block_floor: u128,
    audited_permanent_principal_e8s: u128,
    transfer_retry_delay_nanos: u64,
    ledger_deduplication_window_nanos: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
enum Lifecycle {
    Paused,
    Ready,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct LifecycleStatus {
    lifecycle: Lifecycle,
}

struct Fixture {
    pic: PocketIc,
    root: Principal,
    governance: Principal,
    stream: Principal,
    nns_manager: Principal,
}

fn workspace_path(path: &str) -> PathBuf {
    let direct = PathBuf::from(path);
    if direct.exists() {
        direct
    } else {
        PathBuf::from("../../").join(path)
    }
}

fn required_wasm(path: &str) -> Vec<u8> {
    std::fs::read(workspace_path(path)).unwrap_or_else(|error| panic!("{path}: {error}"))
}

fn hex_encode(bytes: impl AsRef<[u8]>) -> String {
    bytes
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn install(pic: &PocketIc, wasm: Vec<u8>, arg: Vec<u8>) -> Principal {
    let canister = pic.create_canister();
    pic.add_cycles(canister, CYCLES);
    pic.install_canister(canister, wasm, arg, None);
    canister
}

fn update(
    pic: &PocketIc,
    canister: Principal,
    caller: Principal,
    method: &str,
    arg: Vec<u8>,
) -> Vec<u8> {
    pic.update_call(canister, caller, method, arg)
        .unwrap_or_else(|error| panic!("{method}: {error}"))
}

fn setup() -> Fixture {
    let pic = PocketIc::new();
    let root = install(
        &pic,
        required_wasm("target/wasm32-unknown-unknown/debug/mock_sns_root.wasm"),
        vec![],
    );
    let governance = install(
        &pic,
        required_wasm("target/wasm32-unknown-unknown/debug/mock_sns_governance.wasm"),
        vec![],
    );
    update(
        &pic,
        governance,
        Principal::anonymous(),
        "debug_set_root_principal",
        encode_one(root).unwrap(),
    );
    update(
        &pic,
        root,
        Principal::anonymous(),
        "debug_set_governance_principal",
        encode_one(governance).unwrap(),
    );

    let stream = pic.create_canister();
    pic.add_cycles(stream, CYCLES);
    let nns_manager = pic.create_canister();
    pic.add_cycles(nns_manager, CYCLES);
    let io_ledger = Principal::from_slice(&[1; 29]);
    let icp_ledger = Principal::from_slice(&[2; 29]);
    let nns_governance = Principal::from_slice(&[3; 29]);
    let jupiter = Principal::from_slice(&[4; 29]);
    let stream_liquid = io_stream_manager::Account {
        owner: stream,
        subaccount: Some(vec![1; 32]),
    };

    pic.install_canister(
        stream,
        required_wasm("target/wasm32-unknown-unknown/debug/io_stream_manager.wasm"),
        encode_one(io_stream_manager::InitArgs {
            config: io_stream_manager::StreamConfig {
                io_ledger,
                icp_ledger,
                nns_manager,
                jupiter_io_account: io_stream_manager::Account {
                    owner: jupiter,
                    subaccount: None,
                },
                sns_governance: governance,
                sns_root: root,
                expected_sns_governance_module_hash: vec![0; 32],
                approved_reward_event_duration_seconds: 86_400,
                io_reserve: io_stream_manager::Account {
                    owner: stream,
                    subaccount: None,
                },
                liquid_icp: stream_liquid.clone(),
                nonredeemable_governance_io_accounts: Vec::new(),
                minimum_redemption_io_e8s: 20_000,
                expected_io_fee_e8s: 10_000,
                expected_icp_fee_e8s: 10_000,
                maximum_request_lifetime_nanos: 900_000_000_000,
                retry_delay_nanos: 1_000_000_000,
                ledger_deduplication_window_nanos: 86_400_000_000_000,
            },
        })
        .unwrap(),
        None,
    );
    pic.install_canister(
        nns_manager,
        required_wasm("target/wasm32-unknown-unknown/debug/io_nns_neuron_manager.wasm"),
        encode_one(NnsInitArgs {
            config: NnsConfig {
                sns_governance: governance,
                stream_manager: stream,
                jupiter,
                icp_ledger,
                nns_governance,
                two_year_neuron_id: 42,
                pooled_parent_memo: 43,
                pooled_parent_followee_id: 42,
                minimum_parent_stake_e8s: 100_000_000,
                jupiter_account: io_stream_manager::Account {
                    owner: jupiter,
                    subaccount: None,
                },
                jupiter_staging: io_stream_manager::Account {
                    owner: nns_manager,
                    subaccount: None,
                },
                stream_liquid_account: stream_liquid,
                expected_io_fee_e8s: 10_000,
                expected_icp_fee_e8s: 10_000,
                jupiter_activation_block_floor: 1,
                audited_permanent_principal_e8s: 1,
                transfer_retry_delay_nanos: 1_000_000_000,
                ledger_deduplication_window_nanos: 86_400_000_000_000,
            },
        })
        .unwrap(),
        None,
    );

    for (name, target) in [
        ("io_stream_manager", stream),
        ("io_nns_neuron_manager", nns_manager),
    ] {
        update(
            &pic,
            root,
            Principal::anonymous(),
            "debug_register_dapp_canister",
            encode_one(RegisterDappCanisterRequest {
                name: name.to_string(),
                principal: target,
            })
            .unwrap(),
        );
        pic.set_controllers(target, None, vec![root]).unwrap();
    }

    Fixture {
        pic,
        root,
        governance,
        stream,
        nns_manager,
    }
}

fn lifecycle(pic: &PocketIc, canister: Principal) -> Lifecycle {
    let bytes = pic
        .query_call(
            canister,
            Principal::anonymous(),
            "get_status",
            encode_one(()).unwrap(),
        )
        .unwrap();
    decode_one::<LifecycleStatus>(&bytes).unwrap().lifecycle
}

fn submit(
    pic: &PocketIc,
    governance: Principal,
    request: UpgradeProposalRequest,
) -> UpgradeProposal {
    decode_one(&update(
        pic,
        governance,
        Principal::anonymous(),
        "debug_submit_upgrade_proposal",
        encode_one(request).unwrap(),
    ))
    .unwrap()
}

fn execute_same_schema_upgrade(fixture: &Fixture, artifact: &str, target: Principal) {
    let before = fixture
        .pic
        .canister_status(target, Some(fixture.root))
        .unwrap()
        .module_hash
        .unwrap();
    let before_hex = hex_encode(&before);
    update(
        &fixture.pic,
        fixture.root,
        Principal::anonymous(),
        "debug_record_expected_module_hash",
        encode_one(ExpectedModuleHashRequest {
            target_canister: target,
            expected_module_hash: before_hex.clone(),
        })
        .unwrap(),
    );
    let current_wasm = required_wasm(&format!(
        "target/wasm32-unknown-unknown/debug/{artifact}.wasm"
    ));
    let current_hash = hex_encode(Sha256::digest(&current_wasm));
    assert_eq!(before_hex, current_hash);
    let request = UpgradeProposalRequest {
        target_canister: target,
        wasm_sha256: current_hash.clone(),
        wasm_gz_sha256: current_hash,
        artifact_name: artifact.to_string(),
        artifact_path: format!("debug/{artifact}.wasm"),
        expected_module_hash: Some(before_hex),
    };
    let expected_raw_hash = request.wasm_sha256.clone();
    let proposal = submit(&fixture.pic, fixture.governance, request);
    let _: Result<UpgradeProposal, String> = decode_one(&update(
        &fixture.pic,
        fixture.governance,
        Principal::anonymous(),
        "debug_vote_proposal",
        encode_one((proposal.proposal_id, UpgradeVote::Yes)).unwrap(),
    ))
    .unwrap();
    let adopted: Result<UpgradeProposal, String> = decode_one(&update(
        &fixture.pic,
        fixture.governance,
        Principal::anonymous(),
        "debug_adopt_upgrade_proposal",
        encode_one(proposal.proposal_id).unwrap(),
    ))
    .unwrap();
    assert_eq!(adopted.unwrap().status, UpgradeProposalStatus::Adopted);
    let intent: Result<RootUpgradeIntent, String> = decode_one(&update(
        &fixture.pic,
        fixture.governance,
        Principal::anonymous(),
        "debug_finalize_proposal",
        encode_one(proposal.proposal_id).unwrap(),
    ))
    .unwrap();
    let intent = intent.unwrap();
    assert_eq!(intent.wasm_sha256, expected_raw_hash);

    fixture
        .pic
        .upgrade_canister(
            target,
            current_wasm,
            encode_one(()).unwrap(),
            Some(fixture.root),
        )
        .unwrap();
    let outcome: Result<(), String> = decode_one(&update(
        &fixture.pic,
        fixture.root,
        Principal::anonymous(),
        "debug_record_upgrade_outcome",
        encode_one(RootUpgradeOutcomeRequest {
            attempt_id: intent.attempt_id,
            success: true,
            failure_reason: None,
        })
        .unwrap(),
    ))
    .unwrap();
    outcome.unwrap();

    let after = fixture
        .pic
        .canister_status(target, Some(fixture.root))
        .unwrap()
        .module_hash
        .unwrap();
    assert_eq!(before, after);
    assert_eq!(hex_encode(after), expected_raw_hash);
    assert_eq!(fixture.pic.get_controllers(target), vec![fixture.root]);
    assert_eq!(lifecycle(&fixture.pic, target), Lifecycle::Paused);
    let listed: Option<UpgradeProposal> = decode_one(
        &fixture
            .pic
            .query_call(
                fixture.governance,
                Principal::anonymous(),
                "debug_get_upgrade_proposal",
                encode_one(proposal.proposal_id).unwrap(),
            )
            .unwrap(),
    )
    .unwrap();
    assert_eq!(listed.unwrap().status, UpgradeProposalStatus::Executed);
    let history: Vec<RootUpgradeAttempt> = decode_one(
        &fixture
            .pic
            .query_call(
                fixture.root,
                Principal::anonymous(),
                "debug_get_upgrade_history",
                encode_one(()).unwrap(),
            )
            .unwrap(),
    )
    .unwrap();
    assert!(history.iter().any(|attempt| {
        attempt.proposal_id == proposal.proposal_id
            && attempt.status == RootUpgradeAttemptStatus::Succeeded
    }));
}

#[test]
fn pocketic_sns_root_preserves_both_current_launch_states_on_same_schema_upgrade() {
    if std::env::var_os("POCKET_IC_BIN").is_none() {
        eprintln!("skipping SNS Root lifecycle PocketIC test because POCKET_IC_BIN is not set");
        return;
    }
    let fixture = setup();
    assert_eq!(lifecycle(&fixture.pic, fixture.stream), Lifecycle::Paused);
    assert_eq!(
        lifecycle(&fixture.pic, fixture.nns_manager),
        Lifecycle::Paused
    );
    execute_same_schema_upgrade(&fixture, "io_stream_manager", fixture.stream);
    execute_same_schema_upgrade(&fixture, "io_nns_neuron_manager", fixture.nns_manager);
}

#[test]
fn pocketic_sns_root_rejects_unknown_dapp_and_unauthorized_caller() {
    if std::env::var_os("POCKET_IC_BIN").is_none() {
        eprintln!("skipping SNS Root lifecycle PocketIC test because POCKET_IC_BIN is not set");
        return;
    }
    let fixture = setup();
    let before = fixture
        .pic
        .canister_status(fixture.stream, Some(fixture.root))
        .unwrap()
        .module_hash
        .unwrap();
    let current_hash = hex_encode(Sha256::digest(required_wasm(
        "target/wasm32-unknown-unknown/debug/io_stream_manager.wasm",
    )));
    let proposal = submit(
        &fixture.pic,
        fixture.governance,
        UpgradeProposalRequest {
            target_canister: Principal::from_slice(&[42; 29]),
            wasm_sha256: current_hash.clone(),
            wasm_gz_sha256: current_hash,
            artifact_name: "io_stream_manager".to_string(),
            artifact_path: "debug/io_stream_manager.wasm".to_string(),
            expected_module_hash: Some(hex_encode(before)),
        },
    );
    let _: Result<UpgradeProposal, String> = decode_one(&update(
        &fixture.pic,
        fixture.governance,
        Principal::anonymous(),
        "debug_vote_proposal",
        encode_one((proposal.proposal_id, UpgradeVote::Yes)).unwrap(),
    ))
    .unwrap();
    let _: Result<UpgradeProposal, String> = decode_one(&update(
        &fixture.pic,
        fixture.governance,
        Principal::anonymous(),
        "debug_adopt_upgrade_proposal",
        encode_one(proposal.proposal_id).unwrap(),
    ))
    .unwrap();
    let failed: Result<RootUpgradeIntent, String> = decode_one(&update(
        &fixture.pic,
        fixture.governance,
        Principal::anonymous(),
        "debug_finalize_proposal",
        encode_one(proposal.proposal_id).unwrap(),
    ))
    .unwrap();
    assert!(failed.unwrap_err().contains("unknown dapp canister"));

    let unauthorized: Result<RootUpgradeIntent, String> = decode_one(&update(
        &fixture.pic,
        fixture.root,
        Principal::anonymous(),
        "debug_upgrade_dapp_canister",
        encode_one(RootUpgradeRequest {
            proposal_id: 99,
            target_canister: fixture.stream,
            wasm_sha256: "raw".to_string(),
            wasm_gz_sha256: "gzip".to_string(),
            artifact_name: "io_stream_manager".to_string(),
            artifact_path: "release-artifacts/io_stream_manager.wasm".to_string(),
            expected_module_hash: None,
        })
        .unwrap(),
    ))
    .unwrap();
    assert!(unauthorized.unwrap_err().contains("unauthorized caller"));
}
