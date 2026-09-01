#![cfg(test)]

use candid::{decode_one, encode_one, CandidType, Principal, Reserved};
use io_governance_types::{
    nns_refresh_voting_power_request, EmptyRecord, NnsAccount, NnsChangeAutoStakeMaturity,
    NnsClaimOrRefresh, NnsClaimOrRefreshBy, NnsClaimOrRefreshNeuronFromAccount,
    NnsDissolveStateRecord, NnsIncreaseDissolveDelay, NnsManageNeuronCommandRequest,
    NnsManageNeuronResponseCommandRecord, NnsNeuronIdOrSubaccount, NnsNeuronIdRecord,
    NnsProductionConfigure, NnsProductionConfigureOperation, NnsProductionDisburseMaturity,
    NnsProductionListNeuronsRequest, NnsProductionListNeuronsResponse,
    NnsProductionManageNeuronRequest, NnsProductionManageNeuronResponse, NnsRegisterVote,
    NnsSetVisibility, NnsStakeMaturity,
};
use io_ledger_types::{
    Account as IcpAccount, IcpTokens, IcpTransferArgs, IcpTransferError, Subaccount,
};
use pocket_ic::PocketIc;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::rc::Rc;
use std::time::Duration;

use crate::{icrc, nns_setup, pocketic_env};

const ICP_FEE_E8S: u64 = 10_000;
const PROTECTED_STAKE_E8S: u64 = 100_000_000 * 100_000_000;
const PROTECTED_MEMO: u64 = 8_002;
const PROPOSER_MEMO: u64 = 8_003;
const TWO_YEAR_MEMO: u64 = 8_004;
const PERMANENT_DELAY_SECONDS: u32 = 63_115_200;
const POOLED_PARENT_DELAY_SECONDS: u32 = 1_209_600;
const ONE_YEAR_SECONDS: u32 = 365 * 24 * 60 * 60;
const XRC_CANISTER_ID: &str = "uf6dk-hyaaa-aaaaq-qaaaq-cai";
const UNWIND_EXCESS_E8S: u64 = 10 * 100_000_000;
const RECONCILED_TARGET_E8S: u64 = PROTECTED_STAKE_E8S - UNWIND_EXCESS_E8S;
const ACTIVE_IO_E8S: u128 = 600 * 100_000_000;

#[derive(Clone, Debug, CandidType)]
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

