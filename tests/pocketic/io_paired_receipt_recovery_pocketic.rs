use candid::{decode_one, encode_one, CandidType, Principal};
use io_receipt_types::{ClaimBackingReceiptKind, PrepareClaimBackingReceiptArgs};
use io_stream_manager::{
    redemption::RedemptionPhase,
    state::{DispatchEpoch, StreamOperation},
    transfer::TransferState,
    Account, ApiError, CallerRedemptionState, ClaimBackingReceiptPermit, InitArgs, Lifecycle,
    PreparedRedemption, RedeemArgs, RedemptionProgress, StreamConfig, StreamStateV1,
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
struct DebugFeeArgs {
    fee_e8s: u128,
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
struct SnsUint128 {
    high: u64,
    low: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, CandidType, Deserialize)]
struct LedgerCallCounters {
    fee: u64,
    total_supply: u64,
    balance: u64,
    transfer: u64,
    query_blocks: u64,
}

fn wasm(name: &str) -> Vec<u8> {
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
    pic.install_canister(canister, wasm(name), Vec::new(), None);
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
        .unwrap_or_else(|error| panic!("{method}: {error}")),
    )
    .unwrap()
}

fn query_as<R: for<'de> Deserialize<'de> + CandidType>(
    pic: &PocketIc,
    canister: Principal,
    caller: Principal,
    method: &str,
) -> R {
    decode_one(
        &pic.query_call(canister, caller, method, encode_one(()).unwrap())
            .unwrap_or_else(|error| panic!("{method}: {error}")),
    )
    .unwrap()
}

fn replace_state(pic: &PocketIc, stream: Principal, state: StreamStateV1) {
    update::<_, Result<(), String>>(
        pic,
        stream,
        Principal::anonymous(),
        "debug_replace_state",
        state,
    )
    .unwrap();
}

fn prepare_and_push(
    pic: &PocketIc,
    stream: Principal,
    io_ledger: Principal,
    user: Principal,
    args: RedeemArgs,
) -> (PreparedRedemption, u128) {
    let prepared = update::<_, Result<PreparedRedemption, ApiError>>(
        pic,
        stream,
        user,
        "prepare_redemption",
        args,
    )
    .expect("redemption push must prepare");
    let pushed: io_ledger_boundary::IcrcTransferResult = update(
        pic,
        io_ledger,
        user,
        "icrc1_transfer",
        io_ledger_boundary::IcrcTransferArg {
            from_subaccount: prepared.account.subaccount.clone(),
            to: prepared.reserve.clone(),
            amount: candid::Nat::from(prepared.request.io_amount_e8s),
            fee: Some(candid::Nat::from(prepared.snapshot.io_fee_e8s)),
            memo: Some(prepared.push_memo.clone()),
            created_at_time: Some(prepared.prepared_at_nanos),
        },
    );
    let block = pushed
        .expect("exact prepared IO push must succeed")
        .0
        .try_into()
        .unwrap();
    (prepared, block)
}

