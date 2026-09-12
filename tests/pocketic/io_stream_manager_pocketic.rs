use candid::{decode_one, encode_one, CandidType, Nat, Principal};
use io_stream_manager::{
    Account, ApiError, InitArgs, Lifecycle, RewardEventClassification, RewardEventObservation,
    Status, StreamConfig, StreamStateV1,
};
use pocket_ic::{PocketIc, PocketIcBuilder};
use serde::Deserialize;
use std::{
    process::{Child, Command, Stdio},
    sync::{Mutex, MutexGuard, OnceLock},
    thread,
    time::{Duration, Instant},
};

const CYCLES: u128 = 2_000_000_000_000;

struct StreamServer {
    url: String,
    _child: Mutex<Child>,
}

fn stream_server() -> &'static StreamServer {
    static SERVER: OnceLock<StreamServer> = OnceLock::new();
    SERVER.get_or_init(|| {
        let binary = std::env::var_os("POCKET_IC_BIN")
            .expect("POCKET_IC_BIN must be set for live Stream Manager tests");
        let port_file = std::env::temp_dir().join(format!(
            "io_stream_manager_pocketic_{}.port",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&port_file);
        let mut command = Command::new(binary);
        command
            .args(["--ttl", "300", "--hard-ttl", "3600", "--port-file"])
            .arg(&port_file);
        if std::env::var_os("POCKET_IC_MUTE_SERVER").is_some() {
            command.stdout(Stdio::null()).stderr(Stdio::null());
        }
        let mut child = command
            .spawn()
            .expect("failed to start the dedicated Stream Manager PocketIC server");
        let started = Instant::now();
        let port = loop {
            if let Ok(value) = std::fs::read_to_string(&port_file) {
                if let Ok(port) = value.trim().parse::<u16>() {
                    break port;
                }
            }
            if let Some(status) = child.try_wait().expect("failed to inspect PocketIC server") {
                panic!("Stream Manager PocketIC server exited early: {status}");
            }
            assert!(
                started.elapsed() < Duration::from_secs(30),
                "Stream Manager PocketIC server did not publish its port"
            );
            thread::sleep(Duration::from_millis(20));
        };
        let _ = std::fs::remove_file(port_file);
        StreamServer {
            url: format!("http://127.0.0.1:{port}/"),
            _child: Mutex::new(child),
        }
    })
}

fn stream_pocket_ic() -> PocketIc {
    PocketIcBuilder::new()
        .with_application_subnet()
        .with_server_url(
            stream_server()
                .url
                .parse()
                .expect("dedicated PocketIC URL must parse"),
        )
        .build()
}