#[derive(Clone, Debug, CandidType)]
struct GetMaturityModulationRequest {}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct MaturityModulationResponse {
    maturity_modulation: Option<MaturityModulationProbe>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct MaturityModulationProbe {
    updated_at_timestamp_seconds: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
enum ManagerApiError {
    Unauthorized,
    Paused,
    Busy,
    Invalid(String),
    Pending(String),
    Stuck(String),
    BelowMaturityThreshold {
        remaining_e8s: u64,
        minimum_e8s: u64,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
enum ManagerLifecycle {
    Paused,
    Ready,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
struct ManagerStatus {
    lifecycle: ManagerLifecycle,
    active_operation: Option<String>,
    two_year_maturity_baseline_reconciled: bool,
    latest_started_two_week_generation: u64,
    latest_completed_two_week_generation: u64,
    latest_pooled_target: Option<ManagerPooledTarget>,
    live_child_physical_principal_e8s: u128,
    live_child_net_backing_e8s: u128,
    live_child_committed_fee_liability_e8s: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
struct ManagerPooledTarget {
    target_e8s: u128,
    status: ManagerPooledTargetStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
enum ManagerPooledTargetStatus {
    UnderTarget,
    AtTarget,
    AtTargetWithinUnwindTolerance,
    OverTarget,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
enum ManagerMaturityKind {
    TwoYear,
    TwoWeek,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
struct ManagerCompletedMaturity {
    kind: ManagerMaturityKind,
    captured_e8s: u128,
    anchor_reimbursement_e8s: u128,
    permanent_reimbursement_e8s: u128,
    reimbursement_transfer_fees_e8s: u128,
    carried_e8s: u128,
    permanent_credit_e8s: u128,
    claim_credit_e8s: u128,
    entitlement_batch_generation: Option<u64>,
    two_week_target_e8s: Option<u128>,
    completed_at_nanos: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
enum ManagerMaturityProgress {
    Pending,
    Completed(Box<ManagerCompletedMaturity>),
    Stuck(String),
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
enum ManagerNnsProgress {
    Jupiter(ManagerJupiterProgress),
    Maturity(ManagerMaturityProgress),
    Pool(io_nns_types::backing::PoolProgress),
    Unwind(ManagerUnwindProgress),
    Idle,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
enum ManagerUnwindProgress {
    Pending,
    AwaitingTransferProof,
    Completed,
    Stuck(String),
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
struct ManagerAccount {
    owner: Principal,
    subaccount: Option<Vec<u8>>,
}

#[derive(Clone, Debug, CandidType)]
struct ManagerConfig {
    sns_governance: Principal,
    stream_manager: Principal,
    jupiter: Principal,
    icp_ledger: Principal,
    nns_governance: Principal,
    two_year_neuron_id: u64,
    pooled_parent_memo: u64,
    pooled_parent_followee_id: u64,
    jupiter_account: ManagerAccount,
    jupiter_staging: ManagerAccount,
    stream_liquid_account: ManagerAccount,
    expected_io_fee_e8s: u128,
    expected_icp_fee_e8s: u128,
    jupiter_activation_block_floor: u128,
    audited_permanent_principal_e8s: u128,
    transfer_retry_delay_nanos: u64,
    ledger_deduplication_window_nanos: u64,
}

#[derive(Clone, Debug, CandidType)]
struct ManagerInitArgs {
    config: ManagerConfig,
}

#[derive(Clone, Debug, CandidType)]
struct NotifyJupiterDepositArgs {
    block_index: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
struct ManagerJupiterCompleted {
    deposit_block: u128,
    gross_e8s: u128,
    stake_e8s: u128,
    observed_after_cached_stake_e8s: u128,
    liquid_e8s: u128,
    stake_transfer_block: u128,
    liquid_transfer_block: u128,
    stream_receipt_sequence: u64,
    backed_io_e8s: u128,
    io_transfer_block: u128,
    io_fee_e8s: u128,
    completed_at_nanos: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
enum ManagerJupiterProgress {
    Pending,
    Completed(ManagerJupiterCompleted),
    Stuck(String),
}

#[derive(Clone, Debug, CandidType)]
struct ProposalRequest {
    neuron_id_or_subaccount: Option<NnsNeuronIdOrSubaccount>,
    command: Option<ProposalCommand>,
    id: Option<NnsNeuronIdRecord>,
}

#[derive(Clone, Debug, CandidType)]
enum ProposalCommand {
    MakeProposal(Proposal),
}

#[derive(Clone, Debug, CandidType)]
struct Proposal {
    url: String,
    title: Option<String>,
    action: Option<ProposalAction>,
    summary: String,
}

#[derive(Clone, Debug, CandidType)]
enum ProposalAction {
    Motion(Motion),
}

#[derive(Clone, Debug, CandidType)]
struct Motion {
    motion_text: String,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct RewardEvent {
    day_after_genesis: u64,
    settled_proposals: Vec<NnsNeuronIdRecord>,
    distributed_e8s_equivalent: u64,
    total_available_e8s_equivalent: u64,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct GetBlocksArgs {
    start: u64,
    length: u64,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct QueryBlocksResponse {
    blocks: Vec<IcpBlock>,
    first_block_index: u64,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct IcpBlock {
    transaction: IcpTransaction,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct IcpTransaction {
    memo: u64,
    operation: Option<IcpOperation>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
enum IcpOperation {
    Mint { to: Vec<u8>, amount: IcpTokens },
    Burn(Reserved),
    Transfer(Reserved),
    Approve(Reserved),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ControlledMaturityEvidence {
    pub original_maturity_e8s: u64,
    pub staked_maturity_e8s: u64,
    pub nominal_disbursed_maturity_e8s: u64,
    pub mint_block: u64,
    pub actual_minted_e8s: u64,
}

pub struct ControlledNnsNeuron {
    pub pic: Rc<PocketIc>,
    pub governance: Principal,
    pub ledger: Principal,
    pub controller: Principal,
    pub neuron_id: u64,
    pub two_year_neuron_id: u64,
    pub proposer_neuron_id: u64,
    pub protected_principal_e8s: u64,
    pub two_year_principal_e8s: u64,
}

pub fn create_zero_maturity_protected_neuron() -> ControlledNnsNeuron {
    create_zero_maturity_protected_neuron_with_stake(PROTECTED_STAKE_E8S)
}

fn create_zero_maturity_protected_neuron_with_stake(
    protected_principal_e8s: u64,
) -> ControlledNnsNeuron {
    let pic = Rc::new(nns_setup::controlled_pinned_nns_with_fiduciary(true).unwrap());
    let xrc = Principal::from_text(XRC_CANISTER_ID).unwrap();
    let created = pic
        .create_canister_with_id(None, None, xrc)
        .expect("source-shaped XRC fixture should use the canonical XRC principal");
    assert_eq!(created, xrc);
    pic.install_canister(xrc, debug_wasm("mock_nns_xrc"), Vec::new(), None);
    let governance = Principal::from_text(nns_setup::install_nns_governance().canister_id).unwrap();
    let ledger = Principal::from_text(nns_setup::install_nns_ledger().canister_id).unwrap();
    let controller = pocketic_env::create_empty_application_canister(&pic);
    let proposer_neuron_id = stake_neuron(
        &pic,
        governance,
        ledger,
        Principal::anonymous(),
        PROPOSER_MEMO,
        1_000 * 100_000_000,
        ONE_YEAR_SECONDS,
    );
    configure_neuron(
        &pic,
        governance,
        Principal::anonymous(),
        proposer_neuron_id,
        NnsProductionConfigureOperation::SetVisibility(NnsSetVisibility {
            visibility: Some(2),
        }),
    );
    let two_year_principal_e8s = 100 * 100_000_000;
    let two_year_neuron_id = stake_neuron(
        &pic,
        governance,
        ledger,
        controller,
        TWO_YEAR_MEMO,
        two_year_principal_e8s,
        PERMANENT_DELAY_SECONDS,
    );
    let root = Principal::from_text(nns_setup::install_nns_root().canister_id).unwrap();
    let candidate_path = std::env::var_os("IO_POST_M70_NNS_GOVERNANCE_WASM")
        .expect("IO_POST_M70_NNS_GOVERNANCE_WASM is required for controlled IO/NNS evidence");
    let candidate = std::fs::read(candidate_path).expect("read exact post-Mission-70 Governance");
    assert_eq!(
        hex::encode(Sha256::digest(&candidate)),
        "573af1cde5bf55a5e4dbf2d47f8dd340f7a73a107eebbc645fe1202b97f61e85"
    );
    pic.upgrade_canister(governance, candidate, Vec::new(), Some(root))
        .expect("activate exact post-Mission-70 Governance for controlled evidence");
    let neuron_id = if protected_principal_e8s == 0 {
        0
    } else {
        let neuron_id = stake_neuron(
            &pic,
            governance,
            ledger,
            controller,
            PROTECTED_MEMO,
            protected_principal_e8s,
            POOLED_PARENT_DELAY_SECONDS,
        );
        let neuron = neuron(&pic, governance, controller, neuron_id);
        assert_eq!(neuron.cached_neuron_stake_e8s, protected_principal_e8s);
        assert_eq!(neuron.maturity_e8s_equivalent, 0);
        assert!(!neuron.auto_stake_maturity.unwrap_or(false));
        assert_eq!(
            neuron.dissolve_state,
            Some(NnsDissolveStateRecord::DissolveDelaySeconds(
                POOLED_PARENT_DELAY_SECONDS.into()
            ))
        );
        neuron_id
    };
    ControlledNnsNeuron {
        pic,
        governance,
        ledger,
        controller,
        neuron_id,
        two_year_neuron_id,
        proposer_neuron_id,
        protected_principal_e8s,
        two_year_principal_e8s,
    }
}

fn stake_neuron(
    pic: &PocketIc,
    governance: Principal,
    ledger: Principal,
    controller: Principal,
    memo: u64,
    stake_e8s: u64,
    dissolve_delay_seconds: u32,
) -> u64 {
    let subaccount = Subaccount(neuron_subaccount(controller, memo));
    let to = IcpAccount::new(governance, Some(subaccount)).icp_account_identifier_bytes();
    let transfer: Result<u64, IcpTransferError> = icrc::update_one(
        pic,
        ledger,
        Principal::anonymous(),
        "transfer",
        IcpTransferArgs {
            memo: 0,
            amount: IcpTokens { e8s: stake_e8s },
            fee: IcpTokens { e8s: ICP_FEE_E8S },
            from_subaccount: None,
            to: to.to_vec(),
            created_at_time: None,
        },
    );
    transfer.unwrap();
    let response = manage(
        pic,
        governance,
        controller,
        NnsProductionManageNeuronRequest {
            neuron_id_or_subaccount: None,
            command: Some(NnsManageNeuronCommandRequest::ClaimOrRefresh(
                NnsClaimOrRefresh {
                    by: Some(NnsClaimOrRefreshBy::MemoAndController(
                        NnsClaimOrRefreshNeuronFromAccount {
                            controller: Some(controller),
                            memo,
                        },
                    )),
                },
            )),
            id: None,
        },
    );
    let neuron_id = match response.command {
        Some(NnsManageNeuronResponseCommandRecord::ClaimOrRefresh(response)) => {
            response.refreshed_neuron_id.unwrap().id
        }
        other => panic!("controlled claim failed: {other:?}"),
    };
    let initial_delay = match neuron(pic, governance, controller, neuron_id).dissolve_state {
        Some(NnsDissolveStateRecord::DissolveDelaySeconds(seconds)) => seconds,
        other => panic!("fresh controlled neuron has unexpected dissolve state: {other:?}"),
    };
    let additional_dissolve_delay_seconds = u64::from(dissolve_delay_seconds)
        .checked_sub(initial_delay)
        .and_then(|seconds| u32::try_from(seconds).ok())
        .expect("requested controlled delay must not precede the canonical initial delay");
    let configured = manage(
        pic,
        governance,
        controller,
        NnsProductionManageNeuronRequest {
            neuron_id_or_subaccount: Some(NnsNeuronIdOrSubaccount::NeuronId(NnsNeuronIdRecord {
                id: neuron_id,
            })),
            command: Some(NnsManageNeuronCommandRequest::Configure(
                NnsProductionConfigure {
                    operation: Some(NnsProductionConfigureOperation::IncreaseDissolveDelay(
                        NnsIncreaseDissolveDelay {
                            additional_dissolve_delay_seconds,
                        },
                    )),
                },
            )),
            id: None,
        },
    );
    assert!(matches!(
        configured.command,
        Some(NnsManageNeuronResponseCommandRecord::Configure(
            EmptyRecord {}
        ))
    ));
    configure_neuron(
        pic,
        governance,
        controller,
        neuron_id,
        NnsProductionConfigureOperation::ChangeAutoStakeMaturity(NnsChangeAutoStakeMaturity {
            requested_setting_for_auto_stake_maturity: false,
        }),
    );
    neuron_id
}

fn configure_neuron(
    pic: &PocketIc,
    governance: Principal,
    controller: Principal,
    neuron_id: u64,
    operation: NnsProductionConfigureOperation,
) {
    let configured = manage(
        pic,
        governance,
        controller,
        NnsProductionManageNeuronRequest {
            neuron_id_or_subaccount: Some(NnsNeuronIdOrSubaccount::NeuronId(NnsNeuronIdRecord {
                id: neuron_id,
            })),
            command: Some(NnsManageNeuronCommandRequest::Configure(
                NnsProductionConfigure {
                    operation: Some(operation),
                },
            )),
            id: None,
        },
    );
    assert!(matches!(
        configured.command,
        Some(NnsManageNeuronResponseCommandRecord::Configure(
            EmptyRecord {}
        ))
    ));
}

pub fn earn_maturity(fixture: &ControlledNnsNeuron) -> u64 {
    earn_maturity_with_daily(fixture, || {})
}

fn earn_maturity_with_daily(fixture: &ControlledNnsNeuron, after_daily_tick: impl FnMut()) -> u64 {
    earn_maturity_for_with_daily(fixture, fixture.neuron_id, after_daily_tick)
}

pub fn earn_maturity_for(fixture: &ControlledNnsNeuron, neuron_id: u64) -> u64 {
    earn_maturity_for_with_daily(fixture, neuron_id, || {})
}

fn earn_maturity_for_with_daily(
    fixture: &ControlledNnsNeuron,
    neuron_id: u64,
    mut after_daily_tick: impl FnMut(),
) -> u64 {
    // The production spike guard intentionally uses a prior voting-power
    // snapshot. Age that bootstrapped snapshot out, then let the pinned timer
    // record this controlled population before creating the proposal.
    fixture.pic.advance_time(Duration::from_secs(121 * 86_400));
    for _ in 0..100 {
        fixture.pic.tick();
    }
    for attempt in 0..500 {
        let modulation: MaturityModulationResponse = query(
            &fixture.pic,
            fixture.governance,
            Principal::anonymous(),
            "get_maturity_modulation",
            GetMaturityModulationRequest {},
        );
        if modulation
            .maturity_modulation
            .is_some_and(|value| value.updated_at_timestamp_seconds.is_some())
        {
            break;
        }
        assert!(
            attempt < 499,
            "candidate maturity modulation did not settle"
        );
        fixture.pic.advance_time(Duration::from_secs(5));
        for _ in 0..5 {
            fixture.pic.tick();
        }
    }
    for (caller, neuron_id) in [
        (Principal::anonymous(), fixture.proposer_neuron_id),
        (fixture.controller, neuron_id),
    ] {
        let response = manage(
            &fixture.pic,
            fixture.governance,
            caller,
            nns_refresh_voting_power_request(io_governance_types::NnsNeuronId(neuron_id)),
        );
        if matches!(
            response.command,
            Some(NnsManageNeuronResponseCommandRecord::Error(_)) | None
        ) {
            panic!("controlled voting-power refresh failed: {response:?}");
        }
    }
    let request = ProposalRequest {
        neuron_id_or_subaccount: Some(NnsNeuronIdOrSubaccount::NeuronId(NnsNeuronIdRecord {
            id: fixture.proposer_neuron_id,
        })),
        command: Some(ProposalCommand::MakeProposal(Proposal {
            url: String::new(),
            title: Some("Controlled maturity boundary".into()),
            action: Some(ProposalAction::Motion(Motion {
                motion_text: "Exercise the pinned NNS reward path".into(),
            })),
            summary: "Local PocketIC evidence only".into(),
        })),
        id: None,
    };
    let response: NnsProductionManageNeuronResponse = decode_one(
        &fixture
            .pic
            .update_call(
                fixture.governance,
                Principal::anonymous(),
                "manage_neuron",
                encode_one(request).unwrap(),
            )
            .unwrap(),
    )
    .unwrap();
    let proposal_id = match response.command {
        Some(NnsManageNeuronResponseCommandRecord::MakeProposal(response)) => {
            response.proposal_id.unwrap()
        }
        other => panic!("controlled proposal failed: {other:?}"),
    };
    let vote = manage(
        &fixture.pic,
        fixture.governance,
        fixture.controller,
        NnsProductionManageNeuronRequest {
            neuron_id_or_subaccount: Some(NnsNeuronIdOrSubaccount::NeuronId(NnsNeuronIdRecord {
                id: neuron_id,
            })),
            command: Some(NnsManageNeuronCommandRequest::RegisterVote(
                NnsRegisterVote {
                    vote: 1,
                    proposal: Some(proposal_id),
                },
            )),
            id: None,
        },
    );
    match vote.command {
        Some(NnsManageNeuronResponseCommandRecord::RegisterVote(EmptyRecord {})) => {}
        Some(NnsManageNeuronResponseCommandRecord::Error(error))
            if error.error_type == 19 && error.error_message.contains("already voted") => {}
        other => panic!("controlled vote failed: {other:?}"),
    }
    let before: RewardEvent = query(
        &fixture.pic,
        fixture.governance,
        Principal::anonymous(),
        "get_latest_reward_event",
        (),
    );
    let mut maturity = 0;
    for _ in 0..14 {
        fixture.pic.advance_time(Duration::from_secs(86_400));
        for _ in 0..100 {
            fixture.pic.tick();
        }
        after_daily_tick();
        maturity = neuron(
            &fixture.pic,
            fixture.governance,
            fixture.controller,
            neuron_id,
        )
        .maturity_e8s_equivalent;
        if maturity > 0 {
            break;
        }
    }
    let after: RewardEvent = query(
        &fixture.pic,
        fixture.governance,
        Principal::anonymous(),
        "get_latest_reward_event",
        (),
    );
    let proposal: Option<io_governance_types::NnsProposalInfoRecord> = query(
        &fixture.pic,
        fixture.governance,
        fixture.controller,
        "get_proposal_info",
        proposal_id.id,
    );
    assert!(after.day_after_genesis > before.day_after_genesis);
    assert!(
        maturity > 0,
        "latest reward event: {after:?}; proposal: {proposal:?}"
    );
    maturity
}

pub fn execute_maturity(fixture: &ControlledNnsNeuron) -> ControlledMaturityEvidence {
    let original = neuron(
        &fixture.pic,
        fixture.governance,
        fixture.controller,
        fixture.neuron_id,
    )
    .maturity_e8s_equivalent;
    let stake = original * 40 / 100;
    let remaining = original - stake;
    let staked = manage(
        &fixture.pic,
        fixture.governance,
        fixture.controller,
        NnsProductionManageNeuronRequest {
            neuron_id_or_subaccount: Some(NnsNeuronIdOrSubaccount::NeuronId(NnsNeuronIdRecord {
                id: fixture.neuron_id,
            })),
            command: Some(NnsManageNeuronCommandRequest::StakeMaturity(
                NnsStakeMaturity {
                    percentage_to_stake: Some(40),
                },
            )),
            id: None,
        },
    );
    match staked.command {
        Some(NnsManageNeuronResponseCommandRecord::StakeMaturity(response)) => {
            assert_eq!(response.maturity_e8s, remaining);
            assert_eq!(response.staked_maturity_e8s, stake);
        }
        other => panic!("controlled StakeMaturity failed: {other:?}"),
    }
    let destination = NnsAccount {
        owner: Some(fixture.controller),
        subaccount: Some(vec![9; 32]),
    };
    let disbursed = manage(
        &fixture.pic,
        fixture.governance,
        fixture.controller,
        NnsProductionManageNeuronRequest {
            neuron_id_or_subaccount: Some(NnsNeuronIdOrSubaccount::NeuronId(NnsNeuronIdRecord {
                id: fixture.neuron_id,
            })),
            command: Some(NnsManageNeuronCommandRequest::DisburseMaturity(
                NnsProductionDisburseMaturity {
                    percentage_to_disburse: 100,
                    to_account: Some(destination.clone()),
                    to_account_identifier: None,
                },
            )),
            id: None,
        },
    );
    match disbursed.command {
        Some(NnsManageNeuronResponseCommandRecord::DisburseMaturity(response)) => {
            assert_eq!(response.amount_disbursed_e8s, Some(remaining));
        }
        other => panic!("controlled DisburseMaturity failed: {other:?}"),
    }
    let pending = neuron(
        &fixture.pic,
        fixture.governance,
        fixture.controller,
        fixture.neuron_id,
    );
    assert_eq!(pending.maturity_e8s_equivalent, 0);
    let pending = pending.maturity_disbursements_in_progress.unwrap();
    assert_eq!(pending.len(), 1);
    let finalization = pending[0].finalize_disbursement_timestamp_seconds.unwrap();
    let now = fixture.pic.get_time().as_nanos_since_unix_epoch() / 1_000_000_000;
    fixture
        .pic
        .advance_time(Duration::from_secs(finalization - now + 1));
    for _ in 0..100 {
        fixture.pic.tick();
    }
    let settled = neuron(
        &fixture.pic,
        fixture.governance,
        fixture.controller,
        fixture.neuron_id,
    );
    assert!(settled
        .maturity_disbursements_in_progress
        .unwrap_or_default()
        .is_empty());
    let destination = IcpAccount::new(fixture.controller, Some(Subaccount([9; 32])))
        .icp_account_identifier_bytes();
    let (mint_block, amount) = find_mint(fixture, &destination);
    ControlledMaturityEvidence {
        original_maturity_e8s: original,
        staked_maturity_e8s: stake,
        nominal_disbursed_maturity_e8s: remaining,
        mint_block,
        actual_minted_e8s: amount,
    }
}

fn find_mint(fixture: &ControlledNnsNeuron, destination: &[u8]) -> (u64, u64) {
    maybe_find_mint(fixture, destination)
        .expect("delayed maturity Mint must be present in the pinned ICP ledger")
}

fn maybe_find_mint(fixture: &ControlledNnsNeuron, destination: &[u8]) -> Option<(u64, u64)> {
    let blocks: QueryBlocksResponse = query(
        &fixture.pic,
        fixture.ledger,
        Principal::anonymous(),
        "query_blocks",
        GetBlocksArgs {
            start: 0,
            length: 100,
        },
    );
    blocks
        .blocks
        .iter()
        .enumerate()
        .rev()
        .find_map(|(index, block)| match &block.transaction.operation {
            Some(IcpOperation::Mint { to, amount }) if to == destination => {
                Some((index as u64, amount.e8s))
            }
            _ => None,
        })
        .map(|(offset, amount)| (blocks.first_block_index + offset, amount))
}

fn manage(
    pic: &PocketIc,
    governance: Principal,
    caller: Principal,
    request: NnsProductionManageNeuronRequest,
) -> NnsProductionManageNeuronResponse {
    decode_one(
        &pic.update_call(
            governance,
            caller,
            "manage_neuron",
            encode_one(request).unwrap(),
        )
        .unwrap(),
    )
    .unwrap()
}

fn neuron(
    pic: &PocketIc,
    governance: Principal,
    caller: Principal,
    neuron_id: u64,
) -> io_governance_types::NnsNeuronRecord {
    let response: NnsProductionListNeuronsResponse = query(
        pic,
        governance,
        caller,
        "list_neurons",
        NnsProductionListNeuronsRequest {
            neuron_ids: vec![neuron_id],
            include_neurons_readable_by_caller: true,
            include_empty_neurons_readable_by_caller: Some(false),
            include_public_neurons_in_full_neurons: Some(false),
            page_number: Some(0),
            page_size: Some(10),
            neuron_subaccounts: None,
        },
    );
    response
        .full_neurons
        .into_iter()
        .find(|neuron| neuron.id.is_some_and(|id| id.id == neuron_id))
        .expect("controlled neuron must remain readable")
}

fn query<T: CandidType + for<'de> Deserialize<'de>>(
    pic: &PocketIc,
    canister: Principal,
    caller: Principal,
    method: &str,
    arg: impl CandidType,
) -> T {
    decode_one(
        &pic.query_call(canister, caller, method, encode_one(arg).unwrap())
            .unwrap(),
    )
    .unwrap()
}

fn update<T: CandidType + for<'de> Deserialize<'de>>(
    pic: &PocketIc,
    canister: Principal,
    caller: Principal,
    method: &str,
    arg: impl CandidType,
) -> T {
    decode_one(
        &pic.update_call(canister, caller, method, encode_one(arg).unwrap())
            .unwrap(),
    )
    .unwrap()
}

fn neuron_subaccount(controller: Principal, nonce: u64) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update([0x0c]);
    hasher.update(b"neuron-stake");
    hasher.update(controller.as_slice());
    hasher.update(nonce.to_be_bytes());
    hasher.finalize().into()
}

fn fund_manager_staging(fixture: &ControlledNnsNeuron) {
    for (subaccount, memo) in [(None, 11), (Some(Subaccount([2; 32])), 12)] {
        let to = IcpAccount::new(fixture.controller, subaccount).icp_account_identifier_bytes();
        let transfer: Result<u64, IcpTransferError> = icrc::update_one(
            &fixture.pic,
            fixture.ledger,
            Principal::anonymous(),
            "transfer",
            IcpTransferArgs {
                memo,
                amount: IcpTokens { e8s: 20_000 },
                fee: IcpTokens { e8s: ICP_FEE_E8S },
                from_subaccount: None,
                to: to.to_vec(),
                created_at_time: None,
            },
        );
        transfer.unwrap();
    }
    let dynamic_seed_account = IcpAccount::new(
        fixture.governance,
        Some(Subaccount(neuron_subaccount(fixture.controller, 0))),
    )
    .icp_account_identifier_bytes();
    let seed: Result<u64, IcpTransferError> = icrc::update_one(
        &fixture.pic,
        fixture.ledger,
        Principal::anonymous(),
        "transfer",
        IcpTransferArgs {
            memo: 13,
            amount: IcpTokens {
                e8s: u64::try_from(io_nns_types::backing::DYNAMIC_ANCHOR_TARGET_E8S).unwrap(),
            },
            fee: IcpTokens { e8s: ICP_FEE_E8S },
            from_subaccount: None,
            to: dynamic_seed_account.to_vec(),
            created_at_time: None,
        },
    );
    seed.expect("external Dynamic anchor seed should reach the deterministic memo-0 Account");
}

fn run_jupiter_credit(
    fixture: &ControlledNnsNeuron,
    stream: Principal,
    stream_governance: Principal,
    jupiter: Principal,
    gross_e8s: u64,
    memo: u64,
    donation_before_refresh_e8s: u64,
) -> ManagerJupiterCompleted {
    for _ in 0..16 {
        let status: ManagerStatus = query(
            &fixture.pic,
            fixture.controller,
            Principal::anonymous(),
            "get_status",
            (),
        );
        if status.active_operation.is_none() {
            break;
        }
        let _: Result<ManagerNnsProgress, ManagerApiError> = update(
            &fixture.pic,
            fixture.controller,
            Principal::anonymous(),
            "resume",
            (),
        );
    }
    assert!(
        query::<ManagerStatus>(
            &fixture.pic,
            fixture.controller,
            Principal::anonymous(),
            "get_status",
            (),
        )
        .active_operation
        .is_none(),
        "Jupiter must not overlap an earlier NNS operation"
    );
    let manager_status: ManagerStatus = query(
        &fixture.pic,
        fixture.controller,
        Principal::anonymous(),
        "get_status",
        (),
    );
    if manager_status.lifecycle == ManagerLifecycle::Paused {
        let manager_ready: Result<(), ManagerApiError> = update(
            &fixture.pic,
            fixture.controller,
            stream_governance,
            "set_paused",
            false,
        );
        manager_ready.unwrap();
    }
    let stream_status: io_stream_manager::Status = query(
        &fixture.pic,
        stream,
        Principal::anonymous(),
        "get_status",
        (),
    );
    if stream_status.lifecycle == io_stream_manager::Lifecycle::Paused {
        let stream_ready: Result<(), io_stream_manager::ApiError> =
            update(&fixture.pic, stream, stream_governance, "set_paused", false);
        stream_ready.unwrap();
    }

    let jupiter_account = IcpAccount::new(jupiter, None).icp_account_identifier_bytes();
    let funding: Result<u64, IcpTransferError> = icrc::update_one(
        &fixture.pic,
        fixture.ledger,
        Principal::anonymous(),
        "transfer",
        IcpTransferArgs {
            memo,
            amount: IcpTokens {
                e8s: gross_e8s + ICP_FEE_E8S,
            },
            fee: IcpTokens { e8s: ICP_FEE_E8S },
            from_subaccount: None,
            to: jupiter_account.to_vec(),
            created_at_time: None,
        },
    );
    funding.unwrap();
    let manager_account = IcpAccount::new(fixture.controller, None).icp_account_identifier_bytes();
    let deposit: Result<u64, IcpTransferError> = icrc::update_one(
        &fixture.pic,
        fixture.ledger,
        jupiter,
        "transfer",
        IcpTransferArgs {
            memo: memo + 1,
            amount: IcpTokens { e8s: gross_e8s },
            fee: IcpTokens { e8s: ICP_FEE_E8S },
            from_subaccount: None,
            to: manager_account.to_vec(),
            created_at_time: None,
        },
    );
    let deposit_block = deposit.unwrap();
    let notified: Result<ManagerJupiterProgress, ManagerApiError> = update(
        &fixture.pic,
        fixture.controller,
        jupiter,
        "notify_jupiter_deposit",
        NotifyJupiterDepositArgs {
            block_index: deposit_block.into(),
        },
    );
    if let Ok(ManagerJupiterProgress::Completed(completed)) = notified {
        assert_eq!(donation_before_refresh_e8s, 0);
        assert_eq!(completed.deposit_block, u128::from(deposit_block));
        assert_eq!(completed.gross_e8s, u128::from(gross_e8s));
        return completed;
    }
    assert_eq!(notified, Ok(ManagerJupiterProgress::Pending));

    let mut donated = false;
    if donation_before_refresh_e8s > 0 {
        let refresh_boundary_ready: bool = query(
            &fixture.pic,
            fixture.controller,
            Principal::anonymous(),
            "debug_jupiter_refresh_boundary_ready",
            (),
        );
        assert!(
            refresh_boundary_ready,
            "donation requires the debug one-shot boundary after the exact protocol transfer and before ClaimOrRefresh"
        );
        let staking = IcpAccount::new(
            fixture.governance,
            Some(Subaccount(neuron_subaccount(
                fixture.controller,
                TWO_YEAR_MEMO,
            ))),
        )
        .icp_account_identifier_bytes();
        let donation: Result<u64, IcpTransferError> = icrc::update_one(
            &fixture.pic,
            fixture.ledger,
            Principal::anonymous(),
            "transfer",
            IcpTransferArgs {
                memo: memo + 2,
                amount: IcpTokens {
                    e8s: donation_before_refresh_e8s,
                },
                fee: IcpTokens { e8s: ICP_FEE_E8S },
                from_subaccount: None,
                to: staking.to_vec(),
                created_at_time: None,
            },
        );
        donation.unwrap();
        donated = true;
    }
    for _ in 0..24 {
        let progress: Result<ManagerNnsProgress, ManagerApiError> = update(
            &fixture.pic,
            fixture.controller,
            Principal::anonymous(),
            "resume",
            (),
        );
        if let Ok(ManagerNnsProgress::Jupiter(ManagerJupiterProgress::Completed(completed))) =
            progress
        {
            assert_eq!(completed.deposit_block, u128::from(deposit_block));
            assert_eq!(completed.gross_e8s, u128::from(gross_e8s));
            assert_eq!(donated, donation_before_refresh_e8s > 0);
            return completed;
        }
    }
    panic!("Jupiter credit did not complete within its bounded phase count")
}

fn install_manager(
    fixture: &ControlledNnsNeuron,
    stream: Principal,
    governance: Principal,
    wasm: Vec<u8>,
) -> Principal {
    let account = |owner, byte| ManagerAccount {
        owner,
        subaccount: Some(vec![byte; 32]),
    };
    let jupiter = pocketic_env::create_empty_application_canister(&fixture.pic);
    fixture.pic.install_canister(
        fixture.controller,
        wasm,
        encode_one(ManagerInitArgs {
            config: ManagerConfig {
                sns_governance: governance,
                stream_manager: stream,
                jupiter,
                icp_ledger: fixture.ledger,
                nns_governance: fixture.governance,
                two_year_neuron_id: fixture.two_year_neuron_id,
                pooled_parent_memo: 0,
                pooled_parent_followee_id: fixture.two_year_neuron_id,
                jupiter_account: ManagerAccount {
                    owner: jupiter,
                    subaccount: None,
                },
                jupiter_staging: ManagerAccount {
                    owner: fixture.controller,
                    subaccount: None,
                },
                stream_liquid_account: account(stream, 3),
                expected_io_fee_e8s: 10_000,
                expected_icp_fee_e8s: ICP_FEE_E8S.into(),
                jupiter_activation_block_floor: 1,
                audited_permanent_principal_e8s: fixture.two_year_principal_e8s.into(),
                transfer_retry_delay_nanos: 1_000_000_000,
                ledger_deduplication_window_nanos: 86_400_000_000_000,
            },
        })
        .unwrap(),
        None,
    );
    jupiter
}

fn debug_wasm(name: &str) -> Vec<u8> {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/wasm32-unknown-unknown/debug")
            .join(format!("{name}.wasm")),
    )
    .unwrap_or_else(|error| panic!("build {name} debug Wasm before controlled evidence: {error}"))
}

fn current_io_wasm(name: &str) -> Vec<u8> {
    let env_name = match name {
        "io_stream_manager" => "IO_ACCOUNT_SEMANTIC_STREAM_WASM",
        "io_nns_neuron_manager" => "IO_ACCOUNT_SEMANTIC_NNS_WASM",
        _ => panic!("unsupported current IO Wasm role: {name}"),
    };
    let path = std::env::var_os(env_name).unwrap_or_else(|| {
        panic!("{env_name} must name the exact current release Wasm for canonical evidence")
    });
    let wasm = std::fs::read(&path)
        .unwrap_or_else(|error| panic!("read exact current {name} Wasm {path:?}: {error}"));
    eprintln!(
        "account_semantic_release_wasm role={name} sha256={}",
        hex::encode(Sha256::digest(&wasm))
    );
    wasm
}

fn sns_neuron_subaccount(neuron_id: u64) -> Vec<u8> {
    let mut subaccount = vec![0; 32];
    subaccount[24..].copy_from_slice(&neuron_id.to_be_bytes());
    subaccount
}

struct ControlledStream {
    governance: Principal,
    io_ledger: Principal,
    stream_wasm: Vec<u8>,
    jupiter_destination: io_stream_manager::Account,
    reserve: io_stream_manager::Account,
}

fn install_controlled_stream(
    fixture: &ControlledNnsNeuron,
    stream: Principal,
    stream_wasm: Vec<u8>,
) -> ControlledStream {
    use io_stream_manager::{Account, InitArgs, StreamConfig};

    let governance_wasm = debug_wasm("mock_sns_governance");
    let root_wasm = debug_wasm("mock_sns_root");
    let artifacts = match crate::artifacts::resolve_from_env(true).unwrap() {
        crate::artifacts::ArtifactStatus::Ready(artifacts) => artifacts,
        crate::artifacts::ArtifactStatus::Skipped(message) => panic!("{message}"),
    };
    let io_ledger_wasm = artifacts.load_required("sns_ledger").unwrap();
    let governance_hash = Sha256::digest(&governance_wasm).to_vec();
    let governance =
        pocketic_env::create_application_canister(&fixture.pic, governance_wasm, vec![]);
    let root = pocketic_env::create_application_canister(&fixture.pic, root_wasm, vec![]);
    let reserve_subaccount = icrc::subaccount("controlled-nns-reserve");
    let io_ledger = pocketic_env::create_sns_canister(
        &fixture.pic,
        io_ledger_wasm,
        icrc::ledger_init_arg(
            Principal::anonymous(),
            icrc::account(Principal::from_slice(&[92; 29]), None),
            vec![
                (
                    icrc::account(stream, Some(reserve_subaccount)),
                    u64::try_from(ACTIVE_IO_E8S * 10).unwrap(),
                ),
                (
                    icrc::account(
                        governance,
                        Some(sns_neuron_subaccount(1).try_into().unwrap()),
                    ),
                    u64::try_from(ACTIVE_IO_E8S).unwrap(),
                ),
            ],
        ),
    );

    fixture
        .pic
        .update_call(
            root,
            Principal::anonymous(),
            "debug_set_governance_principal",
            encode_one(governance).unwrap(),
        )
        .unwrap();
    let configured: Result<(), String> = update(
        &fixture.pic,
        root,
        Principal::anonymous(),
        "debug_set_governance_module_hash",
        governance_hash.clone(),
    );
    configured.unwrap();
    fixture
        .pic
        .update_call(
            governance,
            Principal::anonymous(),
            "debug_add_neuron",
            encode_one(MockSnsNeuron {
                neuron_id: 1,
                staked_io_e8s: ACTIVE_IO_E8S,
                dissolve_delay_seconds: io_core_model::SNS_USER_DISSOLVE_DELAY_SECONDS,
                eligible_closed_proposals: 0,
                voted_closed_proposals: 0,
                is_genesis_governance_neuron: false,
                is_protocol_owned: false,
                is_dissolving: false,
            })
            .unwrap(),
        )
        .unwrap();
    let jupiter_destination = Account {
        owner: governance,
        subaccount: Some(vec![4; 32]),
    };
    let reserve = Account {
        owner: stream,
        subaccount: Some(reserve_subaccount.to_vec()),
    };
    fixture.pic.install_canister(
        stream,
        stream_wasm.clone(),
        encode_one(InitArgs {
            config: StreamConfig {
                io_ledger,
                icp_ledger: fixture.ledger,
                nns_manager: fixture.controller,
                jupiter_io_account: jupiter_destination.clone(),
                sns_governance: governance,
                sns_root: root,
                expected_sns_governance_module_hash: governance_hash,
                approved_reward_event_duration_seconds: 86_400,
                io_reserve: reserve.clone(),
                liquid_icp: Account {
                    owner: stream,
                    subaccount: Some(vec![3; 32]),
                },
                nonredeemable_governance_io_accounts: vec![],
                minimum_redemption_io_e8s: 20_000,
                expected_io_fee_e8s: ICP_FEE_E8S.into(),
                expected_icp_fee_e8s: ICP_FEE_E8S.into(),
                maximum_request_lifetime_nanos: 900_000_000_000,
                retry_delay_nanos: 1_000_000_000,
                ledger_deduplication_window_nanos: 86_400_000_000_000,
            },
        })
        .unwrap(),
        None,
    );
    ControlledStream {
        governance,
        io_ledger,
        stream_wasm,
        jupiter_destination,
        reserve,
    }
}

fn fund_stream_liquidity(fixture: &ControlledNnsNeuron, stream: Principal, current_e8s: u64) {
    let amount = RECONCILED_TARGET_E8S
        .checked_sub(current_e8s)
        .expect("unwind liquidity must not exceed the reconciled target");
    let to = IcpAccount::new(stream, Some(Subaccount([3; 32]))).icp_account_identifier_bytes();
    let transfer: Result<u64, IcpTransferError> = icrc::update_one(
        &fixture.pic,
        fixture.ledger,
        Principal::anonymous(),
        "transfer",
        IcpTransferArgs {
            memo: 13,
            amount: IcpTokens { e8s: amount },
            fee: IcpTokens { e8s: ICP_FEE_E8S },
            from_subaccount: None,
            to: to.to_vec(),
            created_at_time: None,
        },
    );
    transfer.unwrap();
}

struct RealSnsMaturityTrigger {
    governance: crate::sns_governance_setup::GovernanceLedgerFixture,
    neuron_id: crate::sns_governance_setup::NeuronId,
}

fn install_real_sns_maturity_trigger(fixture: &ControlledNnsNeuron) -> RealSnsMaturityTrigger {
    let governance = crate::sns_governance_setup::setup_real_sns_governance_with_ledger_on_pic(
        true,
        1_000_000_000,
        fixture.pic.clone(),
    )
    .unwrap();
    let neuron_id = crate::sns_governance_setup::stake_and_claim_neuron(
        &governance,
        500_000_000,
        91,
        b"real-two-year-trigger",
    )
    .unwrap();
    crate::sns_governance_setup::configure_increase_dissolve_delay(
        &governance,
        &neuron_id,
        u32::try_from(io_core_model::SNS_USER_DISSOLVE_DELAY_SECONDS).unwrap(),
    );
    RealSnsMaturityTrigger {
        governance,
        neuron_id,
    }
}

fn register_real_two_year_function(trigger: &RealSnsMaturityTrigger, manager: Principal) {
    use crate::sns_governance_setup::{
        Action, FunctionType, GenericNervousSystemFunction, NervousSystemFunction, Topic,
    };
    crate::sns_governance_setup::make_action(
        &trigger.governance,
        &trigger.neuron_id,
        "Register protected two-year maturity",
        Action::AddGenericNervousSystemFunction(NervousSystemFunction {
            id: 1_001,
            name: "Start protected two-year maturity".into(),
            description: Some("Invoke the reviewed IO NNS maturity operation".into()),
            function_type: Some(FunctionType::GenericNervousSystemFunction(
                GenericNervousSystemFunction {
                    validator_canister_id: Some(manager),
                    target_canister_id: Some(manager),
                    validator_method_name: Some("validate_start_maturity".into()),
                    target_method_name: Some("start_maturity".into()),
                    topic: Some(Topic::ApplicationBusinessLogic),
                },
            )),
        }),
    );
    for _ in 0..20 {
        trigger.governance.pic.tick();
    }
    let listed: crate::sns_governance_setup::ListNervousSystemFunctionsResponse = query(
        &trigger.governance.pic,
        trigger.governance.governance,
        Principal::anonymous(),
        "list_nervous_system_functions",
        (),
    );
    assert!(listed.functions.iter().any(|function| function.id == 1_001));
}

fn propose_real_two_year_start(trigger: &RealSnsMaturityTrigger, title: &str) -> u64 {
    crate::sns_governance_setup::make_action(
        &trigger.governance,
        &trigger.neuron_id,
        title,
        crate::sns_governance_setup::Action::ExecuteGenericNervousSystemFunction(
            crate::sns_governance_setup::ExecuteGenericNervousSystemFunction {
                function_id: 1_001,
                payload: encode_one(ManagerMaturityKind::TwoYear).unwrap(),
            },
        ),
    )
}

fn propose_real_two_year_start_result(
    trigger: &RealSnsMaturityTrigger,
    title: &str,
) -> Result<u64, crate::sns_governance_setup::GovernanceError> {
    use crate::sns_governance_setup::{
        Action, Command, CommandResponse, ExecuteGenericNervousSystemFunction, ManageNeuron,
        ManageNeuronResponse, Proposal,
    };
    let response: ManageNeuronResponse = update(
        &trigger.governance.pic,
        trigger.governance.governance,
        trigger.governance.controller,
        "manage_neuron",
        ManageNeuron {
            subaccount: trigger.neuron_id.id.clone(),
            command: Some(Command::MakeProposal(Proposal {
                url: String::new(),
                title: title.into(),
                summary: title.into(),
                action: Some(Action::ExecuteGenericNervousSystemFunction(
                    ExecuteGenericNervousSystemFunction {
                        function_id: 1_001,
                        payload: encode_one(ManagerMaturityKind::TwoYear).unwrap(),
                    },
                )),
            })),
        },
    );
    match response.command {
        Some(CommandResponse::MakeProposal(response)) => Ok(response
            .proposal_id
            .expect("accepted proposal must return an id")
            .id),
        Some(CommandResponse::Error(error)) => Err(error),
        other => panic!("unexpected maturity proposal response: {other:?}"),
    }
}

fn assert_real_sns_proposal_executed(trigger: &RealSnsMaturityTrigger, proposal_id: u64) {
    use crate::sns_governance_setup::{ListProposals, ListProposalsResponse};
    let proposals: ListProposalsResponse = query(
        &trigger.governance.pic,
        trigger.governance.governance,
        Principal::anonymous(),
        "list_proposals",
        ListProposals {
            include_reward_status: vec![],
            before_proposal: None,
            limit: 100,
            exclude_type: vec![],
            include_status: vec![],
            include_topics: None,
        },
    );
    let proposal = proposals
        .proposals
        .into_iter()
        .find(|proposal| proposal.id.as_ref().is_some_and(|id| id.id == proposal_id))
        .expect("maturity proposal must remain queryable");
    assert!(proposal.executed_timestamp_seconds > 0, "{proposal:?}");
    assert_eq!(proposal.failed_timestamp_seconds, 0, "{proposal:?}");
}

fn settle_controlled_genesis_pool(
    fixture: &ControlledNnsNeuron,
    trigger: &RealSnsMaturityTrigger,
    stream: Principal,
    require_pool: bool,
) -> Vec<String> {
    let mut steps = Vec::new();
    let observation: Result<
        io_stream_manager::RewardEventObservation,
        io_stream_manager::ApiError,
    > = update(
        &fixture.pic,
        stream,
        Principal::anonymous(),
        "resume_reward_work",
        (),
    );
    steps.push(format!("reward={observation:?}"));
    let backing: Result<io_stream_manager::RewardBackingProgress, io_stream_manager::ApiError> =
        update(
            &fixture.pic,
            stream,
            Principal::anonymous(),
            "resume_reward_backing",
            (),
        );
    steps.push(format!("backing={backing:?}"));

    let mut saw_pool = false;
    for attempt in 0..32 {
        let manager: ManagerStatus = query(
            &fixture.pic,
            fixture.controller,
            Principal::anonymous(),
            "get_status",
            (),
        );
        let stream_status: io_stream_manager::Status = query(
            &fixture.pic,
            stream,
            Principal::anonymous(),
            "get_status",
            (),
        );
        if manager.active_operation.as_deref() == Some("Pool") && !saw_pool {
            saw_pool = true;
            let validation: Result<String, String> = query(
                &fixture.pic,
                fixture.controller,
                Principal::anonymous(),
                "validate_start_maturity",
                ManagerMaturityKind::TwoYear,
            );
            assert!(matches!(validation, Err(message) if message.contains("busy with Pool")));
            let rejected = propose_real_two_year_start_result(
                trigger,
                "Maturity must not be admitted while genesis Pool is active",
            )
            .expect_err("real SNS validator must reject the contended proposal");
            assert!(
                rejected.error_message.contains("busy with Pool"),
                "{rejected:?}"
            );
        }
        if manager.active_operation.as_deref() != Some("Pool")
            && stream_status.operation_kind.is_none()
        {
            break;
        }
        let stream_resume: Result<io_stream_manager::StreamProgress, io_stream_manager::ApiError> =
            update(&fixture.pic, stream, Principal::anonymous(), "resume", ());
        let backing_resume: Result<
            io_stream_manager::RewardBackingProgress,
            io_stream_manager::ApiError,
        > = update(
            &fixture.pic,
            stream,
            Principal::anonymous(),
            "resume_reward_backing",
            (),
        );
        let manager_resume: Result<ManagerNnsProgress, ManagerApiError> = update(
            &fixture.pic,
            fixture.controller,
            Principal::anonymous(),
            "resume",
            (),
        );
        steps.push(format!(
            "attempt={attempt} stream={stream_resume:?} backing={backing_resume:?} manager={manager_resume:?}"
        ));
        fixture.pic.tick();
    }
    let final_manager: ManagerStatus = query(
        &fixture.pic,
        fixture.controller,
        Principal::anonymous(),
        "get_status",
        (),
    );
    let final_stream: io_stream_manager::Status = query(
        &fixture.pic,
        stream,
        Principal::anonymous(),
        "get_status",
        (),
    );
    assert_eq!(final_manager.active_operation.as_deref(), None, "{steps:?}");
    assert!(final_stream.operation_kind.is_none(), "{steps:?}");
    assert!(!require_pool || saw_pool, "{steps:?}");
    if saw_pool {
        let backing: io_nns_types::backing::ClaimAssetObservation =
            update::<Result<_, ManagerApiError>>(
                &fixture.pic,
                fixture.controller,
                stream,
                "observe_claim_assets",
                (),
            )
            .unwrap();
        let parent = backing.parent.expect("completed Pool must have a parent");
        let target = final_manager
            .latest_pooled_target
            .expect("completed Pool must retain its canonical target");
        let accounted_physical = target
            .target_e8s
            .checked_add(backing.anchor_available_e8s)
            .and_then(|value| value.checked_add(backing.excluded_dynamic_surplus_e8s))
            .expect("Dynamic parent partition must fit u128");
        match target.status {
            ManagerPooledTargetStatus::AtTarget => {
                assert_eq!(parent.physical_principal_e8s, accounted_physical)
            }
            ManagerPooledTargetStatus::OverTarget => {
                assert!(parent.physical_principal_e8s > accounted_physical)
            }
            ManagerPooledTargetStatus::AtTargetWithinUnwindTolerance => {
                assert!(parent.physical_principal_e8s >= accounted_physical)
            }
            ManagerPooledTargetStatus::UnderTarget => {
                panic!("completed Pool remained under target: {parent:?} {target:?}")
            }
        }
    }
    steps
}

struct CombinedRealSns {
    governance: crate::sns_governance_setup::GovernanceLedgerFixture,
    root: Principal,
    stream: Principal,
    reserve: io_stream_manager::Account,
    liquid: io_stream_manager::Account,
    governance_hash: Vec<u8>,
    neurons: Vec<crate::sns_governance_setup::NeuronId>,
}

fn install_combined_real_sns(fixture: &ControlledNnsNeuron) -> CombinedRealSns {
    use crate::artifacts::{resolve_from_env, ArtifactStatus};
    use crate::sns_root_setup::SnsRootCanister;
    use pocket_ic::CanisterSettings;

    let artifacts = match resolve_from_env(true).unwrap() {
        ArtifactStatus::Ready(artifacts) => artifacts,
        ArtifactStatus::Skipped(message) => panic!("{message}"),
    };
    let governance_wasm = artifacts.load_required("sns_governance").unwrap();
    let root_wasm = artifacts.load_required("sns_root").unwrap();
    let ledger_wasm = artifacts.load_required("sns_ledger").unwrap();
    let governance_hash = Sha256::digest(&governance_wasm).to_vec();
    let sns_subnet = fixture.pic.topology().get_sns().unwrap();
    let root = fixture
        .pic
        .create_canister_on_subnet(None, None, sns_subnet);
    fixture.pic.add_cycles(root, 2_000_000_000_000);
    let governed = CanisterSettings {
        controllers: Some(vec![root]),
        ..Default::default()
    };
    let governance =
        fixture
            .pic
            .create_canister_on_subnet(None, Some(governed.clone()), sns_subnet);
    fixture.pic.add_cycles(governance, 2_000_000_000_000);
    let index = fixture
        .pic
        .create_canister_on_subnet(None, Some(governed.clone()), sns_subnet);
    fixture.pic.add_cycles(index, 2_000_000_000_000);
    let swap = fixture
        .pic
        .create_canister_on_subnet(None, Some(governed), sns_subnet);
    fixture.pic.add_cycles(swap, 2_000_000_000_000);
    let stream = pocketic_env::create_empty_application_canister(&fixture.pic);
    let reserve_subaccount = icrc::subaccount("combined-real-reserve");
    let reserve = io_stream_manager::Account {
        owner: stream,
        subaccount: Some(reserve_subaccount.to_vec()),
    };
    let liquid = io_stream_manager::Account {
        owner: stream,
        subaccount: Some(vec![3; 32]),
    };
    let user = icrc::account(fixture.controller, None);
    let ledger = pocketic_env::create_sns_canister(
        &fixture.pic,
        ledger_wasm,
        icrc::ledger_init_arg(
            Principal::anonymous(),
            icrc::account(Principal::from_slice(&[94; 29]), None),
            vec![
                (user, 700_030_000),
                (
                    icrc::account(stream, Some(reserve_subaccount)),
                    1_000_000_000_000_000,
                ),
            ],
        ),
    );
    fixture.pic.install_canister(
        root,
        root_wasm,
        encode_one(SnsRootCanister {
            dapp_canister_ids: vec![stream, fixture.controller],
            extensions: None,
            testflight: true,
            archive_canister_ids: vec![],
            governance_canister_id: Some(governance),
            index_canister_id: Some(index),
            swap_canister_id: Some(swap),
            ledger_canister_id: Some(ledger),
            timers: None,
        })
        .unwrap(),
        None,
    );
    fixture.pic.install_canister(
        governance,
        governance_wasm.clone(),
        crate::sns_governance_setup::governance_init_arg(Some(ledger), Some(root)),
        Some(root),
    );
    for _ in 0..5 {
        fixture.pic.tick();
    }
    let governance_fixture = crate::sns_governance_setup::GovernanceLedgerFixture {
        pic: fixture.pic.clone(),
        governance,
        ledger,
        controller: fixture.controller,
    };
    let neurons = [100_000_000, 200_000_000, 300_000_000]
        .into_iter()
        .enumerate()
        .map(|(index, stake)| {
            let neuron = crate::sns_governance_setup::stake_and_claim_neuron(
                &governance_fixture,
                stake,
                u64::try_from(index + 201).unwrap(),
                b"combined-real-reward",
            )
            .unwrap();
            crate::sns_governance_setup::configure_increase_dissolve_delay(
                &governance_fixture,
                &neuron,
                u32::try_from(io_core_model::SNS_USER_DISSOLVE_DELAY_SECONDS).unwrap(),
            );
            neuron
        })
        .collect();
    CombinedRealSns {
        governance: governance_fixture,
        root,
        stream,
        reserve,
        liquid,
        governance_hash,
        neurons,
    }
}

fn install_combined_stream(fixture: &ControlledNnsNeuron, sns: &CombinedRealSns) -> Vec<u8> {
    use io_stream_manager::{InitArgs, StreamConfig};
    let wasm = current_io_wasm("io_stream_manager");
    fixture.pic.install_canister(
        sns.stream,
        wasm.clone(),
        encode_one(InitArgs {
            config: StreamConfig {
                io_ledger: sns.governance.ledger,
                icp_ledger: fixture.ledger,
                nns_manager: fixture.controller,
                jupiter_io_account: io_stream_manager::Account {
                    owner: fixture.controller,
                    subaccount: Some(vec![4; 32]),
                },
                sns_governance: sns.governance.governance,
                sns_root: sns.root,
                expected_sns_governance_module_hash: sns.governance_hash.clone(),
                approved_reward_event_duration_seconds: 86_400,
                io_reserve: sns.reserve.clone(),
                liquid_icp: sns.liquid.clone(),
                nonredeemable_governance_io_accounts: vec![],
                minimum_redemption_io_e8s: 20_000,
                expected_io_fee_e8s: ICP_FEE_E8S.into(),
                expected_icp_fee_e8s: ICP_FEE_E8S.into(),
                maximum_request_lifetime_nanos: 900_000_000_000,
                retry_delay_nanos: 1_000_000_000,
                ledger_deduplication_window_nanos: 86_400_000_000_000,
            },
        })
        .unwrap(),
        None,
    );
    wasm
}

impl From<ManagerAccount> for io_stream_manager::Account {
    fn from(value: ManagerAccount) -> Self {
        Self {
            owner: value.owner,
            subaccount: value.subaccount,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pooled_parent_and_permanent_delays_are_role_specific() {
        assert_eq!(u64::from(PERMANENT_DELAY_SECONDS), 63_115_200);
        assert_eq!(u64::from(POOLED_PARENT_DELAY_SECONDS), 1_209_600);
        assert_eq!(io_core_model::SNS_USER_DISSOLVE_DELAY_SECONDS, 1_296_060);

        let manager = include_str!("../../../canisters/io_nns_neuron_manager/src/execution.rs");
        assert!(manager.contains("NNS_DYNAMIC_DISSOLVE_DELAY_SECONDS"));
        let harness = include_str!("nns_backing.rs");
        assert!(harness.contains("POOLED_PARENT_DELAY_SECONDS"));
    }

    #[test]
    fn real_vertical_sources_stay_explicit_and_bounded() {
        let all = include_str!("nns_backing.rs");
        let harness = &all[..all.find("\nmod tests").unwrap()];
        assert_eq!(harness.matches("\"notify_jupiter_deposit\"").count(), 1);
        assert!(all.contains("debug_set_latest_reward_event"));
        assert!(!harness.contains("real SNS Governance generic function"));
        assert!(!harness.contains("merge-back interruption"));
    }

    #[test]
    #[ignore = "requires pinned real NNS Governance/ICP ledger and POCKET_IC_BIN"]
    fn controlled_pinned_nns_produces_zero_baseline_then_canonical_maturity() {
        let _guard = crate::lock_test_env();
        let fixture = super::create_zero_maturity_protected_neuron();
        let maturity = super::earn_maturity(&fixture);
        assert!(maturity >= 200_000_000, "maturity={maturity}");
        let evidence = super::execute_maturity(&fixture);
        assert_eq!(evidence.original_maturity_e8s, maturity);
        assert!(evidence.actual_minted_e8s > 0);
        assert!(evidence.actual_minted_e8s <= evidence.nominal_disbursed_maturity_e8s);
        eprintln!("controlled_maturity_evidence={evidence:?}");
    }

    #[test]
    #[ignore = "requires pinned real NNS Governance/ICP ledger, candidate SNS ledger, current IO release Wasms, and POCKET_IC_BIN"]
    fn controlled_jupiter_uses_real_nns_and_exact_production_receipts() {
        let _guard = crate::lock_test_env();
        let fixture = super::create_zero_maturity_protected_neuron();
        let stream = crate::pocketic_env::create_empty_application_canister(&fixture.pic);
        let manager_wasm = super::current_io_wasm("io_nns_neuron_manager");
        let controlled_stream = super::install_controlled_stream(
            &fixture,
            stream,
            super::current_io_wasm("io_stream_manager"),
        );
        super::fund_manager_staging(&fixture);
        let jupiter = super::install_manager(
            &fixture,
            stream,
            controlled_stream.governance,
            manager_wasm.clone(),
        );
        super::fund_stream_liquidity(&fixture, stream, 0);

        let manager_unpause: Result<(), ManagerApiError> = super::update(
            &fixture.pic,
            fixture.controller,
            controlled_stream.governance,
            "set_paused",
            false,
        );
        assert_eq!(manager_unpause, Ok(()));
        let stream_unpause: Result<(), io_stream_manager::ApiError> = super::update(
            &fixture.pic,
            stream,
            controlled_stream.governance,
            "set_paused",
            false,
        );
        assert_eq!(stream_unpause, Ok(()));

        let gross_e8s = 10 * 100_000_000_u64;
        let transfer = |caller: Principal, to: Vec<u8>, amount_e8s: u64, memo: u64| -> u64 {
            let result: Result<u64, IcpTransferError> = super::icrc::update_one(
                &fixture.pic,
                fixture.ledger,
                caller,
                "transfer",
                IcpTransferArgs {
                    memo,
                    amount: IcpTokens { e8s: amount_e8s },
                    fee: IcpTokens {
                        e8s: super::ICP_FEE_E8S,
                    },
                    from_subaccount: None,
                    to,
                    created_at_time: None,
                },
            );
            result.unwrap()
        };
        let jupiter_account = IcpAccount::new(jupiter, None).icp_account_identifier_bytes();
        let funding_block = transfer(
            Principal::anonymous(),
            jupiter_account.to_vec(),
            gross_e8s + super::ICP_FEE_E8S,
            20,
        );
        let manager_account =
            IcpAccount::new(fixture.controller, None).icp_account_identifier_bytes();
        let deposit_block = transfer(jupiter, manager_account.to_vec(), gross_e8s, 21);

        let pre_activation: Result<ManagerJupiterProgress, ManagerApiError> = super::update(
            &fixture.pic,
            fixture.controller,
            Principal::anonymous(),
            "notify_jupiter_deposit",
            NotifyJupiterDepositArgs { block_index: 0 },
        );
        assert!(matches!(
            pre_activation,
            Err(ManagerApiError::Invalid(message)) if message.contains("predates activation floor")
        ));

        let wrong: Result<ManagerJupiterProgress, ManagerApiError> = super::update(
            &fixture.pic,
            fixture.controller,
            Principal::anonymous(),
            "notify_jupiter_deposit",
            NotifyJupiterDepositArgs {
                block_index: funding_block.into(),
            },
        );
        assert!(matches!(wrong, Err(ManagerApiError::Invalid(_))));
        fixture.pic.advance_time(std::time::Duration::from_secs(1));

        let before_neuron = super::neuron(
            &fixture.pic,
            fixture.governance,
            fixture.controller,
            fixture.two_year_neuron_id,
        );
        let liquid_account = ManagerAccount {
            owner: stream,
            subaccount: Some(vec![3; 32]),
        };
        let liquid_before: candid::Nat = super::query(
            &fixture.pic,
            fixture.ledger,
            Principal::anonymous(),
            "icrc1_balance_of",
            liquid_account.clone(),
        );
        let recipient_before: candid::Nat = super::query(
            &fixture.pic,
            controlled_stream.io_ledger,
            Principal::anonymous(),
            "icrc1_balance_of",
            controlled_stream.jupiter_destination.clone(),
        );
        let reserve_before: candid::Nat = super::query(
            &fixture.pic,
            controlled_stream.io_ledger,
            Principal::anonymous(),
            "icrc1_balance_of",
            controlled_stream.reserve.clone(),
        );
        let supply_before: candid::Nat = super::query(
            &fixture.pic,
            controlled_stream.io_ledger,
            Principal::anonymous(),
            "icrc1_total_supply",
            (),
        );

        let notified: Result<ManagerJupiterProgress, ManagerApiError> = super::update(
            &fixture.pic,
            fixture.controller,
            Principal::anonymous(),
            "notify_jupiter_deposit",
            NotifyJupiterDepositArgs {
                block_index: deposit_block.into(),
            },
        );
        assert_eq!(notified, Ok(ManagerJupiterProgress::Pending));
        let duplicate: Result<ManagerJupiterProgress, ManagerApiError> = super::update(
            &fixture.pic,
            fixture.controller,
            controlled_stream.governance,
            "notify_jupiter_deposit",
            NotifyJupiterDepositArgs {
                block_index: deposit_block.into(),
            },
        );
        assert_eq!(duplicate, notified);

        fixture
            .pic
            .upgrade_canister(
                fixture.controller,
                manager_wasm.clone(),
                encode_one(()).unwrap(),
                None,
            )
            .unwrap();
        let mut phases = Vec::new();
        let mut stream_upgraded = false;
        let completed = loop {
            let progress: Result<ManagerNnsProgress, ManagerApiError> = super::update(
                &fixture.pic,
                fixture.controller,
                Principal::anonymous(),
                "resume",
                (),
            );
            phases.push(format!("{progress:?}"));
            let stream_status: io_stream_manager::Status = super::query(
                &fixture.pic,
                stream,
                Principal::anonymous(),
                "get_status",
                (),
            );
            if !stream_upgraded && stream_status.operation_kind.is_some() {
                fixture
                    .pic
                    .upgrade_canister(
                        stream,
                        controlled_stream.stream_wasm.clone(),
                        encode_one(()).unwrap(),
                        None,
                    )
                    .unwrap();
                stream_upgraded = true;
            }
            fixture
                .pic
                .upgrade_canister(
                    fixture.controller,
                    manager_wasm.clone(),
                    encode_one(()).unwrap(),
                    None,
                )
                .unwrap();
            if let Ok(ManagerNnsProgress::Jupiter(ManagerJupiterProgress::Completed(result))) =
                progress
            {
                break result;
            }
            assert!(phases.len() < 24, "{phases:?}");
        };
        assert!(stream_upgraded, "{phases:?}");
        assert_eq!(completed.deposit_block, u128::from(deposit_block));
        assert_eq!(completed.gross_e8s, u128::from(gross_e8s));
        assert_eq!(
            completed.stake_e8s,
            u128::from(gross_e8s * 40 / 100 - super::ICP_FEE_E8S)
        );
        assert_eq!(
            completed.liquid_e8s,
            u128::from(gross_e8s * 60 / 100 - super::ICP_FEE_E8S)
        );
        assert_ne!(
            completed.stake_transfer_block,
            completed.liquid_transfer_block
        );
        assert_eq!(completed.stream_receipt_sequence, 1);
        assert_eq!(completed.io_fee_e8s, 10_000);

        let after_neuron = super::neuron(
            &fixture.pic,
            fixture.governance,
            fixture.controller,
            fixture.two_year_neuron_id,
        );
        assert_eq!(
            after_neuron.cached_neuron_stake_e8s - before_neuron.cached_neuron_stake_e8s,
            u64::try_from(completed.stake_e8s).unwrap()
        );
        let liquid_after: candid::Nat = super::query(
            &fixture.pic,
            fixture.ledger,
            Principal::anonymous(),
            "icrc1_balance_of",
            liquid_account,
        );
        assert_eq!(
            liquid_after.0 - liquid_before.0,
            completed.liquid_e8s.into()
        );
        let recipient_after: candid::Nat = super::query(
            &fixture.pic,
            controlled_stream.io_ledger,
            Principal::anonymous(),
            "icrc1_balance_of",
            controlled_stream.jupiter_destination,
        );
        assert_eq!(
            recipient_after.0 - recipient_before.0,
            completed.backed_io_e8s.into()
        );
        let reserve_after: candid::Nat = super::query(
            &fixture.pic,
            controlled_stream.io_ledger,
            Principal::anonymous(),
            "icrc1_balance_of",
            controlled_stream.reserve,
        );
        assert_eq!(
            reserve_before.0 - reserve_after.0,
            (completed.backed_io_e8s + completed.io_fee_e8s).into()
        );
        let supply_after: candid::Nat = super::query(
            &fixture.pic,
            controlled_stream.io_ledger,
            Principal::anonymous(),
            "icrc1_total_supply",
            (),
        );
        assert_eq!(
            supply_before.0 - supply_after.0,
            completed.io_fee_e8s.into()
        );
        eprintln!(
            "account_semantic_jupiter gross_e8s={} permanent_credit_e8s={} claim_credit_e8s={} backed_io_e8s={} io_fee_e8s={} deposit_block={} unauthorized_rejected=true wrong_block_rejected=true receipt_sequence=1",
            gross_e8s,
            completed.stake_e8s,
            completed.liquid_e8s,
            completed.backed_io_e8s,
            completed.io_fee_e8s,
            deposit_block,
        );

        let replay: Result<ManagerJupiterProgress, ManagerApiError> = super::update(
            &fixture.pic,
            fixture.controller,
            Principal::anonymous(),
            "notify_jupiter_deposit",
            NotifyJupiterDepositArgs {
                block_index: deposit_block.into(),
            },
        );
        assert_eq!(replay, Ok(ManagerJupiterProgress::Completed(completed)));
        eprintln!("controlled_jupiter_phases={phases:?}");
    }

    #[test]
    #[ignore = "requires pinned real NNS Governance/ICP ledger, candidate SNS ledger, current IO release Wasms, and POCKET_IC_BIN"]
    fn controlled_two_year_compounds_real_maturity_without_io_issuance() {
        let _guard = crate::lock_test_env();
        let fixture = super::create_zero_maturity_protected_neuron();
        let real_sns_trigger = super::install_real_sns_maturity_trigger(&fixture);
        let stream = crate::pocketic_env::create_empty_application_canister(&fixture.pic);
        let manager_wasm = super::current_io_wasm("io_nns_neuron_manager");
        let controlled_stream = super::install_controlled_stream(
            &fixture,
            stream,
            super::current_io_wasm("io_stream_manager"),
        );
        super::fund_manager_staging(&fixture);
        let _ = super::install_manager(
            &fixture,
            stream,
            real_sns_trigger.governance.governance,
            manager_wasm.clone(),
        );
        super::register_real_two_year_function(&real_sns_trigger, fixture.controller);
        super::fund_stream_liquidity(&fixture, stream, 0);
        let prior_staked_maturity = 0;
        let mut actual_mints = Vec::new();
        for cycle in 0..2 {
            let unpause: Result<(), ManagerApiError> = super::update(
                &fixture.pic,
                fixture.controller,
                real_sns_trigger.governance.governance,
                "set_paused",
                false,
            );
            assert_eq!(unpause, Ok(()));
            if cycle == 0 {
                let stream_unpause: Result<(), io_stream_manager::ApiError> = super::update(
                    &fixture.pic,
                    stream,
                    controlled_stream.governance,
                    "set_paused",
                    false,
                );
                assert_eq!(stream_unpause, Ok(()));
            }
            let pool_steps = super::settle_controlled_genesis_pool(
                &fixture,
                &real_sns_trigger,
                stream,
                cycle == 0,
            );
            let maturity_validation: Result<String, String> = super::query(
                &fixture.pic,
                fixture.controller,
                Principal::anonymous(),
                "validate_start_maturity",
                ManagerMaturityKind::TwoYear,
            );
            assert!(
                maturity_validation.is_ok(),
                "cycle={cycle} pool_steps={pool_steps:?} validation={maturity_validation:?}"
            );
            let economics_before: io_nns_types::backing::ClaimAssetObservation =
                super::update::<Result<_, ManagerApiError>>(
                    &fixture.pic,
                    fixture.controller,
                    stream,
                    "observe_claim_assets",
                    (),
                )
                .unwrap();
            let ordinary_maturity = super::earn_maturity_for(&fixture, fixture.two_year_neuron_id);
            let before = super::neuron(
                &fixture.pic,
                fixture.governance,
                fixture.controller,
                fixture.two_year_neuron_id,
            );
            assert_eq!(
                before.staked_maturity_e8s_equivalent.unwrap_or(0),
                prior_staked_maturity
            );
            let liquid_account = ManagerAccount {
                owner: stream,
                subaccount: Some(vec![3; 32]),
            };
            let liquid_before: candid::Nat = super::query(
                &fixture.pic,
                fixture.ledger,
                Principal::anonymous(),
                "icrc1_balance_of",
                liquid_account.clone(),
            );
            let supply_before: candid::Nat = super::query(
                &fixture.pic,
                controlled_stream.io_ledger,
                Principal::anonymous(),
                "icrc1_total_supply",
                (),
            );
            let reserve_before: candid::Nat = super::query(
                &fixture.pic,
                controlled_stream.io_ledger,
                Principal::anonymous(),
                "icrc1_balance_of",
                controlled_stream.reserve.clone(),
            );

            let unauthorized: Result<ManagerMaturityProgress, ManagerApiError> = super::update(
                &fixture.pic,
                fixture.controller,
                Principal::anonymous(),
                "start_maturity",
                ManagerMaturityKind::TwoYear,
            );
            assert_eq!(unauthorized, Err(ManagerApiError::Unauthorized));
            let proposal_id = super::propose_real_two_year_start(
                &real_sns_trigger,
                &format!("Start controlled two-year maturity cycle {cycle}"),
            );
            for _ in 0..20 {
                fixture.pic.tick();
            }
            super::assert_real_sns_proposal_executed(&real_sns_trigger, proposal_id);
            let started: ManagerStatus = super::query(
                &fixture.pic,
                fixture.controller,
                Principal::anonymous(),
                "get_status",
                (),
            );
            assert!(
                matches!(started.active_operation.as_deref(), Some("Maturity") | None),
                "an accepted proposal may expose immediate or passive maturity, never another operation: {started:?}"
            );
            let accepted_neuron = super::neuron(
                &fixture.pic,
                fixture.governance,
                fixture.controller,
                fixture.two_year_neuron_id,
            );
            assert_eq!(
                accepted_neuron
                    .maturity_disbursements_in_progress
                    .as_ref()
                    .map(Vec::len),
                Some(1),
                "executed SNS proposal must have durably accepted real NNS maturity work"
            );
            let replay = fixture
                .pic
                .update_call(
                    fixture.controller,
                    real_sns_trigger.governance.governance,
                    "start_maturity",
                    encode_one(ManagerMaturityKind::TwoYear).unwrap(),
                )
                .expect_err("accepted maturity work must reject a second SNS target call");
            let replay = format!("{replay:?}");
            assert!(
                replay.contains("busy with Maturity") || replay.contains("already pending"),
                "{replay}"
            );
            let replay_error = super::propose_real_two_year_start_result(
                &real_sns_trigger,
                &format!("Replay controlled two-year maturity cycle {cycle}"),
            )
            .expect_err("validator must reject a second maturity proposal");
            assert!(
                replay_error.error_message.contains("busy with Maturity")
                    || replay_error.error_message.contains("already pending"),
                "{replay_error:?}"
            );
            let replayed: ManagerStatus = super::query(
                &fixture.pic,
                fixture.controller,
                Principal::anonymous(),
                "get_status",
                (),
            );
            assert_eq!(replayed, started);
            assert_eq!(
                super::neuron(
                    &fixture.pic,
                    fixture.governance,
                    fixture.controller,
                    fixture.two_year_neuron_id,
                ),
                accepted_neuron,
                "rejected replay must not submit or mutate maturity"
            );
            fixture
                .pic
                .upgrade_canister(
                    fixture.controller,
                    manager_wasm.clone(),
                    encode_one(()).unwrap(),
                    None,
                )
                .unwrap();

            let mut phases = Vec::new();
            loop {
                let progress: Result<ManagerNnsProgress, ManagerApiError> = super::update(
                    &fixture.pic,
                    fixture.controller,
                    Principal::anonymous(),
                    "resume",
                    (),
                );
                phases.push(format!("{progress:?}"));
                fixture
                    .pic
                    .upgrade_canister(
                        fixture.controller,
                        manager_wasm.clone(),
                        encode_one(()).unwrap(),
                        None,
                    )
                    .unwrap();
                if progress
                    == Ok(ManagerNnsProgress::Maturity(
                        ManagerMaturityProgress::Pending,
                    ))
                {
                    break;
                }
                assert!(phases.len() < 12, "cycle={cycle} {phases:?}");
            }
            let pending = super::neuron(
                &fixture.pic,
                fixture.governance,
                fixture.controller,
                fixture.two_year_neuron_id,
            );
            let disbursements = pending.maturity_disbursements_in_progress.unwrap();
            assert_eq!(disbursements.len(), 1);
            let finalization = disbursements[0]
                .finalize_disbursement_timestamp_seconds
                .unwrap();
            let now = fixture.pic.get_time().as_nanos_since_unix_epoch() / 1_000_000_000;
            fixture
                .pic
                .advance_time(Duration::from_secs(finalization - now + 1));
            for _ in 0..100 {
                fixture.pic.tick();
            }
            let completed = loop {
                let progress: Result<ManagerNnsProgress, ManagerApiError> = super::update(
                    &fixture.pic,
                    fixture.controller,
                    Principal::anonymous(),
                    "resume",
                    (),
                );
                phases.push(format!("{progress:?}"));
                if let Ok(ManagerNnsProgress::Maturity(ManagerMaturityProgress::Completed(
                    completed,
                ))) = progress
                {
                    break completed;
                }
                assert!(phases.len() < 24, "cycle={cycle} {phases:?}");
            };
            let plan = io_nns_types::maturity::plan_two_year_replenishment(
                completed.captured_e8s,
                economics_before.anchor_target_e8s,
                economics_before.anchor_available_e8s,
                economics_before.permanent_fee_shortfall_e8s,
                u128::from(super::ICP_FEE_E8S),
            )
            .unwrap();
            assert_eq!(completed.kind, ManagerMaturityKind::TwoYear);
            assert_eq!(
                completed.anchor_reimbursement_e8s,
                plan.anchor_reimbursement
            );
            assert_eq!(
                completed.permanent_reimbursement_e8s,
                plan.permanent_reimbursement
            );
            assert_eq!(
                completed.reimbursement_transfer_fees_e8s,
                plan.reimbursement_transfer_fees
            );
            assert_eq!(completed.carried_e8s, plan.carried);
            assert_eq!(
                completed.permanent_credit_e8s,
                plan.ordinary.map_or(0, |split| split.permanent_credit)
            );
            assert_eq!(
                completed.claim_credit_e8s,
                plan.ordinary.map_or(0, |split| split.claim_credit)
            );
            assert_eq!(completed.entitlement_batch_generation, None);
            assert_eq!(completed.two_week_target_e8s, None);
            assert!(completed.captured_e8s > 0);
            assert!(completed.captured_e8s <= u128::from(ordinary_maturity));
            let after = super::neuron(
                &fixture.pic,
                fixture.governance,
                fixture.controller,
                fixture.two_year_neuron_id,
            );
            assert_eq!(
                after.staked_maturity_e8s_equivalent.unwrap_or(0),
                prior_staked_maturity
            );
            assert_eq!(
                u128::from(after.cached_neuron_stake_e8s),
                u128::from(before.cached_neuron_stake_e8s)
                    + completed.permanent_reimbursement_e8s
                    + completed.permanent_credit_e8s
            );
            let liquid_after: candid::Nat = super::query(
                &fixture.pic,
                fixture.ledger,
                Principal::anonymous(),
                "icrc1_balance_of",
                liquid_account,
            );
            assert_eq!(
                liquid_after.0 - liquid_before.0,
                completed.claim_credit_e8s.into()
            );
            let supply_after: candid::Nat = super::query(
                &fixture.pic,
                controlled_stream.io_ledger,
                Principal::anonymous(),
                "icrc1_total_supply",
                (),
            );
            let reserve_after: candid::Nat = super::query(
                &fixture.pic,
                controlled_stream.io_ledger,
                Principal::anonymous(),
                "icrc1_balance_of",
                controlled_stream.reserve.clone(),
            );
            assert_eq!(supply_after, supply_before);
            assert_eq!(reserve_after, reserve_before);
            let economics_after: io_nns_types::backing::ClaimAssetObservation =
                super::update::<Result<_, ManagerApiError>>(
                    &fixture.pic,
                    fixture.controller,
                    stream,
                    "observe_claim_assets",
                    (),
                )
                .unwrap();
            let ordinary_claim_fee = plan.ordinary.map_or(0, |split| split.claim_fee);
            let ordinary_permanent_fee = plan.ordinary.map_or(0, |split| split.permanent_fee);
            assert_eq!(
                economics_after.anchor_available_e8s,
                economics_before.anchor_available_e8s + plan.anchor_reimbursement
                    - ordinary_claim_fee
            );
            assert_eq!(
                economics_after.permanent_fee_shortfall_e8s,
                economics_before.permanent_fee_shortfall_e8s - plan.permanent_reimbursement
                    + ordinary_permanent_fee
            );
            eprintln!(
                "account_semantic_two_year cycle={} captured_e8s={} permanent_credit_e8s={} claim_credit_e8s={} no_issuance=true supply_unchanged=true reserve_unchanged=true",
                cycle,
                completed.captured_e8s,
                completed.permanent_credit_e8s,
                completed.claim_credit_e8s,
            );
            actual_mints.push(completed.captured_e8s);
            fixture
                .pic
                .upgrade_canister(
                    fixture.controller,
                    manager_wasm.clone(),
                    encode_one(()).unwrap(),
                    None,
                )
                .unwrap();
            eprintln!(
                "controlled_two_year_cycle={cycle} phases={phases:?} completed={completed:?}"
            );
        }
        assert_eq!(actual_mints.len(), 2);
        assert!(actual_mints.iter().all(|amount| *amount > 0));

        let unpause: Result<(), ManagerApiError> = super::update(
            &fixture.pic,
            fixture.controller,
            real_sns_trigger.governance.governance,
            "set_paused",
            false,
        );
        assert_eq!(unpause, Ok(()));
        super::configure_neuron(
            &fixture.pic,
            fixture.governance,
            fixture.controller,
            fixture.two_year_neuron_id,
            NnsProductionConfigureOperation::ChangeAutoStakeMaturity(NnsChangeAutoStakeMaturity {
                requested_setting_for_auto_stake_maturity: true,
            }),
        );
        let auto_stake_drift: Result<ManagerMaturityProgress, ManagerApiError> = super::update(
            &fixture.pic,
            fixture.controller,
            real_sns_trigger.governance.governance,
            "start_maturity",
            ManagerMaturityKind::TwoYear,
        );
        assert!(matches!(
            auto_stake_drift,
            Err(ManagerApiError::Invalid(ref reason)) if reason.contains("auto-stake")
        ));
        super::configure_neuron(
            &fixture.pic,
            fixture.governance,
            fixture.controller,
            fixture.two_year_neuron_id,
            NnsProductionConfigureOperation::ChangeAutoStakeMaturity(NnsChangeAutoStakeMaturity {
                requested_setting_for_auto_stake_maturity: false,
            }),
        );
        let unpause: Result<(), ManagerApiError> = super::update(
            &fixture.pic,
            fixture.controller,
            real_sns_trigger.governance.governance,
            "set_paused",
            false,
        );
        assert_eq!(unpause, Ok(()));
        super::configure_neuron(
            &fixture.pic,
            fixture.governance,
            fixture.controller,
            fixture.two_year_neuron_id,
            NnsProductionConfigureOperation::StartDissolving(EmptyRecord {}),
        );
        let dissolve_state_drift: Result<ManagerMaturityProgress, ManagerApiError> = super::update(
            &fixture.pic,
            fixture.controller,
            real_sns_trigger.governance.governance,
            "start_maturity",
            ManagerMaturityKind::TwoYear,
        );
        assert!(matches!(
            dissolve_state_drift,
            Err(ManagerApiError::Invalid(ref reason)) if reason.contains("dissolve")
        ));
        super::configure_neuron(
            &fixture.pic,
            fixture.governance,
            fixture.controller,
            fixture.two_year_neuron_id,
            NnsProductionConfigureOperation::StopDissolving(EmptyRecord {}),
        );
        let drifted_status: ManagerStatus = super::query(
            &fixture.pic,
            fixture.controller,
            Principal::anonymous(),
            "get_status",
            (),
        );
        assert_eq!(drifted_status.lifecycle, ManagerLifecycle::Paused);
        assert!(drifted_status.active_operation.is_none());
    }

    fn run_combined_real_sns_nns_io_lifecycle(
        jupiter_before_maturity: bool,
        debug_jupiter_interleaving: bool,
    ) {
        use candid::Nat;
        use io_stream_manager::{
            ApiError as StreamApiError, PreparedRedemption, RedeemArgs, RedemptionProgress,
            RewardBackingProgress, RewardEventClassification, Status as StreamStatus,
            StreamProgress,
        };

        let _guard = crate::lock_test_env();
        let mut fixture = super::create_zero_maturity_protected_neuron_with_stake(0);
        let sns = super::install_combined_real_sns(&fixture);
        let stream_wasm = super::install_combined_stream(&fixture, &sns);
        let manager_wasm = if debug_jupiter_interleaving {
            super::debug_wasm("io_nns_neuron_manager")
        } else {
            super::current_io_wasm("io_nns_neuron_manager")
        };
        super::fund_manager_staging(&fixture);
        let jupiter = super::install_manager(
            &fixture,
            sns.stream,
            sns.governance.governance,
            manager_wasm.clone(),
        );
        let liquid_transfer: Result<u64, IcpTransferError> = icrc::update_one(
            &fixture.pic,
            fixture.ledger,
            Principal::anonymous(),
            "transfer",
            IcpTransferArgs {
                memo: 31,
                amount: IcpTokens { e8s: 7_000_000_000 },
                fee: IcpTokens { e8s: ICP_FEE_E8S },
                from_subaccount: None,
                to: IcpAccount::new(sns.stream, Some(Subaccount([3; 32])))
                    .icp_account_identifier_bytes()
                    .to_vec(),
                created_at_time: None,
            },
        );
        liquid_transfer.unwrap();
        let manager_unpause: Result<(), ManagerApiError> = super::update(
            &fixture.pic,
            fixture.controller,
            sns.governance.governance,
            "set_paused",
            false,
        );
        assert_eq!(manager_unpause, Ok(()));
        let stream_unpause: Result<(), StreamApiError> = super::update(
            &fixture.pic,
            sns.stream,
            sns.governance.governance,
            "set_paused",
            false,
        );
        assert_eq!(stream_unpause, Ok(()));
        let _initial_event =
            crate::sns_governance_setup::advance_until_reward_event(&sns.governance, 0, 0);
        let mut initial_status: StreamStatus = super::query(
            &fixture.pic,
            sns.stream,
            Principal::anonymous(),
            "get_status",
            (),
        );
        for _ in 0..20 {
            if initial_status.latest_reconciliation_checkpoint.is_some() {
                break;
            }
            fixture.pic.tick();
            initial_status = super::query(
                &fixture.pic,
                sns.stream,
                Principal::anonymous(),
                "get_status",
                (),
            );
        }
        if initial_status.latest_reconciliation_checkpoint.is_none() {
            let initial_observation: Result<
                io_stream_manager::RewardEventObservation,
                StreamApiError,
            > = super::update(
                &fixture.pic,
                sns.stream,
                Principal::anonymous(),
                "resume_reward_work",
                (),
            );
            assert_eq!(initial_observation.unwrap().eligible_credit_total, 0);
            initial_status = super::query(
                &fixture.pic,
                sns.stream,
                Principal::anonymous(),
                "get_status",
                (),
            );
        }
        assert!(initial_status.latest_reconciliation_checkpoint.is_some());
        let first_reconciliation: Result<RewardBackingProgress, StreamApiError> = super::update(
            &fixture.pic,
            sns.stream,
            Principal::anonymous(),
            "resume_reward_backing",
            (),
        );
        assert!(
            matches!(
                first_reconciliation,
                Ok(RewardBackingProgress::Pending { .. }) | Err(StreamApiError::Pending(_))
            ),
            "{first_reconciliation:?}"
        );
        let mut pool_steps = Vec::new();
        let mut stream_pool_upgraded = false;
        let mut manager_pool_upgraded = false;
        let mut bootstrap_donation_sent = false;
        for _ in 0..24 {
            let status: StreamStatus = super::query(
                &fixture.pic,
                sns.stream,
                Principal::anonymous(),
                "get_status",
                (),
            );
            if status.operation_kind.is_none() {
                break;
            }
            if !bootstrap_donation_sent
                && status.operation_kind.as_deref() == Some("BackingReconciliation")
            {
                let staking = IcpAccount::new(
                    fixture.governance,
                    Some(Subaccount(super::neuron_subaccount(
                        fixture.controller,
                        PROTECTED_MEMO,
                    ))),
                )
                .icp_account_identifier_bytes();
                let donation: Result<u64, IcpTransferError> = icrc::update_one(
                    &fixture.pic,
                    fixture.ledger,
                    Principal::anonymous(),
                    "transfer",
                    IcpTransferArgs {
                        memo: 32,
                        amount: IcpTokens { e8s: 1_000_000 },
                        fee: IcpTokens { e8s: ICP_FEE_E8S },
                        from_subaccount: None,
                        to: staking.to_vec(),
                        created_at_time: None,
                    },
                );
                donation.unwrap();
                bootstrap_donation_sent = true;
            }
            if !stream_pool_upgraded
                && status.operation_kind.as_deref() == Some("BackingReconciliation")
            {
                fixture
                    .pic
                    .upgrade_canister(
                        sns.stream,
                        stream_wasm.clone(),
                        encode_one(()).unwrap(),
                        None,
                    )
                    .unwrap();
                assert_eq!(
                    super::query::<StreamStatus>(
                        &fixture.pic,
                        sns.stream,
                        Principal::anonymous(),
                        "get_status",
                        (),
                    )
                    .lifecycle,
                    io_stream_manager::Lifecycle::Paused
                );
                stream_pool_upgraded = true;
            }
            let manager_status: ManagerStatus = super::query(
                &fixture.pic,
                fixture.controller,
                Principal::anonymous(),
                "get_status",
                (),
            );
            if !manager_pool_upgraded && manager_status.active_operation.as_deref() == Some("Pool")
            {
                fixture
                    .pic
                    .upgrade_canister(
                        fixture.controller,
                        manager_wasm.clone(),
                        encode_one(()).unwrap(),
                        None,
                    )
                    .unwrap();
                assert_eq!(
                    super::query::<ManagerStatus>(
                        &fixture.pic,
                        fixture.controller,
                        Principal::anonymous(),
                        "get_status",
                        (),
                    )
                    .lifecycle,
                    ManagerLifecycle::Paused
                );
                manager_pool_upgraded = true;
            }
            let progress: Result<StreamProgress, StreamApiError> = super::update(
                &fixture.pic,
                sns.stream,
                Principal::anonymous(),
                "resume",
                (),
            );
            pool_steps.push((status.operation_phase, format!("{progress:?}")));
        }
        assert!(stream_pool_upgraded, "pool_steps={pool_steps:?}");
        assert!(manager_pool_upgraded, "pool_steps={pool_steps:?}");
        assert!(bootstrap_donation_sent, "pool_steps={pool_steps:?}");
        assert!(
            super::query::<StreamStatus>(
                &fixture.pic,
                sns.stream,
                Principal::anonymous(),
                "get_status",
                (),
            )
            .operation_kind
            .is_none(),
            "pool_steps={pool_steps:?}"
        );
        super::update::<Result<(), ManagerApiError>>(
            &fixture.pic,
            fixture.controller,
            sns.governance.governance,
            "set_paused",
            false,
        )
        .unwrap();
        super::update::<Result<(), StreamApiError>>(
            &fixture.pic,
            sns.stream,
            sns.governance.governance,
            "set_paused",
            false,
        )
        .unwrap();
        let backing: Result<io_nns_types::backing::ClaimAssetObservation, ManagerApiError> =
            super::update(
                &fixture.pic,
                fixture.controller,
                sns.stream,
                "observe_claim_assets",
                (),
            );
        let backing = backing.unwrap();
        let parent = backing
            .parent
            .as_ref()
            .expect("bootstrapped Dynamic parent must be proved");
        let manager_status: ManagerStatus = super::query(
            &fixture.pic,
            fixture.controller,
            Principal::anonymous(),
            "get_status",
            (),
        );
        let target = manager_status
            .latest_pooled_target
            .expect("completed genesis Pool must retain its claim target");
        assert_eq!(target.status, ManagerPooledTargetStatus::AtTarget);
        assert_eq!(
            parent.physical_principal_e8s,
            target.target_e8s + backing.anchor_available_e8s + backing.excluded_dynamic_surplus_e8s
        );
        if jupiter_before_maturity {
            let permanent_before_jupiter = super::neuron(
                &fixture.pic,
                fixture.governance,
                fixture.controller,
                fixture.two_year_neuron_id,
            );
            let stream_before_jupiter = super::query::<StreamStatus>(
                &fixture.pic,
                sns.stream,
                Principal::anonymous(),
                "get_status",
                (),
            );
            let liquid_before_jupiter = u128::try_from(
                super::query::<Nat>(
                    &fixture.pic,
                    fixture.ledger,
                    Principal::anonymous(),
                    "icrc1_balance_of",
                    sns.liquid.clone(),
                )
                .0,
            )
            .unwrap();
            let reserve_before_jupiter = u128::try_from(
                super::query::<Nat>(
                    &fixture.pic,
                    sns.governance.ledger,
                    Principal::anonymous(),
                    "icrc1_balance_of",
                    sns.reserve.clone(),
                )
                .0,
            )
            .unwrap();
            let supply_before_jupiter =
                u128::try_from(icrc::icrc1_total_supply(&fixture.pic, sns.governance.ledger).0)
                    .unwrap();
            let jupiter_io_account = io_stream_manager::Account {
                owner: fixture.controller,
                subaccount: Some(vec![4; 32]),
            };
            let jupiter_io_before = super::query::<Nat>(
                &fixture.pic,
                sns.governance.ledger,
                Principal::anonymous(),
                "icrc1_balance_of",
                jupiter_io_account.clone(),
            );
            let backing_before_jupiter: Result<
                io_nns_types::backing::ClaimAssetObservation,
                ManagerApiError,
            > = super::update(
                &fixture.pic,
                fixture.controller,
                sns.stream,
                "observe_claim_assets",
                (),
            );
            let backing_before_jupiter = backing_before_jupiter.unwrap();
            let jupiter_donation = 1_000_000_u64;
            assert!(debug_jupiter_interleaving);
            let _: () = super::update(
                &fixture.pic,
                fixture.controller,
                Principal::anonymous(),
                "debug_yield_before_jupiter_refresh_once",
                (),
            );
            let first_jupiter = super::run_jupiter_credit(
                &fixture,
                sns.stream,
                sns.governance.governance,
                jupiter,
                10 * 100_000_000,
                40,
                jupiter_donation,
            );
            assert_eq!(
                first_jupiter.observed_after_cached_stake_e8s,
                u128::from(permanent_before_jupiter.cached_neuron_stake_e8s)
                    + first_jupiter.stake_e8s
                    + u128::from(jupiter_donation)
            );
            assert_eq!(
                first_jupiter.stake_e8s,
                u128::from(10 * 100_000_000 * 40 / 100 - ICP_FEE_E8S)
            );
            let permanent_after_jupiter = super::neuron(
                &fixture.pic,
                fixture.governance,
                fixture.controller,
                fixture.two_year_neuron_id,
            );
            assert_eq!(permanent_after_jupiter.id, permanent_before_jupiter.id);
            assert_eq!(
                permanent_after_jupiter.account,
                super::neuron_subaccount(fixture.controller, TWO_YEAR_MEMO)
            );
            let expected_backed_io = io_core_model::backed_io(
                first_jupiter.liquid_e8s,
                liquid_before_jupiter
                    + backing_before_jupiter.claim_bearing_dynamic_principal_e8s
                    + backing_before_jupiter.live_child_net_backing_e8s
                    + backing_before_jupiter.transit_backing_e8s,
                supply_before_jupiter - reserve_before_jupiter,
            )
            .unwrap();
            assert_eq!(first_jupiter.backed_io_e8s, expected_backed_io);
            let jupiter_io_after = super::query::<Nat>(
                &fixture.pic,
                sns.governance.ledger,
                Principal::anonymous(),
                "icrc1_balance_of",
                jupiter_io_account.clone(),
            );
            assert_eq!(
                jupiter_io_after.0.clone() - jupiter_io_before.0,
                first_jupiter.backed_io_e8s.into()
            );
            let stream_after_jupiter = super::query::<StreamStatus>(
                &fixture.pic,
                sns.stream,
                Principal::anonymous(),
                "get_status",
                (),
            );
            assert_eq!(
                stream_after_jupiter.accumulated_eligible_credit,
                stream_before_jupiter.accumulated_eligible_credit
            );
            assert_eq!(
                stream_after_jupiter.accumulated_policy_credit,
                stream_before_jupiter.accumulated_policy_credit
            );
            let replay: Result<ManagerJupiterProgress, ManagerApiError> = super::update(
                &fixture.pic,
                fixture.controller,
                jupiter,
                "notify_jupiter_deposit",
                NotifyJupiterDepositArgs {
                    block_index: first_jupiter.deposit_block,
                },
            );
            assert_eq!(
                replay,
                Ok(ManagerJupiterProgress::Completed(first_jupiter.clone()))
            );
            assert_eq!(
                super::neuron(
                    &fixture.pic,
                    fixture.governance,
                    fixture.controller,
                    fixture.two_year_neuron_id,
                ),
                permanent_after_jupiter
            );
            assert_eq!(
                super::query::<Nat>(
                    &fixture.pic,
                    sns.governance.ledger,
                    Principal::anonymous(),
                    "icrc1_balance_of",
                    jupiter_io_account,
                ),
                jupiter_io_after
            );
            let resumed: Result<ManagerNnsProgress, ManagerApiError> = super::update(
                &fixture.pic,
                fixture.controller,
                Principal::anonymous(),
                "resume",
                (),
            );
            assert_eq!(resumed, Ok(ManagerNnsProgress::Idle));
            return;
        }
        fixture.neuron_id = parent.neuron_id;
        fixture.protected_principal_e8s = u64::try_from(parent.physical_principal_e8s).unwrap();
        let ordinary_maturity = super::earn_maturity_with_daily(&fixture, || {
            let _: Result<io_stream_manager::RewardEventObservation, StreamApiError> =
                super::update(
                    &fixture.pic,
                    sns.stream,
                    Principal::anonymous(),
                    "resume_reward_work",
                    (),
                );
        });
        assert!(ordinary_maturity >= 200_000_000);
        let baseline: StreamStatus = super::query(
            &fixture.pic,
            sns.stream,
            Principal::anonymous(),
            "get_status",
            (),
        );
        let baseline_round = baseline.latest_processed_reward_event.unwrap().round;
        let baseline_policy_credit = baseline.accumulated_policy_credit;
        let mut event = crate::sns_governance_setup::advance_until_reward_event(
            &sns.governance,
            0,
            baseline_round,
        );
        let mut reward_observations = 0;
        let mut reward_steps = Vec::new();
        let accumulated = loop {
            reward_observations += 1;
            assert!(
                reward_observations <= 10,
                "candidate reward event did not converge: {reward_steps:?}"
            );
            fixture.pic.advance_time(Duration::from_secs(301));
            for _ in 0..100 {
                let timer_status: StreamStatus = super::query(
                    &fixture.pic,
                    sns.stream,
                    Principal::anonymous(),
                    "get_status",
                    (),
                );
                if timer_status
                    .latest_processed_reward_event
                    .is_some_and(|processed| processed.round >= event.round)
                {
                    break;
                }
                fixture.pic.tick();
            }
            let mut observation: Result<io_stream_manager::RewardEventObservation, StreamApiError> =
                super::update(
                    &fixture.pic,
                    sns.stream,
                    Principal::anonymous(),
                    "resume_reward_work",
                    (),
                );
            let mut status: StreamStatus = super::query(
                &fixture.pic,
                sns.stream,
                Principal::anonymous(),
                "get_status",
                (),
            );
            if matches!(
                &observation,
                Err(StreamApiError::Pending(reason)) if reason.contains("not due")
            ) {
                super::update::<Result<(), StreamApiError>>(
                    &fixture.pic,
                    sns.stream,
                    sns.governance.governance,
                    "set_paused",
                    true,
                )
                .unwrap();
                super::update::<Result<(), StreamApiError>>(
                    &fixture.pic,
                    sns.stream,
                    sns.governance.governance,
                    "set_paused",
                    false,
                )
                .unwrap();
                observation = super::update(
                    &fixture.pic,
                    sns.stream,
                    Principal::anonymous(),
                    "resume_reward_work",
                    (),
                );
                status = super::query(
                    &fixture.pic,
                    sns.stream,
                    Principal::anonymous(),
                    "get_status",
                    (),
                );
            }
            if status.reward_processing_paused {
                let _: Result<RewardBackingProgress, StreamApiError> = super::update(
                    &fixture.pic,
                    sns.stream,
                    Principal::anonymous(),
                    "resume_reward_backing",
                    (),
                );
                super::update::<Result<(), StreamApiError>>(
                    &fixture.pic,
                    sns.stream,
                    sns.governance.governance,
                    "set_paused",
                    true,
                )
                .unwrap();
                super::update::<Result<(), StreamApiError>>(
                    &fixture.pic,
                    sns.stream,
                    sns.governance.governance,
                    "set_paused",
                    false,
                )
                .unwrap();
                observation = super::update(
                    &fixture.pic,
                    sns.stream,
                    Principal::anonymous(),
                    "resume_reward_work",
                    (),
                );
                status = super::query(
                    &fixture.pic,
                    sns.stream,
                    Principal::anonymous(),
                    "get_status",
                    (),
                );
            }
            reward_steps.push(format!(
                "event={} observation={observation:?} classification={:?} eligible={} policy={} paused={} checkpoint={:?}",
                event.round,
                status.latest_reward_event_classification,
                status.accumulated_eligible_credit,
                status.accumulated_policy_credit,
                status.reward_processing_paused,
                status.latest_reconciliation_checkpoint
            ));
            match status.latest_reward_event_classification {
                Some(RewardEventClassification::NoProposalFallback) => {
                    if status.accumulated_eligible_credit > 0 {
                        break status;
                    }
                    event = crate::sns_governance_setup::advance_until_reward_event(
                        &sns.governance,
                        0,
                        event.round,
                    );
                }
                Some(RewardEventClassification::MissedSkipped) => {
                    assert_eq!(status.accumulated_policy_credit, baseline_policy_credit);
                    event = crate::sns_governance_setup::advance_until_reward_event(
                        &sns.governance,
                        0,
                        event.round,
                    );
                }
                other => panic!("unexpected combined reward classification: {other:?}"),
            }
        };
        assert_eq!(
            accumulated.latest_reward_event_classification,
            Some(RewardEventClassification::NoProposalFallback)
        );
        assert!(
            accumulated.accumulated_policy_credit > accumulated.accumulated_eligible_credit,
            "the frozen batch must retain partial forfeiture"
        );
        assert_eq!(accumulated.accumulated_entitlements.len(), 3);
        let eligible_event_counts = sns
            .neurons
            .iter()
            .zip([100_000_000_u128, 200_000_000, 300_000_000])
            .map(|(neuron, stake)| {
                let daily = io_reward_policy::mul_div_floor(
                    io_reward_policy::DAILY_EVENT_CREDIT,
                    stake,
                    600_000_000,
                )
                .unwrap();
                let accumulated_credit = accumulated
                    .accumulated_entitlements
                    .iter()
                    .find(|credit| credit.sns_neuron_id == neuron.id)
                    .unwrap()
                    .accumulated_eligible_credit;
                assert_eq!(accumulated_credit % daily, 0);
                accumulated_credit / daily
            })
            .collect::<Vec<_>>();
        assert!(eligible_event_counts[0] > 0);
        assert!(eligible_event_counts
            .windows(2)
            .all(|pair| pair[0] == pair[1]));
        let frozen_credits = accumulated.accumulated_entitlements.clone();
        let manager_before_maturity: ManagerStatus = super::query(
            &fixture.pic,
            fixture.controller,
            Principal::anonymous(),
            "get_status",
            (),
        );
        let prepared: Result<RewardBackingProgress, StreamApiError> = super::update(
            &fixture.pic,
            sns.stream,
            Principal::anonymous(),
            "resume_reward_backing",
            (),
        );
        assert_eq!(
            prepared,
            Ok(RewardBackingProgress::MaturityPrepared { generation: 1 }),
            "manager={manager_before_maturity:?} stream={accumulated:?}"
        );

        let recipient_accounts = sns
            .neurons
            .iter()
            .map(|neuron| io_stream_manager::Account {
                owner: sns.governance.governance,
                subaccount: Some(neuron.id.clone()),
            })
            .collect::<Vec<_>>();
        let recipient_before = recipient_accounts
            .iter()
            .map(|account| {
                icrc::icrc1_balance_of(
                    &fixture.pic,
                    sns.governance.ledger,
                    icrc::account(
                        account.owner,
                        account
                            .subaccount
                            .clone()
                            .map(|bytes| bytes.try_into().unwrap()),
                    ),
                )
            })
            .collect::<Vec<_>>();
        let reward_reserve_before = super::query::<Nat>(
            &fixture.pic,
            sns.governance.ledger,
            Principal::anonymous(),
            "icrc1_balance_of",
            sns.reserve.clone(),
        );
        let reward_supply_before = icrc::icrc1_total_supply(&fixture.pic, sns.governance.ledger);
        let mut maturity_phases = Vec::new();
        loop {
            let progress: Result<ManagerNnsProgress, ManagerApiError> = super::update(
                &fixture.pic,
                fixture.controller,
                Principal::anonymous(),
                "resume",
                (),
            );
            maturity_phases.push(format!("{progress:?}"));
            fixture
                .pic
                .upgrade_canister(
                    fixture.controller,
                    manager_wasm.clone(),
                    encode_one(()).unwrap(),
                    None,
                )
                .unwrap();
            if progress
                == Ok(ManagerNnsProgress::Maturity(
                    ManagerMaturityProgress::Pending,
                ))
            {
                break;
            }
            assert!(maturity_phases.len() < 12, "{maturity_phases:?}");
        }
        let pending = super::neuron(
            &fixture.pic,
            fixture.governance,
            fixture.controller,
            fixture.neuron_id,
        );
        let finalization = pending.maturity_disbursements_in_progress.unwrap()[0]
            .finalize_disbursement_timestamp_seconds
            .unwrap();
        let now = fixture.pic.get_time().as_nanos_since_unix_epoch() / 1_000_000_000;
        fixture
            .pic
            .advance_time(Duration::from_secs(finalization - now + 1));
        let semantic_staging = io_accounts::two_week_maturity_staging(fixture.controller);
        let staging_subaccount: [u8; 32] = semantic_staging
            .subaccount
            .clone()
            .unwrap()
            .try_into()
            .unwrap();
        let staging = IcpAccount::new(fixture.controller, Some(Subaccount(staging_subaccount)))
            .icp_account_identifier_bytes();
        let mut mint = None;
        for day in 0..7 {
            for _ in 0..100 {
                fixture.pic.tick();
            }
            mint = super::maybe_find_mint(&fixture, &staging);
            if mint.is_some() {
                break;
            }
            if day < 6 {
                fixture.pic.advance_time(Duration::from_secs(86_400));
            }
        }
        let (_mint_block, actual_minted_e8s) = mint.unwrap_or_else(|| {
            let settled = super::neuron(
                &fixture.pic,
                fixture.governance,
                fixture.controller,
                fixture.neuron_id,
            );
            let blocks: QueryBlocksResponse = super::query(
                &fixture.pic,
                fixture.ledger,
                Principal::anonymous(),
                "query_blocks",
                GetBlocksArgs {
                    start: 0,
                    length: 100,
                },
            );
            let mints = blocks
                .blocks
                .iter()
                .filter_map(|block| match &block.transaction.operation {
                    Some(IcpOperation::Mint { to, amount }) => Some((hex::encode(to), amount.e8s)),
                    _ => None,
                })
                .collect::<Vec<_>>();
            panic!("two-week maturity Mint did not settle: neuron={settled:?} mints={mints:?}")
        });
        let staging_donation_e8s = 2_000_000_u64;
        let staging_donation: Result<u64, IcpTransferError> = icrc::update_one(
            &fixture.pic,
            fixture.ledger,
            Principal::anonymous(),
            "transfer",
            IcpTransferArgs {
                memo: 42,
                amount: IcpTokens {
                    e8s: staging_donation_e8s,
                },
                fee: IcpTokens { e8s: ICP_FEE_E8S },
                from_subaccount: None,
                to: staging.to_vec(),
                created_at_time: None,
            },
        );
        staging_donation.unwrap();

        let permanent_before_maturity = super::neuron(
            &fixture.pic,
            fixture.governance,
            fixture.controller,
            fixture.two_year_neuron_id,
        );
        let permanent_subaccount = super::neuron_subaccount(fixture.controller, TWO_YEAR_MEMO);
        let permanent_staking =
            IcpAccount::new(fixture.governance, Some(Subaccount(permanent_subaccount)))
                .icp_account_identifier_bytes();
        let maturity_donation = 2_000_000_u64;
        let mut maturity_donation_sent = false;
        let completed = loop {
            let progress: Result<ManagerNnsProgress, ManagerApiError> = super::update(
                &fixture.pic,
                fixture.controller,
                Principal::anonymous(),
                "resume",
                (),
            );
            maturity_phases.push(format!("{progress:?}"));
            if !maturity_donation_sent {
                let permanent = super::neuron(
                    &fixture.pic,
                    fixture.governance,
                    fixture.controller,
                    fixture.two_year_neuron_id,
                );
                let ledger_stake = super::query::<Nat>(
                    &fixture.pic,
                    fixture.ledger,
                    Principal::anonymous(),
                    "icrc1_balance_of",
                    ManagerAccount {
                        owner: fixture.governance,
                        subaccount: Some(permanent_subaccount.to_vec()),
                    },
                );
                if ledger_stake > permanent.cached_neuron_stake_e8s {
                    let donation: Result<u64, IcpTransferError> = icrc::update_one(
                        &fixture.pic,
                        fixture.ledger,
                        Principal::anonymous(),
                        "transfer",
                        IcpTransferArgs {
                            memo: 43,
                            amount: IcpTokens {
                                e8s: maturity_donation,
                            },
                            fee: IcpTokens { e8s: ICP_FEE_E8S },
                            from_subaccount: None,
                            to: permanent_staking.to_vec(),
                            created_at_time: None,
                        },
                    );
                    donation.unwrap();
                    maturity_donation_sent = true;
                }
            }
            if let Ok(ManagerNnsProgress::Maturity(ManagerMaturityProgress::Completed(completed))) =
                progress
            {
                break completed;
            }
            assert!(maturity_phases.len() < 40, "{maturity_phases:?}");
        };
        let observed_permanent_donation_e8s = if maturity_donation_sent {
            maturity_donation
        } else {
            0
        };
        let staging_after_completion = u128::try_from(
            super::query::<Nat>(
                &fixture.pic,
                fixture.ledger,
                Principal::anonymous(),
                "icrc1_balance_of",
                ManagerAccount {
                    owner: fixture.controller,
                    subaccount: semantic_staging.subaccount.clone(),
                },
            )
            .0,
        )
        .unwrap();
        assert_eq!(
            completed.captured_e8s + staging_after_completion,
            u128::from(actual_minted_e8s) + u128::from(staging_donation_e8s),
            "value arriving after the semantic capture must remain for the next operation"
        );
        let split = io_nns_types::maturity::capture_40_60(
            completed.captured_e8s,
            u128::from(ICP_FEE_E8S),
            u128::from(ICP_FEE_E8S),
        )
        .unwrap();
        assert_eq!(completed.kind, ManagerMaturityKind::TwoWeek);
        assert_eq!(completed.permanent_credit_e8s, split.permanent_credit);
        assert_eq!(completed.claim_credit_e8s, split.claim_credit);
        assert!(completed.entitlement_batch_generation.is_some());
        let permanent_after_maturity = super::neuron(
            &fixture.pic,
            fixture.governance,
            fixture.controller,
            fixture.two_year_neuron_id,
        );
        assert_eq!(
            u128::from(permanent_after_maturity.cached_neuron_stake_e8s),
            u128::from(permanent_before_maturity.cached_neuron_stake_e8s)
                + completed.permanent_credit_e8s
                + u128::from(observed_permanent_donation_e8s)
        );
        let recipient_after = recipient_accounts
            .iter()
            .map(|account| {
                super::query::<Nat>(
                    &fixture.pic,
                    sns.governance.ledger,
                    Principal::anonymous(),
                    "icrc1_balance_of",
                    account.clone(),
                )
            })
            .collect::<Vec<_>>();
        assert!(recipient_after
            .iter()
            .zip(&recipient_before)
            .all(|(after, before)| after > before));
        assert_eq!(frozen_credits.len(), recipient_after.len());
        let claim_credit = completed.claim_credit_e8s;
        let pre_inflow = accumulated
            .latest_reconciliation_checkpoint
            .as_ref()
            .unwrap();
        let backed_io_pool = io_core_model::backed_io(
            claim_credit,
            pre_inflow.total_claim_backing_e8s,
            pre_inflow.claim_supply_e8s,
        )
        .unwrap();
        let allocation = io_reward_policy::allocate_rewards(
            backed_io_pool,
            accumulated.accumulated_policy_credit,
            &frozen_credits
                .iter()
                .map(|credit| {
                    io_reward_policy::entitlement_credit_from_bytes(
                        credit.sns_neuron_id.clone(),
                        credit.accumulated_eligible_credit,
                    )
                })
                .collect::<Vec<_>>(),
        )
        .unwrap();
        for ((neuron, before), after) in sns
            .neurons
            .iter()
            .zip(&recipient_before)
            .zip(&recipient_after)
        {
            let expected = allocation
                .allocations
                .iter()
                .find(|value| value.sns_neuron_id == neuron.id)
                .unwrap()
                .io_e8s;
            assert_eq!(
                u128::try_from(after.0.clone()).unwrap()
                    - u128::try_from(before.0.clone()).unwrap(),
                expected
            );
        }
        let distributed = allocation
            .allocations
            .iter()
            .map(|value| value.io_e8s)
            .sum::<u128>();
        let reward_reserve_after = super::query::<Nat>(
            &fixture.pic,
            sns.governance.ledger,
            Principal::anonymous(),
            "icrc1_balance_of",
            sns.reserve.clone(),
        );
        assert_eq!(
            reward_reserve_before.0 - reward_reserve_after.0.clone(),
            (distributed + 3 * u128::from(ICP_FEE_E8S)).into()
        );
        assert_eq!(
            reward_supply_before.0
                - icrc::icrc1_total_supply(&fixture.pic, sns.governance.ledger).0,
            (3 * u128::from(ICP_FEE_E8S)).into()
        );

        if !jupiter_before_maturity {
            let permanent_before_second_jupiter = super::neuron(
                &fixture.pic,
                fixture.governance,
                fixture.controller,
                fixture.two_year_neuron_id,
            );
            let second_jupiter = super::run_jupiter_credit(
                &fixture,
                sns.stream,
                sns.governance.governance,
                jupiter,
                12 * 100_000_000,
                50,
                0,
            );
            assert_eq!(
                second_jupiter.observed_after_cached_stake_e8s,
                u128::from(permanent_before_second_jupiter.cached_neuron_stake_e8s)
                    + second_jupiter.stake_e8s
            );
            let maturity_before_reward = super::neuron(
                &fixture.pic,
                fixture.governance,
                fixture.controller,
                fixture.two_year_neuron_id,
            )
            .maturity_e8s_equivalent;
            let maturity_after_reward =
                super::earn_maturity_for(&fixture, fixture.two_year_neuron_id);
            assert!(maturity_after_reward > maturity_before_reward);
        }

        if super::query::<StreamStatus>(
            &fixture.pic,
            sns.stream,
            Principal::anonymous(),
            "get_status",
            (),
        )
        .lifecycle
            == io_stream_manager::Lifecycle::Paused
        {
            let resumed: Result<(), StreamApiError> = super::update(
                &fixture.pic,
                sns.stream,
                sns.governance.governance,
                "set_paused",
                false,
            );
            assert_eq!(resumed, Ok(()));
        }
        let redemption_amount = 20_000_000_u64;
        let now = fixture.pic.get_time().as_nanos_since_unix_epoch();
        let supply_before = u128::try_from(
            icrc::icrc1_total_supply(&fixture.pic, sns.governance.ledger)
                .0
                .clone(),
        )
        .unwrap();
        let reserve_before = u128::try_from(
            super::query::<Nat>(
                &fixture.pic,
                sns.governance.ledger,
                Principal::anonymous(),
                "icrc1_balance_of",
                sns.reserve.clone(),
            )
            .0,
        )
        .unwrap();
        let liquid_before = u128::try_from(
            super::query::<Nat>(
                &fixture.pic,
                fixture.ledger,
                Principal::anonymous(),
                "icrc1_balance_of",
                sns.liquid.clone(),
            )
            .0,
        )
        .unwrap();
        let redemption_backing: Result<
            io_nns_types::backing::ClaimAssetObservation,
            ManagerApiError,
        > = super::update(
            &fixture.pic,
            fixture.controller,
            sns.stream,
            "observe_claim_assets",
            (),
        );
        let redemption_backing = redemption_backing.unwrap();
        let quote = io_core_model::redemption_quote(
            io_core_model::EconomicState {
                backing: io_core_model::Backing {
                    liquid: liquid_before,
                    pooled: redemption_backing.claim_bearing_dynamic_principal_e8s,
                    unwinding: redemption_backing.live_child_net_backing_e8s,
                    transit: redemption_backing.transit_backing_e8s,
                },
                claims: supply_before - reserve_before,
                active_backing: 0,
                active_reward: 0,
            },
            redemption_amount.into(),
            ICP_FEE_E8S.into(),
            ICP_FEE_E8S.into(),
        )
        .unwrap();
        let icp_before = super::query::<Nat>(
            &fixture.pic,
            fixture.ledger,
            Principal::anonymous(),
            "icrc1_balance_of",
            ManagerAccount {
                owner: fixture.controller,
                subaccount: None,
            },
        );
        let redemption_args = RedeemArgs {
            from_subaccount: None,
            io_amount_e8s: redemption_amount.into(),
            min_icp_out_e8s: quote.net_icp,
            max_io_fee_e8s: ICP_FEE_E8S.into(),
            max_icp_fee_e8s: ICP_FEE_E8S.into(),
            expires_at_nanos: now + 800_000_000_000,
            nonce: 0,
        };
        let prepared: Result<PreparedRedemption, StreamApiError> = super::update(
            &fixture.pic,
            sns.stream,
            fixture.controller,
            "prepare_redemption",
            redemption_args.clone(),
        );
        let prepared = prepared.expect("combined redemption prepares an exact push");
        let push_block = icrc::icrc1_transfer(
            &fixture.pic,
            sns.governance.ledger,
            fixture.controller,
            icrc::transfer_arg(
                prepared
                    .account
                    .subaccount
                    .as_deref()
                    .map(|value| value.try_into().unwrap()),
                icrc::account(
                    prepared.reserve.owner,
                    prepared
                        .reserve
                        .subaccount
                        .as_deref()
                        .map(|value| value.try_into().unwrap()),
                ),
                prepared.request.io_amount_e8s.try_into().unwrap(),
                Some(prepared.snapshot.io_fee_e8s.try_into().unwrap()),
                Some(&prepared.push_memo),
                Some(prepared.prepared_at_nanos),
            ),
        )
        .expect("combined redemption sends its exact reserve push");
        let push_block = u128::try_from(push_block.0).unwrap();
        let initial: Result<RedemptionProgress, StreamApiError> = super::update(
            &fixture.pic,
            sns.stream,
            fixture.controller,
            "settle_redemption",
            push_block,
        );
        let mut redemption = match initial {
            Ok(RedemptionProgress::Completed(completed)) => {
                let status: StreamStatus = super::query(
                    &fixture.pic,
                    sns.stream,
                    Principal::anonymous(),
                    "get_status",
                    (),
                );
                assert!(status.operation_kind.is_none());
                Some(completed)
            }
            Ok(RedemptionProgress::Pending) => {
                let status: StreamStatus = super::query(
                    &fixture.pic,
                    sns.stream,
                    Principal::anonymous(),
                    "get_status",
                    (),
                );
                assert!(
                    matches!(
                        status.operation_kind.as_deref(),
                        Some("Redemption") | Some("BackingReconciliation")
                    ),
                    "a proved push may wait behind the one active reconciliation slot: {status:?}"
                );
                assert!(status.operation_phase.is_some());
                None
            }
            other => panic!("combined redemption failed initially: {other:?}"),
        };
        fixture
            .pic
            .upgrade_canister(sns.stream, stream_wasm, encode_one(()).unwrap(), None)
            .unwrap();
        let mut redemption_recovery_steps = Vec::new();
        if redemption.is_none() {
            for _ in 0..24 {
                let status: StreamStatus = super::query(
                    &fixture.pic,
                    sns.stream,
                    Principal::anonymous(),
                    "get_status",
                    (),
                );
                match status.operation_kind.as_deref() {
                    Some("BackingReconciliation") => {
                        let manager_progress: Result<ManagerNnsProgress, ManagerApiError> =
                            super::update(
                                &fixture.pic,
                                fixture.controller,
                                Principal::anonymous(),
                                "resume",
                                (),
                            );
                        let progress: Result<StreamProgress, StreamApiError> = super::update(
                            &fixture.pic,
                            sns.stream,
                            Principal::anonymous(),
                            "resume",
                            (),
                        );
                        redemption_recovery_steps.push(format!(
                            "backing manager={manager_progress:?} stream={progress:?}"
                        ));
                        assert!(
                            matches!(
                                manager_progress,
                                Ok(ManagerNnsProgress::Pool(_))
                                    | Ok(ManagerNnsProgress::Idle)
                                    | Err(ManagerApiError::Pending(_))
                                    | Err(ManagerApiError::Busy)
                            ),
                            "combined NNS backing recovery failed: {manager_progress:?}"
                        );
                        assert!(
                            matches!(
                                progress,
                                Ok(StreamProgress::BackingReconciliation)
                                    | Err(StreamApiError::Pending(_))
                            ),
                            "combined backing contention failed: {progress:?}"
                        );
                    }
                    Some("Redemption") => {
                        let progress: Result<StreamProgress, StreamApiError> = super::update(
                            &fixture.pic,
                            sns.stream,
                            Principal::anonymous(),
                            "resume",
                            (),
                        );
                        redemption_recovery_steps.push(format!("redemption={progress:?}"));
                        match progress {
                            Ok(StreamProgress::Redemption(RedemptionProgress::Pending))
                            | Err(StreamApiError::Pending(_)) => {}
                            Ok(StreamProgress::Redemption(RedemptionProgress::Completed(
                                completed,
                            ))) => {
                                redemption = Some(completed);
                                break;
                            }
                            other => panic!("combined pending redemption failed: {other:?}"),
                        }
                    }
                    None => {
                        let progress: Result<RedemptionProgress, StreamApiError> = super::update(
                            &fixture.pic,
                            sns.stream,
                            Principal::anonymous(),
                            "resume_redemption",
                            fixture.controller,
                        );
                        redemption_recovery_steps.push(format!("activate={progress:?}"));
                        match progress {
                            Ok(RedemptionProgress::Pending) | Err(StreamApiError::Pending(_)) => {}
                            Ok(RedemptionProgress::Completed(completed)) => {
                                redemption = Some(completed);
                                break;
                            }
                            other => panic!("combined pushed redemption failed: {other:?}"),
                        }
                    }
                    other => panic!("unrelated operation blocked redemption: {other:?}"),
                }
            }
        }
        let redemption = redemption.unwrap_or_else(|| {
            panic!(
                "combined redemption exceeded bounded recovery attempts: {redemption_recovery_steps:?}"
            )
        });
        assert_eq!(redemption.gross_icp_e8s, quote.gross_icp);
        assert_eq!(redemption.net_icp_e8s, quote.net_icp);
        let replay: Result<RedemptionProgress, StreamApiError> = super::update(
            &fixture.pic,
            sns.stream,
            fixture.controller,
            "settle_redemption",
            push_block,
        );
        assert_eq!(
            replay,
            Ok(RedemptionProgress::Completed(redemption.clone()))
        );
        let idle: Result<StreamProgress, StreamApiError> = super::update(
            &fixture.pic,
            sns.stream,
            Principal::anonymous(),
            "resume",
            (),
        );
        assert_eq!(idle, Ok(StreamProgress::Idle));
        let icp_after = super::query::<Nat>(
            &fixture.pic,
            fixture.ledger,
            Principal::anonymous(),
            "icrc1_balance_of",
            ManagerAccount {
                owner: fixture.controller,
                subaccount: None,
            },
        );
        assert_eq!(icp_after.0 - icp_before.0, quote.net_icp.into());
        assert_eq!(
            icrc::icrc1_total_supply(&fixture.pic, sns.governance.ledger),
            Nat::from(supply_before - u128::from(ICP_FEE_E8S))
        );
        assert_eq!(
            super::query::<Nat>(
                &fixture.pic,
                sns.governance.ledger,
                Principal::anonymous(),
                "icrc1_balance_of",
                sns.reserve.clone(),
            ),
            Nat::from(reserve_before + u128::from(redemption_amount))
        );
        let final_manager: ManagerStatus = super::query(
            &fixture.pic,
            fixture.controller,
            Principal::anonymous(),
            "get_status",
            (),
        );
        let final_stream: StreamStatus = super::query(
            &fixture.pic,
            sns.stream,
            Principal::anonymous(),
            "get_status",
            (),
        );
        assert_eq!(
            final_manager.lifecycle,
            if jupiter_before_maturity {
                ManagerLifecycle::Paused
            } else {
                ManagerLifecycle::Ready
            }
        );
        assert_eq!(final_stream.lifecycle, io_stream_manager::Lifecycle::Paused);

        if final_manager.lifecycle == ManagerLifecycle::Paused {
            super::update::<Result<(), ManagerApiError>>(
                &fixture.pic,
                fixture.controller,
                sns.governance.governance,
                "set_paused",
                false,
            )
            .unwrap();
        }
        let before_top_up: io_nns_types::backing::ClaimAssetObservation =
            super::update::<Result<_, ManagerApiError>>(
                &fixture.pic,
                fixture.controller,
                sns.stream,
                "observe_claim_assets",
                (),
            )
            .unwrap();
        let top_up_generation = final_stream
            .latest_reconciliation_checkpoint
            .as_ref()
            .unwrap()
            .generation
            .checked_add(1)
            .unwrap();
        let top_up_credit = 100_000_000_u128;
        let top_up_created_at = fixture.pic.get_time().as_nanos_since_unix_epoch();
        let top_up_request = io_nns_types::backing::PreparePoolReconciliationArgs {
            generation: top_up_generation,
            target_e8s: before_top_up.claim_bearing_dynamic_principal_e8s + top_up_credit,
            action: io_nns_types::backing::PoolReconciliationAction::TopUp {
                expected_transfer_e8s: top_up_credit - u128::from(ICP_FEE_E8S),
                expected_claim_credit_e8s: top_up_credit,
            },
            fee_e8s: ICP_FEE_E8S.into(),
            snapshot_fingerprint: before_top_up.fingerprint,
            memo: b"io-top-up-donation".to_vec(),
            created_at_time_nanos: top_up_created_at,
        };
        let prepared: io_nns_types::backing::PoolProgress =
            super::update::<Result<_, ManagerApiError>>(
                &fixture.pic,
                fixture.controller,
                sns.stream,
                "prepare_pool_reconciliation",
                top_up_request,
            )
            .unwrap();
        let io_nns_types::backing::PoolProgress::AwaitingTransfer(permit) = prepared else {
            panic!("top-up did not return its exact permit: {prepared:?}");
        };
        let top_up_donation = 1_000_000_u128;
        let donation_to_parent: Result<u64, IcpTransferError> = icrc::update_one(
            &fixture.pic,
            fixture.ledger,
            Principal::anonymous(),
            "transfer",
            IcpTransferArgs {
                memo: 33,
                amount: IcpTokens {
                    e8s: u64::try_from(top_up_donation).unwrap(),
                },
                fee: IcpTokens { e8s: ICP_FEE_E8S },
                from_subaccount: None,
                to: IcpAccount::new(
                    permit.destination.owner,
                    permit
                        .destination
                        .subaccount
                        .clone()
                        .map(|bytes| Subaccount(bytes.try_into().unwrap())),
                )
                .icp_account_identifier_bytes()
                .to_vec(),
                created_at_time: None,
            },
        );
        let donation_block = u128::from(donation_to_parent.unwrap());
        let rejected_donation: Result<ManagerNnsProgress, ManagerApiError> = super::update(
            &fixture.pic,
            fixture.controller,
            Principal::anonymous(),
            "prove_active_transfer",
            donation_block,
        );
        assert!(
            matches!(rejected_donation, Err(ManagerApiError::Invalid(_))),
            "unattributed dust must not impersonate the exact top-up: {rejected_donation:?}"
        );
        let transfer: Result<candid::Nat, io_ledger_types::IcrcTransferError> = super::update(
            &fixture.pic,
            fixture.ledger,
            sns.stream,
            "icrc1_transfer",
            io_ledger_types::IcrcTransferArg {
                from_subaccount: Some(vec![3; 32]),
                to: io_ledger_types::IcrcAccount {
                    owner: permit.destination.owner,
                    subaccount: permit.destination.subaccount.clone(),
                },
                amount: candid::Nat::from(permit.expected_credit_e8s),
                fee: Some(candid::Nat::from(ICP_FEE_E8S)),
                memo: Some(permit.memo.clone()),
                created_at_time: Some(permit.prepared_at_nanos),
            },
        );
        let top_up_block: u128 = transfer.unwrap().0.try_into().unwrap();
        let proved: Result<ManagerNnsProgress, ManagerApiError> = super::update(
            &fixture.pic,
            fixture.controller,
            Principal::anonymous(),
            "prove_active_transfer",
            top_up_block,
        );
        let completion = |progress: &Result<ManagerNnsProgress, ManagerApiError>| match progress {
            Ok(ManagerNnsProgress::Pool(io_nns_types::backing::PoolProgress::Completed {
                principal_e8s,
                target_status,
                ..
            })) => Some((*principal_e8s, *target_status)),
            _ => None,
        };
        let mut top_up_phases = vec![format!("{proved:?}")];
        let mut completed_top_up = completion(&proved);
        for _ in 0..8 {
            if completed_top_up.is_some() {
                break;
            }
            let progress: Result<ManagerNnsProgress, ManagerApiError> = super::update(
                &fixture.pic,
                fixture.controller,
                Principal::anonymous(),
                "resume",
                (),
            );
            top_up_phases.push(format!("{progress:?}"));
            completed_top_up = completion(&progress);
        }
        assert_eq!(
            completed_top_up,
            Some((
                before_top_up
                    .parent
                    .as_ref()
                    .unwrap()
                    .physical_principal_e8s
                    + top_up_credit
                    - u128::from(ICP_FEE_E8S)
                    + top_up_donation,
                io_nns_types::backing::PoolTargetResult::AtTarget,
            )),
            "{top_up_phases:?}"
        );
        let after_top_up: io_nns_types::backing::ClaimAssetObservation =
            super::update::<Result<_, ManagerApiError>>(
                &fixture.pic,
                fixture.controller,
                sns.stream,
                "observe_claim_assets",
                (),
            )
            .unwrap();
        assert_eq!(
            after_top_up.claim_bearing_dynamic_principal_e8s,
            before_top_up.claim_bearing_dynamic_principal_e8s + top_up_credit
        );
        assert_eq!(
            after_top_up.anchor_available_e8s,
            before_top_up.anchor_available_e8s - u128::from(ICP_FEE_E8S)
        );
        assert_eq!(
            after_top_up.excluded_dynamic_surplus_e8s,
            before_top_up.excluded_dynamic_surplus_e8s + top_up_donation
        );
        assert_eq!(
            after_top_up.parent.as_ref().unwrap().physical_principal_e8s,
            after_top_up.claim_bearing_dynamic_principal_e8s
                + after_top_up.anchor_available_e8s
                + after_top_up.excluded_dynamic_surplus_e8s
        );
        super::update::<Result<(), ManagerApiError>>(
            &fixture.pic,
            fixture.controller,
            sns.governance.governance,
            "set_paused",
            true,
        )
        .unwrap();
        eprintln!(
            "combined_real_summary event_round={} ordinary_maturity={} actual_mint={} reward_recipients={} redemption={redemption:?} phases={maturity_phases:?}",
            event.round,
            ordinary_maturity,
            actual_minted_e8s,
            recipient_after.len(),
        );
        eprintln!(
            "account_semantic_combined jupiter_before_maturity={} two_week_captured_e8s={} permanent_credit_e8s={} claim_credit_e8s={} actual_nns_maturity_e8s={} staging_donation_e8s={} permanent_donation_e8s={} recipient_count={} redemption_gross_e8s={} redemption_net_e8s={} pooled_before_top_up_e8s={} top_up_credit_e8s={} top_up_donation_e8s={}",
            jupiter_before_maturity,
            completed.captured_e8s,
            completed.permanent_credit_e8s,
            completed.claim_credit_e8s,
            actual_minted_e8s,
            staging_donation_e8s,
            observed_permanent_donation_e8s,
            recipient_after.len(),
            redemption.gross_icp_e8s,
            redemption.net_icp_e8s,
            before_top_up.claim_bearing_dynamic_principal_e8s,
            top_up_credit,
            top_up_donation,
        );
    }

    #[test]
    #[ignore = "requires candidate SNS Governance/Root/ledger, pinned real NNS Governance/ICP ledger, current IO release Wasms, and POCKET_IC_BIN"]
    fn combined_real_sns_nns_io_lifecycle_reconciles_maturity_and_redemption() {
        run_combined_real_sns_nns_io_lifecycle(false, false);
    }

    #[test]
    #[ignore = "requires candidate SNS Governance/Root/ledger, pinned real NNS Governance/ICP ledger, current-source debug IO Wasms, and POCKET_IC_BIN"]
    fn current_source_jupiter_donation_interleaving_is_unattributed() {
        run_combined_real_sns_nns_io_lifecycle(true, true);
    }
}
