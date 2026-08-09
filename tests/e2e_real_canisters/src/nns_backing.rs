#![cfg(test)]

use candid::{decode_one, encode_args, encode_one, CandidType, Principal, Reserved};
use io_governance_types::{
    nns_refresh_voting_power_request, EmptyRecord, NnsAccount, NnsChangeAutoStakeMaturity,
    NnsClaimOrRefresh, NnsClaimOrRefreshBy, NnsClaimOrRefreshNeuronFromAccount,
    NnsDissolveStateRecord, NnsIncreaseDissolveDelay, NnsManageNeuronCommandRequest,
    NnsManageNeuronResponseCommandRecord, NnsNeuronIdOrSubaccount, NnsNeuronIdRecord,
    NnsProductionConfigure, NnsProductionConfigureOperation, NnsProductionDisburseMaturity,
    NnsProductionListNeuronsRequest, NnsProductionListNeuronsResponse,
    NnsProductionManageNeuronRequest, NnsProductionManageNeuronResponse, NnsRegisterVote,
    NnsStakeMaturity,
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
const EIGHT_YEARS_SECONDS: u32 = 252_460_800;
const ONE_YEAR_SECONDS: u32 = 365 * 24 * 60 * 60;
const NNS_MINIMUM_DISSOLVE_DELAY_TO_VOTE_SECONDS: u64 = 6 * 30 * 24 * 60 * 60;
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
struct LatestRewardEventFixture {
    round: u64,
    rounds_since_last_distribution: u64,
    end_timestamp_seconds: u64,
    settled_proposal_ids: Vec<u64>,
    neuron_reward_shares: Vec<(u64, io_governance_types::SnsUint128)>,
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
    two_week_maturity_baseline_reconciled: bool,
    latest_started_two_week_generation: u64,
    latest_completed_two_week_generation: u64,
    unwinding_child_principal_e8s: u128,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
enum ManagerMaturityKind {
    TwoYear,
    TwoWeek,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
struct ManagerCompletedMaturity {
    kind: ManagerMaturityKind,
    neuron_id: u64,
    mint_block: u128,
    nominal_disbursed_maturity_e8s: u64,
    actual_minted_icp_e8s: u128,
    destination: ManagerAccount,
    completed_at_nanos: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
enum ManagerMaturityProgress {
    Observed,
    StakeMaturitySubmitted,
    StakeMaturitySucceeded,
    DisburseMaturitySubmitted,
    DisburseMaturitySucceeded,
    AwaitingMintProof,
    MintProved,
    DeliveringTwoWeekReceipt,
    Completed(ManagerCompletedMaturity),
    Stuck(String),
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
enum ManagerNnsProgress {
    Jupiter(ManagerJupiterProgress),
    Maturity(ManagerMaturityProgress),
    Unwind(ManagerUnwindProgress),
    Idle,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
enum ManagerUnwindProgress {
    Waiting,
    AwaitingTransferProof,
    Completed { block_index: u128, liquid_e8s: u128 },
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
    two_week_neuron_id: u64,
    jupiter_account: ManagerAccount,
    jupiter_staging: ManagerAccount,
    two_week_maturity_staging: ManagerAccount,
    stream_liquid_account: ManagerAccount,
    expected_io_fee_e8s: u128,
    expected_icp_fee_e8s: u128,
    jupiter_fee_float_e8s: u128,
    two_week_fee_float_e8s: u128,
    seeded_two_week_principal_e8s: u128,
    transfer_retry_delay_nanos: u64,
    ledger_deduplication_window_nanos: u64,
}

#[derive(Clone, Debug, CandidType)]
struct ManagerInitArgs {
    config: ManagerConfig,
}

#[derive(Clone, Debug, CandidType)]
struct ReconcileTwoWeekBackingReadinessArgs {
    target_e8s: u128,
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
    liquid_e8s: u128,
    stake_transfer_block: u128,
    liquid_transfer_block: u128,
    stream_receipt_sequence: u64,
    backed_io_e8s: u128,
    io_transfer_block: u128,
    io_fee_e8s: u128,
    stream_receipt_fingerprint: Vec<u8>,
    completed_at_nanos: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
enum ManagerJupiterProgress {
    DepositProved,
    StakeTransferPrepared,
    StakeTransferSubmitted,
    StakeTransferSucceeded,
    RefreshSubmitted,
    StakeIncreaseProved,
    ReceiptPermitPrepared,
    LiquidTransferPrepared,
    LiquidTransferSubmitted,
    LiquidTransferSucceeded,
    ReceiptCompletionSubmitted,
    AwaitingStreamSettlement,
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
}

pub fn create_zero_maturity_protected_neuron() -> ControlledNnsNeuron {
    create_zero_maturity_protected_neuron_with_stake(PROTECTED_STAKE_E8S)
}

fn create_zero_maturity_protected_neuron_with_stake(
    protected_principal_e8s: u64,
) -> ControlledNnsNeuron {
    let pic = Rc::new(nns_setup::controlled_pinned_nns(true).unwrap());
    let governance = Principal::from_text(nns_setup::install_nns_governance().canister_id).unwrap();
    let ledger = Principal::from_text(nns_setup::install_nns_ledger().canister_id).unwrap();
    let controller = pocketic_env::create_empty_application_canister(&pic);
    let neuron_id = stake_neuron(
        &pic,
        governance,
        ledger,
        controller,
        PROTECTED_MEMO,
        protected_principal_e8s,
        EIGHT_YEARS_SECONDS,
    );
    let proposer_neuron_id = stake_neuron(
        &pic,
        governance,
        ledger,
        Principal::anonymous(),
        PROPOSER_MEMO,
        1_000 * 100_000_000,
        ONE_YEAR_SECONDS,
    );
    let two_year_neuron_id = stake_neuron(
        &pic,
        governance,
        ledger,
        controller,
        TWO_YEAR_MEMO,
        100 * 100_000_000,
        EIGHT_YEARS_SECONDS,
    );
    let neuron = neuron(&pic, governance, controller, neuron_id);
    assert_eq!(neuron.cached_neuron_stake_e8s, protected_principal_e8s);
    assert_eq!(neuron.maturity_e8s_equivalent, 0);
    assert!(!neuron.auto_stake_maturity.unwrap_or(false));
    assert_eq!(
        neuron.dissolve_state,
        Some(NnsDissolveStateRecord::DissolveDelaySeconds(
            EIGHT_YEARS_SECONDS.into()
        ))
    );
    ControlledNnsNeuron {
        pic,
        governance,
        ledger,
        controller,
        neuron_id,
        two_year_neuron_id,
        proposer_neuron_id,
        protected_principal_e8s,
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
                            additional_dissolve_delay_seconds: dissolve_delay_seconds,
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
    earn_maturity_for(fixture, fixture.neuron_id)
}

pub fn earn_maturity_for(fixture: &ControlledNnsNeuron, neuron_id: u64) -> u64 {
    // The production spike guard intentionally uses a prior voting-power
    // snapshot. Age that bootstrapped snapshot out, then let the pinned timer
    // record this controlled population before creating the proposal.
    fixture.pic.advance_time(Duration::from_secs(121 * 86_400));
    for _ in 0..100 {
        fixture.pic.tick();
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
    let (offset, amount) = blocks
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
        .expect("delayed maturity Mint must be present in the pinned ICP ledger");
    (blocks.first_block_index + offset, amount)
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
}

fn manager_wasm() -> Vec<u8> {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/wasm32-unknown-unknown/debug/io_nns_neuron_manager.wasm"),
    )
    .expect("build the debug NNS manager Wasm before running controlled evidence")
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
                two_week_neuron_id: fixture.neuron_id,
                jupiter_account: ManagerAccount {
                    owner: jupiter,
                    subaccount: None,
                },
                jupiter_staging: ManagerAccount {
                    owner: fixture.controller,
                    subaccount: None,
                },
                two_week_maturity_staging: account(fixture.controller, 2),
                stream_liquid_account: account(stream, 3),
                expected_io_fee_e8s: 10_000,
                expected_icp_fee_e8s: ICP_FEE_E8S.into(),
                jupiter_fee_float_e8s: 20_000,
                two_week_fee_float_e8s: 20_000,
                seeded_two_week_principal_e8s: fixture.protected_principal_e8s.into(),
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

fn production_wasm(name: &str) -> Vec<u8> {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/wasm32-unknown-unknown/release")
            .join(format!("{name}.wasm")),
    )
    .unwrap_or_else(|error| panic!("build {name} release Wasm before controlled evidence: {error}"))
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
    neuron_destination: io_stream_manager::Account,
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
    let neuron_destination = Account {
        owner: governance,
        subaccount: Some(sns_neuron_subaccount(1)),
    };
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
                dissolve_delay_seconds: io_core_model::TWO_WEEK_SECONDS,
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
                jupiter_receipt_source: Account {
                    owner: fixture.controller,
                    subaccount: None,
                },
                two_week_receipt_source: Account {
                    owner: fixture.controller,
                    subaccount: Some(vec![2; 32]),
                },
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
                excluded_io_accounts: vec![],
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
        neuron_destination,
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
        u32::try_from(io_core_model::TWO_WEEK_SECONDS).unwrap(),
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
                u32::try_from(io_core_model::TWO_WEEK_SECONDS).unwrap(),
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
    let wasm = production_wasm("io_stream_manager");
    fixture.pic.install_canister(
        sns.stream,
        wasm.clone(),
        encode_one(InitArgs {
            config: StreamConfig {
                io_ledger: sns.governance.ledger,
                icp_ledger: fixture.ledger,
                nns_manager: fixture.controller,
                jupiter_receipt_source: ManagerAccount {
                    owner: fixture.controller,
                    subaccount: None,
                }
                .into(),
                two_week_receipt_source: ManagerAccount {
                    owner: fixture.controller,
                    subaccount: Some(vec![2; 32]),
                }
                .into(),
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
                excluded_io_accounts: vec![],
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
    fn protected_nns_policy_contradiction_is_recorded_before_correction() {
        assert_eq!(u64::from(EIGHT_YEARS_SECONDS), 252_460_800);
        assert_eq!(NNS_MINIMUM_DISSOLVE_DELAY_TO_VOTE_SECONDS, 15_552_000);
        assert_eq!(io_core_model::TWO_WEEK_SECONDS, 1_209_600);

        let manager = include_str!("../../../canisters/io_nns_neuron_manager/src/execution.rs");
        assert!(manager.contains("APPROVED_REWARD_BACKING_DISSOLVE_DELAY_SECONDS"));
        let harness = include_str!("nns_backing.rs");
        assert!(harness.contains("EIGHT_YEARS_SECONDS"));
    }

    #[test]
    fn remaining_real_vertical_gaps_are_recorded_before_correction() {
        let all = include_str!("nns_backing.rs");
        let harness = &all[..all.find("\nmod tests").unwrap()];
        assert!(!harness.contains("\"notify_jupiter_deposit\""));
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
    #[ignore = "requires pinned real NNS Governance/ICP ledger, candidate SNS ledger, production IO Wasms, and POCKET_IC_BIN"]
    fn controlled_jupiter_uses_real_nns_and_exact_production_receipts() {
        let _guard = crate::lock_test_env();
        let fixture = super::create_zero_maturity_protected_neuron();
        let stream = crate::pocketic_env::create_empty_application_canister(&fixture.pic);
        let manager_wasm = super::production_wasm("io_nns_neuron_manager");
        let controlled_stream = super::install_controlled_stream(
            &fixture,
            stream,
            super::production_wasm("io_stream_manager"),
        );
        super::fund_manager_staging(&fixture);
        let jupiter = super::install_manager(
            &fixture,
            stream,
            controlled_stream.governance,
            manager_wasm.clone(),
        );
        super::fund_stream_liquidity(&fixture, stream, 0);

        let stream_unpause: Result<(), io_stream_manager::ApiError> = super::update(
            &fixture.pic,
            stream,
            controlled_stream.governance,
            "set_paused",
            false,
        );
        assert_eq!(stream_unpause, Ok(()));
        let manager_unpause: Result<(), ManagerApiError> = super::update(
            &fixture.pic,
            fixture.controller,
            controlled_stream.governance,
            "set_paused",
            false,
        );
        assert_eq!(manager_unpause, Ok(()));

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
        assert_eq!(notified, Ok(ManagerJupiterProgress::DepositProved));
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
        assert_eq!(completed.stake_e8s, u128::from(gross_e8s * 40 / 100));
        assert_eq!(completed.liquid_e8s, u128::from(gross_e8s * 60 / 100));
        assert_ne!(
            completed.stake_transfer_block,
            completed.liquid_transfer_block
        );
        assert_eq!(completed.stream_receipt_sequence, 0);
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
    #[ignore = "requires pinned real NNS Governance/ICP ledger, candidate SNS ledger, production IO Wasms, and POCKET_IC_BIN"]
    fn controlled_two_year_compounds_real_maturity_without_io_issuance() {
        let _guard = crate::lock_test_env();
        let fixture = super::create_zero_maturity_protected_neuron();
        let real_sns_trigger = super::install_real_sns_maturity_trigger(&fixture);
        let stream = crate::pocketic_env::create_empty_application_canister(&fixture.pic);
        let manager_wasm = super::production_wasm("io_nns_neuron_manager");
        let controlled_stream = super::install_controlled_stream(
            &fixture,
            stream,
            super::production_wasm("io_stream_manager"),
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

        let mut prior_staked_maturity = 0;
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
            let ordinary_maturity = super::earn_maturity_for(&fixture, fixture.two_year_neuron_id);
            let expected_staked = ordinary_maturity.checked_mul(40).unwrap() / 100;
            let expected_disbursed = ordinary_maturity - expected_staked;
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
            let started: ManagerStatus = super::query(
                &fixture.pic,
                fixture.controller,
                Principal::anonymous(),
                "get_status",
                (),
            );
            assert_eq!(started.active_operation.as_deref(), Some("Maturity"));
            let replay: Result<ManagerMaturityProgress, ManagerApiError> = super::update(
                &fixture.pic,
                fixture.controller,
                real_sns_trigger.governance.governance,
                "start_maturity",
                ManagerMaturityKind::TwoYear,
            );
            assert_eq!(replay, Err(ManagerApiError::Busy));
            let replay_proposal_id = super::propose_real_two_year_start(
                &real_sns_trigger,
                &format!("Replay controlled two-year maturity cycle {cycle}"),
            );
            for _ in 0..20 {
                fixture.pic.tick();
            }
            assert_ne!(replay_proposal_id, proposal_id);
            let replayed: ManagerStatus = super::query(
                &fixture.pic,
                fixture.controller,
                Principal::anonymous(),
                "get_status",
                (),
            );
            assert_eq!(replayed.active_operation.as_deref(), Some("Maturity"));
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
                        ManagerMaturityProgress::AwaitingMintProof,
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
            let destination =
                IcpAccount::new(stream, Some(Subaccount([3; 32]))).icp_account_identifier_bytes();
            let (mint_block, actual_minted_e8s) = super::find_mint(&fixture, &destination);
            for invalid_block in [0_u128, u128::MAX] {
                let rejected: Result<ManagerMaturityProgress, ManagerApiError> = decode_one(
                    &fixture
                        .pic
                        .update_call(
                            fixture.controller,
                            Principal::anonymous(),
                            "prove_maturity_mint",
                            encode_args((ManagerMaturityKind::TwoYear, invalid_block)).unwrap(),
                        )
                        .unwrap(),
                )
                .unwrap();
                assert!(matches!(rejected, Err(ManagerApiError::Invalid(_))));
            }
            let proved: Result<ManagerMaturityProgress, ManagerApiError> = decode_one(
                &fixture
                    .pic
                    .update_call(
                        fixture.controller,
                        Principal::anonymous(),
                        "prove_maturity_mint",
                        encode_args((ManagerMaturityKind::TwoYear, u128::from(mint_block)))
                            .unwrap(),
                    )
                    .unwrap(),
            )
            .unwrap();
            let completed = match proved {
                Ok(ManagerMaturityProgress::Completed(completed)) => completed,
                other => panic!("cycle={cycle} unexpected two-year proof: {other:?}"),
            };
            assert_eq!(completed.neuron_id, fixture.two_year_neuron_id);
            assert_eq!(completed.nominal_disbursed_maturity_e8s, expected_disbursed);
            assert_eq!(
                completed.actual_minted_icp_e8s,
                u128::from(actual_minted_e8s)
            );
            assert!(completed.actual_minted_icp_e8s > 0);
            assert!(completed.actual_minted_icp_e8s <= u128::from(expected_disbursed));
            let replayed: Result<ManagerMaturityProgress, ManagerApiError> = decode_one(
                &fixture
                    .pic
                    .update_call(
                        fixture.controller,
                        Principal::anonymous(),
                        "prove_maturity_mint",
                        encode_args((ManagerMaturityKind::TwoYear, u128::from(mint_block)))
                            .unwrap(),
                    )
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(
                replayed,
                Ok(ManagerMaturityProgress::Completed(completed.clone()))
            );
            prior_staked_maturity = prior_staked_maturity.checked_add(expected_staked).unwrap();
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
            let liquid_after: candid::Nat = super::query(
                &fixture.pic,
                fixture.ledger,
                Principal::anonymous(),
                "icrc1_balance_of",
                liquid_account,
            );
            assert_eq!(
                liquid_after.0 - liquid_before.0,
                completed.actual_minted_icp_e8s.into()
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
            actual_mints.push(completed.actual_minted_icp_e8s);
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

        super::configure_neuron(
            &fixture.pic,
            fixture.governance,
            fixture.controller,
            fixture.two_year_neuron_id,
            NnsProductionConfigureOperation::ChangeAutoStakeMaturity(NnsChangeAutoStakeMaturity {
                requested_setting_for_auto_stake_maturity: true,
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
        super::configure_neuron(
            &fixture.pic,
            fixture.governance,
            fixture.controller,
            fixture.two_year_neuron_id,
            NnsProductionConfigureOperation::StartDissolving(EmptyRecord {}),
        );
        let unpause: Result<(), ManagerApiError> = super::update(
            &fixture.pic,
            fixture.controller,
            real_sns_trigger.governance.governance,
            "set_paused",
            false,
        );
        assert_eq!(unpause, Ok(()));
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

    #[test]
    #[ignore = "requires pinned real NNS Governance/ICP ledger, production NNS manager Wasm, and POCKET_IC_BIN"]
    fn controlled_target_rise_merge_back_survives_manager_upgrade() {
        let _guard = crate::lock_test_env();
        let fixture = super::create_zero_maturity_protected_neuron();
        let stream = pocketic_env::create_empty_application_canister(&fixture.pic);
        let governance = pocketic_env::create_empty_application_canister(&fixture.pic);
        let manager_wasm = super::production_wasm("io_nns_neuron_manager");
        super::fund_manager_staging(&fixture);
        super::install_manager(&fixture, stream, governance, manager_wasm.clone());
        let unpause: Result<(), ManagerApiError> = super::update(
            &fixture.pic,
            fixture.controller,
            governance,
            "set_paused",
            false,
        );
        assert_eq!(unpause, Ok(()));
        let lowered: Result<io_receipt_types::TwoWeekBackingReadiness, ManagerApiError> =
            super::update(
                &fixture.pic,
                fixture.controller,
                stream,
                "reconcile_two_week_backing_readiness",
                ReconcileTwoWeekBackingReadinessArgs {
                    target_e8s: RECONCILED_TARGET_E8S.into(),
                },
            );
        assert_eq!(
            lowered,
            Ok(io_receipt_types::TwoWeekBackingReadiness::NotReady(
                io_receipt_types::BackingNotReadyReason::OverTarget
            ))
        );
        for _ in 0..2 {
            let _: Result<ManagerNnsProgress, ManagerApiError> = super::update(
                &fixture.pic,
                fixture.controller,
                Principal::anonymous(),
                "resume",
                (),
            );
        }
        let passive: ManagerStatus = super::query(
            &fixture.pic,
            fixture.controller,
            Principal::anonymous(),
            "get_status",
            (),
        );
        assert!(passive.active_operation.is_none());
        assert!(passive.unwinding_child_principal_e8s > 0);

        let raised: Result<io_receipt_types::TwoWeekBackingReadiness, ManagerApiError> =
            super::update(
                &fixture.pic,
                fixture.controller,
                stream,
                "reconcile_two_week_backing_readiness",
                ReconcileTwoWeekBackingReadinessArgs {
                    target_e8s: PROTECTED_STAKE_E8S.into(),
                },
            );
        assert_eq!(
            raised,
            Ok(io_receipt_types::TwoWeekBackingReadiness::NotReady(
                io_receipt_types::BackingNotReadyReason::UnderTarget
            ))
        );
        let stopped: Result<ManagerNnsProgress, ManagerApiError> = super::update(
            &fixture.pic,
            fixture.controller,
            Principal::anonymous(),
            "resume",
            (),
        );
        assert_eq!(
            stopped,
            Ok(ManagerNnsProgress::Unwind(ManagerUnwindProgress::Waiting))
        );
        let merging: ManagerStatus = super::query(
            &fixture.pic,
            fixture.controller,
            Principal::anonymous(),
            "get_status",
            (),
        );
        assert_eq!(merging.active_operation.as_deref(), Some("Unwind"));
        assert_eq!(
            merging.unwinding_child_principal_e8s,
            passive.unwinding_child_principal_e8s
        );
        fixture
            .pic
            .upgrade_canister(
                fixture.controller,
                manager_wasm,
                encode_one(()).unwrap(),
                None,
            )
            .unwrap();
        let merged: Result<ManagerNnsProgress, ManagerApiError> = super::update(
            &fixture.pic,
            fixture.controller,
            Principal::anonymous(),
            "resume",
            (),
        );
        assert_eq!(
            merged,
            Ok(ManagerNnsProgress::Unwind(ManagerUnwindProgress::Waiting))
        );
        let final_status: ManagerStatus = super::query(
            &fixture.pic,
            fixture.controller,
            Principal::anonymous(),
            "get_status",
            (),
        );
        assert_eq!(final_status.lifecycle, ManagerLifecycle::Paused);
        assert!(final_status.active_operation.is_none());
        assert_eq!(final_status.unwinding_child_principal_e8s, 0);
        let parent = super::neuron(
            &fixture.pic,
            fixture.governance,
            fixture.controller,
            fixture.neuron_id,
        );
        assert_eq!(
            parent.cached_neuron_stake_e8s,
            RECONCILED_TARGET_E8S + u64::try_from(passive.unwinding_child_principal_e8s).unwrap()
                - ICP_FEE_E8S
        );
        eprintln!(
            "controlled_merge_back parent={} passive_child={}",
            parent.cached_neuron_stake_e8s, passive.unwinding_child_principal_e8s
        );
    }

    #[test]
    #[ignore = "requires candidate SNS Governance/Root/ledger, pinned real NNS Governance/ICP ledger, production IO Wasms, and POCKET_IC_BIN"]
    fn combined_real_sns_nns_io_lifecycle_reconciles_maturity_and_redemption() {
        use candid::Nat;
        use io_stream_manager::{
            ApiError as StreamApiError, RedeemArgs, RedemptionProgress, RewardBackingProgress,
            RewardEventClassification, Status as StreamStatus, StreamProgress,
        };

        let _guard = crate::lock_test_env();
        let fixture = super::create_zero_maturity_protected_neuron_with_stake(6_000_000_000);
        let sns = super::install_combined_real_sns(&fixture);
        let stream_wasm = super::install_combined_stream(&fixture, &sns);
        let manager_wasm = super::production_wasm("io_nns_neuron_manager");
        super::fund_manager_staging(&fixture);
        super::install_manager(
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
        let ordinary_maturity = super::earn_maturity(&fixture);
        assert!(ordinary_maturity >= 200_000_000);

        let stream_unpause: Result<(), StreamApiError> = super::update(
            &fixture.pic,
            sns.stream,
            sns.governance.governance,
            "set_paused",
            false,
        );
        assert_eq!(stream_unpause, Ok(()));
        let baseline: StreamStatus = super::query(
            &fixture.pic,
            sns.stream,
            Principal::anonymous(),
            "get_status",
            (),
        );
        let baseline_round = baseline.latest_processed_reward_event.unwrap().round;
        let mut event = crate::sns_governance_setup::advance_until_reward_event(
            &sns.governance,
            0,
            baseline_round,
        );
        let mut reward_observations = 0;
        let accumulated = loop {
            reward_observations += 1;
            assert!(
                reward_observations <= 4,
                "candidate reward event did not converge"
            );
            let _: Result<io_stream_manager::RewardEventObservation, StreamApiError> =
                super::update(
                    &fixture.pic,
                    sns.stream,
                    Principal::anonymous(),
                    "resume_reward_work",
                    (),
                );
            let status: StreamStatus = super::query(
                &fixture.pic,
                sns.stream,
                Principal::anonymous(),
                "get_status",
                (),
            );
            match status.latest_reward_event_classification {
                Some(RewardEventClassification::NoProposalFallback) => break status,
                Some(RewardEventClassification::MissedSkipped) => {
                    assert_eq!(status.accumulated_policy_credit, 0);
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
        assert_eq!(
            accumulated.accumulated_policy_credit,
            io_reward_policy::DAILY_EVENT_CREDIT
        );
        assert_eq!(accumulated.accumulated_entitlements.len(), 3);
        let expected_credits = [100_000_000_u128, 200_000_000, 300_000_000].map(|stake| {
            io_reward_policy::mul_div_floor(
                io_reward_policy::DAILY_EVENT_CREDIT,
                stake,
                600_000_000,
            )
            .unwrap()
        });
        assert_eq!(
            accumulated.accumulated_eligible_credit,
            expected_credits.into_iter().sum::<u128>()
        );
        assert_eq!(
            accumulated.accumulated_eligible_credit + 1,
            io_reward_policy::DAILY_EVENT_CREDIT
        );
        let frozen_credits = accumulated.accumulated_entitlements.clone();
        let prepared: Result<RewardBackingProgress, StreamApiError> = super::update(
            &fixture.pic,
            sns.stream,
            Principal::anonymous(),
            "resume_reward_backing",
            (),
        );
        assert_eq!(
            prepared,
            Ok(RewardBackingProgress::MaturityPrepared { generation: 1 })
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
                    ManagerMaturityProgress::AwaitingMintProof,
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
        for _ in 0..100 {
            fixture.pic.tick();
        }
        let staging = IcpAccount::new(fixture.controller, Some(Subaccount([2; 32])))
            .icp_account_identifier_bytes();
        let (mint_block, actual_minted_e8s) = super::find_mint(&fixture, &staging);
        let proved: Result<ManagerMaturityProgress, ManagerApiError> = decode_one(
            &fixture
                .pic
                .update_call(
                    fixture.controller,
                    Principal::anonymous(),
                    "prove_maturity_mint",
                    encode_args((ManagerMaturityKind::TwoWeek, u128::from(mint_block))).unwrap(),
                )
                .unwrap(),
        )
        .unwrap();
        assert_eq!(proved, Ok(ManagerMaturityProgress::MintProved));

        let mut stream_upgraded = false;
        let completed = loop {
            let progress: Result<ManagerNnsProgress, ManagerApiError> = super::update(
                &fixture.pic,
                fixture.controller,
                Principal::anonymous(),
                "resume",
                (),
            );
            maturity_phases.push(format!("{progress:?}"));
            let stream_status: StreamStatus = super::query(
                &fixture.pic,
                sns.stream,
                Principal::anonymous(),
                "get_status",
                (),
            );
            if !stream_upgraded && stream_status.operation_phase.as_deref() == Some("ReceiptProved")
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
                stream_upgraded = true;
            }
            if let Ok(ManagerNnsProgress::Maturity(ManagerMaturityProgress::Completed(completed))) =
                progress
            {
                break completed;
            }
            assert!(maturity_phases.len() < 40, "{maturity_phases:?}");
        };
        assert!(stream_upgraded, "{maturity_phases:?}");
        assert_eq!(
            completed.actual_minted_icp_e8s,
            u128::from(actual_minted_e8s)
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
        let backed_io_pool =
            io_core_model::backed_io(u128::from(actual_minted_e8s), 7_000_000_000, 700_000_000)
                .unwrap();
        let allocation = io_reward_policy::allocate_rewards(
            backed_io_pool,
            io_reward_policy::DAILY_EVENT_CREDIT,
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

        let resumed: Result<(), StreamApiError> = super::update(
            &fixture.pic,
            sns.stream,
            sns.governance.governance,
            "set_paused",
            false,
        );
        assert_eq!(resumed, Ok(()));
        let redemption_amount = 20_000_000_u64;
        let now = fixture.pic.get_time().as_nanos_since_unix_epoch();
        icrc::icrc2_approve(
            &fixture.pic,
            sns.governance.ledger,
            fixture.controller,
            icrc::ApproveArgs {
                from_subaccount: None,
                spender: icrc::account(sns.stream, None),
                amount: Nat::from(redemption_amount + ICP_FEE_E8S),
                expected_allowance: Some(Nat::from(0_u8)),
                expires_at: Some(now + 800_000_000_000),
                fee: Some(Nat::from(ICP_FEE_E8S)),
                memo: Some(b"combined-real-redemption".to_vec()),
                created_at_time: Some(now),
            },
        )
        .unwrap();
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
        let quote = io_core_model::redemption_quote(
            redemption_amount.into(),
            ICP_FEE_E8S.into(),
            supply_before,
            reserve_before,
            0,
            liquid_before,
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
        let pulled: Result<RedemptionProgress, StreamApiError> = super::update(
            &fixture.pic,
            sns.stream,
            fixture.controller,
            "redeem",
            RedeemArgs {
                from_subaccount: None,
                io_amount_e8s: redemption_amount.into(),
                min_icp_out_e8s: quote.net_icp_e8s,
                max_io_fee_e8s: ICP_FEE_E8S.into(),
                max_icp_fee_e8s: ICP_FEE_E8S.into(),
                expires_at_nanos: now + 800_000_000_000,
                nonce: 0,
            },
        );
        assert_eq!(pulled, Ok(RedemptionProgress::IoInReserve));
        fixture
            .pic
            .upgrade_canister(sns.stream, stream_wasm, encode_one(()).unwrap(), None)
            .unwrap();
        let redemption = loop {
            let progress: Result<StreamProgress, StreamApiError> = super::update(
                &fixture.pic,
                sns.stream,
                Principal::anonymous(),
                "resume",
                (),
            );
            if let Ok(StreamProgress::Redemption(RedemptionProgress::Completed(completed))) =
                progress
            {
                break completed;
            }
        };
        assert_eq!(redemption.gross_icp_e8s, quote.gross_icp_e8s);
        assert_eq!(redemption.net_icp_e8s, quote.net_icp_e8s);
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
        assert_eq!(icp_after.0 - icp_before.0, quote.net_icp_e8s.into());
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
        assert_eq!(final_manager.lifecycle, ManagerLifecycle::Paused);
        assert_eq!(final_stream.lifecycle, io_stream_manager::Lifecycle::Paused);
        eprintln!(
            "combined_real_summary event_round={} ordinary_maturity={} actual_mint={} reward_recipients={} redemption={redemption:?} phases={maturity_phases:?}",
            event.round,
            ordinary_maturity,
            actual_minted_e8s,
            recipient_after.len(),
        );
    }

    #[test]
    #[ignore = "requires pinned real NNS Governance/ICP ledger, NNS manager Wasm, and POCKET_IC_BIN"]
    fn controlled_manager_proves_baseline_target_and_maturity_through_exact_mint() {
        use io_stream_manager::{
            Lifecycle as StreamLifecycle, RewardBackingProgress, RewardEventClassification,
            RewardEventObservation, Status as StreamStatus,
        };
        let _guard = crate::lock_test_env();
        let fixture = super::create_zero_maturity_protected_neuron();
        let stream = crate::pocketic_env::create_empty_application_canister(&fixture.pic);
        let controlled_stream = super::install_controlled_stream(
            &fixture,
            stream,
            super::debug_wasm("io_stream_manager"),
        );
        super::fund_manager_staging(&fixture);
        let _ = super::install_manager(
            &fixture,
            stream,
            controlled_stream.governance,
            super::manager_wasm(),
        );

        let unpause: Result<(), ManagerApiError> = super::update(
            &fixture.pic,
            fixture.controller,
            controlled_stream.governance,
            "set_paused",
            false,
        );
        assert_eq!(unpause, Ok(()));
        let status: ManagerStatus = super::query(
            &fixture.pic,
            fixture.controller,
            Principal::anonymous(),
            "get_status",
            (),
        );
        assert_eq!(status.lifecycle, ManagerLifecycle::Ready);
        assert!(status.two_week_maturity_baseline_reconciled);

        fixture
            .pic
            .upgrade_canister(
                fixture.controller,
                super::manager_wasm(),
                encode_one(()).unwrap(),
                None,
            )
            .unwrap();
        let status: ManagerStatus = super::query(
            &fixture.pic,
            fixture.controller,
            Principal::anonymous(),
            "get_status",
            (),
        );
        assert_eq!(status.lifecycle, ManagerLifecycle::Paused);
        assert!(status.two_week_maturity_baseline_reconciled);
        let unpause: Result<(), ManagerApiError> = super::update(
            &fixture.pic,
            fixture.controller,
            controlled_stream.governance,
            "set_paused",
            false,
        );
        assert_eq!(unpause, Ok(()));

        let under_target: Result<io_receipt_types::TwoWeekBackingReadiness, ManagerApiError> =
            super::update(
                &fixture.pic,
                fixture.controller,
                stream,
                "reconcile_two_week_backing_readiness",
                ReconcileTwoWeekBackingReadinessArgs {
                    target_e8s: u128::from(super::PROTECTED_STAKE_E8S + 100_000_000),
                },
            );
        assert_eq!(
            under_target,
            Ok(io_receipt_types::TwoWeekBackingReadiness::NotReady(
                io_receipt_types::BackingNotReadyReason::UnderTarget
            ))
        );
        let readiness: Result<io_receipt_types::TwoWeekBackingReadiness, ManagerApiError> =
            super::update(
                &fixture.pic,
                fixture.controller,
                stream,
                "reconcile_two_week_backing_readiness",
                ReconcileTwoWeekBackingReadinessArgs {
                    target_e8s: super::PROTECTED_STAKE_E8S.into(),
                },
            );
        assert_eq!(
            readiness,
            Ok(io_receipt_types::TwoWeekBackingReadiness::NotReady(
                io_receipt_types::BackingNotReadyReason::BelowThreshold
            ))
        );

        let over_target: Result<io_receipt_types::TwoWeekBackingReadiness, ManagerApiError> =
            super::update(
                &fixture.pic,
                fixture.controller,
                stream,
                "reconcile_two_week_backing_readiness",
                ReconcileTwoWeekBackingReadinessArgs {
                    target_e8s: super::RECONCILED_TARGET_E8S.into(),
                },
            );
        assert_eq!(
            over_target,
            Ok(io_receipt_types::TwoWeekBackingReadiness::NotReady(
                io_receipt_types::BackingNotReadyReason::OverTarget
            ))
        );
        let status: ManagerStatus = super::query(
            &fixture.pic,
            fixture.controller,
            Principal::anonymous(),
            "get_status",
            (),
        );
        assert_eq!(status.latest_started_two_week_generation, 0);
        assert_eq!(status.active_operation.as_deref(), Some("Unwind"));
        assert_eq!(status.unwinding_child_principal_e8s, 0);

        let mut unwind_phases = Vec::new();
        for step in 0..2 {
            let progress: Result<ManagerNnsProgress, ManagerApiError> = super::update(
                &fixture.pic,
                fixture.controller,
                Principal::anonymous(),
                "resume",
                (),
            );
            let unwind_status: ManagerStatus = super::query(
                &fixture.pic,
                fixture.controller,
                Principal::anonymous(),
                "get_status",
                (),
            );
            unwind_phases.push(format!("{progress:?}; {unwind_status:?}"));
            if step == 0 {
                assert_eq!(
                    super::neuron(
                        &fixture.pic,
                        fixture.governance,
                        fixture.controller,
                        fixture.neuron_id,
                    )
                    .cached_neuron_stake_e8s,
                    super::RECONCILED_TARGET_E8S,
                    "{unwind_phases:?}"
                );
            }
            fixture
                .pic
                .upgrade_canister(
                    fixture.controller,
                    super::manager_wasm(),
                    encode_one(()).unwrap(),
                    None,
                )
                .unwrap();
        }
        let status: ManagerStatus = super::query(
            &fixture.pic,
            fixture.controller,
            Principal::anonymous(),
            "get_status",
            (),
        );
        assert!(status.active_operation.is_none(), "{unwind_phases:?}");
        assert!(
            status.unwinding_child_principal_e8s > 0,
            "{unwind_phases:?}"
        );
        assert_eq!(
            super::neuron(
                &fixture.pic,
                fixture.governance,
                fixture.controller,
                fixture.neuron_id,
            )
            .cached_neuron_stake_e8s,
            super::RECONCILED_TARGET_E8S
        );
        super::fund_stream_liquidity(&fixture, stream, 0);
        let unpause: Result<(), ManagerApiError> = super::update(
            &fixture.pic,
            fixture.controller,
            controlled_stream.governance,
            "set_paused",
            false,
        );
        assert_eq!(unpause, Ok(()));
        let lower_target: Result<io_receipt_types::TwoWeekBackingReadiness, ManagerApiError> =
            super::update(
                &fixture.pic,
                fixture.controller,
                stream,
                "reconcile_two_week_backing_readiness",
                ReconcileTwoWeekBackingReadinessArgs {
                    target_e8s: u128::from(super::RECONCILED_TARGET_E8S - 100_000_000),
                },
            );
        assert_eq!(
            lower_target,
            Ok(io_receipt_types::TwoWeekBackingReadiness::NotReady(
                io_receipt_types::BackingNotReadyReason::OverTarget
            ))
        );
        let status: ManagerStatus = super::query(
            &fixture.pic,
            fixture.controller,
            Principal::anonymous(),
            "get_status",
            (),
        );
        assert!(status.active_operation.is_none());
        assert!(status.unwinding_child_principal_e8s > 0);
        let readiness: Result<io_receipt_types::TwoWeekBackingReadiness, ManagerApiError> =
            super::update(
                &fixture.pic,
                fixture.controller,
                stream,
                "reconcile_two_week_backing_readiness",
                ReconcileTwoWeekBackingReadinessArgs {
                    target_e8s: super::RECONCILED_TARGET_E8S.into(),
                },
            );
        assert_eq!(
            readiness,
            Ok(io_receipt_types::TwoWeekBackingReadiness::NotReady(
                io_receipt_types::BackingNotReadyReason::BelowThreshold
            ))
        );

        let stream_unpause: Result<(), io_stream_manager::ApiError> = super::update(
            &fixture.pic,
            stream,
            controlled_stream.governance,
            "set_paused",
            false,
        );
        assert_eq!(stream_unpause, Ok(()));
        let baseline: StreamStatus = super::query(
            &fixture.pic,
            stream,
            Principal::anonymous(),
            "get_status",
            (),
        );
        assert_eq!(baseline.lifecycle, StreamLifecycle::Ready);
        assert_eq!(baseline.processed_reward_event_count, 0);
        let event_set: Result<(), String> = super::update(
            &fixture.pic,
            controlled_stream.governance,
            Principal::anonymous(),
            "debug_set_latest_reward_event",
            LatestRewardEventFixture {
                round: 2,
                rounds_since_last_distribution: 1,
                end_timestamp_seconds: 86_401,
                settled_proposal_ids: vec![],
                neuron_reward_shares: vec![],
            },
        );
        event_set.unwrap();
        let observation: Result<RewardEventObservation, io_stream_manager::ApiError> =
            super::update(
                &fixture.pic,
                stream,
                Principal::anonymous(),
                "resume_reward_work",
                (),
            );
        match observation {
            Ok(observation) => assert_eq!(
                observation.classification,
                RewardEventClassification::NoProposalFallback
            ),
            Err(io_stream_manager::ApiError::Pending(message))
                if message == "SNS reward event has not advanced" =>
            {
                let status: StreamStatus = super::query(
                    &fixture.pic,
                    stream,
                    Principal::anonymous(),
                    "get_status",
                    (),
                );
                assert_eq!(status.processed_reward_event_count, 1);
                assert_eq!(
                    status.latest_reward_event_classification,
                    Some(RewardEventClassification::NoProposalFallback)
                );
            }
            other => panic!("controlled entitlement observation failed: {other:?}"),
        }

        let maturity = super::earn_maturity(&fixture);
        assert!(maturity >= 200_000_000);
        let prepared: Result<RewardBackingProgress, io_stream_manager::ApiError> = super::update(
            &fixture.pic,
            stream,
            Principal::anonymous(),
            "resume_reward_backing",
            (),
        );
        assert_eq!(
            prepared,
            Ok(RewardBackingProgress::MaturityPrepared { generation: 1 })
        );

        let mut seen = Vec::new();
        for _ in 0..12 {
            let progress: Result<ManagerNnsProgress, ManagerApiError> = super::update(
                &fixture.pic,
                fixture.controller,
                Principal::anonymous(),
                "resume",
                (),
            );
            seen.push(format!("{progress:?}"));
            if progress
                == Ok(ManagerNnsProgress::Maturity(
                    ManagerMaturityProgress::AwaitingMintProof,
                ))
            {
                break;
            }
            fixture
                .pic
                .upgrade_canister(
                    fixture.controller,
                    super::manager_wasm(),
                    encode_one(()).unwrap(),
                    None,
                )
                .unwrap();
        }
        assert!(
            seen.last()
                .is_some_and(|value| value.contains("AwaitingMintProof")),
            "{seen:?}"
        );
        let pending = super::neuron(
            &fixture.pic,
            fixture.governance,
            fixture.controller,
            fixture.neuron_id,
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
        let destination = IcpAccount::new(fixture.controller, Some(Subaccount([2; 32])))
            .icp_account_identifier_bytes();
        let (mint_block, actual_minted_e8s) = super::find_mint(&fixture, &destination);
        let proved: Result<ManagerMaturityProgress, ManagerApiError> = decode_one(
            &fixture
                .pic
                .update_call(
                    fixture.controller,
                    Principal::anonymous(),
                    "prove_maturity_mint",
                    encode_args((ManagerMaturityKind::TwoWeek, u128::from(mint_block))).unwrap(),
                )
                .unwrap(),
        )
        .unwrap();
        assert_eq!(proved, Ok(ManagerMaturityProgress::MintProved));

        let before: candid::Nat = super::query(
            &fixture.pic,
            controlled_stream.io_ledger,
            Principal::anonymous(),
            "icrc1_balance_of",
            controlled_stream.neuron_destination.clone(),
        );
        let mut delivery = Vec::new();
        let mut stream_upgraded = false;
        for _ in 0..24 {
            let progress: Result<ManagerNnsProgress, ManagerApiError> = super::update(
                &fixture.pic,
                fixture.controller,
                Principal::anonymous(),
                "resume",
                (),
            );
            delivery.push(format!("{progress:?}"));
            let after: candid::Nat = super::query(
                &fixture.pic,
                controlled_stream.io_ledger,
                Principal::anonymous(),
                "icrc1_balance_of",
                controlled_stream.neuron_destination.clone(),
            );
            if !stream_upgraded && after > before {
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
            if matches!(
                progress,
                Ok(ManagerNnsProgress::Maturity(
                    ManagerMaturityProgress::Completed(_)
                ))
            ) {
                break;
            }
            fixture
                .pic
                .upgrade_canister(
                    fixture.controller,
                    super::manager_wasm(),
                    encode_one(()).unwrap(),
                    None,
                )
                .unwrap();
        }
        let after: candid::Nat = super::query(
            &fixture.pic,
            controlled_stream.io_ledger,
            Principal::anonymous(),
            "icrc1_balance_of",
            controlled_stream.neuron_destination,
        );
        assert!(after > before, "{delivery:?}");
        assert!(stream_upgraded, "{delivery:?}");
        let status: ManagerStatus = super::query(
            &fixture.pic,
            fixture.controller,
            Principal::anonymous(),
            "get_status",
            (),
        );
        assert_eq!(
            status.latest_completed_two_week_generation, 1,
            "{delivery:?}"
        );
        assert_eq!(status.latest_started_two_week_generation, 1);

        let stream_unpause: Result<(), io_stream_manager::ApiError> = super::update(
            &fixture.pic,
            stream,
            controlled_stream.governance,
            "set_paused",
            false,
        );
        assert_eq!(stream_unpause, Ok(()));
        let event_set: Result<(), String> = super::update(
            &fixture.pic,
            controlled_stream.governance,
            Principal::anonymous(),
            "debug_set_latest_reward_event",
            LatestRewardEventFixture {
                round: 3,
                rounds_since_last_distribution: 1,
                end_timestamp_seconds: 172_801,
                settled_proposal_ids: vec![],
                neuron_reward_shares: vec![],
            },
        );
        event_set.unwrap();
        let _: Result<RewardEventObservation, io_stream_manager::ApiError> = super::update(
            &fixture.pic,
            stream,
            Principal::anonymous(),
            "resume_reward_work",
            (),
        );
        let next: StreamStatus = super::query(
            &fixture.pic,
            stream,
            Principal::anonymous(),
            "get_status",
            (),
        );
        assert_eq!(next.latest_entitlement_batch_generation, 1);
        assert_eq!(
            next.accumulated_policy_credit,
            io_reward_policy::DAILY_EVENT_CREDIT
        );
        assert!(next.pending_entitlement_batch_policy_credit.is_none());
        assert_eq!(next.next_nns_receipt_sequence, 1);

        fixture.pic.advance_time(Duration::from_secs(
            u64::from(super::EIGHT_YEARS_SECONDS) + 30 * 24 * 60 * 60,
        ));
        for _ in 0..20 {
            fixture.pic.tick();
        }
        for _ in 0..3 {
            let _: Result<ManagerNnsProgress, ManagerApiError> = super::update(
                &fixture.pic,
                fixture.controller,
                Principal::anonymous(),
                "resume",
                (),
            );
            fixture
                .pic
                .upgrade_canister(
                    fixture.controller,
                    super::manager_wasm(),
                    encode_one(()).unwrap(),
                    None,
                )
                .unwrap();
        }
        let status: ManagerStatus = super::query(
            &fixture.pic,
            fixture.controller,
            Principal::anonymous(),
            "get_status",
            (),
        );
        assert_eq!(status.unwinding_child_principal_e8s, 0);
        eprintln!(
            "controlled_unwind_phases={unwind_phases:?} controlled_manager_phases={seen:?} controlled_delivery={delivery:?} mint_block={mint_block} actual_minted_e8s={actual_minted_e8s}"
        );
    }
}
