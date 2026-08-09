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
    pub pic: PocketIc,
    pub governance: Principal,
    pub ledger: Principal,
    pub controller: Principal,
    pub neuron_id: u64,
    pub two_year_neuron_id: u64,
    pub proposer_neuron_id: u64,
}

pub fn create_zero_maturity_protected_neuron() -> ControlledNnsNeuron {
    let pic = nns_setup::controlled_pinned_nns(true).unwrap();
    let governance = Principal::from_text(nns_setup::install_nns_governance().canister_id).unwrap();
    let ledger = Principal::from_text(nns_setup::install_nns_ledger().canister_id).unwrap();
    let controller = pocketic_env::create_empty_application_canister(&pic);
    let neuron_id = stake_neuron(
        &pic,
        governance,
        ledger,
        controller,
        PROTECTED_MEMO,
        PROTECTED_STAKE_E8S,
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
    assert_eq!(neuron.cached_neuron_stake_e8s, PROTECTED_STAKE_E8S);
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
    let auto_stake_disabled = manage(
        pic,
        governance,
        controller,
        NnsProductionManageNeuronRequest {
            neuron_id_or_subaccount: Some(NnsNeuronIdOrSubaccount::NeuronId(NnsNeuronIdRecord {
                id: neuron_id,
            })),
            command: Some(NnsManageNeuronCommandRequest::Configure(
                NnsProductionConfigure {
                    operation: Some(NnsProductionConfigureOperation::ChangeAutoStakeMaturity(
                        NnsChangeAutoStakeMaturity {
                            requested_setting_for_auto_stake_maturity: false,
                        },
                    )),
                },
            )),
            id: None,
        },
    );
    assert!(matches!(
        auto_stake_disabled.command,
        Some(NnsManageNeuronResponseCommandRecord::Configure(
            EmptyRecord {}
        ))
    ));
    neuron_id
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
                seeded_two_week_principal_e8s: PROTECTED_STAKE_E8S.into(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protected_nns_policy_contradiction_is_recorded_before_correction() {
        assert_eq!(u64::from(EIGHT_YEARS_SECONDS), 252_460_800);
        assert_eq!(NNS_MINIMUM_DISSOLVE_DELAY_TO_VOTE_SECONDS, 15_552_000);
        assert_eq!(io_core_model::TWO_WEEK_SECONDS, 1_209_600);
        assert!(io_core_model::TWO_WEEK_SECONDS < NNS_MINIMUM_DISSOLVE_DELAY_TO_VOTE_SECONDS);

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
            controlled_stream.governance,
            manager_wasm.clone(),
        );
        super::fund_stream_liquidity(&fixture, stream, 0);

        let mut prior_staked_maturity = 0;
        let mut actual_mints = Vec::new();
        for cycle in 0..2 {
            let unpause: Result<(), ManagerApiError> = super::update(
                &fixture.pic,
                fixture.controller,
                controlled_stream.governance,
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
            let started: Result<ManagerMaturityProgress, ManagerApiError> = super::update(
                &fixture.pic,
                fixture.controller,
                controlled_stream.governance,
                "start_maturity",
                ManagerMaturityKind::TwoYear,
            );
            assert_eq!(started, Ok(ManagerMaturityProgress::Observed));
            let replay: Result<ManagerMaturityProgress, ManagerApiError> = super::update(
                &fixture.pic,
                fixture.controller,
                controlled_stream.governance,
                "start_maturity",
                ManagerMaturityKind::TwoYear,
            );
            assert_eq!(replay, Err(ManagerApiError::Busy));
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
