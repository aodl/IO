export const idlFactory = ({ IDL }) => {
  const Account = IDL.Record({
    'owner' : IDL.Principal,
    'subaccount' : IDL.Opt(IDL.Vec(IDL.Nat8)),
  });
  const CanisterRole = IDL.Variant({
    'Historian' : IDL.Null,
    'Frontend' : IDL.Null,
    'SnsLedger' : IDL.Null,
    'SnsGovernance' : IDL.Null,
    'SnsIndex' : IDL.Null,
    'NnsManager' : IDL.Null,
    'StreamManager' : IDL.Null,
    'SnsRoot' : IDL.Null,
    'SnsSwap' : IDL.Null,
  });
  const ExpectedModule = IDL.Record({
    'role' : CanisterRole,
    'canister_id' : IDL.Principal,
    'wasm_sha256' : IDL.Vec(IDL.Nat8),
  });
  const NamedAccount = IDL.Record({ 'name' : IDL.Text, 'account' : Account });
  const ObservationConfig = IDL.Record({
    'two_year_neuron_id' : IDL.Nat64,
    'protocol_io_reserve' : Account,
    'nns_manager' : IDL.Principal,
    'liquid_icp_reserve' : Account,
    'stream_manager' : IDL.Principal,
    'reward_share_capable_governance_sha256' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    'expected_modules' : IDL.Vec(ExpectedModule),
    'icp_ledger' : IDL.Principal,
    'sns_root' : IDL.Principal,
    'sns_governance' : IDL.Principal,
    'history_accounts' : IDL.Vec(NamedAccount),
    'sns_ledger' : IDL.Principal,
    'sns_index' : IDL.Principal,
    'nns_governance' : IDL.Principal,
    'excluded_io_accounts' : IDL.Vec(NamedAccount),
    'reward_backing_neuron_id' : IDL.Nat64,
    'refresh_interval_seconds' : IDL.Nat64,
  });
  const CapabilityState = IDL.Variant({
    'ExpectedGovernanceModuleMatching' : IDL.Null,
    'Unavailable' : IDL.Null,
    'ModuleMismatch' : IDL.Null,
  });
  const SnsStatus = IDL.Record({
    'native_initial_reward_rate_basis_points' : IDL.Opt(IDL.Nat64),
    'settled_proposal_count' : IDL.Opt(IDL.Nat64),
    'native_final_reward_rate_basis_points' : IDL.Opt(IDL.Nat64),
    'reward_share_capability' : CapabilityState,
    'max_number_of_neurons' : IDL.Opt(IDL.Nat64),
    'observed_at_timestamp_nanos' : IDL.Nat64,
    'latest_reward_event_end_timestamp_seconds' : IDL.Opt(IDL.Nat64),
    'archive_canisters' : IDL.Vec(IDL.Principal),
    'latest_reward_event_round' : IDL.Opt(IDL.Nat64),
  });
  const RedemptionRateSnapshot = IDL.Record({
    'liquid_icp_e8s' : IDL.Nat,
    'observed_at_timestamp_nanos' : IDL.Nat64,
    'redeemable_io_e8s' : IDL.Nat,
  });
  const DataCompleteness = IDL.Record({
    'liquid_icp_reserve' : IDL.Bool,
    'excluded_io' : IDL.Bool,
    'redeemable_io_supply' : IDL.Bool,
    'redemption_rate' : IDL.Bool,
    'total_io_supply' : IDL.Bool,
    'protocol_reserve_io' : IDL.Bool,
  });
  const ProtocolSnapshot = IDL.Record({
    'total_io_supply_e8s' : IDL.Opt(IDL.Nat),
    'liquid_icp_reserve_e8s' : IDL.Opt(IDL.Nat),
    'redeemable_io_supply_e8s' : IDL.Opt(IDL.Nat),
    'generation' : IDL.Nat64,
    'redemption_rate' : IDL.Opt(RedemptionRateSnapshot),
    'protocol_reserve_io_e8s' : IDL.Opt(IDL.Nat),
    'observed_at_timestamp_nanos' : IDL.Opt(IDL.Nat64),
    'completeness' : DataCompleteness,
    'excluded_io_e8s' : IDL.Opt(IDL.Nat),
  });
  const PublicStatus = IDL.Record({
    'last_success_timestamp_nanos' : IDL.Opt(IDL.Nat64),
    'refresh_active' : IDL.Bool,
    'last_attempt_timestamp_nanos' : IDL.Opt(IDL.Nat64),
    'version' : IDL.Text,
    'schema_version' : IDL.Nat32,
    'refresh_generation' : IDL.Nat64,
    'configured' : IDL.Bool,
  });
  const TwoWeekTargetStatus = IDL.Variant({
    'AtTarget' : IDL.Null,
    'OverTarget' : IDL.Null,
    'UnderTarget' : IDL.Null,
    'AtTargetWithinUnwindTolerance' : IDL.Null,
  });
  const TwoWeekTargetObservation = IDL.Record({
    'status' : TwoWeekTargetStatus,
    'target_e8s' : IDL.Nat,
  });
  const Lifecycle = IDL.Variant({ 'Paused' : IDL.Null, 'Ready' : IDL.Null });
  const NnsManagerStatus = IDL.Record({
    'active_operation' : IDL.Opt(IDL.Text),
    'latest_two_week_target' : IDL.Opt(TwoWeekTargetObservation),
    'latest_started_two_week_generation' : IDL.Nat64,
    'unwinding_child_principal_e8s' : IDL.Nat,
    'latest_completed_two_week_generation' : IDL.Nat64,
    'observed_at_timestamp_nanos' : IDL.Nat64,
    'lifecycle' : Lifecycle,
    'two_week_maturity_baseline_reconciled' : IDL.Bool,
  });
  const RewardEventId = IDL.Record({
    'end_timestamp_seconds' : IDL.Nat64,
    'round' : IDL.Nat64,
  });
  const RewardEventClassification = IDL.Variant({
    'NoProposalFallback' : IDL.Null,
    'ZeroEligibleParticipation' : IDL.Null,
    'MissedSkipped' : IDL.Null,
    'ProposalBearing' : IDL.Null,
  });
  const StreamStatus = IDL.Record({
    'accumulated_policy_credit' : IDL.Nat,
    'operation_kind' : IDL.Opt(IDL.Text),
    'processed_reward_event_count' : IDL.Nat64,
    'pending_entitlement_batch_policy_credit' : IDL.Opt(IDL.Nat),
    'reward_work_due' : IDL.Bool,
    'operation_phase' : IDL.Opt(IDL.Text),
    'governance_parameters_fresh' : IDL.Bool,
    'observed_at_timestamp_nanos' : IDL.Nat64,
    'latest_processed_reward_event' : IDL.Opt(RewardEventId),
    'lifecycle' : Lifecycle,
    'accumulated_eligible_credit' : IDL.Nat,
    'pending_entitlement_batch_eligible_credit' : IDL.Opt(IDL.Nat),
    'latest_reward_event_classification' : IDL.Opt(RewardEventClassification),
    'missed_reward_event_count' : IDL.Nat64,
    'latest_entitlement_batch_generation' : IDL.Nat64,
    'reward_processing_paused' : IDL.Bool,
  });
  const ModuleMatch = IDL.Variant({
    'Mismatch' : IDL.Null,
    'Matching' : IDL.Null,
    'Unknown' : IDL.Null,
    'Unavailable' : IDL.Null,
  });
  const CanisterObservation = IDL.Record({
    'controllers' : IDL.Opt(IDL.Vec(IDL.Principal)),
    'expected_module_hash' : IDL.Vec(IDL.Nat8),
    'role' : CanisterRole,
    'canister_id' : IDL.Principal,
    'observed_at_timestamp_nanos' : IDL.Nat64,
    'observed_module_hash' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    'module_match' : ModuleMatch,
  });
  const RecentTransaction = IDL.Record({
    'to' : IDL.Opt(Account),
    'timestamp_nanos' : IDL.Nat64,
    'block_index' : IDL.Nat,
    'from' : IDL.Opt(Account),
    'kind' : IDL.Text,
    'amount_e8s' : IDL.Opt(IDL.Nat),
  });
  const AccountHistoryObservation = IDL.Record({
    'index_balance_e8s' : IDL.Nat,
    'name' : IDL.Text,
    'oldest_transaction_id' : IDL.Opt(IDL.Nat),
    'account' : Account,
    'newest_transaction_id' : IDL.Opt(IDL.Nat),
    'transactions' : IDL.Vec(RecentTransaction),
  });
  const IndexStatus = IDL.Record({
    'accounts' : IDL.Vec(AccountHistoryObservation),
    'observed_at_timestamp_nanos' : IDL.Nat64,
    'num_blocks_synced' : IDL.Nat,
  });
  const NnsNeuronRole = IDL.Variant({
    'RewardBacking' : IDL.Null,
    'TwoYearProtected' : IDL.Null,
  });
  const NnsNeuronObservation = IDL.Record({
    'dissolve_delay_seconds' : IDL.Nat64,
    'role' : NnsNeuronRole,
    'staked_maturity_e8s' : IDL.Opt(IDL.Nat64),
    'state' : IDL.Int32,
    'stake_e8s' : IDL.Nat64,
    'neuron_id' : IDL.Nat64,
  });
  const NnsGovernanceStatus = IDL.Record({
    'build_metadata' : IDL.Text,
    'observed_at_timestamp_nanos' : IDL.Nat64,
    'neurons' : IDL.Vec(NnsNeuronObservation),
  });
  const ObservationFreshness = IDL.Variant({
    'Missing' : IDL.Null,
    'Stale' : IDL.Null,
    'Fresh' : IDL.Null,
    'PrelaunchNotConfigured' : IDL.Null,
    'ErrorRetryable' : IDL.Null,
  });
  const SourceHealth = IDL.Record({
    'last_success_timestamp_nanos' : IDL.Opt(IDL.Nat64),
    'freshness' : ObservationFreshness,
    'source' : IDL.Text,
    'last_attempt_timestamp_nanos' : IDL.Opt(IDL.Nat64),
    'error' : IDL.Opt(IDL.Text),
  });
  const Dashboard = IDL.Record({
    'sns' : IDL.Opt(SnsStatus),
    'protocol' : ProtocolSnapshot,
    'status' : PublicStatus,
    'nns_manager' : IDL.Opt(NnsManagerStatus),
    'stream' : IDL.Opt(StreamStatus),
    'canisters' : IDL.Vec(CanisterObservation),
    'index' : IDL.Opt(IndexStatus),
    'nns_governance' : IDL.Opt(NnsGovernanceStatus),
    'source_health' : IDL.Vec(SourceHealth),
  });
  return IDL.Service({
    'get_dashboard_state' : IDL.Func([], [Dashboard], ['query']),
    'get_protocol_snapshot' : IDL.Func([], [ProtocolSnapshot], ['query']),
    'get_public_status' : IDL.Func([], [PublicStatus], ['query']),
    'get_redemption_rate' : IDL.Func(
        [],
        [IDL.Opt(RedemptionRateSnapshot)],
        ['query'],
      ),
    'version' : IDL.Func([], [IDL.Text], ['query']),
  });
};
export const init = ({ IDL }) => {
  const Account = IDL.Record({
    'owner' : IDL.Principal,
    'subaccount' : IDL.Opt(IDL.Vec(IDL.Nat8)),
  });
  const CanisterRole = IDL.Variant({
    'Historian' : IDL.Null,
    'Frontend' : IDL.Null,
    'SnsLedger' : IDL.Null,
    'SnsGovernance' : IDL.Null,
    'SnsIndex' : IDL.Null,
    'NnsManager' : IDL.Null,
    'StreamManager' : IDL.Null,
    'SnsRoot' : IDL.Null,
    'SnsSwap' : IDL.Null,
  });
  const ExpectedModule = IDL.Record({
    'role' : CanisterRole,
    'canister_id' : IDL.Principal,
    'wasm_sha256' : IDL.Vec(IDL.Nat8),
  });
  const NamedAccount = IDL.Record({ 'name' : IDL.Text, 'account' : Account });
  const ObservationConfig = IDL.Record({
    'two_year_neuron_id' : IDL.Nat64,
    'protocol_io_reserve' : Account,
    'nns_manager' : IDL.Principal,
    'liquid_icp_reserve' : Account,
    'stream_manager' : IDL.Principal,
    'reward_share_capable_governance_sha256' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    'expected_modules' : IDL.Vec(ExpectedModule),
    'icp_ledger' : IDL.Principal,
    'sns_root' : IDL.Principal,
    'sns_governance' : IDL.Principal,
    'history_accounts' : IDL.Vec(NamedAccount),
    'sns_ledger' : IDL.Principal,
    'sns_index' : IDL.Principal,
    'nns_governance' : IDL.Principal,
    'excluded_io_accounts' : IDL.Vec(NamedAccount),
    'reward_backing_neuron_id' : IDL.Nat64,
    'refresh_interval_seconds' : IDL.Nat64,
  });
  return [IDL.Opt(ObservationConfig)];
};
