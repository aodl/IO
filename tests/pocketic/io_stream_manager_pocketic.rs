use candid::{decode_one, encode_one, CandidType, Principal};
use io_stream_manager::{
    redemption::{RedemptionOperation, RedemptionPhase},
    state::{RedemptionStreamOperation, StreamOperation},
    transfer::{deterministic_memo, OwnTransferIntent, TransferAttempt, TransferState},
    Account, ApiError, InitArgs, Lifecycle, RedemptionProgress, RewardEventClassification,
    RewardEventObservation, Status, StreamConfig, StreamProgress, StreamStateV1,
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
struct MockIndexInitArgs {
    ledger_principal_text: Option<String>,
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, CandidType, Deserialize)]
struct GovernanceCallCounters {
    latest_reward_event: u64,
    list_neurons: u64,
    nervous_system_parameters: u64,
    manage_neuron: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, CandidType, Deserialize)]
struct LedgerCallCounters {
    fee: u64,
    total_supply: u64,
    balance: u64,
    transfer: u64,
    query_blocks: u64,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct DebugRejectAccountArgs {
    account: String,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct DebugLedgerTransaction {
    block_index: u64,
    from_account: Option<Account>,
    to_account: Option<Account>,
    amount_e8s: u128,
    fee_e8s: Option<u128>,
    memo_bytes: Option<Vec<u8>>,
}

#[derive(Clone, Copy, Debug, CandidType, Deserialize)]
struct DebugSchedulerStatus {
    active_deadline_seconds: Option<u64>,
    callback_invocations: u64,
    recovery_deferrals: u64,
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

fn nat_u128(value: candid::Nat) -> u128 {
    value.0.to_str_radix(10).parse().unwrap()
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
                io_index: Principal::from_slice(&[8; 29]),
                icp_ledger,
                nns_manager: manager,
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
                nonredeemable_governance_io_accounts: Vec::new(),
                minimum_redemption_io_e8s: 20_000,
                expected_io_fee_e8s: 10_000,
                expected_icp_fee_e8s: 10_000,
                redemption_poll_interval_seconds: 60,
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
    let rendered = rendered.unwrap();
    assert!(rendered.contains("Set IO stream paused: false"));
    assert!(rendered.contains("Current lifecycle: Paused"));
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
    let rendered_after_upgrade = rendered_after_upgrade.unwrap();
    assert!(rendered_after_upgrade.contains("Set IO stream paused: true"));
    assert!(rendered_after_upgrade.contains("Current lifecycle: Paused"));
    let unauthorized: Result<(), ApiError> = decode_one(
        &pic.update_call(
            canister,
            Principal::anonymous(),
            "set_paused",
            encode_one(false).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(unauthorized, Err(ApiError::Unauthorized));
    let rejected = pic
        .update_call(
            canister,
            governance,
            "set_paused",
            encode_one(false).unwrap(),
        )
        .expect_err("SNS readiness with unavailable dependencies must reject");
    assert!(format!("{rejected:?}").contains("stream lifecycle action not accepted"));
    let still_paused: Status = decode_one(
        &pic.query_call(
            canister,
            Principal::anonymous(),
            "get_status",
            encode_one(()).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(still_paused.lifecycle, Lifecycle::Paused);
    let result: Result<RedemptionProgress, ApiError> = decode_one(
        &pic.update_call(
            canister,
            Principal::anonymous(),
            "process_redemptions",
            encode_one(()).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(result, Err(ApiError::Paused));

    let now_nanos = pic.get_time().as_nanos_since_unix_epoch();
    let mut paused_due: StreamStateV1 = query(&pic, canister, "debug_get_state");
    paused_due.lifecycle = Lifecycle::Ready;
    paused_due.reward_checkpoint.reward_processing_paused = true;
    paused_due.reward_checkpoint.reward_work_due = false;
    paused_due.stake_observation_due = false;
    paused_due.structural_reconciliation_due = true;
    paused_due.last_redemption_poll_started_at_nanos = now_nanos.saturating_sub(100_000_000_000);
    update::<_, Result<(), String>>(
        &pic,
        canister,
        Principal::anonymous(),
        "debug_replace_state",
        paused_due,
    )
    .unwrap();
    update::<_, ()>(
        &pic,
        canister,
        Principal::anonymous(),
        "debug_install_scheduler",
        (),
    );
    let installed: DebugSchedulerStatus = query(&pic, canister, "debug_get_scheduler_status");
    let deadline = installed.active_deadline_seconds.unwrap();
    let now_seconds = now_nanos / 1_000_000_000;
    assert!(deadline > now_seconds);
    let seconds_until_deadline = deadline - now_seconds;
    pic.advance_time(Duration::from_secs(seconds_until_deadline - 1));
    for _ in 0..3 {
        pic.tick();
    }
    assert_eq!(
        query::<DebugSchedulerStatus>(&pic, canister, "debug_get_scheduler_status")
            .callback_invocations,
        installed.callback_invocations
    );
    pic.advance_time(Duration::from_secs(1));
    for _ in 0..3 {
        pic.tick();
    }
    let rearmed: DebugSchedulerStatus = query(&pic, canister, "debug_get_scheduler_status");
    assert_eq!(
        rearmed.callback_invocations,
        installed.callback_invocations + 1
    );
    assert_eq!(rearmed.recovery_deferrals, installed.recovery_deferrals);
    assert!(rearmed.active_deadline_seconds.unwrap() > deadline);
    for _ in 0..3 {
        pic.tick();
    }
    assert_eq!(
        query::<DebugSchedulerStatus>(&pic, canister, "debug_get_scheduler_status")
            .callback_invocations,
        rearmed.callback_invocations,
        "paused retained due work must not rearm a zero-delay timer"
    );

    let mut fully_paused: StreamStateV1 = query(&pic, canister, "debug_get_state");
    fully_paused.lifecycle = Lifecycle::Paused;
    update::<_, Result<(), String>>(
        &pic,
        canister,
        Principal::anonymous(),
        "debug_replace_state",
        fully_paused,
    )
    .unwrap();
    update::<_, ()>(
        &pic,
        canister,
        Principal::anonymous(),
        "debug_install_scheduler",
        (),
    );
    assert_eq!(
        query::<DebugSchedulerStatus>(&pic, canister, "debug_get_scheduler_status")
            .active_deadline_seconds,
        None
    );
}

#[test]
fn staging_waits_claim_bearing_for_liquidity_and_manual_work_is_globally_throttled() {
    if std::env::var_os("POCKET_IC_BIN").is_none() {
        eprintln!(
            "skipping Stream staged-redemption PocketIC test because POCKET_IC_BIN is not set"
        );
        return;
    }
    let pic = PocketIc::new();
    let io_ledger = install(&pic, "mock_io_ledger");
    let io_index = pic.create_canister();
    pic.add_cycles(io_index, CYCLES);
    pic.install_canister(
        io_index,
        debug_wasm("mock_io_index"),
        encode_one(MockIndexInitArgs {
            ledger_principal_text: Some(io_ledger.to_text()),
        })
        .unwrap(),
        None,
    );
    let icp_ledger = install(&pic, "mock_icp_ledger");
    let root = install(&pic, "mock_sns_root");
    let governance = install(&pic, "mock_sns_governance");
    let nns = install(&pic, "mock_nns_governance");
    let stream = pic.create_canister();
    pic.add_cycles(stream, CYCLES);
    let user = Principal::from_slice(&[88; 29]);
    let reserve = Account {
        owner: stream,
        subaccount: None,
    };
    let liquid = Account {
        owner: stream,
        subaccount: Some(vec![1; 32]),
    };
    let user_account = Account {
        owner: user,
        subaccount: None,
    };
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
    update::<_, Result<(), String>>(
        &pic,
        root,
        Principal::anonymous(),
        "debug_set_governance_module_hash",
        governance_hash.clone(),
    )
    .unwrap();
    for (ledger, to, amount) in [
        (io_ledger, reserve.clone(), 900_000_000_u128),
        (io_ledger, user_account.clone(), 100_030_000),
        (icp_ledger, liquid.clone(), 10_000),
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
    let _: () = update(
        &pic,
        governance,
        Principal::anonymous(),
        "debug_add_neuron",
        MockSnsNeuron {
            neuron_id: 99,
            staked_io_e8s: 10_000_000,
            dissolve_delay_seconds: io_core_model::SNS_USER_DISSOLVE_DELAY_SECONDS + 1,
            eligible_closed_proposals: 0,
            voted_closed_proposals: 0,
            is_genesis_governance_neuron: false,
            is_protocol_owned: false,
            is_dissolving: false,
        },
    );
    let _: () = update(
        &pic,
        nns,
        Principal::anonymous(),
        "debug_set_pooled_principal",
        100_020_000_u128,
    );
    pic.install_canister(
        stream,
        debug_wasm("io_stream_manager"),
        encode_one(InitArgs {
            config: StreamConfig {
                io_ledger,
                io_index,
                icp_ledger,
                nns_manager: nns,
                jupiter_io_account: Account {
                    owner: Principal::from_slice(&[77; 29]),
                    subaccount: None,
                },
                sns_governance: governance,
                sns_root: root,
                expected_sns_governance_module_hash: governance_hash,
                approved_reward_event_duration_seconds: 86_400,
                io_reserve: reserve.clone(),
                liquid_icp: liquid.clone(),
                nonredeemable_governance_io_accounts: Vec::new(),
                minimum_redemption_io_e8s: 20_000,
                expected_io_fee_e8s: 10_000,
                expected_icp_fee_e8s: 10_000,
                redemption_poll_interval_seconds: 60,
                retry_delay_nanos: 1_000_000_000,
                ledger_deduplication_window_nanos: 86_400_000_000_000,
            },
        })
        .unwrap(),
        None,
    );
    update::<_, Result<(), ApiError>>(&pic, stream, governance, "set_paused", false).unwrap();
    update::<_, Result<RewardEventObservation, ApiError>>(
        &pic,
        stream,
        Principal::anonymous(),
        "resume_reward_work",
        (),
    )
    .expect("genesis structural/reward safety work should complete before redemption");
    for _ in 0..16 {
        let current: StreamStateV1 = query(&pic, stream, "debug_get_state");
        if !current.reward_checkpoint.reward_work_due
            && !current.stake_observation_due
            && !current.structural_reconciliation_due
            && current.active_operation.is_none()
        {
            break;
        }
        pic.advance_time(Duration::from_secs(1));
        if current.active_operation.is_some() {
            let _: Result<StreamProgress, ApiError> =
                update(&pic, stream, Principal::anonymous(), "resume", ());
        } else {
            let _: Result<RewardEventObservation, ApiError> = update(
                &pic,
                stream,
                Principal::anonymous(),
                "resume_reward_work",
                (),
            );
        }
    }
    let safety_state: StreamStateV1 = query(&pic, stream, "debug_get_state");
    assert!(
        !safety_state.reward_checkpoint.reward_work_due
            && !safety_state.stake_observation_due
            && !safety_state.structural_reconciliation_due
            && safety_state.active_operation.is_none(),
        "redemption fixture must finish higher-priority safety work first: {safety_state:?}"
    );
    let staging: Account = query(&pic, stream, "get_redemption_staging_account");
    assert_eq!(
        query::<candid::Nat>(&pic, stream, "get_minimum_redemption_io_e8s"),
        candid::Nat::from(20_000_u64)
    );
    let supply_before = nat_u128(query::<candid::Nat>(&pic, io_ledger, "icrc1_total_supply"));
    let reserve_before = nat_u128(update::<_, candid::Nat>(
        &pic,
        io_ledger,
        Principal::anonymous(),
        "icrc1_balance_of",
        reserve.clone(),
    ));
    let tiny: io_ledger_boundary::IcrcTransferResult = update(
        &pic,
        io_ledger,
        user,
        "icrc1_transfer",
        io_ledger_boundary::IcrcTransferArg {
            from_subaccount: None,
            to: staging.clone(),
            amount: candid::Nat::from(10_000_u128),
            fee: Some(candid::Nat::from(10_000_u128)),
            memo: None,
            created_at_time: None,
        },
    );
    tiny.expect("below-minimum staging transfer should remain ordinary ledger traffic");
    let staged: io_ledger_boundary::IcrcTransferResult = update(
        &pic,
        io_ledger,
        user,
        "icrc1_transfer",
        io_ledger_boundary::IcrcTransferArg {
            from_subaccount: None,
            to: staging.clone(),
            amount: candid::Nat::from(100_000_000_u128),
            fee: Some(candid::Nat::from(10_000_u128)),
            memo: None,
            created_at_time: None,
        },
    );
    let staged_block: u128 = staged.unwrap().0.try_into().unwrap();
    let supply_after_stage = nat_u128(query::<candid::Nat>(&pic, io_ledger, "icrc1_total_supply"));
    let reserve_after_stage = nat_u128(update::<_, candid::Nat>(
        &pic,
        io_ledger,
        Principal::anonymous(),
        "icrc1_balance_of",
        reserve.clone(),
    ));
    assert_eq!(supply_after_stage, supply_before - 20_000);
    assert_eq!(reserve_after_stage, reserve_before);
    assert_eq!(
        supply_after_stage - reserve_after_stage,
        supply_before - reserve_before - 20_000,
        "staging is claim-bearing; only the user's two ordinary transfer fees leave C"
    );

    let payout_calls_before: LedgerCallCounters =
        query(&pic, icp_ledger, "debug_get_call_counters");
    let awaiting_liquidity: Result<RedemptionProgress, ApiError> =
        update(&pic, stream, user, "process_redemptions", ());
    assert_eq!(awaiting_liquidity, Ok(RedemptionProgress::Pending));
    let index_calls_after_discovery: u64 =
        query(&pic, io_index, "debug_get_account_transaction_call_count");
    let waiting = query::<Status>(&pic, stream, "get_status");
    assert_eq!(
        index_calls_after_discovery, 1,
        "one bounded scan must discover the pending candidate: {waiting:?}"
    );
    assert_eq!(waiting.lifecycle, Lifecycle::Ready);
    assert!(waiting.operation_kind.is_none());
    assert_eq!(waiting.pending_redemption_candidates, 1);
    assert_eq!(waiting.paid_unswept_redemption_io_e8s, Some(0));
    assert_eq!(
        query::<u64>(&pic, io_index, "debug_get_account_transaction_call_count"),
        index_calls_after_discovery,
        "the accepted discovery attempt performs exactly one index call"
    );
    assert_eq!(
        query::<LedgerCallCounters>(&pic, icp_ledger, "debug_get_call_counters").transfer,
        payout_calls_before.transfer,
        "illiquidity must not create or submit a payout obligation"
    );

    for _ in 0..100 {
        let throttled: Result<RedemptionProgress, ApiError> =
            update(&pic, stream, user, "process_redemptions", ());
        assert!(matches!(
            throttled,
            Ok(RedemptionProgress::RateLimited { .. })
        ));
    }
    assert_eq!(
        query::<u64>(&pic, io_index, "debug_get_account_transaction_call_count"),
        index_calls_after_discovery,
        "100 calls by one principal inside the global cooldown perform no index discovery"
    );

    for caller_number in 0..100_u8 {
        let caller = Principal::from_slice(&[caller_number.saturating_add(1); 29]);
        let throttled: Result<RedemptionProgress, ApiError> =
            update(&pic, stream, caller, "process_redemptions", ());
        assert!(matches!(
            throttled,
            Ok(RedemptionProgress::RateLimited { .. })
        ));
    }
    assert_eq!(
        query::<u64>(&pic, io_index, "debug_get_account_transaction_call_count"),
        index_calls_after_discovery,
        "100 distinct principals cannot bypass the global discovery cooldown"
    );
    assert_eq!(
        query::<LedgerCallCounters>(&pic, icp_ledger, "debug_get_call_counters").transfer,
        payout_calls_before.transfer,
        "Sybil callers inside the global cooldown perform no payout work"
    );

    let _: () = update(
        &pic,
        nns,
        Principal::anonymous(),
        "debug_set_pooled_principal",
        0_u128,
    );
    let _: u64 = update(
        &pic,
        icp_ledger,
        Principal::anonymous(),
        "debug_mint_account",
        DebugMintAccountArgs {
            to: liquid.clone(),
            amount_e8s: 100_000_000,
        },
    );
    pic.advance_time(Duration::from_secs(10));
    let mut completed = None;
    for _ in 0..20 {
        let progress = update::<_, Result<RedemptionProgress, ApiError>>(
            &pic,
            stream,
            Principal::anonymous(),
            "process_redemptions",
            (),
        );
        if let Ok(RedemptionProgress::Completed(result)) = progress {
            completed = Some(result);
            break;
        }
        let _: Result<StreamProgress, ApiError> =
            update(&pic, stream, Principal::anonymous(), "resume", ());
        pic.advance_time(Duration::from_secs(1));
        pic.tick();
    }
    let result = completed.unwrap_or_else(|| {
        let stalled: StreamStateV1 = query(&pic, stream, "debug_get_state");
        panic!("staged redemption must complete after exact liquidity arrives: {stalled:?}")
    });
    assert_eq!(result.source_io_block, staged_block);
    assert_eq!(result.source_account, user_account);
    assert_eq!(
        query::<LedgerCallCounters>(&pic, icp_ledger, "debug_get_call_counters").transfer,
        payout_calls_before.transfer + 1
    );
    let final_status: Status = query(&pic, stream, "get_status");
    assert_eq!(final_status.last_completed_redemption, Some(result.clone()));
    assert_eq!(final_status.pending_redemption_candidates, 0);
    assert_eq!(final_status.paid_unswept_redemption_io_e8s, Some(0));
    assert_eq!(
        nat_u128(update::<_, candid::Nat>(
            &pic,
            io_ledger,
            Principal::anonymous(),
            "icrc1_balance_of",
            staging.clone(),
        )),
        10_000,
        "the unsupported tiny transfer remains claim-bearing in staging without blocking the valid redemption"
    );
    assert_eq!(
        query::<LedgerCallCounters>(&pic, icp_ledger, "debug_get_call_counters").transfer,
        payout_calls_before.transfer + 1,
        "completed staging block must not repeat its payout"
    );

    let second_user = Principal::from_slice(&[89; 29]);
    let second_subaccount = vec![42; 32];
    let _: u64 = update(
        &pic,
        io_ledger,
        Principal::anonymous(),
        "debug_mint_account",
        DebugMintAccountArgs {
            to: Account {
                owner: second_user,
                subaccount: Some(second_subaccount.clone()),
            },
            amount_e8s: 50_010_000,
        },
    );
    let _: u64 = update(
        &pic,
        icp_ledger,
        Principal::anonymous(),
        "debug_mint_account",
        DebugMintAccountArgs {
            to: liquid,
            amount_e8s: 50_000_000,
        },
    );
    let _: io_ledger_boundary::IcrcTransferResult = update(
        &pic,
        io_ledger,
        second_user,
        "icrc1_transfer",
        io_ledger_boundary::IcrcTransferArg {
            from_subaccount: Some(second_subaccount.clone()),
            to: staging,
            amount: candid::Nat::from(50_000_000_u128),
            fee: Some(candid::Nat::from(10_000_u128)),
            memo: None,
            created_at_time: None,
        },
    );
    let installed_cadence: DebugSchedulerStatus = query(&pic, stream, "debug_get_scheduler_status");
    let now_seconds = pic.get_time().as_nanos_since_unix_epoch() / 1_000_000_000;
    let seconds_until_due = installed_cadence
        .active_deadline_seconds
        .expect("automatic redemption cadence timer is installed")
        .saturating_sub(now_seconds);
    assert!(seconds_until_due > 0);
    pic.advance_time(Duration::from_secs(seconds_until_due - 1));
    for _ in 0..3 {
        pic.tick();
    }
    assert_eq!(
        query::<Status>(&pic, stream, "get_status").last_completed_redemption,
        Some(result.clone()),
        "automatic discovery must not run before its configured deadline"
    );
    pic.advance_time(Duration::from_secs(1));
    for _ in 0..10 {
        pic.tick();
    }
    let automatic = query::<Status>(&pic, stream, "get_status")
        .last_completed_redemption
        .expect("the shared scheduler automatically processes the next staged transfer");
    assert_eq!(
        automatic.source_account,
        Account {
            owner: second_user,
            subaccount: Some(second_subaccount),
        }
    );

    let recovery_user = Principal::from_slice(&[90; 29]);
    let recovery_account = Account {
        owner: recovery_user,
        subaccount: None,
    };
    let _: u64 = update(
        &pic,
        io_ledger,
        Principal::anonymous(),
        "debug_mint_account",
        DebugMintAccountArgs {
            to: recovery_account.clone(),
            amount_e8s: 30_010_000,
        },
    );
    let _: u64 = update(
        &pic,
        icp_ledger,
        Principal::anonymous(),
        "debug_mint_account",
        DebugMintAccountArgs {
            to: Account {
                owner: stream,
                subaccount: Some(vec![1; 32]),
            },
            amount_e8s: 30_000_000,
        },
    );
    let recovery_staging: Account = query(&pic, stream, "get_redemption_staging_account");
    let staged_recovery: io_ledger_boundary::IcrcTransferResult = update(
        &pic,
        io_ledger,
        recovery_user,
        "icrc1_transfer",
        io_ledger_boundary::IcrcTransferArg {
            from_subaccount: None,
            to: recovery_staging.clone(),
            amount: candid::Nat::from(30_000_000_u128),
            fee: Some(candid::Nat::from(10_000_u128)),
            memo: None,
            created_at_time: None,
        },
    );
    let recovery_source_block: u128 = staged_recovery.unwrap().0.try_into().unwrap();
    let _: () = update(
        &pic,
        io_ledger,
        Principal::anonymous(),
        "debug_commit_then_unavailable_to",
        DebugRejectAccountArgs {
            account: stream.to_text(),
        },
    );
    pic.advance_time(Duration::from_secs(10));
    let ambiguous: Result<RedemptionProgress, ApiError> =
        update(&pic, stream, recovery_user, "process_redemptions", ());
    assert!(matches!(ambiguous, Err(ApiError::Pending(_))));
    let ambiguous_status: Status = query(&pic, stream, "get_status");
    assert_eq!(ambiguous_status.paid_unswept_redemption_io_e8s, None);
    assert_eq!(
        ambiguous_status.operation_phase.as_deref(),
        Some("SweepSubmitted")
    );
    let sweep_block = update::<_, Vec<DebugLedgerTransaction>>(
        &pic,
        io_ledger,
        Principal::anonymous(),
        "debug_get_transactions",
        (),
    )
    .into_iter()
    .filter(|tx| {
        tx.from_account.as_ref() == Some(&recovery_staging)
            && tx.to_account.as_ref() == Some(&reserve)
    })
    .map(|tx| tx.block_index)
    .max()
    .expect("committed reserve sweep is visible in the canonical IO ledger");
    let _: () = update(
        &pic,
        io_ledger,
        Principal::anonymous(),
        "debug_return_too_old_next",
        (),
    );
    pic.advance_time(Duration::from_secs(1));
    let stuck_progress: Result<StreamProgress, ApiError> =
        update(&pic, stream, recovery_user, "resume", ());
    assert!(matches!(stuck_progress, Err(ApiError::Stuck(_))));
    let stuck_status: Status = query(&pic, stream, "get_status");
    assert_eq!(stuck_status.lifecycle, Lifecycle::Paused);
    assert_eq!(stuck_status.operation_phase.as_deref(), Some("Stuck"));
    let transfer_calls_before_proof =
        query::<LedgerCallCounters>(&pic, io_ledger, "debug_get_call_counters").transfer;
    update::<_, Result<(), ApiError>>(
        &pic,
        stream,
        recovery_user,
        "prove_active_transfer",
        u128::from(sweep_block),
    )
    .expect("the exact public proof completes the stuck committed sweep");
    let recovered: Status = query(&pic, stream, "get_status");
    assert!(recovered.operation_kind.is_none());
    assert_eq!(recovered.pending_redemption_candidates, 0);
    assert_eq!(recovered.paid_unswept_redemption_io_e8s, Some(0));
    assert_eq!(
        recovered
            .last_completed_redemption
            .as_ref()
            .map(|value| value.source_io_block),
        Some(recovery_source_block)
    );
    assert_eq!(
        query::<LedgerCallCounters>(&pic, io_ledger, "debug_get_call_counters").transfer,
        transfer_calls_before_proof,
        "proof observes the committed effect without another reserve transfer"
    );
    update::<_, Result<(), ApiError>>(
        &pic,
        stream,
        recovery_user,
        "prove_active_transfer",
        u128::from(sweep_block),
    )
    .expect("the exact completed proof is an idempotent no-op");
    assert_eq!(
        query::<LedgerCallCounters>(&pic, io_ledger, "debug_get_call_counters").transfer,
        transfer_calls_before_proof,
        "duplicate proof cannot repeat an economic effect"
    );

    let concurrent_amount = 1_000_000_u128;
    let _: u64 = update(
        &pic,
        io_ledger,
        Principal::anonymous(),
        "debug_mint_account",
        DebugMintAccountArgs {
            to: recovery_staging.clone(),
            amount_e8s: concurrent_amount,
        },
    );
    let mut concurrent_state: StreamStateV1 = query(&pic, stream, "debug_get_state");
    let sequence = concurrent_state.next_operation_sequence;
    concurrent_state.next_operation_sequence.0 += 1;
    concurrent_state.lifecycle = Lifecycle::Ready;
    concurrent_state.active_operation = Some(StreamOperation::Redemption(Box::new(
        RedemptionStreamOperation::Active(Box::new(RedemptionOperation {
            sequence,
            source_io_block: 999,
            source_account: recovery_account.clone(),
            staged_io_amount_e8s: concurrent_amount,
            gross_icp_e8s: concurrent_amount,
            net_icp_e8s: concurrent_amount - 10_000,
            icp_fee_e8s: 10_000,
            io_sweep_fee_e8s: 10_000,
            icp_payout: TransferAttempt {
                intent: OwnTransferIntent::Icrc1 {
                    ledger: icp_ledger,
                    from_subaccount: [1; 32],
                    to: recovery_account,
                    amount: concurrent_amount - 10_000,
                    fee: 10_000,
                    memo: deterministic_memo(
                        b"io-redemption-pay-v2",
                        Principal::from_slice(&999_u128.to_be_bytes()),
                        sequence.0,
                    ),
                    created_at_time: pic.get_time().as_nanos_since_unix_epoch(),
                },
                state: TransferState::Succeeded { block: 1 },
            },
            reserve_sweep: None,
            last_external_call_started_at_nanos: 0,
            phase: RedemptionPhase::PayoutSucceeded,
        })),
    )));
    update::<_, Result<(), String>>(
        &pic,
        stream,
        Principal::anonymous(),
        "debug_replace_state",
        concurrent_state,
    )
    .unwrap();
    let counters_before_race: LedgerCallCounters =
        query(&pic, io_ledger, "debug_get_call_counters");
    let staging_before_race = nat_u128(update::<_, candid::Nat>(
        &pic,
        io_ledger,
        Principal::anonymous(),
        "icrc1_balance_of",
        recovery_staging.clone(),
    ));
    let reserve_before_race = nat_u128(update::<_, candid::Nat>(
        &pic,
        io_ledger,
        Principal::anonymous(),
        "icrc1_balance_of",
        reserve.clone(),
    ));
    let reserve_transfers_before_race = update::<_, Vec<DebugLedgerTransaction>>(
        &pic,
        io_ledger,
        Principal::anonymous(),
        "debug_get_transactions",
        (),
    )
    .into_iter()
    .filter(|tx| {
        tx.from_account.as_ref() == Some(&recovery_staging)
            && tx.to_account.as_ref() == Some(&reserve)
    })
    .count();
    let first_resume = pic
        .submit_call(
            stream,
            Principal::anonymous(),
            "resume",
            encode_one(()).unwrap(),
        )
        .unwrap();
    let second_resume = pic
        .submit_call(
            stream,
            Principal::anonymous(),
            "resume",
            encode_one(()).unwrap(),
        )
        .unwrap();
    pic.tick();
    let first: Result<StreamProgress, ApiError> =
        decode_one(&pic.await_call(first_resume).unwrap()).unwrap();
    let second: Result<StreamProgress, ApiError> =
        decode_one(&pic.await_call(second_resume).unwrap()).unwrap();
    let resume_results = [first, second];
    assert_eq!(
        resume_results
            .iter()
            .filter(|result| matches!(
                result,
                Ok(StreamProgress::Redemption(RedemptionProgress::Completed(_)))
            ))
            .count(),
        1
    );
    assert_eq!(
        resume_results
            .iter()
            .filter(|result| matches!(result, Err(ApiError::Busy)))
            .count(),
        1
    );
    let counters_after_race: LedgerCallCounters = query(&pic, io_ledger, "debug_get_call_counters");
    assert_eq!(
        counters_after_race.transfer,
        counters_before_race.transfer + 1
    );
    assert_eq!(
        nat_u128(update::<_, candid::Nat>(
            &pic,
            io_ledger,
            Principal::anonymous(),
            "icrc1_balance_of",
            recovery_staging.clone(),
        )),
        staging_before_race - concurrent_amount
    );
    assert_eq!(
        nat_u128(update::<_, candid::Nat>(
            &pic,
            io_ledger,
            Principal::anonymous(),
            "icrc1_balance_of",
            reserve.clone(),
        )),
        reserve_before_race + concurrent_amount - 10_000
    );
    let reserve_transfers_after_race = update::<_, Vec<DebugLedgerTransaction>>(
        &pic,
        io_ledger,
        Principal::anonymous(),
        "debug_get_transactions",
        (),
    )
    .into_iter()
    .filter(|tx| {
        tx.from_account.as_ref() == Some(&recovery_staging)
            && tx.to_account.as_ref() == Some(&reserve)
    })
    .collect::<Vec<_>>();
    assert_eq!(
        reserve_transfers_after_race.len(),
        reserve_transfers_before_race + 1
    );
    let concurrent_sweep = reserve_transfers_after_race.last().unwrap();
    assert_eq!(concurrent_sweep.amount_e8s, concurrent_amount - 10_000);
    assert_eq!(concurrent_sweep.fee_e8s, Some(10_000));
    assert_eq!(
        concurrent_sweep.memo_bytes.as_deref(),
        Some(
            deterministic_memo(
                b"io-redemption-sweep-v1",
                Principal::from_slice(&999_u128.to_be_bytes()),
                sequence.0,
            )
            .as_slice()
        )
    );

    let committed_before_burst = query::<StreamStateV1>(&pic, stream, "debug_get_state")
        .redemption_scan_state
        .cursor
        .latest_cursor;
    for _ in 0..70 {
        let _: u64 = update(
            &pic,
            io_ledger,
            Principal::anonymous(),
            "debug_mint_account",
            DebugMintAccountArgs {
                to: recovery_staging.clone(),
                amount_e8s: 100_000,
            },
        );
    }
    pic.advance_time(Duration::from_secs(10));
    let first_burst_page: Result<RedemptionProgress, ApiError> = update(
        &pic,
        stream,
        Principal::anonymous(),
        "process_redemptions",
        (),
    );
    assert_eq!(first_burst_page, Ok(RedemptionProgress::Idle));
    let mid_scan: StreamStateV1 = query(&pic, stream, "debug_get_state");
    assert_eq!(
        mid_scan.redemption_scan_state.cursor.latest_cursor,
        committed_before_burst
    );
    assert!(!mid_scan.redemption_scan_state.cursor.backfill_complete);
    assert!(mid_scan
        .redemption_scan_state
        .cursor
        .oldest_cursor
        .is_some());
    pic.upgrade_canister(
        stream,
        debug_wasm("io_stream_manager"),
        encode_one(()).unwrap(),
        None,
    )
    .unwrap();
    let mut restarted: StreamStateV1 = query(&pic, stream, "debug_get_state");
    assert_eq!(
        restarted.redemption_scan_state.cursor,
        mid_scan.redemption_scan_state.cursor
    );
    restarted.lifecycle = Lifecycle::Ready;
    update::<_, Result<(), String>>(
        &pic,
        stream,
        Principal::anonymous(),
        "debug_replace_state",
        restarted,
    )
    .unwrap();
    for _ in 0..5 {
        let _: u64 = update(
            &pic,
            io_ledger,
            Principal::anonymous(),
            "debug_mint_account",
            DebugMintAccountArgs {
                to: recovery_staging.clone(),
                amount_e8s: 100_000,
            },
        );
    }
    for _ in 0..4 {
        pic.advance_time(Duration::from_secs(10));
        assert_eq!(
            update::<_, Result<RedemptionProgress, ApiError>>(
                &pic,
                stream,
                Principal::anonymous(),
                "process_redemptions",
                (),
            ),
            Ok(RedemptionProgress::Idle)
        );
    }
    let latest_account_block = update::<_, Vec<DebugLedgerTransaction>>(
        &pic,
        io_ledger,
        Principal::anonymous(),
        "debug_get_transactions",
        (),
    )
    .into_iter()
    .filter(|tx| {
        tx.from_account.as_ref() == Some(&recovery_staging)
            || tx.to_account.as_ref() == Some(&recovery_staging)
    })
    .map(|tx| tx.block_index)
    .max()
    .unwrap();
    let caught_up: StreamStateV1 = query(&pic, stream, "debug_get_state");
    assert_eq!(
        caught_up.redemption_scan_state.cursor.latest_cursor,
        Some(io_ledger_types::BlockIndex(latest_account_block))
    );
    assert!(caught_up.redemption_scan_state.cursor.backfill_complete);

    let bulk_user = Principal::from_slice(&[91; 29]);
    let bulk_account = Account {
        owner: bulk_user,
        subaccount: None,
    };
    const BULK_DEPOSITS: usize = 65;
    const BULK_AMOUNT: u128 = 100_000;
    let _: u64 = update(
        &pic,
        io_ledger,
        Principal::anonymous(),
        "debug_mint_account",
        DebugMintAccountArgs {
            to: bulk_account.clone(),
            amount_e8s: BULK_DEPOSITS as u128 * (BULK_AMOUNT + 10_000),
        },
    );
    let _: u64 = update(
        &pic,
        icp_ledger,
        Principal::anonymous(),
        "debug_mint_account",
        DebugMintAccountArgs {
            to: Account {
                owner: stream,
                subaccount: Some(vec![1; 32]),
            },
            amount_e8s: 100_000_000,
        },
    );
    let staging_before_bulk = nat_u128(update::<_, candid::Nat>(
        &pic,
        io_ledger,
        Principal::anonymous(),
        "icrc1_balance_of",
        recovery_staging.clone(),
    ));
    let reserve_before_bulk = nat_u128(update::<_, candid::Nat>(
        &pic,
        io_ledger,
        Principal::anonymous(),
        "icrc1_balance_of",
        reserve.clone(),
    ));
    let payout_calls_before_bulk =
        query::<LedgerCallCounters>(&pic, icp_ledger, "debug_get_call_counters").transfer;
    let sweep_calls_before_bulk =
        query::<LedgerCallCounters>(&pic, io_ledger, "debug_get_call_counters").transfer;
    let reserve_sweeps_before_bulk = reserve_transfers_after_race.len();
    let mut bulk_sources = std::collections::BTreeSet::new();
    for _ in 0..BULK_DEPOSITS {
        let result: io_ledger_boundary::IcrcTransferResult = update(
            &pic,
            io_ledger,
            bulk_user,
            "icrc1_transfer",
            io_ledger_boundary::IcrcTransferArg {
                from_subaccount: None,
                to: recovery_staging.clone(),
                amount: candid::Nat::from(BULK_AMOUNT),
                fee: Some(candid::Nat::from(10_000_u128)),
                memo: None,
                created_at_time: None,
            },
        );
        let source_block: u128 = result.unwrap().0.try_into().unwrap();
        assert!(bulk_sources.insert(source_block));
    }
    let mut bulk_completed = std::collections::BTreeSet::new();
    let mut restarted_with_queued_candidates = false;
    let mut final_bulk_account_head = None;
    for _ in 0..180 {
        pic.advance_time(Duration::from_secs(10));
        let progress: Result<RedemptionProgress, ApiError> =
            update(&pic, stream, bulk_user, "process_redemptions", ());
        match progress {
            Ok(RedemptionProgress::Completed(result)) => {
                assert!(bulk_sources.contains(&result.source_io_block));
                assert!(
                    bulk_completed.insert(result.source_io_block),
                    "a staged source block completed more than once"
                );
            }
            Ok(RedemptionProgress::Idle | RedemptionProgress::Pending) => {}
            other => panic!("bulk staged redemption failed: {other:?}"),
        }
        let bulk_state: StreamStateV1 = query(&pic, stream, "debug_get_state");
        if !restarted_with_queued_candidates
            && !bulk_completed.is_empty()
            && query::<Status>(&pic, stream, "get_status").pending_redemption_candidates > 0
        {
            let cursor_before_upgrade = bulk_state.redemption_scan_state.cursor.clone();
            pic.upgrade_canister(
                stream,
                debug_wasm("io_stream_manager"),
                encode_one(()).unwrap(),
                None,
            )
            .unwrap();
            let mut after_upgrade: StreamStateV1 = query(&pic, stream, "debug_get_state");
            assert_eq!(
                after_upgrade.redemption_scan_state.cursor,
                cursor_before_upgrade
            );
            after_upgrade.lifecycle = Lifecycle::Ready;
            update::<_, Result<(), String>>(
                &pic,
                stream,
                Principal::anonymous(),
                "debug_replace_state",
                after_upgrade,
            )
            .unwrap();
            restarted_with_queued_candidates = true;
            continue;
        }
        if bulk_completed.len() == BULK_DEPOSITS {
            let head = *final_bulk_account_head.get_or_insert_with(|| {
                update::<_, Vec<DebugLedgerTransaction>>(
                    &pic,
                    io_ledger,
                    Principal::anonymous(),
                    "debug_get_transactions",
                    (),
                )
                .into_iter()
                .filter(|tx| {
                    tx.from_account.as_ref() == Some(&recovery_staging)
                        || tx.to_account.as_ref() == Some(&recovery_staging)
                })
                .map(|tx| tx.block_index)
                .max()
                .unwrap()
            });
            if bulk_state.redemption_scan_state.cursor.backfill_complete
                && bulk_state.redemption_scan_state.cursor.latest_cursor
                    == Some(io_ledger_types::BlockIndex(head))
            {
                break;
            }
        }
    }
    assert!(restarted_with_queued_candidates);
    assert_eq!(bulk_completed, bulk_sources);
    assert_eq!(
        query::<LedgerCallCounters>(&pic, icp_ledger, "debug_get_call_counters").transfer,
        payout_calls_before_bulk + BULK_DEPOSITS as u64
    );
    assert_eq!(
        query::<LedgerCallCounters>(&pic, io_ledger, "debug_get_call_counters").transfer,
        sweep_calls_before_bulk + (BULK_DEPOSITS as u64 * 2)
    );
    assert_eq!(
        update::<_, Vec<DebugLedgerTransaction>>(
            &pic,
            io_ledger,
            Principal::anonymous(),
            "debug_get_transactions",
            (),
        )
        .into_iter()
        .filter(|tx| {
            tx.from_account.as_ref() == Some(&recovery_staging)
                && tx.to_account.as_ref() == Some(&reserve)
        })
        .count(),
        reserve_sweeps_before_bulk + BULK_DEPOSITS
    );
    assert_eq!(
        nat_u128(update::<_, candid::Nat>(
            &pic,
            io_ledger,
            Principal::anonymous(),
            "icrc1_balance_of",
            recovery_staging.clone(),
        )),
        staging_before_bulk
    );
    assert_eq!(
        nat_u128(update::<_, candid::Nat>(
            &pic,
            io_ledger,
            Principal::anonymous(),
            "icrc1_balance_of",
            reserve.clone(),
        )),
        reserve_before_bulk + BULK_DEPOSITS as u128 * (BULK_AMOUNT - 10_000)
    );

    let _: u64 = update(
        &pic,
        io_ledger,
        Principal::anonymous(),
        "debug_mint_account",
        DebugMintAccountArgs {
            to: recovery_staging.clone(),
            amount_e8s: concurrent_amount,
        },
    );
    let mut held_state: StreamStateV1 = query(&pic, stream, "debug_get_state");
    let held_sequence = held_state.next_operation_sequence;
    held_state.next_operation_sequence.0 += 1;
    held_state.lifecycle = Lifecycle::Ready;
    held_state.reward_checkpoint.reward_work_due = false;
    held_state.stake_observation_due = false;
    held_state.structural_reconciliation_due = false;
    let held_now = pic.get_time().as_nanos_since_unix_epoch();
    let held_intent = OwnTransferIntent::Icrc1 {
        ledger: io_ledger,
        from_subaccount: io_accounts::REDEMPTION_STAGING_SUBACCOUNT,
        to: reserve.clone(),
        amount: concurrent_amount - 10_000,
        fee: 10_000,
        memo: deterministic_memo(
            b"io-redemption-sweep-v1",
            Principal::from_slice(&1_001_u128.to_be_bytes()),
            held_sequence.0,
        ),
        created_at_time: held_now.saturating_sub(2_000_000_000),
    };
    held_state.active_operation = Some(StreamOperation::Redemption(Box::new(
        RedemptionStreamOperation::Active(Box::new(RedemptionOperation {
            sequence: held_sequence,
            source_io_block: 1_001,
            source_account: bulk_account,
            staged_io_amount_e8s: concurrent_amount,
            gross_icp_e8s: concurrent_amount,
            net_icp_e8s: concurrent_amount - 10_000,
            icp_fee_e8s: 10_000,
            io_sweep_fee_e8s: 10_000,
            icp_payout: TransferAttempt {
                intent: OwnTransferIntent::Icrc1 {
                    ledger: icp_ledger,
                    from_subaccount: [1; 32],
                    to: Account {
                        owner: bulk_user,
                        subaccount: None,
                    },
                    amount: concurrent_amount - 10_000,
                    fee: 10_000,
                    memo: deterministic_memo(
                        b"io-redemption-pay-v2",
                        Principal::from_slice(&1_001_u128.to_be_bytes()),
                        held_sequence.0,
                    ),
                    created_at_time: held_now.saturating_sub(3_000_000_000),
                },
                state: TransferState::Succeeded { block: 1 },
            },
            reserve_sweep: Some(TransferAttempt {
                intent: held_intent,
                state: TransferState::Submitted {
                    epoch: io_stream_manager::state::DispatchEpoch(1),
                    first_submitted_at: held_now.saturating_sub(2_000_000_000),
                    last_submitted_at: held_now.saturating_sub(2_000_000_000),
                },
            }),
            last_external_call_started_at_nanos: 0,
            phase: RedemptionPhase::SweepSubmitted,
        })),
    )));
    update::<_, Result<(), String>>(
        &pic,
        stream,
        Principal::anonymous(),
        "debug_replace_state",
        held_state,
    )
    .unwrap();
    update::<_, ()>(
        &pic,
        io_ledger,
        Principal::anonymous(),
        "debug_delay_next_transfer",
        10_000_u32,
    );
    let held_resume = pic
        .submit_call(
            stream,
            Principal::anonymous(),
            "resume",
            encode_one(()).unwrap(),
        )
        .unwrap();
    for _ in 0..3 {
        pic.tick();
    }
    update::<_, ()>(
        &pic,
        stream,
        Principal::anonymous(),
        "debug_install_scheduler_at",
        pic.get_time().as_nanos_since_unix_epoch() / 1_000_000_000 + 2,
    );
    let before_guard_timer: DebugSchedulerStatus =
        query(&pic, stream, "debug_get_scheduler_status");
    pic.advance_time(Duration::from_secs(2));
    for _ in 0..10 {
        pic.tick();
        if query::<DebugSchedulerStatus>(&pic, stream, "debug_get_scheduler_status")
            .recovery_deferrals
            > before_guard_timer.recovery_deferrals
        {
            break;
        }
    }
    let after_guard_timer: DebugSchedulerStatus = query(&pic, stream, "debug_get_scheduler_status");
    assert_eq!(
        after_guard_timer.recovery_deferrals,
        before_guard_timer.recovery_deferrals + 1
    );
    assert_eq!(
        after_guard_timer.callback_invocations,
        before_guard_timer.callback_invocations + 1
    );
    assert!(
        after_guard_timer.active_deadline_seconds.unwrap()
            > pic.get_time().as_nanos_since_unix_epoch() / 1_000_000_000
    );
    let held_after_timer: StreamStateV1 = query(&pic, stream, "debug_get_state");
    let Some(StreamOperation::Redemption(held_after_timer)) = held_after_timer.active_operation
    else {
        panic!("held redemption must remain active")
    };
    let RedemptionStreamOperation::Active(held_after_timer) = held_after_timer.as_ref();
    assert!(matches!(
        held_after_timer
            .reserve_sweep
            .as_ref()
            .map(|attempt| &attempt.state),
        Some(TransferState::Submitted {
            epoch: io_stream_manager::state::DispatchEpoch(2),
            ..
        })
    ));
    let _ = held_resume;
    eprintln!(
        "semantic_staging block={staged_block} claim_bearing_before_payout=true global_manual_throttle=true low_liquidity_debt=false recovered_once=true automatic_sixty_second_poll=true stuck_sweep_public_proof_once=true concurrent_resumes_completed=1 concurrent_resumes_busy=1 concurrent_sweep_transfers=1 burst_account_transactions=75 restart_between_pages=true captured_head_caught_up=true bulk_valid_sources=65 bulk_paid_once=65 queue_restart=true interspersed_sweeps=65 held_guard_timer_deferrals=1 held_guard_dispatch_epoch=2"
    );
}

#[test]
fn reward_observation_and_best_effort_refresh_are_bounded_and_monetary_once() {
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
                dissolve_delay_seconds: io_core_model::SNS_USER_DISSOLVE_DELAY_SECONDS,
                eligible_closed_proposals: 1,
                voted_closed_proposals: 1,
                is_genesis_governance_neuron: false,
                is_protocol_owned: false,
                is_dissolving: false,
            },
        );
    }
    let _: () = update(
        &pic,
        governance,
        Principal::anonymous(),
        "debug_add_neuron",
        MockSnsNeuron {
            neuron_id: 7,
            staked_io_e8s: 30_000_000,
            dissolve_delay_seconds: io_core_model::SNS_USER_DISSOLVE_DELAY_SECONDS + 1,
            eligible_closed_proposals: 1,
            voted_closed_proposals: 1,
            is_genesis_governance_neuron: false,
            is_protocol_owned: false,
            is_dissolving: false,
        },
    );
    let baseline_end = pic.get_time().as_nanos_since_unix_epoch() / 1_000_000_000;
    let baseline: Result<(), String> = update(
        &pic,
        governance,
        Principal::anonymous(),
        "debug_set_latest_reward_event",
        LatestRewardEventFixture {
            round: 0,
            rounds_since_last_distribution: 0,
            end_timestamp_seconds: baseline_end,
            settled_proposal_ids: Vec::new(),
            neuron_reward_shares: Vec::new(),
        },
    );
    baseline.unwrap();

    let stream = pic.create_canister();
    pic.add_cycles(stream, CYCLES);
    let user = Principal::from_slice(&[99; 29]);
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
                owner: user,
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
                io_index: Principal::from_slice(&[8; 29]),
                icp_ledger,
                nns_manager: nns,
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
                nonredeemable_governance_io_accounts: Vec::new(),
                minimum_redemption_io_e8s: 20_000,
                expected_io_fee_e8s: 10_000,
                expected_icp_fee_e8s: 10_000,
                redemption_poll_interval_seconds: 60,
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
    assert!(initial.reward_work_due);
    assert_eq!(
        initial
            .latest_processed_reward_event
            .map(|event| event.round),
        Some(0)
    );
    assert_eq!(initial.processed_reward_event_count, 0);
    assert_eq!(initial.accumulated_policy_credit, 0);
    let initial_observation: Result<RewardEventObservation, ApiError> = update(
        &pic,
        stream,
        Principal::anonymous(),
        "resume_reward_work",
        (),
    );
    let genesis = initial_observation.unwrap();
    assert_eq!(genesis.event.round, 0);
    assert_eq!(
        genesis.classification,
        RewardEventClassification::StructuralOnly
    );
    assert_eq!(genesis.policy_credit, 0);
    assert_eq!(genesis.eligible_credit_total, 0);
    let genesis_status = query::<Status>(&pic, stream, "get_status");
    assert!(!genesis_status.reward_work_due);
    assert_eq!(genesis_status.processed_reward_event_count, 0);
    assert_eq!(genesis_status.accumulated_policy_credit, 0);
    assert_eq!(
        genesis_status
            .latest_reconciliation_checkpoint
            .as_ref()
            .map(|checkpoint| checkpoint.event_marker),
        Some(0)
    );
    let genesis_generation = genesis_status
        .latest_reconciliation_checkpoint
        .as_ref()
        .expect("genesis structural checkpoint")
        .generation;
    let reconciliations_before_structural: u64 = query(&pic, nns, "debug_get_reconcile_call_count");
    let _: () = update(
        &pic,
        nns,
        Principal::anonymous(),
        "debug_reject_next_reconciliations",
        1_u64,
    );
    pic.advance_time(Duration::from_secs(
        io_core_model::STRUCTURAL_SYNC_INTERVAL_SECONDS + 1,
    ));
    for _ in 0..5 {
        pic.tick();
    }
    let structural_status = query::<Status>(&pic, stream, "get_status");
    let structural_checkpoint = structural_status
        .latest_reconciliation_checkpoint
        .as_ref()
        .expect("12-hour structural checkpoint");
    assert_eq!(structural_checkpoint.generation, genesis_generation + 1);
    assert_eq!(structural_checkpoint.event_marker, 0);
    assert_eq!(structural_status.processed_reward_event_count, 0);
    assert_eq!(structural_status.accumulated_policy_credit, 0);
    assert_eq!(structural_status.accumulated_eligible_credit, 0);
    assert_eq!(
        structural_status.latest_reward_event_classification,
        Some(RewardEventClassification::StructuralOnly)
    );
    assert_eq!(
        query::<u64>(&pic, nns, "debug_get_reconcile_call_count"),
        reconciliations_before_structural + 1,
        "a structural wake must immediately attempt reconciliation without awarding a reward event"
    );
    assert!(structural_status
        .latest_reconciliation_checkpoint
        .as_ref()
        .is_some_and(|checkpoint| checkpoint.generation == genesis_generation + 1));
    let structural_retry: DebugSchedulerStatus = query(&pic, stream, "debug_get_scheduler_status");
    let structural_now_seconds = pic.get_time().as_nanos_since_unix_epoch() / 1_000_000_000;
    let seconds_until_structural_retry = structural_retry
        .active_deadline_seconds
        .expect("structural failure installs a retry")
        .saturating_sub(structural_now_seconds);
    assert!(seconds_until_structural_retry > 0);
    pic.advance_time(Duration::from_secs(seconds_until_structural_retry - 1));
    for _ in 0..3 {
        pic.tick();
    }
    assert_eq!(
        query::<u64>(&pic, nns, "debug_get_reconcile_call_count"),
        reconciliations_before_structural + 1,
        "the retry must not run before its rounded absolute 60-second deadline"
    );
    pic.advance_time(Duration::from_secs(1));
    for _ in 0..5 {
        pic.tick();
    }
    let recovered_structural = query::<Status>(&pic, stream, "get_status");
    assert_eq!(
        recovered_structural
            .latest_reconciliation_checkpoint
            .as_ref()
            .map(|checkpoint| checkpoint.generation),
        Some(genesis_generation + 1),
        "retrying reconciliation must not manufacture another structural generation"
    );
    assert_eq!(
        query::<u64>(&pic, nns, "debug_get_reconcile_call_count"),
        reconciliations_before_structural + 2
    );
    eprintln!(
        "anchored_structural_scheduler cadence_seconds={} generation={} reward_event_count=0 policy_credit=0 eligible_credit=0 reconciliation_calls=2 retry_seconds=60 same_generation=true",
        io_core_model::STRUCTURAL_SYNC_INTERVAL_SECONDS,
        structural_checkpoint.generation,
    );
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
    assert!(!query::<Status>(&pic, stream, "get_status").reward_work_due);
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
    assert!(
        matches!(cooled, Err(ApiError::Pending(_))),
        "cooled scheduler call was not pending: {cooled:?}"
    );
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
            round: 1,
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
    for _ in 0..3 {
        pic.tick();
    }
    let first_real_status = query::<Status>(&pic, stream, "get_status");
    assert!(!first_real_status.reward_work_due);
    assert_eq!(
        first_real_status
            .latest_processed_reward_event
            .map(|event| event.round),
        Some(1),
        "first real event was not processed: {first_real_status:?}"
    );
    assert_eq!(first_real_status.processed_reward_event_count, 1);
    assert!(first_real_status.accumulated_policy_credit > 0);
    let credited_once = first_real_status.accumulated_policy_credit;
    let replay: Result<RewardEventObservation, ApiError> = update(
        &pic,
        stream,
        Principal::anonymous(),
        "resume_reward_work",
        (),
    );
    assert!(matches!(replay, Err(ApiError::Pending(_))));
    assert_eq!(
        query::<Status>(&pic, stream, "get_status").accumulated_policy_credit,
        credited_once
    );

    let transport_event: Result<(), String> = update(
        &pic,
        governance,
        Principal::anonymous(),
        "debug_set_latest_reward_event",
        LatestRewardEventFixture {
            round: 2,
            rounds_since_last_distribution: 1,
            end_timestamp_seconds: baseline_end + 172_800,
            settled_proposal_ids: vec![2],
            neuron_reward_shares: (1_u64..=6)
                .map(|id| (id, SnsUint128 { high: 0, low: 1 }))
                .collect(),
        },
    );
    transport_event.unwrap();
    let _: () = update(
        &pic,
        governance,
        Principal::anonymous(),
        "debug_set_available",
        false,
    );
    let governance_before_transport: GovernanceCallCounters =
        query(&pic, governance, "debug_get_call_counters");
    let root_before_transport: u64 = query(&pic, root, "debug_get_summary_call_count");
    pic.advance_time(Duration::from_secs(86_701));
    for _ in 0..3 {
        pic.tick();
    }
    let retrying = query::<Status>(&pic, stream, "get_status");
    assert!(!retrying.reward_work_due);
    assert!(!retrying.reward_processing_paused);
    let governance_after_transport: GovernanceCallCounters =
        query(&pic, governance, "debug_get_call_counters");
    let root_after_transport: u64 = query(&pic, root, "debug_get_summary_call_count");
    assert!(
        governance_after_transport.latest_reward_event
            > governance_before_transport.latest_reward_event
    );
    assert!(root_after_transport > root_before_transport);
    for _ in 0..3 {
        let retry_too_early: Result<RewardEventObservation, ApiError> = update(
            &pic,
            stream,
            Principal::anonymous(),
            "resume_reward_work",
            (),
        );
        assert!(matches!(retry_too_early, Err(ApiError::Pending(_))));
    }
    assert_eq!(
        query::<GovernanceCallCounters>(&pic, governance, "debug_get_call_counters"),
        governance_after_transport
    );
    assert_eq!(
        query::<u64>(&pic, root, "debug_get_summary_call_count"),
        root_after_transport
    );
    let _: () = update(
        &pic,
        governance,
        Principal::anonymous(),
        "debug_set_available",
        true,
    );
    pic.advance_time(Duration::from_secs(61));
    for _ in 0..3 {
        pic.tick();
    }
    let recovered = query::<Status>(&pic, stream, "get_status");
    assert_eq!(recovered.latest_processed_reward_event.unwrap().round, 2);
    assert!(!recovered.reward_work_due);
    assert!(!recovered.reward_processing_paused);
}
