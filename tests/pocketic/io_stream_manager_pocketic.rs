use candid::{decode_one, encode_one, CandidType, Principal};
use io_stream_manager::{
    Account, ApiError, InitArgs, Lifecycle, NeuronRefreshStatus, ReceiptKind, RedeemArgs,
    RedemptionProgress, RewardBackingProgress, RewardEventObservation, Status, StreamConfig,
};
use pocket_ic::PocketIc;
use serde::Deserialize;
use std::time::Duration;

const CYCLES: u128 = 2_000_000_000_000;

#[derive(Clone, Debug, CandidType, Deserialize)]
struct DebugMintAccountArgs {
    to: Account,
    amount_e8s: u128,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct MockSnsNeuron {
    neuron_id: u64,
    staked_io_e8s: u128,
    dissolve_delay_seconds: u64,
    eligible_closed_proposals: u64,
    voted_closed_proposals: u64,
    is_genesis_governance_neuron: bool,
    is_protocol_owned: bool,
    is_dissolving: bool,
}

#[derive(Clone, Copy, Debug, CandidType, Deserialize)]
struct SnsUint128 {
    high: u64,
    low: u64,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct LatestRewardEventFixture {
    round: u64,
    rounds_since_last_distribution: u64,
    end_timestamp_seconds: u64,
    settled_proposal_ids: Vec<u64>,
    neuron_reward_shares: Vec<(u64, SnsUint128)>,
}

#[derive(Clone, Copy, Debug, CandidType, Deserialize)]
enum ClaimOrRefreshMode {
    Normal,
    GovernanceError,
    MissingCommand,
    MissingNeuronId,
    WrongNeuronId,
    Trap,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, CandidType, Deserialize)]
struct GovernanceCallCounters {
    latest_reward_event: u64,
    list_neurons: u64,
    nervous_system_parameters: u64,
    manage_neuron: u64,
}

#[derive(Clone, Copy, Debug, Default, CandidType, Deserialize)]
struct LedgerCallCounters {
    transfer: u64,
    transfer_from: u64,
}

fn debug_wasm(name: &str) -> Vec<u8> {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../../target/wasm32-unknown-unknown/debug/{name}.wasm"
        )),
    )
    .unwrap_or_else(|error| panic!("missing {name} debug Wasm: {error}"))
}

fn install(pic: &PocketIc, name: &str) -> Principal {
    let canister = pic.create_canister();
    pic.add_cycles(canister, CYCLES);
    pic.install_canister(canister, debug_wasm(name), Vec::new(), None);
    canister
}

fn update<A: CandidType, R: for<'de> Deserialize<'de> + CandidType>(
    pic: &PocketIc,
    canister: Principal,
    caller: Principal,
    method: &str,
    arg: A,
) -> R {
    decode_one(
        &pic.update_call(canister, caller, method, encode_one(arg).unwrap())
            .unwrap_or_else(|error| panic!("{method}: {error}")),
    )
    .unwrap()
}

fn query<R: for<'de> Deserialize<'de> + CandidType>(
    pic: &PocketIc,
    canister: Principal,
    method: &str,
) -> R {
    decode_one(
        &pic.query_call(
            canister,
            Principal::anonymous(),
            method,
            encode_one(()).unwrap(),
        )
        .unwrap(),
    )
    .unwrap()
}