#[test]
fn malformed_prepare_after_persistence_replays_and_quarantines_redemption() {
    if std::env::var_os("POCKET_IC_BIN").is_none() {
        eprintln!("skipping paired receipt PocketIC test because POCKET_IC_BIN is not set");
        return;
    }
    let pic = PocketIc::new();
    let io_ledger = install(&pic, "mock_io_ledger");
    let icp_ledger = install(&pic, "mock_icp_ledger");
    let nns = install(&pic, "mock_nns_governance");
    let stream = pic.create_canister();
    pic.add_cycles(stream, CYCLES);
    let governance = install(&pic, "mock_sns_governance");
    let root = install(&pic, "mock_sns_root");
    let user = Principal::from_slice(&[53; 29]);
    let jupiter_io = Account {
        owner: Principal::from_slice(&[54; 29]),
        subaccount: None,
    };
    let reserve = Account {
        owner: stream,
        subaccount: None,
    };
    let liquid = Account {
        owner: stream,
        subaccount: Some(vec![1; 32]),
    };
    let source = Account {
        owner: nns,
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
    let _: () = update(
        &pic,
        governance,
        Principal::anonymous(),
        "debug_set_io_ledger_principal",
        io_ledger,
    );
    update::<_, Result<(), String>>(
        &pic,
        governance,
        Principal::anonymous(),
        "debug_set_latest_reward_event",
        LatestRewardEventFixture {
            round: 0,
            rounds_since_last_distribution: 0,
            end_timestamp_seconds: 1,
            settled_proposal_ids: Vec::new(),
            neuron_reward_shares: Vec::new(),
        },
    )
    .unwrap();
    pic.install_canister(
        stream,
        wasm("io_stream_manager"),
        encode_one(InitArgs {
            config: StreamConfig {
                io_ledger,
                icp_ledger,
                nns_manager: nns,
                jupiter_io_account: jupiter_io,
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
                maximum_request_lifetime_nanos: 900_000_000_000,
                retry_delay_nanos: 1_000_000_000,
                ledger_deduplication_window_nanos: 86_400_000_000_000,
            },
        })
        .unwrap(),
        None,
    );
    for ledger in [io_ledger, icp_ledger] {
        let _: () = update(
            &pic,
            ledger,
            Principal::anonymous(),
            "debug_set_fee",
            DebugFeeArgs { fee_e8s: 10_000 },
        );
    }
    for (ledger, to, amount_e8s) in [
        (io_ledger, reserve.clone(), 60_010_000),
        (
            io_ledger,
            Account {
                owner: user,
                subaccount: None,
            },
            100_000_000,
        ),
        (icp_ledger, liquid.clone(), 100_000_000),
        (icp_ledger, source.clone(), 60_010_000),
    ] {
        let _: u64 = update(
            &pic,
            ledger,
            Principal::anonymous(),
            "debug_mint_account",
            DebugMintAccountArgs { to, amount_e8s },
        );
    }
    update::<_, Result<(), ApiError>>(&pic, stream, governance, "set_paused", false).unwrap();
    let activated: StreamStateV1 = query(&pic, stream, "debug_get_state");
    assert_eq!(activated.lifecycle, Lifecycle::Ready);
    assert_eq!(
        activated
            .reward_checkpoint
            .last_processed_event
            .as_ref()
            .map(|event| event.round),
        Some(0)
    );

    let now = pic.get_time().as_nanos_since_unix_epoch();
    let first_args = RedeemArgs {
        from_subaccount: None,
        io_amount_e8s: 10_000_000,
        min_icp_out_e8s: 9_990_000,
        max_io_fee_e8s: 10_000,
        max_icp_fee_e8s: 10_000,
        expires_at_nanos: now + 60_000_000_000,
        nonce: 0,
    };
    let (_, first_push_block) = prepare_and_push(&pic, stream, io_ledger, user, first_args);
    let completed = match update::<_, Result<RedemptionProgress, ApiError>>(
        &pic,
        stream,
        user,
        "settle_redemption",
        first_push_block,
    ) {
        Ok(RedemptionProgress::Completed(result)) => result,
        other => panic!("all-success redemption did not complete in one invocation: {other:?}"),
    };
    assert_eq!(completed.gross_icp_e8s, 10_000_000);
    assert_eq!(completed.net_icp_e8s, 9_990_000);

    let ledger_before_trap = (
        query::<LedgerCallCounters>(&pic, io_ledger, "debug_get_call_counters"),
        query::<LedgerCallCounters>(&pic, icp_ledger, "debug_get_call_counters"),
    );
    let user_icp_before_trap: candid::Nat = update(
        &pic,
        icp_ledger,
        Principal::anonymous(),
        "icrc1_balance_of",
        Account {
            owner: user,
            subaccount: None,
        },
    );
    let _: () = update(
        &pic,
        stream,
        Principal::anonymous(),
        "debug_trap_after_caller_result_write",
        true,
    );
    let trapped_args = RedeemArgs {
        from_subaccount: None,
        io_amount_e8s: 10_000_000,
        min_icp_out_e8s: 9_990_000,
        max_io_fee_e8s: 10_000,
        max_icp_fee_e8s: 10_000,
        expires_at_nanos: now + 60_000_000_000,
        nonce: 1,
    };
    let (_, trapped_push_block) =
        prepare_and_push(&pic, stream, io_ledger, user, trapped_args.clone());
    let prepared_before_trap = query_as::<Result<CallerRedemptionState, ApiError>>(
        &pic,
        stream,
        user,
        "get_caller_redemption_state",
    )
    .unwrap();
    let trapped = pic.update_call(
        stream,
        user,
        "settle_redemption",
        encode_one(trapped_push_block).unwrap(),
    );
    assert!(trapped
        .unwrap_err()
        .to_string()
        .contains("debug trap after caller redemption result write"));
    let pushed_after_trap = query_as::<Result<CallerRedemptionState, ApiError>>(
        &pic,
        stream,
        user,
        "get_caller_redemption_state",
    )
    .unwrap();
    assert_eq!(
        pushed_after_trap.next_nonce,
        prepared_before_trap.next_nonce
    );
    assert_eq!(
        pushed_after_trap.last_request_fingerprint,
        prepared_before_trap.last_request_fingerprint
    );
    assert_eq!(
        pushed_after_trap.last_result,
        prepared_before_trap.last_result
    );
    assert!(matches!(
        pushed_after_trap.pending,
        Some(io_stream_manager::state::CallerRedemptionPending::Pushed(ref pushed))
            if pushed.io_block == trapped_push_block
    ));
    let trapped_state: StreamStateV1 = query(&pic, stream, "debug_get_state");
    let trapped_phase = match trapped_state.active_operation {
        Some(StreamOperation::Redemption(operation)) => match operation.as_ref() {
            io_stream_manager::state::RedemptionStreamOperation::Active(active) => active.phase,
        },
        other => panic!("trapped payout lost its active obligation: {other:?}"),
    };
    assert!(matches!(
        trapped_phase,
        RedemptionPhase::PayoutSubmitted | RedemptionPhase::PayoutSucceeded
    ));
    let ledger_after_trap = (
        query::<LedgerCallCounters>(&pic, io_ledger, "debug_get_call_counters"),
        query::<LedgerCallCounters>(&pic, icp_ledger, "debug_get_call_counters"),
    );
    assert_eq!(
        ledger_after_trap.0.transfer,
        ledger_before_trap.0.transfer + 1
    );
    assert_eq!(
        ledger_after_trap.1.transfer,
        ledger_before_trap.1.transfer + 1
    );
    let before_upgrade: StreamStateV1 = query(&pic, stream, "debug_get_state");
    pic.upgrade_canister(
        stream,
        wasm("io_stream_manager"),
        encode_one(()).unwrap(),
        None,
    )
    .unwrap();
    let mut expected_after_upgrade = before_upgrade;
    expected_after_upgrade.lifecycle = Lifecycle::Paused;
    assert_eq!(
        query::<StreamStateV1>(&pic, stream, "debug_get_state"),
        expected_after_upgrade,
        "round-zero PayoutSucceeded state must reopen Paused without loss"
    );
    update::<_, Result<(), ApiError>>(&pic, stream, governance, "set_paused", false).unwrap();
    assert_eq!(
        query::<StreamStateV1>(&pic, stream, "debug_get_state").lifecycle,
        Lifecycle::Ready,
        "authenticated readiness restoration must accept the resumable checkpoint"
    );
    pic.advance_time(Duration::from_secs(2));
    let recovered = match update::<_, Result<io_stream_manager::StreamProgress, ApiError>>(
        &pic,
        stream,
        Principal::anonymous(),
        "resume",
        (),
    ) {
        Ok(io_stream_manager::StreamProgress::Redemption(RedemptionProgress::Completed(
            result,
        ))) => result,
        other => panic!("trapped redemption did not recover: {other:?}"),
    };
    assert_eq!(
        update::<_, Result<RedemptionProgress, ApiError>>(
            &pic,
            stream,
            user,
            "settle_redemption",
            trapped_push_block,
        ),
        Ok(RedemptionProgress::Completed(recovered.clone()))
    );
    assert_eq!(
        query_as::<Result<CallerRedemptionState, ApiError>>(
            &pic,
            stream,
            user,
            "get_caller_redemption_state",
        )
        .unwrap()
        .next_nonce,
        2
    );
    let ledger_after_recovery = (
        query::<LedgerCallCounters>(&pic, io_ledger, "debug_get_call_counters"),
        query::<LedgerCallCounters>(&pic, icp_ledger, "debug_get_call_counters"),
    );
    assert_eq!(
        ledger_after_recovery.0.transfer, ledger_after_trap.0.transfer,
        "resume and completed replay must not repeat the IO push"
    );
    assert_eq!(
        ledger_after_recovery.1.transfer,
        ledger_after_trap.1.transfer + u64::from(trapped_phase == RedemptionPhase::PayoutSubmitted),
        "only an ambiguous submitted payout may make one exact deduplicated retry call"
    );
    let user_icp_after_recovery: candid::Nat = update(
        &pic,
        icp_ledger,
        Principal::anonymous(),
        "icrc1_balance_of",
        Account {
            owner: user,
            subaccount: None,
        },
    );
    assert_eq!(
        user_icp_after_recovery,
        user_icp_before_trap + candid::Nat::from(recovered.net_icp_e8s),
        "ambiguous payout recovery must have exactly one economic effect"
    );

    let request = PrepareClaimBackingReceiptArgs {
        nns_operation_sequence: 1,
        kind: ClaimBackingReceiptKind::Jupiter,
        net_liquid_credit_e8s: 60_000_000,
    };
    let _: () = update(
        &pic,
        stream,
        Principal::anonymous(),
        "debug_fail_malformed_prepare_after_persist",
        true,
    );
    let malformed: Result<ClaimBackingReceiptPermit, ApiError> = update(
        &pic,
        stream,
        nns,
        "prepare_claim_backing_receipt",
        request.clone(),
    );
    assert!(matches!(
        malformed,
        Err(ApiError::Pending(ref message)) if message.contains("after permit persistence")
    ));
    let persisted: StreamStateV1 = query(&pic, stream, "debug_get_state");
    assert!(matches!(
        persisted.active_operation,
        Some(StreamOperation::ClaimReceipt(_))
    ));
    let ledger_before_replay: LedgerCallCounters =
        query(&pic, icp_ledger, "debug_get_call_counters");
    let permit = update::<_, Result<ClaimBackingReceiptPermit, ApiError>>(
        &pic,
        stream,
        nns,
        "prepare_claim_backing_receipt",
        request,
    )
    .unwrap();
    assert_eq!(
        query::<LedgerCallCounters>(&pic, icp_ledger, "debug_get_call_counters"),
        ledger_before_replay,
        "exact malformed-response replay must not rebuild the canonical snapshot"
    );
    let blocked = update::<_, Result<PreparedRedemption, ApiError>>(
        &pic,
        stream,
        user,
        "prepare_redemption",
        RedeemArgs {
            from_subaccount: None,
            io_amount_e8s: 10_000_000,
            min_icp_out_e8s: 1,
            max_io_fee_e8s: 10_000,
            max_icp_fee_e8s: 10_000,
            expires_at_nanos: now + 60_000_000_000,
            nonce: 2,
        },
    );
    assert!(matches!(
        blocked,
        Err(ApiError::Ledger(ref reason))
            if reason.contains("claim receipt ownership awaits exact liquid-block proof")
    ));

    let before_receipt_upgrade: StreamStateV1 = query(&pic, stream, "debug_get_state");
    pic.upgrade_canister(
        stream,
        wasm("io_stream_manager"),
        encode_one(()).unwrap(),
        None,
    )
    .unwrap();
    let mut expected = before_receipt_upgrade;
    expected.lifecycle = Lifecycle::Paused;
    assert_eq!(
        query::<StreamStateV1>(&pic, stream, "debug_get_state"),
        expected,
        "receipt permit checkpoint must survive same-Wasm upgrade"
    );
    expected.lifecycle = Lifecycle::Ready;
    replace_state(&pic, stream, expected);

    let transfer: io_ledger_boundary::IcrcTransferResult = update(
        &pic,
        icp_ledger,
        nns,
        "icrc1_transfer",
        io_ledger_boundary::IcrcTransferArg {
            from_subaccount: source.subaccount,
            to: liquid,
            amount: candid::Nat::from(permit.amount_e8s),
            fee: Some(candid::Nat::from(10_000_u64)),
            memo: Some(permit.memo.clone()),
            created_at_time: Some(1),
        },
    );
    let block: u128 = transfer.unwrap().0.try_into().unwrap();
    update::<_, Result<io_receipt_types::ClaimBackingReceiptProgress, ApiError>>(
        &pic,
        stream,
        nns,
        "prove_claim_backing_receipt",
        io_receipt_types::ProveClaimBackingReceiptArgs {
            stream_operation_sequence: permit.stream_operation_sequence,
            block_index: block,
        },
    )
    .unwrap();
    update::<_, Result<io_stream_manager::StreamProgress, ApiError>>(
        &pic,
        stream,
        Principal::anonymous(),
        "resume",
        (),
    )
    .unwrap();
    let mut submitted: StreamStateV1 = query(&pic, stream, "debug_get_state");
    let Some(StreamOperation::ClaimReceipt(receipt)) = &mut submitted.active_operation else {
        panic!("claim receipt was not retained")
    };
    let attempt = receipt
        .current_recipient
        .as_mut()
        .expect("Jupiter transfer attempt must be persisted");
    attempt.state = TransferState::Submitted {
        epoch: DispatchEpoch(1),
        first_submitted_at: now,
        last_submitted_at: now,
    };
    replace_state(&pic, stream, submitted.clone());
    pic.upgrade_canister(
        stream,
        wasm("io_stream_manager"),
        encode_one(()).unwrap(),
        None,
    )
    .unwrap();
    submitted.lifecycle = Lifecycle::Paused;
    assert_eq!(
        query::<StreamStateV1>(&pic, stream, "debug_get_state"),
        submitted,
        "submitted claim transfer identity must survive same-Wasm upgrade"
    );
    eprintln!(
        "account_semantic_receipt_recovery malformed_after_persist=true exact_replay=true quarantined_redemption=true same_wasm_upgrade=true duplicate_transfer=false stream_operation_sequence={} proof_block={}",
        permit.stream_operation_sequence,
        block,
    );
}
