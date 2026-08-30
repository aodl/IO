export const idlFactory = ({ IDL }) => {
  const Account = IDL.Record({
    'owner' : IDL.Principal,
    'subaccount' : IDL.Opt(IDL.Vec(IDL.Nat8)),
  });
  const StreamConfig = IDL.Record({
    'ledger_deduplication_window_nanos' : IDL.Nat64,
    'nns_manager' : IDL.Principal,
    'expected_sns_governance_module_hash' : IDL.Vec(IDL.Nat8),
    'maximum_request_lifetime_nanos' : IDL.Nat64,
    'expected_io_fee_e8s' : IDL.Nat,
    'io_ledger' : IDL.Principal,
    'icp_ledger' : IDL.Principal,
    'nonredeemable_governance_io_accounts' : IDL.Vec(Account),
    'sns_root' : IDL.Principal,
    'expected_icp_fee_e8s' : IDL.Nat,
    'sns_governance' : IDL.Principal,
    'retry_delay_nanos' : IDL.Nat64,
    'liquid_icp' : Account,
    'jupiter_io_account' : Account,
    'minimum_redemption_io_e8s' : IDL.Nat,
    'approved_reward_event_duration_seconds' : IDL.Nat64,
    'io_reserve' : Account,
  });
  const InitArgs = IDL.Record({ 'config' : StreamConfig });
  const FrozenRedemptionEconomics = IDL.Record({
    'icp_fee_e8s' : IDL.Nat,
    'total_supply_e8s' : IDL.Nat,
    'claim_supply_e8s' : IDL.Nat,
    'excluded_io_balances' : IDL.Vec(IDL.Tuple(Account, IDL.Nat)),
    'total_claim_backing_e8s' : IDL.Nat,
    'liquid_icp_e8s' : IDL.Nat,
    'observation_fingerprint' : IDL.Vec(IDL.Nat8),
    'reserve_io_e8s' : IDL.Nat,
    'io_fee_e8s' : IDL.Nat,
  });
  const CanonicalRedeemRequestV1 = IDL.Record({
    'effective_subaccount' : IDL.Vec(IDL.Nat8),
    'expires_at_nanos' : IDL.Nat64,
    'min_icp_out_e8s' : IDL.Nat,
    'max_icp_fee_e8s' : IDL.Nat,
    'io_amount_e8s' : IDL.Nat,
    'nonce' : IDL.Nat64,
    'max_io_fee_e8s' : IDL.Nat,
  });
  const PreparedRedemption = IDL.Record({
    'snapshot' : FrozenRedemptionEconomics,
    'request_fingerprint' : IDL.Vec(IDL.Nat8),
    'request' : CanonicalRedeemRequestV1,
    'reserve' : Account,
    'net_icp_e8s' : IDL.Nat,
    'account' : Account,
    'push_memo' : IDL.Vec(IDL.Nat8),
    'caller' : IDL.Principal,
    'gross_icp_e8s' : IDL.Nat,
    'prepared_at_nanos' : IDL.Nat64,
  });
  const PushedRedemption = IDL.Record({
    'io_block' : IDL.Nat,
    'transfer_created_at_nanos' : IDL.Nat64,
    'prepared' : PreparedRedemption,
  });
  const CallerRedemptionPending = IDL.Variant({
    'Prepared' : PreparedRedemption,
    'Pushed' : PushedRedemption,
  });
  const RedemptionResult = IDL.Record({
    'icp_fee_e8s' : IDL.Nat,
    'io_block' : IDL.Nat,
    'request_fingerprint' : IDL.Vec(IDL.Nat8),
    'net_icp_e8s' : IDL.Nat,
    'icp_block' : IDL.Nat,
    'nonce' : IDL.Nat64,
    'completed_at_nanos' : IDL.Nat64,
    'io_fee_e8s' : IDL.Nat,
    'gross_icp_e8s' : IDL.Nat,
  });
  const CallerRedemptionState = IDL.Record({
    'next_nonce' : IDL.Nat64,
    'pending' : IDL.Opt(CallerRedemptionPending),
    'last_result' : IDL.Opt(RedemptionResult),
    'last_request_fingerprint' : IDL.Opt(IDL.Vec(IDL.Nat8)),
  });
  const ApiError = IDL.Variant({
    'Invalid' : IDL.Text,
    'Stuck' : IDL.Text,
    'Anonymous' : IDL.Null,
    'Paused' : IDL.Null,
    'Busy' : IDL.Null,
    'WrongNonce' : IDL.Record({ 'expected' : IDL.Nat64 }),
    'Unauthorized' : IDL.Null,
    'Ledger' : IDL.Text,
    'NonceAlreadyUsed' : IDL.Null,
    'Pending' : IDL.Text,
  });
  const FrozenEntitlement = IDL.Record({
    'destination' : Account,
    'accumulated_eligible_credit' : IDL.Nat,
    'sns_neuron_id' : IDL.Vec(IDL.Nat8),
  });
  const ReconciliationCheckpoint = IDL.Record({
    'pooled_target_e8s' : IDL.Nat,
    'unwinding_backing_e8s' : IDL.Nat,
    'claim_supply_e8s' : IDL.Nat,
    'liquid_backing_e8s' : IDL.Nat,
    'active_reward_io_e8s' : IDL.Nat,
    'total_claim_backing_e8s' : IDL.Nat,
    'generation' : IDL.Nat64,
    'observed_at_nanos' : IDL.Nat64,
    'snapshot_fingerprint' : IDL.Vec(IDL.Nat8),
    'transit_backing_e8s' : IDL.Nat,
    'pooled_backing_e8s' : IDL.Nat,
    'live_cohort_count' : IDL.Nat32,
    'oldest_ready_at_seconds' : IDL.Opt(IDL.Nat64),
    'active_backing_io_e8s' : IDL.Nat,
    'observed_pooled_e8s' : IDL.Nat,
    'event_marker' : IDL.Nat64,
  });
  const RewardEventId = IDL.Record({
    'end_timestamp_seconds' : IDL.Nat64,
    'round' : IDL.Nat64,
  });
  const Lifecycle = IDL.Variant({ 'Paused' : IDL.Null, 'Ready' : IDL.Null });
  const RewardEventClassification = IDL.Variant({
    'NoProposalFallback' : IDL.Null,
    'ZeroEligibleParticipation' : IDL.Null,
    'StructuralOnly' : IDL.Null,
    'MissedSkipped' : IDL.Null,
    'ProposalBearing' : IDL.Null,
  });
  const Status = IDL.Record({
    'accumulated_policy_credit' : IDL.Nat,
    'accumulated_entitlements' : IDL.Vec(FrozenEntitlement),
    'prepared_exit_generation' : IDL.Opt(IDL.Nat64),
    'prepared_exit_member_count' : IDL.Nat32,
    'operation_kind' : IDL.Opt(IDL.Text),
    'processed_reward_event_count' : IDL.Nat64,
    'pending_entitlement_batch_policy_credit' : IDL.Opt(IDL.Nat),
    'reward_work_due' : IDL.Bool,
    'operation_phase' : IDL.Opt(IDL.Text),
    'latest_reconciliation_checkpoint' : IDL.Opt(ReconciliationCheckpoint),
    'governance_parameters_fresh' : IDL.Bool,
    'latest_processed_reward_event' : IDL.Opt(RewardEventId),
    'lifecycle' : Lifecycle,
    'accumulated_eligible_credit' : IDL.Nat,
    'pending_entitlement_batch_eligible_credit' : IDL.Opt(IDL.Nat),
    'next_operation_sequence' : IDL.Nat64,
    'latest_reward_event_classification' : IDL.Opt(RewardEventClassification),
    'committed_exit_member_count' : IDL.Nat32,
    'missed_reward_event_count' : IDL.Nat64,
    'latest_entitlement_batch_generation' : IDL.Nat64,
    'reward_processing_paused' : IDL.Bool,
  });
  const ClaimBackingReceiptKind = IDL.Variant({
    'TwoWeek' : IDL.Record({ 'entitlement_generation' : IDL.Nat64 }),
    'Jupiter' : IDL.Null,
  });
  const PrepareClaimBackingReceiptArgs = IDL.Record({
    'kind' : ClaimBackingReceiptKind,
    'net_liquid_credit_e8s' : IDL.Nat,
    'nns_operation_sequence' : IDL.Nat64,
  });
  const ClaimBackingReceiptPermit = IDL.Record({
    'destination' : Account,
    'memo' : IDL.Vec(IDL.Nat8),
    'amount_e8s' : IDL.Nat,
    'stream_operation_sequence' : IDL.Nat64,
  });
  const RedeemArgs = IDL.Record({
    'expires_at_nanos' : IDL.Nat64,
    'min_icp_out_e8s' : IDL.Nat,
    'max_icp_fee_e8s' : IDL.Nat,
    'from_subaccount' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    'io_amount_e8s' : IDL.Nat,
    'nonce' : IDL.Nat64,
    'max_io_fee_e8s' : IDL.Nat,
  });
  const ProveClaimBackingReceiptArgs = IDL.Record({
    'block_index' : IDL.Nat,
    'stream_operation_sequence' : IDL.Nat64,
  });
  const ClaimBackingReceiptResult = IDL.Record({
    'kind' : ClaimBackingReceiptKind,
    'distributed_io_e8s' : IDL.Nat,
    'completed_at_nanos' : IDL.Nat64,
    'recipient_transfer_block' : IDL.Opt(IDL.Nat),
    'nns_operation_sequence' : IDL.Nat64,
    'io_fee_e8s' : IDL.Nat,
    'liquid_credit_e8s' : IDL.Nat,
  });
  const ClaimBackingReceiptProgress = IDL.Variant({
    'Stuck' : IDL.Text,
    'AwaitingLiquidProof' : ClaimBackingReceiptPermit,
    'Completed' : ClaimBackingReceiptResult,
    'Pending' : IDL.Null,
  });
  const RedemptionProgress = IDL.Variant({
    'Stuck' : IDL.Text,
    'Completed' : RedemptionResult,
    'Pending' : IDL.Null,
  });
  const StreamProgress = IDL.Variant({
    'BackingReconciliation' : IDL.Null,
    'Idle' : IDL.Null,
    'ClaimReceipt' : ClaimBackingReceiptProgress,
    'Redemption' : RedemptionProgress,
  });
  const BackingNotReadyReason = IDL.Variant({
    'Paused' : IDL.Null,
    'Busy' : IDL.Null,
    'BelowThreshold' : IDL.Null,
    'ReconciliationPending' : IDL.Null,
  });
  const RewardBackingProgress = IDL.Variant({
    'MaturityPrepared' : IDL.Record({ 'generation' : IDL.Nat64 }),
    'Pending' : IDL.Record({ 'reason' : BackingNotReadyReason }),
  });
  const RewardEventObservation = IDL.Record({
    'eligible_credit_total' : IDL.Nat,
    'observed_at_nanos' : IDL.Nat64,
    'event' : RewardEventId,
    'proposal_count' : IDL.Nat64,
    'policy_credit' : IDL.Nat,
    'classification' : RewardEventClassification,
  });
  return IDL.Service({
    'get_caller_redemption_state' : IDL.Func(
        [],
        [IDL.Variant({ 'Ok' : CallerRedemptionState, 'Err' : ApiError })],
        ['query'],
      ),
    'get_status' : IDL.Func([], [Status], ['query']),
    'prepare_claim_backing_receipt' : IDL.Func(
        [PrepareClaimBackingReceiptArgs],
        [IDL.Variant({ 'Ok' : ClaimBackingReceiptPermit, 'Err' : ApiError })],
        [],
      ),
    'prepare_redemption' : IDL.Func(
        [RedeemArgs],
        [IDL.Variant({ 'Ok' : PreparedRedemption, 'Err' : ApiError })],
        [],
      ),
    'prove_active_transfer' : IDL.Func(
        [IDL.Nat],
        [IDL.Variant({ 'Ok' : IDL.Null, 'Err' : ApiError })],
        [],
      ),
    'prove_claim_backing_receipt' : IDL.Func(
        [ProveClaimBackingReceiptArgs],
        [IDL.Variant({ 'Ok' : ClaimBackingReceiptProgress, 'Err' : ApiError })],
        [],
      ),
    'resume' : IDL.Func(
        [],
        [IDL.Variant({ 'Ok' : StreamProgress, 'Err' : ApiError })],
        [],
      ),
    'resume_redemption' : IDL.Func(
        [IDL.Principal],
        [IDL.Variant({ 'Ok' : RedemptionProgress, 'Err' : ApiError })],
        [],
      ),
    'resume_reward_backing' : IDL.Func(
        [],
        [IDL.Variant({ 'Ok' : RewardBackingProgress, 'Err' : ApiError })],
        [],
      ),
    'resume_reward_work' : IDL.Func(
        [],
        [IDL.Variant({ 'Ok' : RewardEventObservation, 'Err' : ApiError })],
        [],
      ),
    'set_paused' : IDL.Func(
        [IDL.Bool],
        [IDL.Variant({ 'Ok' : IDL.Null, 'Err' : ApiError })],
        [],
      ),
    'settle_redemption' : IDL.Func(
        [IDL.Nat],
        [IDL.Variant({ 'Ok' : RedemptionProgress, 'Err' : ApiError })],
        [],
      ),
    'validate_set_paused' : IDL.Func(
        [IDL.Bool],
        [IDL.Variant({ 'Ok' : IDL.Text, 'Err' : IDL.Text })],
        ['query'],
      ),
  });
};
export const init = ({ IDL }) => {
  const Account = IDL.Record({
    'owner' : IDL.Principal,
    'subaccount' : IDL.Opt(IDL.Vec(IDL.Nat8)),
  });
  const StreamConfig = IDL.Record({
    'ledger_deduplication_window_nanos' : IDL.Nat64,
    'nns_manager' : IDL.Principal,
    'expected_sns_governance_module_hash' : IDL.Vec(IDL.Nat8),
    'maximum_request_lifetime_nanos' : IDL.Nat64,
    'expected_io_fee_e8s' : IDL.Nat,
    'io_ledger' : IDL.Principal,
    'icp_ledger' : IDL.Principal,
    'nonredeemable_governance_io_accounts' : IDL.Vec(Account),
    'sns_root' : IDL.Principal,
    'expected_icp_fee_e8s' : IDL.Nat,
    'sns_governance' : IDL.Principal,
    'retry_delay_nanos' : IDL.Nat64,
    'liquid_icp' : Account,
    'jupiter_io_account' : Account,
    'minimum_redemption_io_e8s' : IDL.Nat,
    'approved_reward_event_duration_seconds' : IDL.Nat64,
    'io_reserve' : Account,
  });
  const InitArgs = IDL.Record({ 'config' : StreamConfig });
  return [InitArgs];
};
