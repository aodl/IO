#![cfg(test)]

use candid::{decode_one, encode_one, CandidType, Nat, Principal, Reserved};
use io_governance_types::{
    nns_refresh_voting_power_request, EmptyRecord, NnsAccount, NnsClaimOrRefresh,
    NnsClaimOrRefreshBy, NnsClaimOrRefreshNeuronFromAccount, NnsDissolveStateRecord,
    NnsFolloweesForTopic, NnsGovernanceErrorRecord, NnsIncreaseDissolveDelay,
    NnsManageNeuronCommandRequest, NnsManageNeuronResponseCommandRecord, NnsMerge,
    NnsNeuronIdOrSubaccount, NnsNeuronIdRecord, NnsNeuronRecord, NnsProductionConfigure,
    NnsProductionConfigureOperation, NnsProductionDisburseMaturity,
    NnsProductionManageNeuronRequest, NnsProductionManageNeuronResponse, NnsProposalIdRecord,
    NnsRegisterVote, NnsSetFollowing, NnsSplit, NnsStakeMaturity,
};
use io_ledger_types::{
    Account as IcpAccount, IcpTokens, IcpTransferArgs, IcpTransferError, Subaccount,
};
use pocket_ic::PocketIc;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use crate::{icrc, nns_setup, pocketic_env};

const CANDIDATE_COMMIT: &str = "8aa4680e378f3248e7e7b9b8237915aded999bd9";
const CANDIDATE_COMPRESSED_SHA256: &str =
    "b41a5add38d54751d53fb4f0c826b09aaee38e0c5bea632400f1dbaaa11cfd4b";
const CANDIDATE_RAW_SHA256: &str =
    "eaa2da45722d980b25405525873571ab7dad426a93e1d4971f6b555d80906d85";
const CANDIDATE_DID_SHA256: &str =
    "6e9a397f4bf0adc913980ef6c176e765534617d0ce59d52e7bcc66add2b0cd71";
const CANDIDATE_PROPOSAL_ID: u64 = 143_577;
const FOURTEEN_DAYS_SECONDS: u64 = 1_209_600;
const BELOW_FOURTEEN_DAYS_SECONDS: u64 = FOURTEEN_DAYS_SECONDS - 1;
const PROPOSAL_SUBMISSION_THRESHOLD_SECONDS: u64 = 15_778_800;
const ONE_YEAR_SECONDS: u32 = 365 * 24 * 60 * 60;
const ICP_FEE_E8S: u64 = 10_000;
const MINIMUM_STAKE_E8S: u64 = 100_000_000;
const EXACT_PARENT_STAKE_E8S: u64 = 100_000_000 * 100_000_000;
const TOP_UP_E8S: u64 = 200_000_000;
const CHILD_GROSS_E8S: u64 = 200_010_000;
const CHILD_CREDITED_E8S: u64 = CHILD_GROSS_E8S - ICP_FEE_E8S;
const MATURITY_FINALIZATION_SECONDS: u64 = 7 * 24 * 60 * 60;
const XRC_CANISTER_ID: &str = "uf6dk-hyaaa-aaaaq-qaaaq-cai";
const GOVERNANCE_CANISTER_ID: &str = "rrkah-fqaaa-aaaaa-aaaaq-cai";
const LEDGER_CANISTER_ID: &str = "ryjl3-tyaaa-aaaaa-aaaba-cai";
const ROOT_CANISTER_ID: &str = "r7inp-6aaaa-aaaaa-aaabq-cai";
const CANDIDATE_WASM_ENV: &str = "IO_POST_M70_NNS_GOVERNANCE_WASM";
const XRC_WASM_ENV: &str = "IO_POST_M70_XRC_WASM";
const ACTOR_WASM_ENV: &str = "IO_POST_M70_ACTOR_WASM";