#[test]
fn simplified_stream_installs_paused_and_rejects_anonymous_before_funds_move() {
    if std::env::var_os("POCKET_IC_BIN").is_none() {
        eprintln!("skipping stream-manager PocketIC test because POCKET_IC_BIN is not set");
        return;
    }
    let wasm_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/wasm32-unknown-unknown/debug/io_stream_manager.wasm");
    let wasm = match std::fs::read(wasm_path) {
        Ok(wasm) => wasm,
        Err(_) => {
            eprintln!("skipping stream-manager PocketIC test because debug Wasm is missing");
            return;
        }
    };
    let pic = PocketIc::new();
    let canister = pic.create_canister();
    pic.add_cycles(canister, CYCLES);
    let io_ledger = Principal::from_slice(&[1; 29]);
    let icp_ledger = Principal::from_slice(&[4; 29]);
    let manager = Principal::from_slice(&[2; 29]);
    let governance = Principal::from_slice(&[3; 29]);
    let account = Account {
        owner: canister,
        subaccount: None,
    };
    pic.install_canister(
        canister,
        wasm.clone(),
        encode_one(InitArgs {
            config: StreamConfig {
                io_ledger,
                icp_ledger,
                nns_manager: manager,
                jupiter_receipt_source: Account {
                    owner: manager,
                    subaccount: Some(vec![7; 32]),
                },
                two_week_receipt_source: Account {
                    owner: manager,
                    subaccount: Some(vec![8; 32]),
                },
                jupiter_io_account: Account {
                    owner: manager,
                    subaccount: Some(vec![9; 32]),
                },
                sns_governance: governance,
                sns_root: Principal::from_slice(&[6; 29]),
                expected_sns_governance_module_hash: vec![0; 32],
                approved_reward_event_duration_seconds: 86_400,
                io_reserve: account.clone(),
                liquid_icp: Account {
                    owner: canister,
                    subaccount: Some(vec![1; 32]),
                },
                excluded_io_accounts: Vec::new(),
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
    let status: Status = decode_one(
        &pic.query_call(
            canister,
            Principal::anonymous(),
            "get_status",
            encode_one(()).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(status.lifecycle, Lifecycle::Paused);
    assert!(status.operation_kind.is_none());
    let rendered: Result<String, String> = decode_one(
        &pic.query_call(
            canister,
            Principal::anonymous(),
            "validate_set_paused",
            encode_one(false).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(rendered.unwrap(), "Set IO stream paused: false");
    assert!(pic
        .query_call(
            canister,
            Principal::anonymous(),
            "validate_set_paused",
            encode_one(()).unwrap(),
        )
        .is_err());
    pic.upgrade_canister(canister, wasm, encode_one(()).unwrap(), None)
        .unwrap();
    let upgraded: Status = decode_one(
        &pic.query_call(
            canister,
            Principal::anonymous(),
            "get_status",
            encode_one(()).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(upgraded.lifecycle, Lifecycle::Paused);
    let rendered_after_upgrade: Result<String, String> = decode_one(
        &pic.query_call(
            canister,
            Principal::anonymous(),
            "validate_set_paused",
            encode_one(true).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        rendered_after_upgrade.unwrap(),
        "Set IO stream paused: true"
    );
    let result: Result<RedemptionProgress, ApiError> = decode_one(
        &pic.update_call(
            canister,
            Principal::anonymous(),
            "redeem",
            encode_one(RedeemArgs {
                from_subaccount: None,
                io_amount_e8s: 1,
                min_icp_out_e8s: 1,
                max_io_fee_e8s: 10_000,
                max_icp_fee_e8s: 10_000,
                expires_at_nanos: u64::MAX,
                nonce: 0,
            })
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(result, Err(ApiError::Anonymous));
}

#[test]
fn reward_observation_backing_and_refresh_retries_are_bounded_and_monetary_once() {
    if std::env::var_os("POCKET_IC_BIN").is_none() {
        eprintln!("skipping Stream liveness PocketIC test because POCKET_IC_BIN is not set");
        return;
    }
    let pic = PocketIc::new();
    let io_ledger = install(&pic, "mock_io_ledger");
    let icp_ledger = install(&pic, "mock_icp_ledger");
    let root = install(&pic, "mock_sns_root");
    let governance = install(&pic, "mock_sns_governance");
    let nns = install(&pic, "mock_nns_governance");
    let governance_hash = pic
        .canister_status(governance, None)
        .unwrap()
        .module_hash
        .unwrap();
    let _: () = update(
        &pic,
        root,
        Principal::anonymous(),
        "debug_set_governance_principal",
        governance,
    );
    let configured_hash: Result<(), String> = update(
        &pic,
        root,
        Principal::anonymous(),
        "debug_set_governance_module_hash",
        governance_hash.clone(),
    );
    configured_hash.unwrap();
    let _: () = update(
        &pic,
        governance,
        Principal::anonymous(),
        "debug_set_io_ledger_principal",
        io_ledger,
    );
    for id in 1_u64..=6 {
        let _: () = update(
            &pic,
            governance,
            Principal::anonymous(),
            "debug_add_neuron",
            MockSnsNeuron {
                neuron_id: id,
                staked_io_e8s: 30_000_000,
                dissolve_delay_seconds: 1_209_600,
                eligible_closed_proposals: 1,
                voted_closed_proposals: 1,
                is_genesis_governance_neuron: false,
                is_protocol_owned: false,
                is_dissolving: false,
            },
        );
    }
    let baseline_end = pic.get_time().as_nanos_since_unix_epoch() / 1_000_000_000;
    let baseline: Result<(), String> = update(
        &pic,
        governance,
        Principal::anonymous(),
        "debug_set_latest_reward_event",
        LatestRewardEventFixture {
            round: 1,
            rounds_since_last_distribution: 1,
            end_timestamp_seconds: baseline_end,
            settled_proposal_ids: Vec::new(),
            neuron_reward_shares: Vec::new(),
        },
    );
    baseline.unwrap();

    let stream = pic.create_canister();
    pic.add_cycles(stream, CYCLES);
    let reserve = Account {
        owner: stream,
        subaccount: None,
    };
    let liquid = Account {
        owner: stream,
        subaccount: Some(vec![1; 32]),
    };
    let reward_source = Account {
        owner: nns,
        subaccount: Some(vec![8; 32]),
    };
    for (ledger, to, amount) in [
        (io_ledger, reserve.clone(), 800_000_000_u128),
        (
            io_ledger,
            Account {
                owner: Principal::from_slice(&[99; 29]),
                subaccount: None,
            },
            200_000_000,
        ),
        (icp_ledger, liquid.clone(), 1_000_000_000),
        (icp_ledger, reward_source.clone(), 200_000_000),
    ] {
        let _: u64 = update(
            &pic,
            ledger,
            Principal::anonymous(),
            "debug_mint_account",
            DebugMintAccountArgs {
                to,
                amount_e8s: amount,
            },
        );
    }
    pic.install_canister(
        stream,
        debug_wasm("io_stream_manager"),
        encode_one(InitArgs {
            config: StreamConfig {
                io_ledger,
                icp_ledger,
                nns_manager: nns,
                jupiter_receipt_source: Account {
                    owner: nns,
                    subaccount: Some(vec![7; 32]),
                },
                two_week_receipt_source: reward_source,
                jupiter_io_account: Account {
                    owner: Principal::from_slice(&[77; 29]),
                    subaccount: None,
                },
                sns_governance: governance,
                sns_root: root,
                expected_sns_governance_module_hash: governance_hash,
                approved_reward_event_duration_seconds: 86_400,
                io_reserve: reserve,
                liquid_icp: liquid,
                excluded_io_accounts: Vec::new(),
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
    let unpaused: Result<(), ApiError> = update(&pic, stream, governance, "set_paused", false);
    unpaused.unwrap();
    let initial: Status = query(&pic, stream, "get_status");
    assert!(!initial.reward_work_due);

    let governance_before: GovernanceCallCounters =
        query(&pic, governance, "debug_get_call_counters");
    let root_before: u64 = query(&pic, root, "debug_get_summary_call_count");
    let premature: Result<RewardEventObservation, ApiError> = update(
        &pic,
        stream,
        Principal::anonymous(),
        "resume_reward_work",
        (),
    );
    assert!(matches!(premature, Err(ApiError::Pending(_))));
    assert_eq!(
        query::<GovernanceCallCounters>(&pic, governance, "debug_get_call_counters"),
        governance_before
    );
    assert_eq!(
        query::<u64>(&pic, root, "debug_get_summary_call_count"),
        root_before
    );

    pic.advance_time(Duration::from_secs(86_701));
    for _ in 0..3 {
        pic.tick();
    }
    assert!(query::<Status>(&pic, stream, "get_status").reward_work_due);
    let after_wait: GovernanceCallCounters = query(&pic, governance, "debug_get_call_counters");
    let root_after_wait: u64 = query(&pic, root, "debug_get_summary_call_count");
    assert!(after_wait.latest_reward_event > governance_before.latest_reward_event);
    assert!(root_after_wait > root_before);
    let cooled: Result<RewardEventObservation, ApiError> = update(
        &pic,
        stream,
        Principal::anonymous(),
        "resume_reward_work",
        (),
    );
    assert!(matches!(cooled, Err(ApiError::Pending(message)) if message.contains("cooling down")));
    assert_eq!(
        query::<GovernanceCallCounters>(&pic, governance, "debug_get_call_counters"),
        after_wait
    );
    assert_eq!(
        query::<u64>(&pic, root, "debug_get_summary_call_count"),
        root_after_wait
    );

    let advanced: Result<(), String> = update(
        &pic,
        governance,
        Principal::anonymous(),
        "debug_set_latest_reward_event",
        LatestRewardEventFixture {
            round: 2,
            rounds_since_last_distribution: 1,
            end_timestamp_seconds: baseline_end + 86_400,
            settled_proposal_ids: vec![1],
            neuron_reward_shares: (1_u64..=6)
                .map(|id| (id, SnsUint128 { high: 0, low: 1 }))
                .collect(),
        },
    );
    advanced.unwrap();
    pic.advance_time(Duration::from_secs(61));
    let observed: Result<RewardEventObservation, ApiError> = update(
        &pic,
        stream,
        Principal::anonymous(),
        "resume_reward_work",
        (),
    );
    observed.unwrap();
    assert!(!query::<Status>(&pic, stream, "get_status").reward_work_due);

    let _: () = update(
        &pic,
        nns,
        Principal::anonymous(),
        "debug_set_backing_readiness",
        io_receipt_types::TwoWeekBackingReadiness::NotReady(
            io_receipt_types::BackingNotReadyReason::UnderTarget,
        ),
    );
    let pending: Result<RewardBackingProgress, ApiError> = update(
        &pic,
        stream,
        Principal::anonymous(),
        "resume_reward_backing",
        (),
    );
    assert!(
        matches!(pending, Ok(RewardBackingProgress::Pending { .. })),
        "unexpected first backing result: {pending:?}"
    );
    let reconcile_calls: u64 = query(&pic, nns, "debug_get_reconcile_call_count");
    let cooled: Result<RewardBackingProgress, ApiError> = update(
        &pic,
        stream,
        Principal::anonymous(),
        "resume_reward_backing",
        (),
    );
    assert!(matches!(cooled, Err(ApiError::Pending(message)) if message.contains("cooling down")));
    assert_eq!(
        query::<u64>(&pic, nns, "debug_get_reconcile_call_count"),
        reconcile_calls
    );

    pic.advance_time(Duration::from_secs(61));
    let _: () = update(
        &pic,
        nns,
        Principal::anonymous(),
        "debug_set_backing_readiness",
        io_receipt_types::TwoWeekBackingReadiness::Ready {
            target_status: io_receipt_types::BackingTargetStatus::AtTarget,
            ordinary_maturity_e8s: 200_000_000,
            retained_maturity_e8s: 80_000_000,
            liquid_maturity_e8s: 120_000_000,
            minimum_disbursement_e8s: 100_000_000,
        },
    );
    let prepared: Result<RewardBackingProgress, ApiError> = update(
        &pic,
        stream,
        Principal::anonymous(),
        "resume_reward_backing",
        (),
    );
    assert_eq!(
        prepared,
        Ok(RewardBackingProgress::MaturityPrepared { generation: 1 })
    );

    let request = io_stream_manager::PrepareLiquidReceiptArgs {
        receipt_sequence: 0,
        receipt_kind: ReceiptKind::TwoWeekMaturity,
        source_operation_id: vec![4; 32],
        liquid_amount_e8s: 100_000_000,
        entitlement_batch_generation: Some(1),
    };
    let permit: Result<io_stream_manager::LiquidReceiptPermit, ApiError> =
        update(&pic, stream, nns, "prepare_liquid_receipt", request);
    let permit = permit.unwrap();
    let receipt_block: io_ledger_boundary::IcrcTransferResult = update(
        &pic,
        icp_ledger,
        nns,
        "icrc1_transfer",
        io_ledger_boundary::IcrcTransferArg {
            from_subaccount: Some(vec![8; 32]),
            to: permit.destination.clone(),
            amount: candid::Nat::from(100_000_000_u64),
            fee: Some(candid::Nat::from(10_000_u64)),
            memo: Some(permit.memo.clone()),
            created_at_time: Some(1),
        },
    );
    let receipt_block: u128 = receipt_block.unwrap().0.try_into().unwrap();
    let completed: Result<io_stream_manager::LiquidReceiptProgress, ApiError> = update(
        &pic,
        stream,
        nns,
        "complete_liquid_receipt",
        io_stream_manager::CompleteLiquidReceiptArgs {
            receipt_sequence: 0,
            block_index: receipt_block,
        },
    );
    assert_eq!(
        completed,
        Ok(io_stream_manager::LiquidReceiptProgress::ReceiptProved)
    );

    let transfers_before: LedgerCallCounters = query(&pic, io_ledger, "debug_get_call_counters");
    for _ in 0..50 {
        if query::<Status>(&pic, stream, "get_status")
            .operation_kind
            .is_none()
        {
            break;
        }
        let delivered = query::<LedgerCallCounters>(&pic, io_ledger, "debug_get_call_counters")
            .transfer
            - transfers_before.transfer;
        let mode = match delivered {
            0..=2 => ClaimOrRefreshMode::Trap,
            3 => ClaimOrRefreshMode::GovernanceError,
            4 => ClaimOrRefreshMode::MissingCommand,
            5 => ClaimOrRefreshMode::MissingNeuronId,
            _ => ClaimOrRefreshMode::WrongNeuronId,
        };
        let _: () = update(
            &pic,
            governance,
            Principal::anonymous(),
            "debug_set_claim_or_refresh_mode",
            mode,
        );
        let _: Result<io_stream_manager::StreamProgress, ApiError> =
            update(&pic, stream, Principal::anonymous(), "resume", ());
    }
    let settled = query::<Status>(&pic, stream, "get_status");
    assert!(settled.operation_kind.is_none());
    assert_eq!(settled.pending_neuron_refresh_count, 6);
    let after_settlement: LedgerCallCounters = query(&pic, io_ledger, "debug_get_call_counters");
    assert_eq!(after_settlement.transfer, transfers_before.transfer + 6);

    pic.upgrade_canister(
        stream,
        debug_wasm("io_stream_manager"),
        encode_one(()).unwrap(),
        None,
    )
    .unwrap();
    let upgraded = query::<Status>(&pic, stream, "get_status");
    assert_eq!(upgraded.lifecycle, Lifecycle::Paused);
    assert_eq!(upgraded.pending_neuron_refresh_count, 6);
    let oldest_before = upgraded
        .oldest_pending_neuron_refresh
        .unwrap()
        .sns_neuron_id;
    let unpaused: Result<(), ApiError> = update(&pic, stream, governance, "set_paused", false);
    unpaused.unwrap();
    let _: () = update(
        &pic,
        governance,
        Principal::anonymous(),
        "debug_set_claim_or_refresh_mode",
        ClaimOrRefreshMode::Trap,
    );

    let failed_retry: Result<NeuronRefreshStatus, ApiError> = update(
        &pic,
        stream,
        Principal::anonymous(),
        "retry_neuron_refresh",
        (),
    );
    assert!(matches!(
        failed_retry,
        Ok(NeuronRefreshStatus::TransportFailure { .. })
    ));
    let rotated = query::<Status>(&pic, stream, "get_status");
    assert_eq!(rotated.pending_neuron_refresh_count, 6);
    assert_ne!(
        rotated.oldest_pending_neuron_refresh.unwrap().sns_neuron_id,
        oldest_before
    );
    pic.advance_time(Duration::from_secs(61));
    let _: () = update(
        &pic,
        governance,
        Principal::anonymous(),
        "debug_set_claim_or_refresh_mode",
        ClaimOrRefreshMode::Normal,
    );
    let succeeded_retry: Result<NeuronRefreshStatus, ApiError> = update(
        &pic,
        stream,
        Principal::anonymous(),
        "retry_neuron_refresh",
        (),
    );
    assert_eq!(succeeded_retry, Ok(NeuronRefreshStatus::Confirmed));
    let final_status = query::<Status>(&pic, stream, "get_status");
    assert_eq!(final_status.pending_neuron_refresh_count, 5);
    let transfers_after_retry: LedgerCallCounters =
        query(&pic, io_ledger, "debug_get_call_counters");
    assert_eq!(transfers_after_retry.transfer, after_settlement.transfer);
    assert_eq!(
        transfers_after_retry.transfer_from,
        after_settlement.transfer_from
    );

    let redemption_expiry = pic
        .get_time()
        .as_nanos_since_unix_epoch()
        .checked_add(600_000_000_000)
        .unwrap();
    let redemption: Result<RedemptionProgress, ApiError> = update(
        &pic,
        stream,
        Principal::from_slice(&[99; 29]),
        "redeem",
        RedeemArgs {
            from_subaccount: None,
            io_amount_e8s: 100_000,
            min_icp_out_e8s: 1,
            max_io_fee_e8s: 10_000,
            max_icp_fee_e8s: 10_000,
            expires_at_nanos: redemption_expiry,
            nonce: 0,
        },
    );
    assert!(
        redemption.is_ok(),
        "unexpected redemption result: {redemption:?}"
    );
    let icp_before_drift: LedgerCallCounters = query(&pic, icp_ledger, "debug_get_call_counters");
    let _: u64 = update(
        &pic,
        io_ledger,
        Principal::anonymous(),
        "debug_mint_account",
        DebugMintAccountArgs {
            to: Account {
                owner: Principal::from_slice(&[55; 29]),
                subaccount: None,
            },
            amount_e8s: 1,
        },
    );
    let adverse: Result<io_stream_manager::StreamProgress, ApiError> =
        update(&pic, stream, Principal::anonymous(), "resume", ());
    assert!(matches!(adverse, Err(ApiError::Stuck(message)) if message.contains("supply")));
    let icp_after_drift: LedgerCallCounters = query(&pic, icp_ledger, "debug_get_call_counters");
    assert_eq!(icp_after_drift.transfer, icp_before_drift.transfer);
    assert_eq!(
        query::<Status>(&pic, stream, "get_status").lifecycle,
        Lifecycle::Paused
    );
}
