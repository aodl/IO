use candid::{decode_one, encode_one, CandidType, Principal};
use io_stream_manager::{
    Account, ApiError, InitArgs, Lifecycle, PreparedRedemption, RedeemArgs, RedemptionProgress,
    RewardEventClassification, RewardEventObservation, Status, StreamConfig,
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, CandidType, Deserialize)]
struct NnsObservationCallCounters {
    claim_assets: u64,
    pool_policy: u64,
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
    let result: Result<PreparedRedemption, ApiError> = decode_one(
        &pic.update_call(
            canister,
            Principal::anonymous(),
            "prepare_redemption",
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
fn preparation_uses_scalar_claim_reads_without_requiring_liquid_icp() {
    if std::env::var_os("POCKET_IC_BIN").is_none() {
        eprintln!(
            "skipping Stream scalar-redemption PocketIC test because POCKET_IC_BIN is not set"
        );
        return;
    }
    let pic = PocketIc::new();
    let io_ledger = install(&pic, "mock_io_ledger");
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
        (io_ledger, user_account.clone(), 100_010_000),
        (icp_ledger, liquid.clone(), 100_000),
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
        99_910_000_u128,
    );
    pic.install_canister(
        stream,
        debug_wasm("io_stream_manager"),
        encode_one(InitArgs {
            config: StreamConfig {
                io_ledger,
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
    update::<_, Result<(), ApiError>>(&pic, stream, governance, "set_paused", false).unwrap();
    let _: () = update(
        &pic,
        nns,
        Principal::anonymous(),
        "debug_set_pool_policy_valid",
        false,
    );
    let blocked_reward: Result<RewardEventObservation, ApiError> = update(
        &pic,
        stream,
        Principal::anonymous(),
        "resume_reward_work",
        (),
    );
    assert!(matches!(
        blocked_reward,
        Err(ApiError::Invalid(ref reason)) if reason.contains("pool policy")
    ));
    let policy_blocked_status: Status = query(&pic, stream, "get_status");
    assert!(policy_blocked_status.reward_processing_paused);
    assert!(!policy_blocked_status.governance_parameters_fresh);
    let governance_before: GovernanceCallCounters =
        query(&pic, governance, "debug_get_call_counters");
    let nns_before: NnsObservationCallCounters =
        query(&pic, nns, "debug_get_observation_call_counters");
    let permanent_queries_before: u64 = query(&pic, nns, "debug_get_full_neuron_call_count");
    let now = pic.get_time().as_nanos_since_unix_epoch();
    let result: Result<PreparedRedemption, ApiError> = update(
        &pic,
        stream,
        user,
        "prepare_redemption",
        RedeemArgs {
            from_subaccount: None,
            io_amount_e8s: 100_000_000,
            min_icp_out_e8s: 1,
            max_io_fee_e8s: 10_000,
            max_icp_fee_e8s: 10_000,
            expires_at_nanos: now + 60_000_000_000,
            nonce: 0,
        },
    );
    let prepared = result.expect("preparation must not impose a liquid-ICP admission gate");
    assert_eq!(prepared.gross_icp_e8s, 100_000_000);
    assert_eq!(
        query::<GovernanceCallCounters>(&pic, governance, "debug_get_call_counters"),
        governance_before,
        "redemption must not list or inspect SNS neurons"
    );
    let nns_after: NnsObservationCallCounters =
        query(&pic, nns, "debug_get_observation_call_counters");
    assert_eq!(nns_after.claim_assets - nns_before.claim_assets, 2);
    assert_eq!(nns_after.pool_policy, nns_before.pool_policy);
    assert_eq!(
        query::<u64>(&pic, nns, "debug_get_full_neuron_call_count"),
        permanent_queries_before,
        "redemption must not query the permanent neuron"
    );
    assert!(query::<Status>(&pic, stream, "get_status")
        .operation_kind
        .is_none());

    for neuron_id in 100..164 {
        let _: () = update(
            &pic,
            governance,
            Principal::anonymous(),
            "debug_add_neuron",
            MockSnsNeuron {
                neuron_id,
                staked_io_e8s: 1,
                dissolve_delay_seconds: io_core_model::SNS_USER_DISSOLVE_DELAY_SECONDS,
                eligible_closed_proposals: 0,
                voted_closed_proposals: 0,
                is_genesis_governance_neuron: false,
                is_protocol_owned: false,
                is_dissolving: false,
            },
        );
    }
    let governance_many_before: GovernanceCallCounters =
        query(&pic, governance, "debug_get_call_counters");
    let nns_many_before: NnsObservationCallCounters =
        query(&pic, nns, "debug_get_observation_call_counters");
    let result_many: Result<PreparedRedemption, ApiError> = update(
        &pic,
        stream,
        user,
        "prepare_redemption",
        RedeemArgs {
            from_subaccount: None,
            io_amount_e8s: 100_000_000,
            min_icp_out_e8s: 1,
            max_io_fee_e8s: 10_000,
            max_icp_fee_e8s: 10_000,
            expires_at_nanos: now + 60_000_000_000,
            // Preparation is replay-safe and does not consume the nonce.
            nonce: 0,
        },
    );
    assert_eq!(result_many.unwrap(), prepared);
    assert_eq!(
        query::<GovernanceCallCounters>(&pic, governance, "debug_get_call_counters"),
        governance_many_before,
        "redemption call count must remain independent of 65 SNS neurons"
    );
    let nns_many_after: NnsObservationCallCounters =
        query(&pic, nns, "debug_get_observation_call_counters");
    assert_eq!(nns_many_after.claim_assets, nns_many_before.claim_assets);
    assert_eq!(nns_many_after.pool_policy, nns_many_before.pool_policy);

    let _: () = update(
        &pic,
        nns,
        Principal::anonymous(),
        "debug_set_pool_policy_valid",
        true,
    );
    update::<_, Result<(), ApiError>>(&pic, stream, governance, "set_paused", true).unwrap();
    update::<_, Result<(), ApiError>>(&pic, stream, governance, "set_paused", false).unwrap();
    let restored: Status = query(&pic, stream, "get_status");
    assert!(!restored.reward_processing_paused);
    assert!(restored.governance_parameters_fresh);

    let pushed: io_ledger_boundary::IcrcTransferResult = update(
        &pic,
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
    let push_block: u128 = pushed.unwrap().0.try_into().unwrap();
    let payout_calls_before: LedgerCallCounters =
        query(&pic, icp_ledger, "debug_get_call_counters");
    let awaiting_liquidity = update::<_, Result<RedemptionProgress, ApiError>>(
        &pic,
        stream,
        user,
        "settle_redemption",
        push_block,
    );
    assert!(matches!(
        awaiting_liquidity,
        Err(ApiError::Pending(ref reason))
            if reason.contains("durable payout obligation awaiting liquid ICP")
    ));
    let owed = query::<Status>(&pic, stream, "get_status");
    assert_eq!(owed.lifecycle, Lifecycle::Paused);
    assert_eq!(owed.operation_kind.as_deref(), Some("Redemption"));
    assert_eq!(owed.operation_phase.as_deref(), Some("PayoutOwed"));
    assert_eq!(
        query::<LedgerCallCounters>(&pic, icp_ledger, "debug_get_call_counters").transfer,
        payout_calls_before.transfer,
        "an unfunded durable obligation must not submit a payout"
    );

    let _: u64 = update(
        &pic,
        icp_ledger,
        Principal::anonymous(),
        "debug_mint_account",
        DebugMintAccountArgs {
            to: liquid,
            amount_e8s: 100_000_000,
        },
    );
    let completed = update::<_, Result<RedemptionProgress, ApiError>>(
        &pic,
        stream,
        Principal::anonymous(),
        "resume_redemption",
        user,
    )
    .expect("durable payout must resume after exact liquidity arrives");
    let RedemptionProgress::Completed(result) = completed.clone() else {
        panic!("durable payout did not complete after liquidity recovery: {completed:?}");
    };
    assert_eq!(result.io_block, push_block);
    assert_eq!(result.gross_icp_e8s, prepared.gross_icp_e8s);
    assert_eq!(
        query::<LedgerCallCounters>(&pic, icp_ledger, "debug_get_call_counters").transfer,
        payout_calls_before.transfer + 1
    );
    assert_eq!(
        update::<_, Result<RedemptionProgress, ApiError>>(
            &pic,
            stream,
            user,
            "settle_redemption",
            push_block,
        ),
        Ok(completed)
    );
    assert_eq!(
        query::<LedgerCallCounters>(&pic, icp_ledger, "debug_get_call_counters").transfer,
        payout_calls_before.transfer + 1,
        "completed replay must not repeat the recovered payout"
    );
    eprintln!(
        "prepared_push_without_liquidity available_liquid_e8s=100000 claim_snapshot_reads=2 io_pulled=false durable_payout=true recovered_once=true"
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
    pic.advance_time(Duration::from_secs(59));
    for _ in 0..3 {
        pic.tick();
    }
    assert_eq!(
        query::<u64>(&pic, nns, "debug_get_reconcile_call_count"),
        reconciliations_before_structural + 1,
        "the retry must not run before its 60-second deadline"
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
    let ledger_before_redemption = (
        query::<LedgerCallCounters>(&pic, io_ledger, "debug_get_call_counters"),
        query::<LedgerCallCounters>(&pic, icp_ledger, "debug_get_call_counters"),
    );
    let redemption_args = RedeemArgs {
        from_subaccount: None,
        io_amount_e8s: 100_000_000,
        min_icp_out_e8s: 1,
        max_io_fee_e8s: 10_000,
        max_icp_fee_e8s: 10_000,
        expires_at_nanos: pic.get_time().as_nanos_since_unix_epoch() + 60_000_000_000,
        nonce: 0,
    };
    let prepared = update::<_, Result<PreparedRedemption, ApiError>>(
        &pic,
        stream,
        user,
        "prepare_redemption",
        redemption_args.clone(),
    )
    .unwrap();
    let pushed: io_ledger_boundary::IcrcTransferResult = update(
        &pic,
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
    let push_block: u128 = pushed.unwrap().0.try_into().unwrap();
    let completed = update::<_, Result<RedemptionProgress, ApiError>>(
        &pic,
        stream,
        user,
        "settle_redemption",
        push_block,
    );
    assert!(
        matches!(completed, Ok(RedemptionProgress::Completed(_))),
        "push redemption did not complete: {completed:?}"
    );
    let ledger_after_redemption = (
        query::<LedgerCallCounters>(&pic, io_ledger, "debug_get_call_counters"),
        query::<LedgerCallCounters>(&pic, icp_ledger, "debug_get_call_counters"),
    );
    assert_eq!(
        update::<_, Result<RedemptionProgress, ApiError>>(
            &pic,
            stream,
            user,
            "settle_redemption",
            push_block,
        ),
        completed
    );
    assert_eq!(
        query::<LedgerCallCounters>(&pic, io_ledger, "debug_get_call_counters").transfer,
        ledger_after_redemption.0.transfer
    );
    assert_eq!(
        query::<LedgerCallCounters>(&pic, icp_ledger, "debug_get_call_counters").transfer,
        ledger_after_redemption.1.transfer
    );
    assert_eq!(
        ledger_after_redemption.0.transfer,
        ledger_before_redemption.0.transfer + 1
    );
    assert_eq!(
        ledger_after_redemption.1.transfer,
        ledger_before_redemption.1.transfer + 1
    );
    assert_eq!(
        query::<u64>(&pic, nns, "debug_get_reconcile_call_count"),
        reconciliations_before_structural + 2,
        "push redemption must not add another pool reconciliation"
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