#[derive(Clone, Debug, CandidType, Deserialize)]
struct VotingPowerEconomicsProbe {
    start_reducing_voting_power_after_seconds: Option<u64>,
    clear_following_after_seconds: Option<u64>,
    neuron_minimum_dissolve_delay_to_vote_seconds: Option<u64>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct NetworkEconomicsProbe {
    voting_power_economics: Option<VotingPowerEconomicsProbe>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct MaturityModulationProbe {
    current_value_permyriad: Option<i32>,
    updated_at_timestamp_seconds: Option<u64>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct MaturityModulationResponse {
    maturity_modulation: Option<MaturityModulationProbe>,
}

#[derive(Clone, Debug, CandidType)]
struct GetMaturityModulationRequest {}

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
struct ProposalInfoProbe {
    id: Option<NnsProposalIdRecord>,
    reward_event_round: u64,
    ballots: Vec<(u64, BallotProbe)>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct BallotProbe {
    vote: i32,
    voting_power: u64,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct RewardEventProbe {
    day_after_genesis: u64,
    settled_proposals: Vec<NnsProposalIdRecord>,
    distributed_e8s_equivalent: u64,
    total_available_e8s_equivalent: u64,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct TransferIcpArgs {
    ledger: Principal,
    from_subaccount: Option<Vec<u8>>,
    to: Vec<u8>,
    amount_e8s: u64,
    fee_e8s: u64,
    memo: u64,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct RefreshNeuronArgs {
    governance: Principal,
    neuron_id: u64,
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
    chain_length: u64,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct IcpBlock {
    transaction: IcpTransaction,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct IcpTransaction {
    operation: Option<IcpOperation>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
enum IcpOperation {
    Mint { to: Vec<u8>, amount: IcpTokens },
    Burn(Reserved),
    Transfer(Reserved),
    Approve(Reserved),
}

struct CandidateFixture {
    pic: Rc<PocketIc>,
    governance: Principal,
    ledger: Principal,
    root: Principal,
    manager: Principal,
    stream: Principal,
    xrc: Principal,
}

impl CandidateFixture {
    fn old_io_boundary() -> Self {
        let pic = Rc::new(nns_setup::controlled_pinned_nns_with_fiduciary(true).unwrap());
        let governance = principal(GOVERNANCE_CANISTER_ID);
        let ledger = principal(LEDGER_CANISTER_ID);
        let root = principal(ROOT_CANISTER_ID);
        let xrc = principal(XRC_CANISTER_ID);
        let created = pic
            .create_canister_with_id(None, None, xrc)
            .expect("source-shaped XRC fixture should use the canonical XRC principal");
        assert_eq!(created, xrc);
        pic.install_canister(
            xrc,
            fixture_wasm(XRC_WASM_ENV, "mock_nns_xrc"),
            Vec::new(),
            None,
        );
        let actor_wasm = fixture_wasm(ACTOR_WASM_ENV, "mock_nns_candidate_actor");
        let manager =
            pocketic_env::create_application_canister(&pic, actor_wasm.clone(), Vec::new());
        let stream = pocketic_env::create_application_canister(&pic, actor_wasm, Vec::new());
        Self {
            pic,
            governance,
            ledger,
            root,
            manager,
            stream,
            xrc,
        }
    }

    fn upgrade_to_candidate(&self) {
        self.pic
            .upgrade_canister(
                self.governance,
                candidate_wasm(),
                Vec::new(),
                Some(self.root),
            )
            .expect("old IO-pinned Governance state should upgrade to the exact candidate");
        for _ in 0..20 {
            self.pic.tick();
        }
        let installed = self
            .pic
            .canister_status(self.governance, Some(self.root))
            .unwrap()
            .module_hash
            .expect("candidate Governance should have a module hash");
        assert_eq!(hex::encode(installed), CANDIDATE_RAW_SHA256);
    }
}

fn principal(text: &str) -> Principal {
    Principal::from_text(text).expect("well-known principal should parse")
}

fn candidate_wasm() -> Vec<u8> {
    let path = std::env::var_os(CANDIDATE_WASM_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("{CANDIDATE_WASM_ENV} is required for this ignored test"));
    let bytes = std::fs::read(&path).unwrap_or_else(|error| {
        panic!("failed to read candidate Wasm {}: {error}", path.display())
    });
    assert_sha256(&path, &bytes, CANDIDATE_RAW_SHA256);
    bytes
}

fn fixture_wasm(env_name: &str, target_name: &str) -> Vec<u8> {
    let path = std::env::var_os(env_name)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/wasm32-unknown-unknown/debug")
                .join(format!("{target_name}.wasm"))
        });
    std::fs::read(&path).unwrap_or_else(|error| {
        panic!(
            "build test-only Wasm {} or set {env_name}: {error}",
            path.display()
        )
    })
}

fn assert_sha256(path: &Path, bytes: &[u8], expected: &str) {
    let actual = hex::encode(Sha256::digest(bytes));
    assert_eq!(actual, expected, "SHA-256 mismatch for {}", path.display());
}

fn query<T: CandidType + for<'de> Deserialize<'de>>(
    fixture: &CandidateFixture,
    canister: Principal,
    caller: Principal,
    method: &str,
    arg: impl CandidType,
) -> T {
    decode_one(
        &fixture
            .pic
            .query_call(canister, caller, method, encode_one(arg).unwrap())
            .unwrap_or_else(|error| panic!("query {method} failed: {error}")),
    )
    .unwrap_or_else(|error| panic!("query {method} decode failed: {error}"))
}

fn update<T: CandidType + for<'de> Deserialize<'de>>(
    fixture: &CandidateFixture,
    canister: Principal,
    caller: Principal,
    method: &str,
    arg: impl CandidType,
) -> T {
    decode_one(
        &fixture
            .pic
            .update_call(canister, caller, method, encode_one(arg).unwrap())
            .unwrap_or_else(|error| panic!("update {method} failed: {error}")),
    )
    .unwrap_or_else(|error| panic!("update {method} decode failed: {error}"))
}

fn network_voting_threshold(fixture: &CandidateFixture) -> Option<u64> {
    let economics: NetworkEconomicsProbe = query(
        fixture,
        fixture.governance,
        Principal::anonymous(),
        "get_network_economics_parameters",
        (),
    );
    economics
        .voting_power_economics
        .and_then(|economics| economics.neuron_minimum_dissolve_delay_to_vote_seconds)
}

fn staking_subaccount(controller: Principal, memo: u64) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update([0x0c]);
    hasher.update(b"neuron-stake");
    hasher.update(controller.as_slice());
    hasher.update(memo.to_be_bytes());
    hasher.finalize().into()
}

fn staking_account_identifier(
    fixture: &CandidateFixture,
    controller: Principal,
    memo: u64,
) -> Vec<u8> {
    IcpAccount::new(
        fixture.governance,
        Some(Subaccount(staking_subaccount(controller, memo))),
    )
    .icp_account_identifier_bytes()
    .to_vec()
}

fn transfer_from_mint(fixture: &CandidateFixture, to: Vec<u8>, amount_e8s: u64, memo: u64) -> u64 {
    let result: Result<u64, IcpTransferError> = update(
        fixture,
        fixture.ledger,
        Principal::anonymous(),
        "transfer",
        IcpTransferArgs {
            memo,
            amount: IcpTokens { e8s: amount_e8s },
            fee: IcpTokens { e8s: ICP_FEE_E8S },
            from_subaccount: None,
            to,
            created_at_time: None,
        },
    );
    result.expect("controlled ICP transfer should succeed")
}

fn fund_staking_account(
    fixture: &CandidateFixture,
    controller: Principal,
    memo: u64,
    stake_e8s: u64,
) -> u64 {
    transfer_from_mint(
        fixture,
        staking_account_identifier(fixture, controller, memo),
        stake_e8s,
        memo,
    )
}

fn claim_neuron(
    fixture: &CandidateFixture,
    controller: Principal,
    memo: u64,
) -> Result<u64, NnsGovernanceErrorRecord> {
    let response = manage(
        fixture,
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
    match response.command {
        Some(NnsManageNeuronResponseCommandRecord::ClaimOrRefresh(response)) => Ok(response
            .refreshed_neuron_id
            .expect("successful claim should return a neuron ID")
            .id),
        Some(NnsManageNeuronResponseCommandRecord::Error(error)) => Err(error),
        other => panic!("unexpected ClaimOrRefresh response: {other:?}"),
    }
}

fn create_neuron(
    fixture: &CandidateFixture,
    controller: Principal,
    memo: u64,
    stake_e8s: u64,
    dissolve_delay_seconds: u32,
) -> u64 {
    fund_staking_account(fixture, controller, memo, stake_e8s);
    let neuron_id = claim_neuron(fixture, controller, memo).unwrap();
    let initial_delay = match full_neuron(fixture, controller, neuron_id)
        .unwrap()
        .dissolve_state
        .unwrap()
    {
        NnsDissolveStateRecord::DissolveDelaySeconds(delay) => delay,
        other => panic!("newly claimed neuron should not be dissolving: {other:?}"),
    };
    let target_delay = u64::from(dissolve_delay_seconds);
    assert!(
        initial_delay <= target_delay,
        "newly claimed delay {initial_delay} exceeds requested target {target_delay}"
    );
    let additional_delay = target_delay - initial_delay;
    if additional_delay > 0 {
        configure(
            fixture,
            controller,
            neuron_id,
            NnsProductionConfigureOperation::IncreaseDissolveDelay(NnsIncreaseDissolveDelay {
                additional_dissolve_delay_seconds: u32::try_from(additional_delay).unwrap(),
            }),
        );
    }
    assert_eq!(
        full_neuron(fixture, controller, neuron_id)
            .unwrap()
            .dissolve_state,
        Some(NnsDissolveStateRecord::DissolveDelaySeconds(target_delay))
    );
    neuron_id
}

fn manage(
    fixture: &CandidateFixture,
    caller: Principal,
    request: NnsProductionManageNeuronRequest,
) -> NnsProductionManageNeuronResponse {
    update(
        fixture,
        fixture.governance,
        caller,
        "manage_neuron",
        request,
    )
}

fn configure(
    fixture: &CandidateFixture,
    controller: Principal,
    neuron_id: u64,
    operation: NnsProductionConfigureOperation,
) {
    try_configure(fixture, controller, neuron_id, operation)
        .unwrap_or_else(|error| panic!("Configure failed: {error:?}"));
}

fn try_configure(
    fixture: &CandidateFixture,
    controller: Principal,
    neuron_id: u64,
    operation: NnsProductionConfigureOperation,
) -> Result<(), NnsGovernanceErrorRecord> {
    let response = manage(
        fixture,
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
    match response.command {
        Some(NnsManageNeuronResponseCommandRecord::Configure(EmptyRecord {})) => Ok(()),
        Some(NnsManageNeuronResponseCommandRecord::Error(error)) => Err(error),
        other => panic!("unexpected Configure response: {other:?}"),
    }
}

fn full_neuron(
    fixture: &CandidateFixture,
    caller: Principal,
    neuron_id: u64,
) -> Result<NnsNeuronRecord, NnsGovernanceErrorRecord> {
    query(
        fixture,
        fixture.governance,
        caller,
        "get_full_neuron",
        neuron_id,
    )
}

fn refresh_voting_power(fixture: &CandidateFixture, caller: Principal, neuron_id: u64) {
    let response = manage(
        fixture,
        caller,
        nns_refresh_voting_power_request(io_governance_types::NnsNeuronId(neuron_id)),
    );
    assert!(matches!(
        response.command,
        Some(NnsManageNeuronResponseCommandRecord::RefreshVotingPower(_))
    ));
}

fn set_following(fixture: &CandidateFixture, neuron_id: u64, followee: u64) {
    let response = manage(
        fixture,
        fixture.manager,
        NnsProductionManageNeuronRequest {
            neuron_id_or_subaccount: Some(NnsNeuronIdOrSubaccount::NeuronId(NnsNeuronIdRecord {
                id: neuron_id,
            })),
            command: Some(NnsManageNeuronCommandRequest::SetFollowing(
                NnsSetFollowing {
                    topic_following: Some(
                        [0, 4, 14]
                            .into_iter()
                            .map(|topic| NnsFolloweesForTopic {
                                followees: Some(vec![NnsNeuronIdRecord { id: followee }]),
                                topic: Some(topic),
                            })
                            .collect(),
                    ),
                },
            )),
            id: None,
        },
    );
    assert!(matches!(
        response.command,
        Some(NnsManageNeuronResponseCommandRecord::SetFollowing(_))
    ));
}

fn make_motion(fixture: &CandidateFixture, proposer: u64) -> u64 {
    let response: NnsProductionManageNeuronResponse = update(
        fixture,
        fixture.governance,
        Principal::anonymous(),
        "manage_neuron",
        ProposalRequest {
            neuron_id_or_subaccount: Some(NnsNeuronIdOrSubaccount::NeuronId(NnsNeuronIdRecord {
                id: proposer,
            })),
            command: Some(ProposalCommand::MakeProposal(Proposal {
                url: String::new(),
                title: Some("Post-Mission-70 controlled mechanics".to_string()),
                action: Some(ProposalAction::Motion(Motion {
                    motion_text: "Exercise the exact 14-day candidate boundary".to_string(),
                })),
                summary: "Local PocketIC candidate evidence only".to_string(),
            })),
            id: None,
        },
    );
    match response.command {
        Some(NnsManageNeuronResponseCommandRecord::MakeProposal(response)) => {
            response
                .proposal_id
                .expect("proposal response should contain an ID")
                .id
        }
        other => panic!("controlled proposal failed: {other:?}"),
    }
}

fn proposal_info(
    fixture: &CandidateFixture,
    caller: Principal,
    proposal_id: u64,
) -> ProposalInfoProbe {
    let response: Option<ProposalInfoProbe> = query(
        fixture,
        fixture.governance,
        caller,
        "get_proposal_info",
        proposal_id,
    );
    response.expect("proposal should be observable")
}

fn register_yes_vote(fixture: &CandidateFixture, neuron_id: u64, proposal_id: u64) {
    let response = manage(
        fixture,
        fixture.manager,
        NnsProductionManageNeuronRequest {
            neuron_id_or_subaccount: Some(NnsNeuronIdOrSubaccount::NeuronId(NnsNeuronIdRecord {
                id: neuron_id,
            })),
            command: Some(NnsManageNeuronCommandRequest::RegisterVote(
                NnsRegisterVote {
                    vote: 1,
                    proposal: Some(NnsProposalIdRecord { id: proposal_id }),
                },
            )),
            id: None,
        },
    );
    assert!(matches!(
        response.command,
        Some(NnsManageNeuronResponseCommandRecord::RegisterVote(
            EmptyRecord {}
        ))
    ));
}

fn await_maturity_modulation(fixture: &CandidateFixture) -> MaturityModulationProbe {
    for _ in 0..500 {
        fixture.pic.advance_time(Duration::from_secs(5));
        for _ in 0..5 {
            fixture.pic.tick();
        }
        let response: MaturityModulationResponse = query(
            fixture,
            fixture.governance,
            Principal::anonymous(),
            "get_maturity_modulation",
            GetMaturityModulationRequest {},
        );
        if let Some(modulation) = response.maturity_modulation {
            if modulation.updated_at_timestamp_seconds.is_some() {
                return modulation;
            }
        }
    }
    panic!("candidate Governance did not finish deterministic XRC backfill");
}

fn await_reward(
    fixture: &CandidateFixture,
    neuron_id: u64,
    proposal_id: u64,
    maturity_before_e8s: u64,
) -> (RewardEventProbe, u64) {
    let mut settlement_event = None;
    for day in 1..=30 {
        fixture.pic.advance_time(Duration::from_secs(86_400));
        for _ in 0..100 {
            fixture.pic.tick();
        }
        let neuron = full_neuron(fixture, fixture.manager, neuron_id).unwrap();
        let event: RewardEventProbe = query(
            fixture,
            fixture.governance,
            Principal::anonymous(),
            "get_latest_reward_event",
            (),
        );
        let proposal = proposal_info(fixture, fixture.manager, proposal_id);
        let settled = event
            .settled_proposals
            .iter()
            .any(|proposal| proposal.id == proposal_id);
        if settled {
            settlement_event = Some(event.clone());
        }
        if settled || neuron.maturity_e8s_equivalent > maturity_before_e8s {
            eprintln!(
                "post_m70_reward_diagnostic day={} maturity_before={} maturity_now={} latest_reward_day={} proposal_reward_round={} settled={}",
                day,
                maturity_before_e8s,
                neuron.maturity_e8s_equivalent,
                event.day_after_genesis,
                proposal.reward_event_round,
                settled,
            );
        }
        if neuron.maturity_e8s_equivalent > maturity_before_e8s {
            return (
                settlement_event.expect("maturity increase should follow proposal settlement"),
                neuron.maturity_e8s_equivalent,
            );
        }
    }
    panic!("exact 14-day neuron did not receive ordinary voting maturity within 30 days");
}

fn actor_transfer(
    fixture: &CandidateFixture,
    actor: Principal,
    from_subaccount: Option<Vec<u8>>,
    to: Vec<u8>,
    amount_e8s: u64,
    memo: u64,
) -> Result<u64, String> {
    update(
        fixture,
        actor,
        Principal::anonymous(),
        "transfer_icp",
        TransferIcpArgs {
            ledger: fixture.ledger,
            from_subaccount,
            to,
            amount_e8s,
            fee_e8s: ICP_FEE_E8S,
            memo,
        },
    )
}

fn manager_refresh(fixture: &CandidateFixture, neuron_id: u64) {
    let response: Result<NnsProductionManageNeuronResponse, String> = update(
        fixture,
        fixture.manager,
        Principal::anonymous(),
        "refresh_neuron",
        RefreshNeuronArgs {
            governance: fixture.governance,
            neuron_id,
        },
    );
    assert!(matches!(
        response.unwrap().command,
        Some(NnsManageNeuronResponseCommandRecord::ClaimOrRefresh(_))
    ));
}

fn split(
    fixture: &CandidateFixture,
    parent_id: u64,
    gross_e8s: u64,
) -> Result<u64, NnsGovernanceErrorRecord> {
    let response = manage(
        fixture,
        fixture.manager,
        NnsProductionManageNeuronRequest {
            neuron_id_or_subaccount: Some(NnsNeuronIdOrSubaccount::NeuronId(NnsNeuronIdRecord {
                id: parent_id,
            })),
            command: Some(NnsManageNeuronCommandRequest::Split(NnsSplit {
                amount_e8s: gross_e8s,
            })),
            id: None,
        },
    );
    match response.command {
        Some(NnsManageNeuronResponseCommandRecord::Split(response)) => Ok(response
            .created_neuron_id
            .expect("successful split should return a child")
            .id),
        Some(NnsManageNeuronResponseCommandRecord::Error(error)) => Err(error),
        other => panic!("unexpected Split response: {other:?}"),
    }
}

fn merge(
    fixture: &CandidateFixture,
    parent_id: u64,
    child_id: u64,
) -> io_governance_types::NnsMergeResponse {
    try_merge(fixture, parent_id, child_id)
        .unwrap_or_else(|error| panic!("Merge failed: {error:?}"))
}

fn try_merge(
    fixture: &CandidateFixture,
    parent_id: u64,
    child_id: u64,
) -> Result<io_governance_types::NnsMergeResponse, NnsGovernanceErrorRecord> {
    let response = manage(
        fixture,
        fixture.manager,
        NnsProductionManageNeuronRequest {
            neuron_id_or_subaccount: Some(NnsNeuronIdOrSubaccount::NeuronId(NnsNeuronIdRecord {
                id: parent_id,
            })),
            command: Some(NnsManageNeuronCommandRequest::Merge(NnsMerge {
                source_neuron_id: Some(NnsNeuronIdRecord { id: child_id }),
            })),
            id: None,
        },
    );
    match response.command {
        Some(NnsManageNeuronResponseCommandRecord::Merge(response)) => Ok(*response),
        Some(NnsManageNeuronResponseCommandRecord::Error(error)) => Err(error),
        other => panic!("unexpected Merge response: {other:?}"),
    }
}

fn stake_maturity(
    fixture: &CandidateFixture,
    neuron_id: u64,
    percentage_to_stake: u32,
) -> io_governance_types::NnsStakeMaturityResponse {
    let response = manage(
        fixture,
        fixture.manager,
        NnsProductionManageNeuronRequest {
            neuron_id_or_subaccount: Some(NnsNeuronIdOrSubaccount::NeuronId(NnsNeuronIdRecord {
                id: neuron_id,
            })),
            command: Some(NnsManageNeuronCommandRequest::StakeMaturity(
                NnsStakeMaturity {
                    percentage_to_stake: Some(percentage_to_stake),
                },
            )),
            id: None,
        },
    );
    match response.command {
        Some(NnsManageNeuronResponseCommandRecord::StakeMaturity(response)) => response,
        other => panic!("StakeMaturity failed: {other:?}"),
    }
}

fn disburse(
    fixture: &CandidateFixture,
    neuron_id: u64,
    destination: Vec<u8>,
) -> Result<u64, NnsGovernanceErrorRecord> {
    let response = manage(
        fixture,
        fixture.manager,
        NnsProductionManageNeuronRequest {
            neuron_id_or_subaccount: Some(NnsNeuronIdOrSubaccount::NeuronId(NnsNeuronIdRecord {
                id: neuron_id,
            })),
            command: Some(NnsManageNeuronCommandRequest::Disburse(
                io_governance_types::NnsDisburse {
                    to_account: Some(io_governance_types::NnsAccountIdentifier {
                        hash: destination,
                    }),
                    amount: None,
                },
            )),
            id: None,
        },
    );
    match response.command {
        Some(NnsManageNeuronResponseCommandRecord::Disburse(response)) => {
            Ok(response.transfer_block_height)
        }
        Some(NnsManageNeuronResponseCommandRecord::Error(error)) => Err(error),
        other => panic!("unexpected Disburse response: {other:?}"),
    }
}

fn now_seconds(fixture: &CandidateFixture) -> u64 {
    fixture.pic.get_time().as_nanos_since_unix_epoch() / 1_000_000_000
}

fn icrc_balance(
    fixture: &CandidateFixture,
    owner: Principal,
    subaccount: Option<[u8; 32]>,
) -> u128 {
    let balance: Nat = query(
        fixture,
        fixture.ledger,
        Principal::anonymous(),
        "icrc1_balance_of",
        icrc::account(owner, subaccount),
    );
    u128::try_from(balance.0).unwrap()
}

fn find_mint(fixture: &CandidateFixture, destination: &[u8]) -> (u64, u64) {
    let response: QueryBlocksResponse = query(
        fixture,
        fixture.ledger,
        Principal::anonymous(),
        "query_blocks",
        GetBlocksArgs {
            start: 0,
            length: 2_000,
        },
    );
    response
        .blocks
        .into_iter()
        .enumerate()
        .rev()
        .find_map(|(offset, block)| match block.transaction.operation {
            Some(IcpOperation::Mint { to, amount }) if to == destination => Some((
                response.first_block_index + u64::try_from(offset).unwrap(),
                amount.e8s,
            )),
            _ => None,
        })
        .expect("canonical maturity Mint should be present in the ICP ledger")
}

fn ledger_chain_length(fixture: &CandidateFixture) -> u64 {
    let response: QueryBlocksResponse = query(
        fixture,
        fixture.ledger,
        Principal::anonymous(),
        "query_blocks",
        GetBlocksArgs {
            start: 0,
            length: 0,
        },
    );
    response.chain_length
}

fn lock_value<'a>(text: &'a str, section: &str, key: &str) -> &'a str {
    let mut active_section = "";
    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            active_section = &line[1..line.len() - 1];
            continue;
        }
        if active_section != section {
            continue;
        }
        if let Some((candidate_key, value)) = line.split_once('=') {
            if candidate_key.trim() == key {
                return value.trim().trim_matches('"');
            }
        }
    }
    panic!("candidate lock is missing [{section}] {key}");
}

fn assert_lower_hex(value: &str, expected_length: usize) {
    assert_eq!(value.len(), expected_length);
    assert!(value
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)));
}

#[test]
fn post_mission70_candidate_lock_is_self_consistent() {
    let lock = include_str!("../post_mission70_nns_candidate.toml");
    io_sns_manifest::SnsManifest::parse(lock).expect("candidate lock should be valid TOML");
    assert_eq!(lock_value(lock, "candidate", "status"), "active_boundary");
    assert_eq!(
        lock_value(lock, "candidate", "production_active_pin"),
        "true"
    );
    let source = lock_value(lock, "candidate", "source_commit");
    assert_lower_hex(source, 40);
    assert_eq!(source, CANDIDATE_COMMIT);
    assert_eq!(
        lock_value(lock, "candidate", "proposal_id")
            .parse::<u64>()
            .unwrap(),
        CANDIDATE_PROPOSAL_ID
    );
    assert_eq!(
        lock_value(lock, "candidate", "voting_threshold_seconds")
            .parse::<u64>()
            .unwrap(),
        FOURTEEN_DAYS_SECONDS
    );
    for (key, expected) in [
        ("compressed_sha256", CANDIDATE_COMPRESSED_SHA256),
        ("raw_sha256", CANDIDATE_RAW_SHA256),
        ("did_sha256", CANDIDATE_DID_SHA256),
    ] {
        let value = lock_value(lock, "governance_artifact", key);
        assert_lower_hex(value, 64);
        assert_eq!(value, expected);
    }

    let evidence = include_str!("../../../docs/testing/post-mission70-nns-candidate.md");
    for expected in [
        CANDIDATE_COMMIT,
        CANDIDATE_COMPRESSED_SHA256,
        CANDIDATE_RAW_SHA256,
        CANDIDATE_DID_SHA256,
        "143577",
        "exact_post_m70_upgrade_rewards_fourteen_day_boundary",
        "exact_post_m70_fourteen_day_parent_follows_and_earns_maturity",
        "exact_post_m70_minimum_stake_boundaries",
    ] {
        assert!(
            evidence.contains(expected),
            "candidate evidence is missing {expected}"
        );
    }
    for (key, evidence_fact) in [
        (
            "split_and_start_dissolving_are_distinct",
            "split and `StartDissolving` are distinct",
        ),
        (
            "child_maturity_accrues_while_dissolving",
            "accrues additional\nordinary reward maturity while dissolving",
        ),
        (
            "staked_maturity_converts_at_dissolution",
            "staked maturity\nconverts to ordinary maturity at dissolution",
        ),
        (
            "zero_principal_child_maturity_merge",
            "merge that moves all child maturity\nto the pooled parent",
        ),
        (
            "zero_principal_cleanup_has_no_ledger_fee",
            "creates\nno ICP ledger block, and charges no ICP fee",
        ),
    ] {
        assert_eq!(lock_value(lock, "proved_mechanics", key), "true");
        assert!(
            evidence.contains(evidence_fact),
            "candidate evidence and lock disagree on {key}"
        );
    }
    for obsolete in [
        "Later reward accrual while a child is dissolving.",
        "The final IO child-maturity policy.",
    ] {
        assert!(
            !evidence.contains(obsolete),
            "candidate evidence retains obsolete unproved claim: {obsolete}"
        );
    }

    let active_boundary = [
        include_str!("../../../tools/xtask/src/main.rs"),
        include_str!("../../../crates/io_nns_types/src/jupiter.rs"),
        include_str!("../wasms.example.toml"),
        include_str!("../../../docs/testing/nns-boundary-pin.md"),
    ]
    .join("\n");
    for required in [
        CANDIDATE_COMMIT,
        CANDIDATE_COMPRESSED_SHA256,
        CANDIDATE_RAW_SHA256,
        CANDIDATE_DID_SHA256,
    ] {
        assert!(
            active_boundary.contains(required),
            "active boundary is missing candidate identity {required}"
        );
    }
}

#[test]
#[ignore = "requires exact old/candidate NNS Governance, ICP ledger, test-only actor/XRC Wasms, and POCKET_IC_BIN"]
fn exact_post_m70_upgrade_rewards_fourteen_day_boundary() {
    let _guard = crate::lock_test_env();
    let fixture = CandidateFixture::old_io_boundary();
    assert_eq!(
        network_voting_threshold(&fixture).unwrap_or(PROPOSAL_SUBMISSION_THRESHOLD_SECONDS),
        PROPOSAL_SUBMISSION_THRESHOLD_SECONDS
    );

    let exact = create_neuron(
        &fixture,
        fixture.manager,
        70_001,
        EXACT_PARENT_STAKE_E8S,
        u32::try_from(FOURTEEN_DAYS_SECONDS).unwrap(),
    );
    let below = create_neuron(
        &fixture,
        fixture.manager,
        70_002,
        500_000_000,
        u32::try_from(BELOW_FOURTEEN_DAYS_SECONDS).unwrap(),
    );
    let proposer = create_neuron(
        &fixture,
        Principal::anonymous(),
        70_003,
        1_000 * 100_000_000,
        ONE_YEAR_SECONDS,
    );

    fixture.upgrade_to_candidate();
    assert_eq!(
        network_voting_threshold(&fixture),
        Some(FOURTEEN_DAYS_SECONDS)
    );
    fixture.pic.advance_time(Duration::from_secs(121 * 86_400));
    for _ in 0..100 {
        fixture.pic.tick();
    }
    let modulation = await_maturity_modulation(&fixture);
    assert_eq!(modulation.current_value_permyriad, Some(0));
    let xrc_timestamps: Vec<u64> = query(
        &fixture,
        fixture.xrc,
        Principal::anonymous(),
        "observed_timestamps",
        (),
    );
    assert!(xrc_timestamps.len() >= 365);
    assert!(xrc_timestamps
        .iter()
        .all(|timestamp| timestamp.is_multiple_of(86_400)));

    for (caller, neuron_id) in [
        (fixture.manager, exact),
        (fixture.manager, below),
        (Principal::anonymous(), proposer),
    ] {
        refresh_voting_power(&fixture, caller, neuron_id);
    }
    let maturity_before = full_neuron(&fixture, fixture.manager, exact)
        .unwrap()
        .maturity_e8s_equivalent;
    let proposal_id = make_motion(&fixture, proposer);
    let initial_proposal = proposal_info(&fixture, fixture.manager, proposal_id);
    eprintln!(
        "post_m70_ballot_diagnostic exact_state={:?} below_state={:?} ballots={:?}",
        full_neuron(&fixture, fixture.manager, exact)
            .unwrap()
            .dissolve_state,
        full_neuron(&fixture, fixture.manager, below)
            .unwrap()
            .dissolve_state,
        initial_proposal.ballots,
    );
    assert!(initial_proposal
        .ballots
        .iter()
        .any(|(neuron_id, ballot)| *neuron_id == exact && ballot.voting_power > 0));
    assert!(!initial_proposal
        .ballots
        .iter()
        .any(|(neuron_id, _)| *neuron_id == below));
    register_yes_vote(&fixture, exact, proposal_id);
    let voted = proposal_info(&fixture, fixture.manager, proposal_id);
    assert!(voted
        .ballots
        .iter()
        .any(|(neuron_id, ballot)| *neuron_id == exact && ballot.vote == 1));
    let (reward_event, ordinary_maturity_e8s) =
        await_reward(&fixture, exact, proposal_id, maturity_before);
    let reward_round = proposal_info(&fixture, fixture.manager, proposal_id).reward_event_round;
    assert_eq!(reward_round, reward_event.day_after_genesis);
    assert!(ordinary_maturity_e8s > maturity_before);
    let maturity_after = full_neuron(&fixture, fixture.manager, exact)
        .unwrap()
        .maturity_e8s_equivalent;
    assert_eq!(maturity_after, ordinary_maturity_e8s);

    let parent_before_top_up = full_neuron(&fixture, fixture.manager, exact)
        .unwrap()
        .cached_neuron_stake_e8s;
    transfer_from_mint(
        &fixture,
        IcpAccount::new(fixture.stream, None)
            .icp_account_identifier_bytes()
            .to_vec(),
        TOP_UP_E8S + ICP_FEE_E8S,
        70_004,
    );
    let top_up_block = actor_transfer(
        &fixture,
        fixture.stream,
        None,
        IcpAccount::new(
            fixture.governance,
            Some(Subaccount(
                full_neuron(&fixture, fixture.manager, exact)
                    .unwrap()
                    .account
                    .try_into()
                    .unwrap(),
            )),
        )
        .icp_account_identifier_bytes()
        .to_vec(),
        TOP_UP_E8S,
        70_005,
    )
    .unwrap();
    manager_refresh(&fixture, exact);
    let parent_after_top_up = full_neuron(&fixture, fixture.manager, exact)
        .unwrap()
        .cached_neuron_stake_e8s;
    assert_eq!(parent_after_top_up - parent_before_top_up, TOP_UP_E8S);

    let staked = stake_maturity(&fixture, exact, 40);
    assert!(staked.maturity_e8s > 0);
    assert!(staked.staked_maturity_e8s > 0);
    let parent_before_splits = full_neuron(&fixture, fixture.manager, exact).unwrap();
    assert_eq!(
        parent_before_splits.maturity_e8s_equivalent,
        staked.maturity_e8s
    );
    assert_eq!(
        parent_before_splits
            .staked_maturity_e8s_equivalent
            .unwrap_or(0),
        staked.staked_maturity_e8s
    );

    let merge_child_id = split(&fixture, exact, CHILD_GROSS_E8S).unwrap();
    let selected_split_submission_timestamp = now_seconds(&fixture);
    let disburse_child_id = split(&fixture, exact, CHILD_GROSS_E8S).unwrap();
    let continuing_child_id = split(&fixture, exact, CHILD_GROSS_E8S).unwrap();
    let children = [merge_child_id, disburse_child_id, continuing_child_id];
    assert_eq!(
        children
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        3
    );
    let child_records: Vec<NnsNeuronRecord> = children
        .iter()
        .map(|child| full_neuron(&fixture, fixture.manager, *child).unwrap())
        .collect();
    for child in &child_records {
        assert_eq!(child.cached_neuron_stake_e8s, CHILD_CREDITED_E8S);
        assert_eq!(
            child.dissolve_state,
            Some(NnsDissolveStateRecord::DissolveDelaySeconds(
                FOURTEEN_DAYS_SECONDS
            ))
        );
    }
    let parent_after_splits = full_neuron(&fixture, fixture.manager, exact).unwrap();
    assert_eq!(
        parent_after_splits.maturity_e8s_equivalent
            + child_records
                .iter()
                .map(|child| child.maturity_e8s_equivalent)
                .sum::<u64>(),
        parent_before_splits.maturity_e8s_equivalent
    );
    assert_eq!(
        parent_after_splits
            .staked_maturity_e8s_equivalent
            .unwrap_or(0)
            + child_records
                .iter()
                .map(|child| child.staked_maturity_e8s_equivalent.unwrap_or(0))
                .sum::<u64>(),
        parent_before_splits
            .staked_maturity_e8s_equivalent
            .unwrap_or(0)
    );
    let inherited_child_maturity = child_records[1].maturity_e8s_equivalent;
    let inherited_child_staked_maturity =
        child_records[1].staked_maturity_e8s_equivalent.unwrap_or(0);
    assert!(inherited_child_maturity > 0);
    assert!(inherited_child_staked_maturity > 0);

    let merge_child_started_at = now_seconds(&fixture);
    configure(
        &fixture,
        fixture.manager,
        merge_child_id,
        NnsProductionConfigureOperation::StartDissolving(EmptyRecord {}),
    );
    configure(
        &fixture,
        fixture.manager,
        merge_child_id,
        NnsProductionConfigureOperation::StopDissolving(EmptyRecord {}),
    );
    let parent_before_merge = full_neuron(&fixture, fixture.manager, exact)
        .unwrap()
        .cached_neuron_stake_e8s;
    let merged = merge(&fixture, exact, merge_child_id);
    assert_eq!(
        merged
            .source_neuron
            .expect("merge response should include the emptied source")
            .cached_neuron_stake_e8s,
        0
    );
    let parent_after_merge = full_neuron(&fixture, fixture.manager, exact)
        .unwrap()
        .cached_neuron_stake_e8s;
    assert_eq!(
        parent_after_merge - parent_before_merge,
        CHILD_CREDITED_E8S - ICP_FEE_E8S
    );
    assert_eq!(
        full_neuron(&fixture, fixture.manager, merge_child_id)
            .unwrap()
            .cached_neuron_stake_e8s,
        0
    );

    refresh_voting_power(&fixture, fixture.manager, disburse_child_id);
    let child_reward_proposal = make_motion(&fixture, proposer);
    let child_ballot = proposal_info(&fixture, fixture.manager, child_reward_proposal);
    assert!(child_ballot
        .ballots
        .iter()
        .any(|(neuron_id, ballot)| { *neuron_id == disburse_child_id && ballot.voting_power > 0 }));
    register_yes_vote(&fixture, disburse_child_id, child_reward_proposal);

    fixture.pic.advance_time(Duration::from_secs(37));
    let disburse_child_started_at = now_seconds(&fixture);
    assert!(disburse_child_started_at > selected_split_submission_timestamp);
    configure(
        &fixture,
        fixture.manager,
        disburse_child_id,
        NnsProductionConfigureOperation::StartDissolving(EmptyRecord {}),
    );
    fixture.pic.advance_time(Duration::from_secs(1));
    let continuing_child_started_at = now_seconds(&fixture);
    configure(
        &fixture,
        fixture.manager,
        continuing_child_id,
        NnsProductionConfigureOperation::StartDissolving(EmptyRecord {}),
    );
    let readiness = match full_neuron(&fixture, fixture.manager, disburse_child_id)
        .unwrap()
        .dissolve_state
        .unwrap()
    {
        NnsDissolveStateRecord::WhenDissolvedTimestampSeconds(timestamp) => timestamp,
        other => panic!("child should be dissolving: {other:?}"),
    };
    assert_eq!(readiness, disburse_child_started_at + FOURTEEN_DAYS_SECONDS);
    assert_ne!(
        readiness,
        selected_split_submission_timestamp + FOURTEEN_DAYS_SECONDS,
        "the split timestamp must not start the dissolve clock"
    );
    let canonical_start_effective_timestamp = readiness - FOURTEEN_DAYS_SECONDS;
    assert_eq!(
        canonical_start_effective_timestamp,
        disburse_child_started_at
    );
    let continuing_readiness = match full_neuron(&fixture, fixture.manager, continuing_child_id)
        .unwrap()
        .dissolve_state
        .unwrap()
    {
        NnsDissolveStateRecord::WhenDissolvedTimestampSeconds(timestamp) => timestamp,
        other => panic!("continuing child should be dissolving: {other:?}"),
    };
    assert_eq!(continuing_readiness, readiness + 1);

    let (_, child_maturity_after_reward) = await_reward(
        &fixture,
        disburse_child_id,
        child_reward_proposal,
        inherited_child_maturity,
    );
    assert!(child_maturity_after_reward > inherited_child_maturity);
    assert!(now_seconds(&fixture) < readiness);
    let dissolving_after_reward =
        full_neuron(&fixture, fixture.manager, disburse_child_id).unwrap();
    assert_eq!(
        dissolving_after_reward.dissolve_state,
        Some(NnsDissolveStateRecord::WhenDissolvedTimestampSeconds(
            readiness
        ))
    );
    assert_eq!(
        dissolving_after_reward
            .staked_maturity_e8s_equivalent
            .unwrap_or(0),
        inherited_child_staked_maturity
    );
    fixture
        .pic
        .advance_time(Duration::from_secs(readiness - now_seconds(&fixture) - 1));
    let disbursement_subaccount = [8_u8; 32];
    let disbursement_destination =
        IcpAccount::new(fixture.manager, Some(Subaccount(disbursement_subaccount)))
            .icp_account_identifier_bytes()
            .to_vec();
    let disbursement_balance_before =
        icrc_balance(&fixture, fixture.manager, Some(disbursement_subaccount));
    let one_second_early_error = disburse(
        &fixture,
        disburse_child_id,
        disbursement_destination.clone(),
    )
    .unwrap_err();
    assert!(one_second_early_error
        .error_message
        .to_ascii_lowercase()
        .contains("dissolv"));
    fixture.pic.advance_time(Duration::from_secs(1));
    assert_eq!(now_seconds(&fixture), readiness);
    let disbursement_block =
        disburse(&fixture, disburse_child_id, disbursement_destination).unwrap();
    let disbursement_balance_after =
        icrc_balance(&fixture, fixture.manager, Some(disbursement_subaccount));
    assert_eq!(
        disbursement_balance_after - disbursement_balance_before,
        u128::from(CHILD_CREDITED_E8S - ICP_FEE_E8S)
    );
    let child_after_principal_disbursement =
        full_neuron(&fixture, fixture.manager, disburse_child_id).unwrap();
    assert_eq!(
        child_after_principal_disbursement.cached_neuron_stake_e8s,
        0
    );
    assert_eq!(
        child_after_principal_disbursement.dissolve_state,
        Some(NnsDissolveStateRecord::WhenDissolvedTimestampSeconds(
            readiness
        ))
    );
    assert_eq!(
        child_after_principal_disbursement.maturity_e8s_equivalent
            + child_after_principal_disbursement
                .staked_maturity_e8s_equivalent
                .unwrap_or(0),
        child_maturity_after_reward + inherited_child_staked_maturity
    );

    fixture.pic.advance_time(Duration::from_secs(60));
    for _ in 0..100 {
        fixture.pic.tick();
    }
    let child_before_cleanup = full_neuron(&fixture, fixture.manager, disburse_child_id).unwrap();
    assert_eq!(child_before_cleanup.cached_neuron_stake_e8s, 0);
    assert_eq!(child_before_cleanup.staked_maturity_e8s_equivalent, None);
    assert_eq!(
        child_before_cleanup.maturity_e8s_equivalent,
        child_maturity_after_reward + inherited_child_staked_maturity
    );

    let parent_before_cleanup = full_neuron(&fixture, fixture.manager, exact).unwrap();
    let ledger_before_cleanup = ledger_chain_length(&fixture);
    let mut cleanup_required_stop = false;
    let mut cleanup_required_delay = false;
    let mut cleanup_direct_error = None;
    let mut cleanup_stop_error = None;
    let cleanup_merge = match try_merge(&fixture, exact, disburse_child_id) {
        Ok(response) => response,
        Err(error) => {
            cleanup_direct_error = Some(error.error_message);
            cleanup_required_stop = true;
            let stopped = try_configure(
                &fixture,
                fixture.manager,
                disburse_child_id,
                NnsProductionConfigureOperation::StopDissolving(EmptyRecord {}),
            );
            if let Err(error) = &stopped {
                cleanup_stop_error = Some(error.error_message.clone());
            }
            let merge_after_stop = stopped
                .is_ok()
                .then(|| try_merge(&fixture, exact, disburse_child_id));
            match merge_after_stop {
                Some(Ok(response)) => response,
                Some(Err(error)) => {
                    cleanup_stop_error = Some(error.error_message);
                    cleanup_required_delay = true;
                    configure(
                        &fixture,
                        fixture.manager,
                        disburse_child_id,
                        NnsProductionConfigureOperation::IncreaseDissolveDelay(
                            NnsIncreaseDissolveDelay {
                                additional_dissolve_delay_seconds: 1,
                            },
                        ),
                    );
                    merge(&fixture, exact, disburse_child_id)
                }
                None => {
                    cleanup_required_delay = true;
                    configure(
                        &fixture,
                        fixture.manager,
                        disburse_child_id,
                        NnsProductionConfigureOperation::IncreaseDissolveDelay(
                            NnsIncreaseDissolveDelay {
                                additional_dissolve_delay_seconds: 1,
                            },
                        ),
                    );
                    merge(&fixture, exact, disburse_child_id)
                }
            }
        }
    };
    let parent_after_cleanup = full_neuron(&fixture, fixture.manager, exact).unwrap();
    let child_after_cleanup = full_neuron(&fixture, fixture.manager, disburse_child_id).unwrap();
    assert_eq!(
        parent_after_cleanup.maturity_e8s_equivalent
            - parent_before_cleanup.maturity_e8s_equivalent,
        child_before_cleanup.maturity_e8s_equivalent
    );
    assert_eq!(
        parent_after_cleanup
            .staked_maturity_e8s_equivalent
            .unwrap_or(0),
        parent_before_cleanup
            .staked_maturity_e8s_equivalent
            .unwrap_or(0)
    );
    assert_eq!(child_after_cleanup.cached_neuron_stake_e8s, 0);
    assert_eq!(child_after_cleanup.maturity_e8s_equivalent, 0);
    assert_eq!(child_after_cleanup.staked_maturity_e8s_equivalent, None);
    assert_eq!(ledger_chain_length(&fixture), ledger_before_cleanup);
    assert_eq!(
        cleanup_merge
            .source_neuron
            .expect("cleanup merge should report the emptied child")
            .maturity_e8s_equivalent,
        0
    );

    let continuing = full_neuron(&fixture, fixture.manager, continuing_child_id).unwrap();
    assert_eq!(
        continuing.dissolve_state,
        Some(NnsDissolveStateRecord::WhenDissolvedTimestampSeconds(
            continuing_readiness
        ))
    );

    let staging_subaccount = [7_u8; 32];
    let staging_identifier = IcpAccount::new(fixture.stream, Some(Subaccount(staging_subaccount)))
        .icp_account_identifier_bytes();
    let staging_before = icrc_balance(&fixture, fixture.stream, Some(staging_subaccount));
    let disbursed_maturity = manage(
        &fixture,
        fixture.manager,
        NnsProductionManageNeuronRequest {
            neuron_id_or_subaccount: Some(NnsNeuronIdOrSubaccount::NeuronId(NnsNeuronIdRecord {
                id: exact,
            })),
            command: Some(NnsManageNeuronCommandRequest::DisburseMaturity(
                NnsProductionDisburseMaturity {
                    percentage_to_disburse: 100,
                    to_account: Some(NnsAccount {
                        owner: Some(fixture.stream),
                        subaccount: Some(staging_subaccount.to_vec()),
                    }),
                    to_account_identifier: None,
                },
            )),
            id: None,
        },
    );
    let nominal_maturity_e8s = match disbursed_maturity.command {
        Some(NnsManageNeuronResponseCommandRecord::DisburseMaturity(response)) => response
            .amount_disbursed_e8s
            .expect("candidate should report nominal disbursed maturity"),
        other => panic!("DisburseMaturity failed: {other:?}"),
    };
    let pending = full_neuron(&fixture, fixture.manager, exact).unwrap();
    let pending = pending
        .maturity_disbursements_in_progress
        .unwrap()
        .into_iter()
        .next()
        .expect("ordinary maturity should be scheduled");
    let scheduled_at = pending.timestamp_of_disbursement_seconds.unwrap();
    let finalize_at = pending.finalize_disbursement_timestamp_seconds.unwrap();
    assert_eq!(finalize_at - scheduled_at, MATURITY_FINALIZATION_SECONDS);
    fixture.pic.advance_time(Duration::from_secs(
        finalize_at.saturating_sub(now_seconds(&fixture)),
    ));
    for _ in 0..200 {
        fixture.pic.tick();
    }
    let (mint_block, actual_mint_e8s) = find_mint(&fixture, &staging_identifier);
    assert_eq!(actual_mint_e8s, nominal_maturity_e8s);
    let staging_after = icrc_balance(&fixture, fixture.stream, Some(staging_subaccount));
    assert_eq!(staging_after - staging_before, u128::from(actual_mint_e8s));
    let spend_block = actor_transfer(
        &fixture,
        fixture.stream,
        Some(staging_subaccount.to_vec()),
        IcpAccount::new(fixture.manager, None)
            .icp_account_identifier_bytes()
            .to_vec(),
        actual_mint_e8s - ICP_FEE_E8S,
        70_006,
    )
    .expect("canonical maturity Mint should be spendable");

    eprintln!(
        "post_m70_evidence old_threshold={} new_threshold={} exact_neuron={} below_neuron={} proposer={} proposal={} reward_round={} maturity_before={} maturity_after={} top_up_amount={} top_up_fee={} top_up_block={} cached_before={} cached_after={} parent_ordinary_before_split={} parent_staked_before_split={} children={:?} split_gross={} child_credited={} selected_child={} inherited_child_ordinary={} inherited_child_staked={} merge_child_started_at={} merge_parent_before={} merge_parent_after={} split_submission={} start_effective={} disburse_child_started_at={} continuing_child_started_at={} readiness={} child_reward_proposal={} child_ordinary_after_reward={} early_error={:?} disbursement_block={} zero_principal_ordinary={} converted_ordinary={} cleanup_stop={} cleanup_delay={} cleanup_direct_error={:?} cleanup_stop_error={:?} cleanup_fee={} child_retained_after_cleanup={} nominal_maturity={} finalization_delay={} modulation={} actual_mint={} mint_block={} spend_block={}",
        PROPOSAL_SUBMISSION_THRESHOLD_SECONDS,
        FOURTEEN_DAYS_SECONDS,
        exact,
        below,
        proposer,
        proposal_id,
        reward_round,
        maturity_before,
        maturity_after,
        TOP_UP_E8S,
        ICP_FEE_E8S,
        top_up_block,
        parent_before_top_up,
        parent_after_top_up,
        parent_before_splits.maturity_e8s_equivalent,
        parent_before_splits
            .staked_maturity_e8s_equivalent
            .unwrap_or(0),
        children,
        CHILD_GROSS_E8S,
        CHILD_CREDITED_E8S,
        disburse_child_id,
        inherited_child_maturity,
        inherited_child_staked_maturity,
        merge_child_started_at,
        parent_before_merge,
        parent_after_merge,
        selected_split_submission_timestamp,
        canonical_start_effective_timestamp,
        disburse_child_started_at,
        continuing_child_started_at,
        readiness,
        child_reward_proposal,
        child_maturity_after_reward,
        one_second_early_error,
        disbursement_block,
        child_after_principal_disbursement.maturity_e8s_equivalent,
        child_before_cleanup.maturity_e8s_equivalent,
        cleanup_required_stop,
        cleanup_required_delay,
        cleanup_direct_error,
        cleanup_stop_error,
        0,
        full_neuron(&fixture, fixture.manager, disburse_child_id).is_ok(),
        nominal_maturity_e8s,
        MATURITY_FINALIZATION_SECONDS,
        modulation.current_value_permyriad.unwrap(),
        actual_mint_e8s,
        mint_block,
        spend_block,
    );
}

#[test]
#[ignore = "requires exact old/candidate NNS Governance, ICP ledger, test-only actor/XRC Wasms, and POCKET_IC_BIN"]
fn exact_post_m70_fourteen_day_parent_follows_and_earns_maturity() {
    let _guard = crate::lock_test_env();
    let fixture = CandidateFixture::old_io_boundary();
    let parent = create_neuron(
        &fixture,
        fixture.manager,
        70_101,
        EXACT_PARENT_STAKE_E8S,
        u32::try_from(FOURTEEN_DAYS_SECONDS).unwrap(),
    );
    let leader = create_neuron(
        &fixture,
        fixture.manager,
        70_102,
        EXACT_PARENT_STAKE_E8S,
        ONE_YEAR_SECONDS,
    );
    let proposer = create_neuron(
        &fixture,
        Principal::anonymous(),
        70_103,
        1_000 * 100_000_000,
        ONE_YEAR_SECONDS,
    );

    fixture.upgrade_to_candidate();
    fixture.pic.advance_time(Duration::from_secs(121 * 86_400));
    for _ in 0..100 {
        fixture.pic.tick();
    }
    await_maturity_modulation(&fixture);
    set_following(&fixture, parent, leader);
    for (caller, neuron_id) in [
        (fixture.manager, parent),
        (fixture.manager, leader),
        (Principal::anonymous(), proposer),
    ] {
        refresh_voting_power(&fixture, caller, neuron_id);
    }
    let configured = full_neuron(&fixture, fixture.manager, parent).unwrap();
    for topic in [0, 4, 14] {
        assert!(configured
            .followees
            .iter()
            .any(|(actual_topic, followees)| {
                *actual_topic == topic
                    && followees.followees == vec![NnsNeuronIdRecord { id: leader }]
            }));
    }
    let maturity_before = configured.maturity_e8s_equivalent;
    let proposal_id = make_motion(&fixture, proposer);
    assert_eq!(
        proposal_info(&fixture, fixture.manager, proposal_id)
            .ballots
            .iter()
            .find(|(id, _)| *id == parent)
            .map(|(_, ballot)| ballot.vote),
        Some(0),
        "the pooled parent must not register a manual vote"
    );
    register_yes_vote(&fixture, leader, proposal_id);
    for _ in 0..10 {
        fixture.pic.tick();
    }
    assert_eq!(
        proposal_info(&fixture, fixture.manager, proposal_id)
            .ballots
            .iter()
            .find(|(id, _)| *id == parent)
            .map(|(_, ballot)| ballot.vote),
        Some(1),
        "the pooled parent ballot must follow the configured leader"
    );
    let (_, maturity_after) = await_reward(&fixture, parent, proposal_id, maturity_before);
    assert!(maturity_after > maturity_before);
    refresh_voting_power(&fixture, fixture.manager, parent);
    let refreshed = full_neuron(&fixture, fixture.manager, parent).unwrap();
    assert!(refreshed.followees.iter().any(|(topic, followees)| {
        *topic == 4 && followees.followees == vec![NnsNeuronIdRecord { id: leader }]
    }));
    assert!(refreshed.voting_power_refreshed_timestamp_seconds.is_some());
}

#[test]
#[ignore = "requires exact candidate NNS Governance, ICP ledger, test-only actor/XRC Wasms, and POCKET_IC_BIN"]
fn exact_post_m70_minimum_stake_boundaries() {
    let _guard = crate::lock_test_env();
    let fixture = CandidateFixture::old_io_boundary();
    fixture.upgrade_to_candidate();

    fund_staking_account(&fixture, fixture.manager, 71_001, MINIMUM_STAKE_E8S - 1);
    let below_creation_error = claim_neuron(&fixture, fixture.manager, 71_001).unwrap_err();
    assert!(below_creation_error.error_message.contains("100000000"));

    fund_staking_account(&fixture, fixture.manager, 71_002, MINIMUM_STAKE_E8S);
    let exact_minimum_neuron = claim_neuron(&fixture, fixture.manager, 71_002).unwrap();
    assert_eq!(
        full_neuron(&fixture, fixture.manager, exact_minimum_neuron)
            .unwrap()
            .cached_neuron_stake_e8s,
        MINIMUM_STAKE_E8S
    );

    let exact_gross = MINIMUM_STAKE_E8S + ICP_FEE_E8S;
    transfer_from_mint(
        &fixture,
        IcpAccount::new(fixture.stream, None)
            .icp_account_identifier_bytes()
            .to_vec(),
        exact_gross,
        71_003,
    );
    assert_eq!(
        icrc_balance(&fixture, fixture.stream, None),
        u128::from(exact_gross)
    );
    actor_transfer(
        &fixture,
        fixture.stream,
        None,
        staking_account_identifier(&fixture, fixture.manager, 71_004),
        MINIMUM_STAKE_E8S,
        71_004,
    )
    .unwrap();
    assert_eq!(icrc_balance(&fixture, fixture.stream, None), 0);
    let fee_separate_neuron = claim_neuron(&fixture, fixture.manager, 71_004).unwrap();
    assert_eq!(
        full_neuron(&fixture, fixture.manager, fee_separate_neuron)
            .unwrap()
            .cached_neuron_stake_e8s,
        MINIMUM_STAKE_E8S
    );

    let split_parent = create_neuron(
        &fixture,
        fixture.manager,
        71_005,
        400_020_000,
        u32::try_from(FOURTEEN_DAYS_SECONDS).unwrap(),
    );
    let below_split_error = split(&fixture, split_parent, exact_gross - 1).unwrap_err();
    assert!(below_split_error
        .error_message
        .to_ascii_lowercase()
        .contains("minimum"));
    let exact_child = split(&fixture, split_parent, exact_gross).unwrap();
    assert_eq!(
        full_neuron(&fixture, fixture.manager, exact_child)
            .unwrap()
            .cached_neuron_stake_e8s,
        MINIMUM_STAKE_E8S
    );
    let parent_after_exact = full_neuron(&fixture, fixture.manager, split_parent)
        .unwrap()
        .cached_neuron_stake_e8s;
    assert_eq!(parent_after_exact, 300_010_000);

    let leaves_parent_below_minimum = parent_after_exact - (MINIMUM_STAKE_E8S - 1);
    let parent_minimum_error =
        split(&fixture, split_parent, leaves_parent_below_minimum).unwrap_err();
    assert!(parent_minimum_error
        .error_message
        .to_ascii_lowercase()
        .contains("minimum"));
    let complete_split_error = split(&fixture, split_parent, parent_after_exact).unwrap_err();
    assert!(complete_split_error
        .error_message
        .to_ascii_lowercase()
        .contains("minimum"));
    assert_eq!(
        full_neuron(&fixture, fixture.manager, split_parent)
            .unwrap()
            .cached_neuron_stake_e8s,
        parent_after_exact
    );

    eprintln!(
        "post_m70_minimum_evidence below_balance={} below_creation_error={:?} exact_balance={} exact_neuron={} source_gross={} transfer_amount={} transfer_fee={} fee_separate_neuron={} below_split_gross={} below_split_error={:?} exact_split_gross={} exact_child={} exact_child_stake={} parent_after_exact={} retain_attempt={} retain_error={:?} complete_attempt={} complete_error={:?}",
        MINIMUM_STAKE_E8S - 1,
        below_creation_error,
        MINIMUM_STAKE_E8S,
        exact_minimum_neuron,
        exact_gross,
        MINIMUM_STAKE_E8S,
        ICP_FEE_E8S,
        fee_separate_neuron,
        exact_gross - 1,
        below_split_error,
        exact_gross,
        exact_child,
        MINIMUM_STAKE_E8S,
        parent_after_exact,
        leaves_parent_below_minimum,
        parent_minimum_error,
        parent_after_exact,
        complete_split_error,
    );
}