fn lock_stream_test() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

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
struct DebugRejectAccountArgs {
    account: String,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct DebugUnreadableArgs {
    unreadable: bool,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct DebugNnsDisbursementArgs {
    from: Account,
    to: Account,
    amount_e8s: u128,
    native_memo_u64: u64,
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

#[derive(Clone, Debug, CandidType, Deserialize)]
struct LatestRewardEventFixture {
    round: u64,
    rounds_since_last_distribution: u64,
    end_timestamp_seconds: u64,
    settled_proposal_ids: Vec<u64>,
    neuron_reward_shares: Vec<(u64, Nat)>,
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
    get_transactions: u64,
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

struct RedemptionFixture {
    pic: PocketIc,
    io_ledger: Principal,
    io_index: Principal,
    icp_ledger: Principal,
    nns: Principal,
    stream: Principal,
    user: Principal,
    user_account: Account,
    reserve: Account,
    staging: Account,
    _server_guard: MutexGuard<'static, ()>,
}

fn redemption_fixture() -> RedemptionFixture {
    let server_guard = lock_stream_test();
    let pic = stream_pocket_ic();
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
    let nns = install(&pic, "mock_nns_governance");
    let sns_root = install(&pic, "mock_sns_root");
    let governance = install(&pic, "mock_sns_governance");
    let governance_hash = pic
        .canister_status(governance, None)
        .unwrap()
        .module_hash
        .unwrap();
    let _: () = update(
        &pic,
        sns_root,
        Principal::anonymous(),
        "debug_set_governance_principal",
        governance,
    );
    update::<_, Result<(), String>>(
        &pic,
        sns_root,
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
    let stream = pic.create_canister();
    pic.add_cycles(stream, CYCLES);
    let user = Principal::from_slice(&[88; 29]);
    let user_account = Account {
        owner: user,
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
    for (ledger, to, amount) in [
        (io_ledger, reserve.clone(), 900_000_000_u128),
        (io_ledger, user_account.clone(), 300_030_000),
        (icp_ledger, liquid.clone(), 400_000_000),
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
                io_index,
                icp_ledger,
                nns_manager: nns,
                jupiter_io_account: Account {
                    owner: Principal::from_slice(&[77; 29]),
                    subaccount: None,
                },
                sns_governance: governance,
                sns_root,
                expected_sns_governance_module_hash: governance_hash,
                approved_reward_event_duration_seconds: 86_400,
                io_reserve: reserve.clone(),
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
    let mut ready: StreamStateV1 = query(&pic, stream, "debug_get_state");
    ready.lifecycle = Lifecycle::Ready;
    ready.reward_checkpoint.reward_work_due = false;
    ready.stake_observation_due = false;
    ready.structural_reconciliation_due = false;
    update::<_, Result<(), String>>(
        &pic,
        stream,
        Principal::anonymous(),
        "debug_replace_state",
        ready,
    )
    .unwrap();
    let staging = query(&pic, stream, "get_redemption_staging_account");
    RedemptionFixture {
        pic,
        io_ledger,
        io_index,
        icp_ledger,
        nns,
        stream,
        user,
        user_account,
        reserve,
        staging,
        _server_guard: server_guard,
    }
}

fn stage_redemption(fixture: &RedemptionFixture, amount: u128) -> u128 {
    stage_redemption_from(fixture, fixture.user, amount)
}

fn stage_redemption_from(fixture: &RedemptionFixture, caller: Principal, amount: u128) -> u128 {
    let result: io_ledger_boundary::IcrcTransferResult = update(
        &fixture.pic,
        fixture.io_ledger,
        caller,
        "icrc1_transfer",
        io_ledger_boundary::IcrcTransferArg {
            from_subaccount: None,
            to: fixture.staging.clone(),
            amount: candid::Nat::from(amount),
            fee: Some(candid::Nat::from(10_000_u128)),
            memo: None,
            created_at_time: None,
        },
    );
    result.unwrap().0.try_into().unwrap()
}

fn mint_io(fixture: &RedemptionFixture, account: Account, amount_e8s: u128) {
    let _: u64 = update(
        &fixture.pic,
        fixture.io_ledger,
        Principal::anonymous(),
        "debug_mint_account",
        DebugMintAccountArgs {
            to: account,
            amount_e8s,
        },
    );
}

fn set_pooled_principal(fixture: &RedemptionFixture, amount_e8s: u128) {
    let _: () = update(
        &fixture.pic,
        fixture.nns,
        Principal::anonymous(),
        "debug_set_pooled_principal",
        amount_e8s,
    );
}

fn advance_and_tick(fixture: &RedemptionFixture, seconds: u64) {
    fixture.pic.advance_time(Duration::from_secs(seconds));
    for _ in 0..40 {
        fixture.pic.tick();
    }
}

fn transfers_to(fixture: &RedemptionFixture, account: &Account) -> usize {
    update::<_, Vec<DebugLedgerTransaction>>(
        &fixture.pic,
        fixture.icp_ledger,
        Principal::anonymous(),
        "debug_get_transactions",
        (),
    )
    .into_iter()
    .filter(|tx| tx.to_account.as_ref() == Some(account))
    .count()
}

fn reserve_sweeps(fixture: &RedemptionFixture) -> usize {
    update::<_, Vec<DebugLedgerTransaction>>(
        &fixture.pic,
        fixture.io_ledger,
        Principal::anonymous(),
        "debug_get_transactions",
        (),
    )
    .into_iter()
    .filter(|tx| {
        tx.from_account.as_ref() == Some(&fixture.staging)
            && tx.to_account.as_ref() == Some(&fixture.reserve)
    })
    .count()
}

fn wake_and_tick(fixture: &RedemptionFixture) {
    let wake: Result<(), ApiError> = update(
        &fixture.pic,
        fixture.stream,
        fixture.user,
        "process_redemptions",
        (),
    );
    wake.unwrap();
    fixture.pic.advance_time(Duration::from_secs(1));
    for _ in 0..20 {
        fixture.pic.tick();
    }
}

fn tick_until_index_calls(fixture: &RedemptionFixture, expected: u64) {
    for _ in 0..30 {
        fixture.pic.tick();
        if query::<u64>(
            &fixture.pic,
            fixture.io_index,
            "debug_get_account_transaction_call_count",
        ) >= expected
        {
            return;
        }
    }
    panic!("redemption worker did not reach index call {expected}");
}

fn index_call_times(fixture: &RedemptionFixture) -> Vec<u64> {
    query(
        &fixture.pic,
        fixture.io_index,
        "debug_get_account_transaction_call_times",
    )
}

fn index_max_results(fixture: &RedemptionFixture) -> Vec<u64> {
    query(
        &fixture.pic,
        fixture.io_index,
        "debug_get_account_transaction_max_results",
    )
}

fn replace_stream_state(fixture: &RedemptionFixture, replacement: StreamStateV1) {
    update::<_, Result<(), String>>(
        &fixture.pic,
        fixture.stream,
        Principal::anonymous(),
        "debug_replace_state",
        replacement,
    )
    .unwrap();
}

#[test]
fn sustained_sybil_wake_spam_is_globally_bounded_across_time() {
    if std::env::var_os("POCKET_IC_BIN").is_none() {
        eprintln!("skipping staged-redemption PocketIC test because POCKET_IC_BIN is not set");
        return;
    }
    let fixture = redemption_fixture();
    let index_before: u64 = query(
        &fixture.pic,
        fixture.io_index,
        "debug_get_account_transaction_call_count",
    );
    let io_before: LedgerCallCounters =
        query(&fixture.pic, fixture.io_ledger, "debug_get_call_counters");
    let icp_before: LedgerCallCounters =
        query(&fixture.pic, fixture.icp_ledger, "debug_get_call_counters");

    for caller in 1..=100_u8 {
        assert_eq!(
            update::<_, Result<(), ApiError>>(
                &fixture.pic,
                fixture.stream,
                Principal::from_slice(&[caller; 29]),
                "process_redemptions",
                (),
            ),
            Ok(())
        );
    }
    fixture.pic.advance_time(Duration::from_secs(1));
    tick_until_index_calls(&fixture, index_before + 1);

    for caller in 101..=200_u8 {
        assert_eq!(
            update::<_, Result<(), ApiError>>(
                &fixture.pic,
                fixture.stream,
                Principal::from_slice(&[caller; 29]),
                "process_redemptions",
                (),
            ),
            Ok(())
        );
    }
    fixture.pic.advance_time(Duration::from_secs(8));
    for _ in 0..10 {
        fixture.pic.tick();
    }
    assert_eq!(
        query::<u64>(
            &fixture.pic,
            fixture.io_index,
            "debug_get_account_transaction_call_count"
        ),
        index_before + 1,
        "Sybil callers cannot add a worker before the global cooldown expires"
    );

    fixture.pic.advance_time(Duration::from_secs(1));
    assert_eq!(
        update::<_, Result<(), ApiError>>(
            &fixture.pic,
            fixture.stream,
            Principal::from_slice(&[201; 29]),
            "process_redemptions",
            (),
        ),
        Ok(())
    );
    fixture.pic.advance_time(Duration::from_secs(1));
    tick_until_index_calls(&fixture, index_before + 2);
    assert_eq!(
        query::<LedgerCallCounters>(&fixture.pic, fixture.io_ledger, "debug_get_call_counters")
            .get_transactions,
        io_before.get_transactions,
        "two empty workers make no canonical IO-ledger proof calls"
    );
    assert_eq!(
        query::<LedgerCallCounters>(&fixture.pic, fixture.icp_ledger, "debug_get_call_counters"),
        icp_before,
        "two empty workers make no ICP-ledger calls"
    );
    eprintln!(
        "sustained_wake_evidence admitted_workers=2 index_calls=2 canonical_io_calls=0 icp_calls=0"
    );
}

#[test]
fn scanner_filters_thirty_one_tiny_transfers_before_canonical_proof() {
    if std::env::var_os("POCKET_IC_BIN").is_none() {
        eprintln!("skipping staged-redemption PocketIC test because POCKET_IC_BIN is not set");
        return;
    }
    let fixture = redemption_fixture();
    let mut oldest_tiny = 0_u128;
    for _ in 0..31 {
        let block = stage_redemption(&fixture, 1);
        if oldest_tiny == 0 {
            oldest_tiny = block;
        }
    }
    let valid = stage_redemption(&fixture, 100_000_000);
    let index_before: u64 = query(
        &fixture.pic,
        fixture.io_index,
        "debug_get_account_transaction_call_count",
    );
    let io_before: LedgerCallCounters =
        query(&fixture.pic, fixture.io_ledger, "debug_get_call_counters");
    let icp_before: LedgerCallCounters =
        query(&fixture.pic, fixture.icp_ledger, "debug_get_call_counters");

    wake_and_tick(&fixture);

    let state: StreamStateV1 = query(&fixture.pic, fixture.stream, "debug_get_state");
    assert!(state.pending_redemption_blocks.is_empty());
    assert_eq!(
        state.redemption_scan_cursor.captured_head,
        Some(valid as u64)
    );
    assert_eq!(
        state.redemption_scan_cursor.resume_before,
        Some(oldest_tiny as u64)
    );
    assert_eq!(
        query::<u64>(
            &fixture.pic,
            fixture.io_index,
            "debug_get_account_transaction_call_count"
        ),
        index_before + 1
    );
    assert_eq!(
        query::<LedgerCallCounters>(&fixture.pic, fixture.io_ledger, "debug_get_call_counters")
            .get_transactions,
        io_before.get_transactions + 1,
        "only the valid hint consumes canonical IO-ledger proof"
    );
    assert_eq!(
        query::<LedgerCallCounters>(&fixture.pic, fixture.icp_ledger, "debug_get_call_counters")
            .transfer,
        icp_before.transfer + 1
    );
}

#[test]
fn scanner_skips_own_reserve_sweep_and_queues_new_incoming_transfer() {
    if std::env::var_os("POCKET_IC_BIN").is_none() {
        eprintln!("skipping staged-redemption PocketIC test because POCKET_IC_BIN is not set");
        return;
    }
    let fixture = redemption_fixture();
    let own_sweep: u64 = update(
        &fixture.pic,
        fixture.io_ledger,
        Principal::anonymous(),
        "debug_record_nns_disbursement",
        DebugNnsDisbursementArgs {
            from: fixture.staging.clone(),
            to: fixture.reserve.clone(),
            amount_e8s: 50_000,
            native_memo_u64: 0,
        },
    );
    let valid = stage_redemption(&fixture, 100_000_000);
    let io_before: LedgerCallCounters =
        query(&fixture.pic, fixture.io_ledger, "debug_get_call_counters");

    wake_and_tick(&fixture);

    let state: StreamStateV1 = query(&fixture.pic, fixture.stream, "debug_get_state");
    assert!(state.pending_redemption_blocks.is_empty());
    assert_eq!(
        state.redemption_scan_cursor.committed_head,
        Some(valid as u64)
    );
    assert!(valid > u128::from(own_sweep));
    assert_eq!(
        query::<LedgerCallCounters>(&fixture.pic, fixture.io_ledger, "debug_get_call_counters")
            .get_transactions,
        io_before.get_transactions + 1,
        "the outgoing sweep never enters the canonical-proof queue"
    );
}

#[test]
fn canonical_proof_discards_a_valid_hint_with_forbidden_source() {
    if std::env::var_os("POCKET_IC_BIN").is_none() {
        eprintln!("skipping staged-redemption PocketIC test because POCKET_IC_BIN is not set");
        return;
    }
    let fixture = redemption_fixture();
    let anonymous = Account {
        owner: Principal::anonymous(),
        subaccount: None,
    };
    let _: u64 = update(
        &fixture.pic,
        fixture.io_ledger,
        Principal::anonymous(),
        "debug_mint_account",
        DebugMintAccountArgs {
            to: anonymous,
            amount_e8s: 100_010_000,
        },
    );
    let hinted: io_ledger_boundary::IcrcTransferResult = update(
        &fixture.pic,
        fixture.io_ledger,
        Principal::anonymous(),
        "icrc1_transfer",
        io_ledger_boundary::IcrcTransferArg {
            from_subaccount: None,
            to: fixture.staging.clone(),
            amount: candid::Nat::from(100_000_000_u128),
            fee: Some(candid::Nat::from(10_000_u128)),
            memo: None,
            created_at_time: None,
        },
    );
    hinted.unwrap();
    let io_before: LedgerCallCounters =
        query(&fixture.pic, fixture.io_ledger, "debug_get_call_counters");
    let icp_before: LedgerCallCounters =
        query(&fixture.pic, fixture.icp_ledger, "debug_get_call_counters");

    wake_and_tick(&fixture);

    assert_eq!(
        query::<Status>(&fixture.pic, fixture.stream, "get_status").pending_redemption_candidates,
        0
    );
    assert_eq!(
        query::<LedgerCallCounters>(&fixture.pic, fixture.io_ledger, "debug_get_call_counters")
            .get_transactions,
        io_before.get_transactions + 1
    );
    assert_eq!(
        query::<LedgerCallCounters>(&fixture.pic, fixture.icp_ledger, "debug_get_call_counters")
            .transfer,
        icp_before.transfer,
        "a contradictory canonical proof authorizes zero payout"
    );
}

#[test]
fn discovered_valid_backlog_drains_near_term_without_extra_index_polls() {
    if std::env::var_os("POCKET_IC_BIN").is_none() {
        eprintln!("skipping staged-redemption PocketIC test because POCKET_IC_BIN is not set");
        return;
    }
    let fixture = redemption_fixture();
    for _ in 0..3 {
        stage_redemption(&fixture, 50_000_000);
    }
    let index_before: u64 = query(
        &fixture.pic,
        fixture.io_index,
        "debug_get_account_transaction_call_count",
    );
    let io_before: LedgerCallCounters =
        query(&fixture.pic, fixture.io_ledger, "debug_get_call_counters");
    let icp_before: LedgerCallCounters =
        query(&fixture.pic, fixture.icp_ledger, "debug_get_call_counters");

    wake_and_tick(&fixture);
    for _ in 0..2 {
        fixture.pic.advance_time(Duration::from_secs(1));
        for _ in 0..20 {
            fixture.pic.tick();
        }
    }

    assert_eq!(
        query::<Status>(&fixture.pic, fixture.stream, "get_status").pending_redemption_candidates,
        0
    );
    assert_eq!(
        query::<u64>(
            &fixture.pic,
            fixture.io_index,
            "debug_get_account_transaction_call_count"
        ),
        index_before + 1,
        "the already-discovered backlog does not multiply index polling"
    );
    assert_eq!(
        query::<LedgerCallCounters>(&fixture.pic, fixture.io_ledger, "debug_get_call_counters")
            .get_transactions,
        io_before.get_transactions + 3
    );
    assert_eq!(
        query::<LedgerCallCounters>(&fixture.pic, fixture.icp_ledger, "debug_get_call_counters")
            .transfer,
        icp_before.transfer + 3
    );
}

#[test]
fn illiquid_head_rotates_and_does_not_block_later_payable_candidate() {
    if std::env::var_os("POCKET_IC_BIN").is_none() {
        eprintln!("skipping staged-redemption PocketIC test because POCKET_IC_BIN is not set");
        return;
    }
    let fixture = redemption_fixture();
    set_pooled_principal(&fixture, 2_000_000_000);
    let later_user = Principal::from_slice(&[89; 29]);
    let later_account = Account {
        owner: later_user,
        subaccount: None,
    };
    mint_io(&fixture, later_account.clone(), 20_010_000);
    let oversized = stage_redemption(&fixture, 80_000_000) as u64;
    let payable = stage_redemption_from(&fixture, later_user, 20_000_000) as u64;
    let proofs_before =
        query::<LedgerCallCounters>(&fixture.pic, fixture.io_ledger, "debug_get_call_counters")
            .get_transactions;

    wake_and_tick(&fixture);

    let rotated: StreamStateV1 = query(&fixture.pic, fixture.stream, "debug_get_state");
    assert_eq!(rotated.pending_redemption_blocks, vec![payable, oversized]);
    assert!(rotated.active_operation.is_none());
    assert_eq!(transfers_to(&fixture, &fixture.user_account), 0);
    assert_eq!(transfers_to(&fixture, &later_account), 0);
    assert_eq!(reserve_sweeps(&fixture), 0);
    assert_eq!(
        query::<LedgerCallCounters>(&fixture.pic, fixture.io_ledger, "debug_get_call_counters")
            .get_transactions,
        proofs_before + 1
    );

    advance_and_tick(&fixture, 60);

    let completed: StreamStateV1 = query(&fixture.pic, fixture.stream, "debug_get_state");
    assert_eq!(completed.pending_redemption_blocks, vec![oversized]);
    assert!(completed.active_operation.is_none());
    assert_eq!(transfers_to(&fixture, &fixture.user_account), 0);
    assert_eq!(transfers_to(&fixture, &later_account), 1);
    assert_eq!(reserve_sweeps(&fixture), 1);
}

#[test]
fn all_illiquid_candidates_rotate_only_at_coarse_poll_cadence() {
    if std::env::var_os("POCKET_IC_BIN").is_none() {
        eprintln!("skipping staged-redemption PocketIC test because POCKET_IC_BIN is not set");
        return;
    }
    let fixture = redemption_fixture();
    set_pooled_principal(&fixture, 2_000_000_000);
    let blocks = (0..3)
        .map(|_| stage_redemption(&fixture, 80_000_000) as u64)
        .collect::<Vec<_>>();
    let io_before: LedgerCallCounters =
        query(&fixture.pic, fixture.io_ledger, "debug_get_call_counters");
    let icp_before: LedgerCallCounters =
        query(&fixture.pic, fixture.icp_ledger, "debug_get_call_counters");

    wake_and_tick(&fixture);
    assert_eq!(
        query::<StreamStateV1>(&fixture.pic, fixture.stream, "debug_get_state")
            .pending_redemption_blocks,
        vec![blocks[1], blocks[2], blocks[0]]
    );
    assert_eq!(
        query::<LedgerCallCounters>(&fixture.pic, fixture.io_ledger, "debug_get_call_counters")
            .get_transactions,
        io_before.get_transactions + 1
    );

    advance_and_tick(&fixture, 10);
    assert_eq!(
        query::<LedgerCallCounters>(&fixture.pic, fixture.io_ledger, "debug_get_call_counters")
            .get_transactions,
        io_before.get_transactions + 1,
        "illiquidity must not install a one-second proof loop"
    );
    assert_eq!(
        query::<LedgerCallCounters>(&fixture.pic, fixture.icp_ledger, "debug_get_call_counters")
            .transfer,
        icp_before.transfer
    );

    advance_and_tick(&fixture, 50);
    assert_eq!(
        query::<StreamStateV1>(&fixture.pic, fixture.stream, "debug_get_state")
            .pending_redemption_blocks,
        vec![blocks[2], blocks[0], blocks[1]]
    );
    assert_eq!(
        query::<LedgerCallCounters>(&fixture.pic, fixture.io_ledger, "debug_get_call_counters")
            .get_transactions,
        io_before.get_transactions + 2,
        "one ordinary poll retries only one service head"
    );
    assert_eq!(
        query::<LedgerCallCounters>(&fixture.pic, fixture.icp_ledger, "debug_get_call_counters")
            .transfer,
        icp_before.transfer
    );
}

#[test]
fn payable_candidate_after_three_illiquid_candidates_is_reached_by_coarse_rotations() {
    if std::env::var_os("POCKET_IC_BIN").is_none() {
        eprintln!("skipping staged-redemption PocketIC test because POCKET_IC_BIN is not set");
        return;
    }
    let fixture = redemption_fixture();
    set_pooled_principal(&fixture, 2_000_000_000);
    let later_user = Principal::from_slice(&[90; 29]);
    let later_account = Account {
        owner: later_user,
        subaccount: None,
    };
    mint_io(&fixture, later_account.clone(), 10_010_000);
    let illiquid = [80_000_000, 70_000_000, 60_000_000]
        .into_iter()
        .map(|amount| stage_redemption(&fixture, amount) as u64)
        .collect::<Vec<_>>();
    let payable = stage_redemption_from(&fixture, later_user, 10_000_000) as u64;

    wake_and_tick(&fixture);
    assert_eq!(
        query::<StreamStateV1>(&fixture.pic, fixture.stream, "debug_get_state")
            .pending_redemption_blocks,
        vec![illiquid[1], illiquid[2], payable, illiquid[0]]
    );
    advance_and_tick(&fixture, 60);
    advance_and_tick(&fixture, 60);
    assert_eq!(transfers_to(&fixture, &later_account), 0);
    advance_and_tick(&fixture, 60);

    let completed: StreamStateV1 = query(&fixture.pic, fixture.stream, "debug_get_state");
    assert_eq!(completed.pending_redemption_blocks, illiquid);
    assert!(completed.active_operation.is_none());
    assert_eq!(transfers_to(&fixture, &later_account), 1);
    assert_eq!(reserve_sweeps(&fixture), 1);
}

#[test]
fn rotated_service_queue_survives_upgrade_and_keeps_payable_candidate_reachable() {
    if std::env::var_os("POCKET_IC_BIN").is_none() {
        eprintln!("skipping staged-redemption PocketIC test because POCKET_IC_BIN is not set");
        return;
    }
    let fixture = redemption_fixture();
    set_pooled_principal(&fixture, 2_000_000_000);
    let later_user = Principal::from_slice(&[91; 29]);
    let later_account = Account {
        owner: later_user,
        subaccount: None,
    };
    mint_io(&fixture, later_account.clone(), 20_010_000);
    let oversized = stage_redemption(&fixture, 80_000_000) as u64;
    let payable = stage_redemption_from(&fixture, later_user, 20_000_000) as u64;
    wake_and_tick(&fixture);
    assert_eq!(
        query::<StreamStateV1>(&fixture.pic, fixture.stream, "debug_get_state")
            .pending_redemption_blocks,
        vec![payable, oversized]
    );

    fixture
        .pic
        .upgrade_canister(
            fixture.stream,
            debug_wasm("io_stream_manager"),
            encode_one(()).unwrap(),
            None,
        )
        .unwrap();
    let restored: StreamStateV1 = query(&fixture.pic, fixture.stream, "debug_get_state");
    assert_eq!(restored.lifecycle, Lifecycle::Paused);
    assert_eq!(restored.pending_redemption_blocks, vec![payable, oversized]);
    assert!(restored.active_operation.is_none());

    advance_and_tick(&fixture, 1);
    assert_eq!(
        query::<Status>(&fixture.pic, fixture.stream, "get_status").lifecycle,
        Lifecycle::Ready
    );
    let mut ready: StreamStateV1 = query(&fixture.pic, fixture.stream, "debug_get_state");
    ready.reward_checkpoint.reward_work_due = false;
    ready.stake_observation_due = false;
    ready.structural_reconciliation_due = false;
    update::<_, Result<(), String>>(
        &fixture.pic,
        fixture.stream,
        Principal::anonymous(),
        "debug_replace_state",
        ready,
    )
    .unwrap();
    advance_and_tick(&fixture, 60);

    let completed: StreamStateV1 = query(&fixture.pic, fixture.stream, "debug_get_state");
    assert_eq!(completed.pending_redemption_blocks, vec![oversized]);
    assert_eq!(transfers_to(&fixture, &fixture.user_account), 0);
    assert_eq!(transfers_to(&fixture, &later_account), 1);
    assert_eq!(reserve_sweeps(&fixture), 1);
}

#[test]
fn first_payout_definitive_no_effect_defers_candidate_at_coarse_cadence() {
    if std::env::var_os("POCKET_IC_BIN").is_none() {
        eprintln!("skipping staged-redemption PocketIC test because POCKET_IC_BIN is not set");
        return;
    }
    let fixture = redemption_fixture();
    let later_user = Principal::from_slice(&[92; 29]);
    let later_account = Account {
        owner: later_user,
        subaccount: None,
    };
    mint_io(&fixture, later_account.clone(), 20_010_000);
    let first = stage_redemption(&fixture, 20_000_000) as u64;
    let later = stage_redemption_from(&fixture, later_user, 20_000_000) as u64;
    let _: () = update(
        &fixture.pic,
        fixture.icp_ledger,
        Principal::anonymous(),
        "debug_return_too_old_next",
        (),
    );

    wake_and_tick(&fixture);
    assert_eq!(
        query::<StreamStateV1>(&fixture.pic, fixture.stream, "debug_get_state")
            .pending_redemption_blocks,
        vec![later, first]
    );
    assert_eq!(transfers_to(&fixture, &fixture.user_account), 0);
    assert_eq!(transfers_to(&fixture, &later_account), 0);
    advance_and_tick(&fixture, 60);

    let completed: StreamStateV1 = query(&fixture.pic, fixture.stream, "debug_get_state");
    assert_eq!(completed.pending_redemption_blocks, vec![first]);
    assert_eq!(transfers_to(&fixture, &fixture.user_account), 0);
    assert_eq!(transfers_to(&fixture, &later_account), 1);
    assert_eq!(reserve_sweeps(&fixture), 1);
}

#[test]
fn coarse_poll_discovers_later_payable_candidate_behind_retained_illiquid_head() {
    if std::env::var_os("POCKET_IC_BIN").is_none() {
        eprintln!("skipping staged-redemption PocketIC test because POCKET_IC_BIN is not set");
        return;
    }
    let fixture = redemption_fixture();
    set_pooled_principal(&fixture, 2_000_000_000);
    let later_user = Principal::from_slice(&[93; 29]);
    let later_account = Account {
        owner: later_user,
        subaccount: None,
    };
    mint_io(&fixture, later_account.clone(), 20_010_000);
    let oversized = stage_redemption(&fixture, 80_000_000) as u64;
    wake_and_tick(&fixture);
    assert_eq!(
        query::<StreamStateV1>(&fixture.pic, fixture.stream, "debug_get_state")
            .pending_redemption_blocks,
        vec![oversized]
    );

    let payable = stage_redemption_from(&fixture, later_user, 20_000_000) as u64;
    let index_before = index_call_times(&fixture).len();
    advance_and_tick(&fixture, 60);

    let rotated: StreamStateV1 = query(&fixture.pic, fixture.stream, "debug_get_state");
    assert_eq!(index_call_times(&fixture).len(), index_before + 1);
    assert_eq!(rotated.pending_redemption_blocks, vec![payable, oversized]);
    assert_eq!(transfers_to(&fixture, &later_account), 0);
    advance_and_tick(&fixture, 60);

    let completed: StreamStateV1 = query(&fixture.pic, fixture.stream, "debug_get_state");
    assert_eq!(completed.pending_redemption_blocks, vec![oversized]);
    assert_eq!(transfers_to(&fixture, &fixture.user_account), 0);
    assert_eq!(transfers_to(&fixture, &later_account), 1);
    assert_eq!(reserve_sweeps(&fixture), 1);
}

#[test]
fn retained_illiquid_candidate_does_not_freeze_captured_scanner_continuation() {
    if std::env::var_os("POCKET_IC_BIN").is_none() {
        eprintln!("skipping staged-redemption PocketIC test because POCKET_IC_BIN is not set");
        return;
    }
    let fixture = redemption_fixture();
    set_pooled_principal(&fixture, 2_000_000_000);
    let earlier_user = Principal::from_slice(&[94; 29]);
    let earlier_account = Account {
        owner: earlier_user,
        subaccount: None,
    };
    mint_io(&fixture, earlier_account.clone(), 20_010_000);
    let payable = stage_redemption_from(&fixture, earlier_user, 20_000_000) as u64;
    for _ in 0..31 {
        stage_redemption(&fixture, 1);
    }
    let oversized = stage_redemption(&fixture, 80_000_000) as u64;

    wake_and_tick(&fixture);
    let captured: StreamStateV1 = query(&fixture.pic, fixture.stream, "debug_get_state");
    assert_eq!(captured.pending_redemption_blocks, vec![oversized]);
    assert!(captured.redemption_scan_cursor.resume_before.is_some());
    let index_before = index_call_times(&fixture).len();
    advance_and_tick(&fixture, 60);

    let continued: StreamStateV1 = query(&fixture.pic, fixture.stream, "debug_get_state");
    assert_eq!(index_call_times(&fixture).len(), index_before + 1);
    assert_eq!(continued.redemption_scan_cursor.resume_before, None);
    assert_eq!(
        continued.pending_redemption_blocks,
        vec![payable, oversized]
    );
    assert_eq!(transfers_to(&fixture, &earlier_account), 0);
    advance_and_tick(&fixture, 60);
    assert_eq!(transfers_to(&fixture, &earlier_account), 1);
    assert_eq!(reserve_sweeps(&fixture), 1);
    assert_eq!(
        query::<StreamStateV1>(&fixture.pic, fixture.stream, "debug_get_state")
            .pending_redemption_blocks,
        vec![oversized]
    );
}

#[test]
fn scanner_outage_does_not_block_already_discovered_payable_candidate() {
    if std::env::var_os("POCKET_IC_BIN").is_none() {
        eprintln!("skipping staged-redemption PocketIC test because POCKET_IC_BIN is not set");
        return;
    }
    let fixture = redemption_fixture();
    set_pooled_principal(&fixture, 2_000_000_000);
    let payable = stage_redemption(&fixture, 80_000_000) as u64;
    wake_and_tick(&fixture);
    assert_eq!(
        query::<StreamStateV1>(&fixture.pic, fixture.stream, "debug_get_state")
            .pending_redemption_blocks,
        vec![payable]
    );
    set_pooled_principal(&fixture, 0);
    let _: () = update(
        &fixture.pic,
        fixture.io_index,
        Principal::anonymous(),
        "debug_set_unreadable",
        DebugUnreadableArgs { unreadable: true },
    );
    let index_before = index_call_times(&fixture).len();
    advance_and_tick(&fixture, 60);

    let completed: StreamStateV1 = query(&fixture.pic, fixture.stream, "debug_get_state");
    assert_eq!(index_call_times(&fixture).len(), index_before + 1);
    assert!(completed.redemption_scan_cursor.last_error.is_some());
    assert!(completed.pending_redemption_blocks.is_empty());
    assert_eq!(transfers_to(&fixture, &fixture.user_account), 1);
    assert_eq!(reserve_sweeps(&fixture), 1);
}

#[test]
fn capacity_limited_page_uses_available_slots_and_preserves_continuation() {
    if std::env::var_os("POCKET_IC_BIN").is_none() {
        eprintln!("skipping staged-redemption PocketIC test because POCKET_IC_BIN is not set");
        return;
    }
    let fixture = redemption_fixture();
    set_pooled_principal(&fixture, 2_000_000_000);
    let oversized = stage_redemption(&fixture, 80_000_000) as u64;
    wake_and_tick(&fixture);
    let dust = (0..59)
        .map(|_| stage_redemption(&fixture, 1) as u64)
        .collect::<Vec<_>>();
    let candidates = (0..6)
        .map(|_| stage_redemption(&fixture, 20_000) as u64)
        .collect::<Vec<_>>();
    let mut prepopulated: StreamStateV1 = query(&fixture.pic, fixture.stream, "debug_get_state");
    prepopulated.pending_redemption_blocks.extend(dust.clone());
    replace_stream_state(&fixture, prepopulated);
    let index_before = index_call_times(&fixture).len();
    let limit_before = index_max_results(&fixture).len();

    advance_and_tick(&fixture, 60);

    let full: StreamStateV1 = query(&fixture.pic, fixture.stream, "debug_get_state");
    assert_eq!(index_call_times(&fixture).len(), index_before + 1);
    assert_eq!(index_max_results(&fixture)[limit_before], 4);
    assert_eq!(full.pending_redemption_blocks.len(), 64);
    for block in &candidates[2..] {
        assert!(full.pending_redemption_blocks.contains(block));
    }
    assert_eq!(
        full.redemption_scan_cursor.captured_head,
        candidates.last().copied()
    );
    assert_eq!(
        full.redemption_scan_cursor.resume_before,
        Some(candidates[2])
    );
    assert!(full
        .redemption_scan_cursor
        .committed_head
        .is_some_and(|head| head < candidates[0]));

    advance_and_tick(&fixture, 60);
    let slot_open: StreamStateV1 = query(&fixture.pic, fixture.stream, "debug_get_state");
    assert_eq!(slot_open.pending_redemption_blocks.len(), 63);
    assert_eq!(index_call_times(&fixture).len(), index_before + 1);
    advance_and_tick(&fixture, 1);

    let continued: StreamStateV1 = query(&fixture.pic, fixture.stream, "debug_get_state");
    assert_eq!(index_max_results(&fixture).len(), limit_before + 2);
    assert_eq!(index_max_results(&fixture)[limit_before + 1], 1);
    assert!(continued.pending_redemption_blocks.contains(&candidates[1]));
    assert_eq!(
        continued.redemption_scan_cursor.resume_before,
        Some(candidates[1])
    );
    assert_eq!(
        continued.redemption_scan_cursor.captured_head,
        candidates.last().copied()
    );
    assert_eq!(continued.pending_redemption_blocks.len(), 63);
    assert!(continued.pending_redemption_blocks.contains(&oversized));
}

#[test]
fn full_service_queue_backpressures_discovery_but_still_services_one_head() {
    if std::env::var_os("POCKET_IC_BIN").is_none() {
        eprintln!("skipping staged-redemption PocketIC test because POCKET_IC_BIN is not set");
        return;
    }
    let fixture = redemption_fixture();
    set_pooled_principal(&fixture, 2_000_000_000);
    let oversized = stage_redemption(&fixture, 80_000_000) as u64;
    wake_and_tick(&fixture);
    let dust = (0..63)
        .map(|_| stage_redemption(&fixture, 1) as u64)
        .collect::<Vec<_>>();
    let later_user = Principal::from_slice(&[95; 29]);
    let later_account = Account {
        owner: later_user,
        subaccount: None,
    };
    mint_io(&fixture, later_account.clone(), 20_010_000);
    let undiscovered = stage_redemption_from(&fixture, later_user, 20_000_000) as u64;
    let mut full: StreamStateV1 = query(&fixture.pic, fixture.stream, "debug_get_state");
    full.pending_redemption_blocks.extend(dust.clone());
    replace_stream_state(&fixture, full.clone());
    let cursor_before = full.redemption_scan_cursor;
    let index_before = index_call_times(&fixture).len();
    let proofs_before =
        query::<LedgerCallCounters>(&fixture.pic, fixture.io_ledger, "debug_get_call_counters")
            .get_transactions;

    advance_and_tick(&fixture, 60);

    let rotated: StreamStateV1 = query(&fixture.pic, fixture.stream, "debug_get_state");
    assert_eq!(index_call_times(&fixture).len(), index_before);
    assert_eq!(rotated.redemption_scan_cursor, cursor_before);
    assert_eq!(rotated.pending_redemption_blocks.len(), 64);
    assert_eq!(rotated.pending_redemption_blocks.first(), dust.first());
    assert_eq!(rotated.pending_redemption_blocks.last(), Some(&oversized));
    assert!(!rotated.pending_redemption_blocks.contains(&undiscovered));
    assert_eq!(
        query::<LedgerCallCounters>(&fixture.pic, fixture.io_ledger, "debug_get_call_counters")
            .get_transactions,
        proofs_before + 1
    );
    assert_eq!(transfers_to(&fixture, &later_account), 0);
    assert_eq!(reserve_sweeps(&fixture), 0);
}

#[test]
fn simplified_stream_installs_paused_retries_and_has_no_proposal_controls() {
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
    let _server_guard = lock_stream_test();
    let pic = stream_pocket_ic();
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
    assert!(pic
        .query_call(
            canister,
            Principal::anonymous(),
            "validate_set_paused",
            encode_one(false).unwrap(),
        )
        .is_err());
    assert!(pic
        .update_call(
            canister,
            governance,
            "set_paused",
            encode_one(false).unwrap(),
        )
        .is_err());
    pic.advance_time(Duration::from_secs(1));
    for _ in 0..20 {
        pic.tick();
    }
    assert_eq!(
        query::<Status>(&pic, canister, "get_status").lifecycle,
        Lifecycle::Paused
    );
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
    pic.advance_time(Duration::from_secs(1));
    for _ in 0..20 {
        pic.tick();
    }
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
    let result: Result<(), ApiError> = decode_one(
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
}

#[test]
fn staged_redemption_wake_is_local_coalesced_and_economically_exact_once() {
    if std::env::var_os("POCKET_IC_BIN").is_none() {
        eprintln!("skipping staged-redemption PocketIC test because POCKET_IC_BIN is not set");
        return;
    }
    let _server_guard = lock_stream_test();
    let pic = stream_pocket_ic();
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
    let nns = install(&pic, "mock_nns_governance");
    let stream = pic.create_canister();
    pic.add_cycles(stream, CYCLES);
    let user = Principal::from_slice(&[88; 29]);
    let user_account = Account {
        owner: user,
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
    for (ledger, to, amount) in [
        (io_ledger, reserve.clone(), 900_000_000_u128),
        (io_ledger, user_account.clone(), 100_010_000),
        (icp_ledger, liquid.clone(), 200_000_000),
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
                io_index,
                icp_ledger,
                nns_manager: nns,
                jupiter_io_account: Account {
                    owner: Principal::from_slice(&[77; 29]),
                    subaccount: None,
                },
                sns_governance: Principal::from_slice(&[5; 29]),
                sns_root: Principal::from_slice(&[6; 29]),
                expected_sns_governance_module_hash: vec![8; 32],
                approved_reward_event_duration_seconds: 86_400,
                io_reserve: reserve.clone(),
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
    let mut ready: StreamStateV1 = query(&pic, stream, "debug_get_state");
    ready.lifecycle = Lifecycle::Ready;
    ready.reward_checkpoint.reward_work_due = false;
    ready.stake_observation_due = false;
    ready.structural_reconciliation_due = true;
    update::<_, Result<(), String>>(
        &pic,
        stream,
        Principal::anonymous(),
        "debug_replace_state",
        ready,
    )
    .unwrap();
    let staging: Account = query(&pic, stream, "get_redemption_staging_account");
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
    staged.unwrap();
    let index_before: u64 = query(&pic, io_index, "debug_get_account_transaction_call_count");
    let payout_before: LedgerCallCounters = query(&pic, icp_ledger, "debug_get_call_counters");
    for caller in 1..=100_u8 {
        let result: Result<(), ApiError> = update(
            &pic,
            stream,
            Principal::from_slice(&[caller; 29]),
            "process_redemptions",
            (),
        );
        assert_eq!(result, Ok(()));
    }
    assert_eq!(
        query::<u64>(&pic, io_index, "debug_get_account_transaction_call_count"),
        index_before
    );
    assert_eq!(
        query::<LedgerCallCounters>(&pic, icp_ledger, "debug_get_call_counters").transfer,
        payout_before.transfer,
        "wake hints perform no external work in their own invocation"
    );
    pic.advance_time(Duration::from_secs(1));
    for _ in 0..10 {
        pic.tick();
    }
    assert_eq!(
        query::<u64>(&pic, io_index, "debug_get_account_transaction_call_count"),
        index_before,
        "higher-priority structural work delays redemption without external scan work"
    );
    let mut unblocked: StreamStateV1 = query(&pic, stream, "debug_get_state");
    unblocked.structural_reconciliation_due = false;
    update::<_, Result<(), String>>(
        &pic,
        stream,
        Principal::anonymous(),
        "debug_replace_state",
        unblocked,
    )
    .unwrap();
    pic.advance_time(Duration::from_secs(60));
    for _ in 0..30 {
        pic.tick();
        if query::<Status>(&pic, stream, "get_status").pending_redemption_candidates == 0
            && query::<Status>(&pic, stream, "get_status")
                .operation_kind
                .is_none()
            && query::<u64>(&pic, io_index, "debug_get_account_transaction_call_count")
                > index_before
        {
            break;
        }
    }
    let final_status: Status = query(&pic, stream, "get_status");
    assert_eq!(final_status.pending_redemption_candidates, 0);
    assert!(final_status.operation_kind.is_none());
    assert_eq!(
        query::<LedgerCallCounters>(&pic, icp_ledger, "debug_get_call_counters").transfer,
        payout_before.transfer + 1,
        "one staging block causes one ICP payout"
    );
    let sweeps = update::<_, Vec<DebugLedgerTransaction>>(
        &pic,
        io_ledger,
        Principal::anonymous(),
        "debug_get_transactions",
        (),
    )
    .into_iter()
    .filter(|tx| {
        tx.from_account.as_ref() == Some(&staging) && tx.to_account.as_ref() == Some(&reserve)
    })
    .collect::<Vec<_>>();
    assert_eq!(
        sweeps.len(),
        1,
        "one staging block causes one reserve sweep"
    );
    assert_eq!(sweeps[0].amount_e8s, 99_990_000);
    assert_eq!(sweeps[0].fee_e8s, Some(10_000));
}

#[test]
fn ambiguous_payout_survives_upgrade_and_exact_retry_pays_once() {
    if std::env::var_os("POCKET_IC_BIN").is_none() {
        eprintln!("skipping staged-redemption PocketIC test because POCKET_IC_BIN is not set");
        return;
    }
    let fixture = redemption_fixture();
    let _: () = update(
        &fixture.pic,
        fixture.icp_ledger,
        Principal::anonymous(),
        "debug_commit_then_unavailable_to",
        DebugRejectAccountArgs {
            account: fixture.user.to_text(),
        },
    );
    stage_redemption(&fixture, 100_000_000);
    wake_and_tick(&fixture);
    let ambiguous: Status = query(&fixture.pic, fixture.stream, "get_status");
    assert_eq!(
        ambiguous.operation_phase.as_deref(),
        Some("PayoutSubmitted")
    );
    let io_before_upgrade: LedgerCallCounters =
        query(&fixture.pic, fixture.io_ledger, "debug_get_call_counters");
    let icp_before_upgrade: LedgerCallCounters =
        query(&fixture.pic, fixture.icp_ledger, "debug_get_call_counters");

    fixture
        .pic
        .upgrade_canister(
            fixture.stream,
            debug_wasm("io_stream_manager"),
            encode_one(()).unwrap(),
            None,
        )
        .unwrap();
    assert_eq!(
        query::<LedgerCallCounters>(&fixture.pic, fixture.io_ledger, "debug_get_call_counters"),
        io_before_upgrade,
        "readiness rejects before IO monetary reads"
    );
    assert_eq!(
        query::<LedgerCallCounters>(&fixture.pic, fixture.icp_ledger, "debug_get_call_counters"),
        icp_before_upgrade,
        "readiness rejects before ICP monetary reads"
    );
    fixture.pic.advance_time(Duration::from_secs(1));
    let resumed: Result<io_stream_manager::StreamProgress, ApiError> = update(
        &fixture.pic,
        fixture.stream,
        Principal::from_slice(&[99; 29]),
        "resume",
        (),
    );
    assert!(matches!(
        resumed,
        Ok(io_stream_manager::StreamProgress::Redemption(
            io_stream_manager::RedemptionProgress::Completed
        ))
    ));
    let io_after_resume: LedgerCallCounters =
        query(&fixture.pic, fixture.io_ledger, "debug_get_call_counters");
    let icp_after_resume: LedgerCallCounters =
        query(&fixture.pic, fixture.icp_ledger, "debug_get_call_counters");
    assert_eq!(io_after_resume.transfer, io_before_upgrade.transfer + 1);
    assert_eq!(icp_after_resume.transfer, icp_before_upgrade.transfer + 1);
    eprintln!(
        "redemption_resume_evidence arbitrary_caller=true icp_retry_calls=1 io_sweep_calls=1"
    );
    let payouts = update::<_, Vec<DebugLedgerTransaction>>(
        &fixture.pic,
        fixture.icp_ledger,
        Principal::anonymous(),
        "debug_get_transactions",
        (),
    )
    .into_iter()
    .filter(|tx| tx.to_account.as_ref() == Some(&fixture.user_account))
    .collect::<Vec<_>>();
    assert_eq!(payouts.len(), 1, "ambiguous retry must not pay twice");
    let complete: Status = query(&fixture.pic, fixture.stream, "get_status");
    assert!(complete.operation_kind.is_none());
    assert_eq!(complete.pending_redemption_candidates, 0);

    advance_and_tick(&fixture, 60);
    assert_eq!(
        query::<Status>(&fixture.pic, fixture.stream, "get_status").lifecycle,
        Lifecycle::Ready
    );
    assert!(
        query::<DebugSchedulerStatus>(&fixture.pic, fixture.stream, "debug_get_scheduler_status")
            .active_deadline_seconds
            .is_some(),
        "ordinary readiness installs the reward timer"
    );
    let mut ready: StreamStateV1 = query(&fixture.pic, fixture.stream, "debug_get_state");
    ready.reward_checkpoint.reward_work_due = false;
    ready.stake_observation_due = false;
    ready.structural_reconciliation_due = false;
    update::<_, Result<(), String>>(
        &fixture.pic,
        fixture.stream,
        Principal::anonymous(),
        "debug_replace_state",
        ready,
    )
    .unwrap();
    stage_redemption(&fixture, 50_000_000);
    fixture.pic.advance_time(Duration::from_secs(60));
    for _ in 0..30 {
        fixture.pic.tick();
        if query::<Status>(&fixture.pic, fixture.stream, "get_status").pending_redemption_candidates
            == 0
            && update::<_, Vec<DebugLedgerTransaction>>(
                &fixture.pic,
                fixture.icp_ledger,
                Principal::anonymous(),
                "debug_get_transactions",
                (),
            )
            .into_iter()
            .filter(|tx| tx.to_account.as_ref() == Some(&fixture.user_account))
            .count()
                == 2
        {
            break;
        }
    }
    assert_eq!(
        update::<_, Vec<DebugLedgerTransaction>>(
            &fixture.pic,
            fixture.icp_ledger,
            Principal::anonymous(),
            "debug_get_transactions",
            (),
        )
        .into_iter()
        .filter(|tx| tx.to_account.as_ref() == Some(&fixture.user_account))
        .count(),
        2,
        "ordinary readiness reinstalls the coarse redemption timer"
    );
}

#[test]
fn first_definitive_no_effect_requeues_for_a_fresh_safe_attempt() {
    if std::env::var_os("POCKET_IC_BIN").is_none() {
        eprintln!("skipping staged-redemption PocketIC test because POCKET_IC_BIN is not set");
        return;
    }
    let fixture = redemption_fixture();
    let _: () = update(
        &fixture.pic,
        fixture.icp_ledger,
        Principal::anonymous(),
        "debug_return_too_old_next",
        (),
    );
    stage_redemption(&fixture, 100_000_000);
    wake_and_tick(&fixture);
    let requeued: Status = query(&fixture.pic, fixture.stream, "get_status");
    assert!(requeued.operation_kind.is_none());
    assert_eq!(requeued.pending_redemption_candidates, 1);
    assert!(update::<_, Vec<DebugLedgerTransaction>>(
        &fixture.pic,
        fixture.icp_ledger,
        Principal::anonymous(),
        "debug_get_transactions",
        (),
    )
    .into_iter()
    .all(|tx| tx.to_account.as_ref() != Some(&fixture.user_account)));

    advance_and_tick(&fixture, 60);
    let complete: Status = query(&fixture.pic, fixture.stream, "get_status");
    assert!(complete.operation_kind.is_none());
    assert_eq!(complete.pending_redemption_candidates, 0);
    let payouts = update::<_, Vec<DebugLedgerTransaction>>(
        &fixture.pic,
        fixture.icp_ledger,
        Principal::anonymous(),
        "debug_get_transactions",
        (),
    )
    .into_iter()
    .filter(|tx| tx.to_account.as_ref() == Some(&fixture.user_account))
    .count();
    assert_eq!(payouts, 1);
}

#[test]
fn newest_first_multi_page_scan_survives_queue_drain_upgrade_and_new_arrival() {
    if std::env::var_os("POCKET_IC_BIN").is_none() {
        eprintln!("skipping staged-redemption PocketIC test because POCKET_IC_BIN is not set");
        return;
    }
    let fixture = redemption_fixture();
    let mut newest_tiny_block = 0_u128;
    for _ in 0..70 {
        newest_tiny_block = stage_redemption(&fixture, 1);
    }
    let index_before: u64 = query(
        &fixture.pic,
        fixture.io_index,
        "debug_get_account_transaction_call_count",
    );
    let io_before: LedgerCallCounters =
        query(&fixture.pic, fixture.io_ledger, "debug_get_call_counters");
    wake_and_tick(&fixture);
    let before_upgrade: StreamStateV1 = query(&fixture.pic, fixture.stream, "debug_get_state");
    assert!(before_upgrade.pending_redemption_blocks.is_empty());
    assert_eq!(before_upgrade.redemption_scan_cursor.committed_head, None);
    assert_eq!(
        before_upgrade.redemption_scan_cursor.captured_head,
        Some(newest_tiny_block.try_into().unwrap())
    );
    assert!(before_upgrade
        .redemption_scan_cursor
        .resume_before
        .is_some());

    fixture
        .pic
        .upgrade_canister(
            fixture.stream,
            debug_wasm("io_stream_manager"),
            encode_one(()).unwrap(),
            None,
        )
        .unwrap();
    let mut restored: StreamStateV1 = query(&fixture.pic, fixture.stream, "debug_get_state");
    assert_eq!(
        restored.redemption_scan_cursor,
        before_upgrade.redemption_scan_cursor
    );
    restored.lifecycle = Lifecycle::Ready;
    restored.reward_checkpoint.reward_work_due = false;
    restored.stake_observation_due = false;
    restored.structural_reconciliation_due = false;
    update::<_, Result<(), String>>(
        &fixture.pic,
        fixture.stream,
        Principal::anonymous(),
        "debug_replace_state",
        restored,
    )
    .unwrap();

    for _ in 0..4 {
        wake_and_tick(&fixture);
        let state: StreamStateV1 = query(&fixture.pic, fixture.stream, "debug_get_state");
        if state.pending_redemption_blocks.is_empty()
            && state.redemption_scan_cursor.resume_before.is_none()
            && state.redemption_scan_cursor.committed_head
                == Some(newest_tiny_block.try_into().unwrap())
        {
            break;
        }
        fixture.pic.advance_time(Duration::from_secs(9));
    }
    let caught_up: StreamStateV1 = query(&fixture.pic, fixture.stream, "debug_get_state");
    assert!(caught_up.pending_redemption_blocks.is_empty());
    assert_eq!(caught_up.redemption_scan_cursor.resume_before, None);
    assert_eq!(
        caught_up.redemption_scan_cursor.committed_head,
        Some(newest_tiny_block.try_into().unwrap())
    );

    let valid_block = stage_redemption(&fixture, 100_000_000);
    assert!(valid_block > newest_tiny_block);
    fixture.pic.advance_time(Duration::from_secs(9));
    wake_and_tick(&fixture);
    let completed: Status = query(&fixture.pic, fixture.stream, "get_status");
    assert!(completed.operation_kind.is_none());
    assert_eq!(completed.pending_redemption_candidates, 0);
    assert_eq!(
        update::<_, Vec<DebugLedgerTransaction>>(
            &fixture.pic,
            fixture.icp_ledger,
            Principal::anonymous(),
            "debug_get_transactions",
            (),
        )
        .into_iter()
        .filter(|tx| tx.to_account.as_ref() == Some(&fixture.user_account))
        .count(),
        1
    );
    assert_eq!(
        query::<u64>(
            &fixture.pic,
            fixture.io_index,
            "debug_get_account_transaction_call_count"
        ),
        index_before + 4,
        "three all-ID pages plus one new-head page preserve coverage"
    );
    assert_eq!(
        query::<LedgerCallCounters>(&fixture.pic, fixture.io_ledger, "debug_get_call_counters")
            .get_transactions,
        io_before.get_transactions + 1,
        "seventy filtered tiny transfers consume no canonical proof slots"
    );
}

#[test]
fn buried_valid_redemption_drains_captured_filtered_pages_without_public_followup() {
    if std::env::var_os("POCKET_IC_BIN").is_none() {
        eprintln!("skipping staged-redemption PocketIC test because POCKET_IC_BIN is not set");
        return;
    }
    let fixture = redemption_fixture();
    let valid_block = stage_redemption(&fixture, 100_000_000);
    let mut newest_tiny_block = 0_u128;
    for _ in 0..70 {
        newest_tiny_block = stage_redemption(&fixture, 1);
    }
    let index_before = index_call_times(&fixture).len();
    let io_before: LedgerCallCounters =
        query(&fixture.pic, fixture.io_ledger, "debug_get_call_counters");
    let icp_before: LedgerCallCounters =
        query(&fixture.pic, fixture.icp_ledger, "debug_get_call_counters");

    assert_eq!(
        update::<_, Result<(), ApiError>>(
            &fixture.pic,
            fixture.stream,
            fixture.user,
            "process_redemptions",
            (),
        ),
        Ok(())
    );
    for expected in 1..=3 {
        fixture.pic.advance_time(Duration::from_secs(1));
        tick_until_index_calls(&fixture, index_before as u64 + expected);
    }
    for _ in 0..50 {
        fixture.pic.tick();
        let status: Status = query(&fixture.pic, fixture.stream, "get_status");
        if status.operation_kind.is_none() && status.pending_redemption_candidates == 0 {
            break;
        }
    }

    let state: StreamStateV1 = query(&fixture.pic, fixture.stream, "debug_get_state");
    assert_eq!(state.redemption_scan_cursor.resume_before, None);
    assert_eq!(
        state.redemption_scan_cursor.committed_head,
        Some(newest_tiny_block as u64),
        "all 71 returned IDs advance the captured interval to its head"
    );
    assert!(valid_block < newest_tiny_block);
    let times = index_call_times(&fixture);
    let continuation_times = &times[index_before..];
    assert_eq!(
        continuation_times.len(),
        3,
        "71 entries require three pages"
    );
    for pair in continuation_times.windows(2) {
        assert!(
            pair[1].saturating_sub(pair[0]) < 60_000_000_000,
            "captured-page continuation must run before the coarse poll"
        );
    }
    let continuation_gaps_nanos = continuation_times
        .windows(2)
        .map(|pair| pair[1].saturating_sub(pair[0]))
        .collect::<Vec<_>>();
    assert_eq!(
        query::<LedgerCallCounters>(&fixture.pic, fixture.io_ledger, "debug_get_call_counters")
            .get_transactions,
        io_before.get_transactions + 1,
        "seventy filtered transfers consume zero canonical proofs"
    );
    assert_eq!(
        query::<LedgerCallCounters>(&fixture.pic, fixture.icp_ledger, "debug_get_call_counters")
            .transfer,
        icp_before.transfer + 1,
        "the buried valid transfer receives exactly one payout"
    );
    assert_eq!(
        query::<LedgerCallCounters>(&fixture.pic, fixture.io_ledger, "debug_get_call_counters")
            .transfer,
        io_before.transfer + 1,
        "the buried valid transfer receives exactly one reserve sweep"
    );
    assert_eq!(
        update::<_, Vec<DebugLedgerTransaction>>(
            &fixture.pic,
            fixture.icp_ledger,
            Principal::anonymous(),
            "debug_get_transactions",
            (),
        )
        .into_iter()
        .filter(|tx| tx.to_account.as_ref() == Some(&fixture.user_account))
        .count(),
        1
    );
    assert_eq!(
        update::<_, Vec<DebugLedgerTransaction>>(
            &fixture.pic,
            fixture.io_ledger,
            Principal::anonymous(),
            "debug_get_transactions",
            (),
        )
        .into_iter()
        .filter(|tx| {
            tx.from_account.as_ref() == Some(&fixture.staging)
                && tx.to_account.as_ref() == Some(&fixture.reserve)
        })
        .count(),
        1
    );
    eprintln!(
        "buried_redemption_evidence index_pages=3 continuation_gaps_nanos={continuation_gaps_nanos:?} irrelevant_canonical_proofs=0 valid_canonical_proofs=1 payouts=1 sweeps=1 public_followup_wakes=0"
    );
}

#[test]
fn scanner_error_during_catch_up_returns_to_coarse_polling() {
    if std::env::var_os("POCKET_IC_BIN").is_none() {
        eprintln!("skipping staged-redemption PocketIC test because POCKET_IC_BIN is not set");
        return;
    }
    let fixture = redemption_fixture();
    for _ in 0..70 {
        stage_redemption(&fixture, 1);
    }
    let index_before = index_call_times(&fixture).len();
    assert_eq!(
        update::<_, Result<(), ApiError>>(
            &fixture.pic,
            fixture.stream,
            fixture.user,
            "process_redemptions",
            (),
        ),
        Ok(())
    );
    fixture.pic.advance_time(Duration::from_secs(1));
    tick_until_index_calls(&fixture, index_before as u64 + 1);
    assert!(
        query::<StreamStateV1>(&fixture.pic, fixture.stream, "debug_get_state")
            .redemption_scan_cursor
            .resume_before
            .is_some()
    );
    let _: () = update(
        &fixture.pic,
        fixture.io_index,
        Principal::anonymous(),
        "debug_set_unreadable",
        DebugUnreadableArgs { unreadable: true },
    );
    fixture.pic.advance_time(Duration::from_secs(1));
    tick_until_index_calls(&fixture, index_before as u64 + 2);
    fixture.pic.advance_time(Duration::from_secs(59));
    for _ in 0..20 {
        fixture.pic.tick();
    }
    assert_eq!(
        index_call_times(&fixture).len(),
        index_before + 2,
        "a failed continuation cannot create a near-term error loop"
    );
    fixture.pic.advance_time(Duration::from_secs(1));
    tick_until_index_calls(&fixture, index_before as u64 + 3);
    let times = index_call_times(&fixture);
    let observed = &times[index_before..];
    assert!(observed[1].saturating_sub(observed[0]) < 60_000_000_000);
    assert!(
        observed[2].saturating_sub(observed[1]) >= 60_000_000_000,
        "index failure must restore the configured coarse retry"
    );
    eprintln!(
        "scanner_error_backoff_evidence near_term_gap_nanos={} post_error_gap_nanos={}",
        observed[1].saturating_sub(observed[0]),
        observed[2].saturating_sub(observed[1])
    );
}

#[test]
fn committed_ambiguous_sweep_is_completed_by_exact_proof_without_retransfer() {
    if std::env::var_os("POCKET_IC_BIN").is_none() {
        eprintln!("skipping staged-redemption PocketIC test because POCKET_IC_BIN is not set");
        return;
    }
    let fixture = redemption_fixture();
    let _: () = update(
        &fixture.pic,
        fixture.io_ledger,
        Principal::anonymous(),
        "debug_commit_then_unavailable_to",
        DebugRejectAccountArgs {
            account: fixture.stream.to_text(),
        },
    );
    stage_redemption(&fixture, 100_000_000);
    wake_and_tick(&fixture);
    let ambiguous: Status = query(&fixture.pic, fixture.stream, "get_status");
    assert_eq!(ambiguous.operation_phase.as_deref(), Some("SweepSubmitted"));
    let sweep_block = update::<_, Vec<DebugLedgerTransaction>>(
        &fixture.pic,
        fixture.io_ledger,
        Principal::anonymous(),
        "debug_get_transactions",
        (),
    )
    .into_iter()
    .find(|tx| {
        tx.from_account.as_ref() == Some(&fixture.staging)
            && tx.to_account.as_ref() == Some(&fixture.reserve)
    })
    .expect("committed sweep is visible in canonical ledger")
    .block_index;
    let _: () = update(
        &fixture.pic,
        fixture.io_ledger,
        Principal::anonymous(),
        "debug_return_too_old_next",
        (),
    );
    fixture.pic.advance_time(Duration::from_secs(1));
    let stuck: Result<io_stream_manager::StreamProgress, ApiError> = update(
        &fixture.pic,
        fixture.stream,
        Principal::from_slice(&[81; 29]),
        "resume",
        (),
    );
    assert!(matches!(stuck, Err(ApiError::Stuck(_))));
    let transfer_calls =
        query::<LedgerCallCounters>(&fixture.pic, fixture.io_ledger, "debug_get_call_counters")
            .transfer;
    let state_before_wrong: StreamStateV1 = query(&fixture.pic, fixture.stream, "debug_get_state");
    let wrong: Result<(), ApiError> = update(
        &fixture.pic,
        fixture.stream,
        Principal::from_slice(&[82; 29]),
        "prove_active_transfer",
        u128::from(sweep_block + 1),
    );
    assert!(wrong.is_err());
    assert_eq!(
        query::<StreamStateV1>(&fixture.pic, fixture.stream, "debug_get_state"),
        state_before_wrong,
        "a wrong proof cannot alter the persisted transfer intent"
    );
    assert_eq!(
        query::<LedgerCallCounters>(&fixture.pic, fixture.io_ledger, "debug_get_call_counters")
            .transfer,
        transfer_calls,
        "a wrong proof cannot submit another transfer"
    );
    let proof: Result<(), ApiError> = update(
        &fixture.pic,
        fixture.stream,
        Principal::from_slice(&[83; 29]),
        "prove_active_transfer",
        u128::from(sweep_block),
    );
    assert_eq!(proof, Ok(()));
    assert_eq!(
        query::<LedgerCallCounters>(&fixture.pic, fixture.io_ledger, "debug_get_call_counters")
            .transfer,
        transfer_calls,
        "exact proof observes the committed sweep without another transfer"
    );
    let repeated: Result<(), ApiError> = update(
        &fixture.pic,
        fixture.stream,
        Principal::from_slice(&[84; 29]),
        "prove_active_transfer",
        u128::from(sweep_block),
    );
    assert!(matches!(repeated, Err(ApiError::Invalid(_))));
    let complete: Status = query(&fixture.pic, fixture.stream, "get_status");
    assert!(complete.operation_kind.is_none());
    assert_eq!(complete.pending_redemption_candidates, 0);
}

#[test]
fn reward_observation_and_best_effort_refresh_are_bounded_and_monetary_once() {
    if std::env::var_os("POCKET_IC_BIN").is_none() {
        eprintln!("skipping Stream liveness PocketIC test because POCKET_IC_BIN is not set");
        return;
    }
    let _server_guard = lock_stream_test();
    let pic = stream_pocket_ic();
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
    pic.advance_time(Duration::from_secs(1));
    for _ in 0..40 {
        pic.tick();
    }
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
            neuron_reward_shares: (1_u64..=6).map(|id| (id, Nat::from(1_u8))).collect(),
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
            neuron_reward_shares: (1_u64..=6).map(|id| (id, Nat::from(1_u8))).collect(),
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
