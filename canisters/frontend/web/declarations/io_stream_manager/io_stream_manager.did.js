export const idlFactory = ({ IDL }) => {
  const Account = IDL.Record({
    'owner' : IDL.Principal,
    'subaccount' : IDL.Opt(IDL.Vec(IDL.Nat8)),
  });
  const StreamConfig = IDL.Record({
    'jupiter_receipt_source' : Account,
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
  const CompleteJupiterReceiptArgs = IDL.Record({
    'block_index' : IDL.Nat,
    'receipt_sequence' : IDL.Nat64,
  });
  const JupiterReceiptResult = IDL.Record({
    'io_transfer_block' : IDL.Nat,
    'request_fingerprint' : IDL.Vec(IDL.Nat8),
    'completed_at_nanos' : IDL.Nat64,
    'receipt_block' : IDL.Nat,
    'io_fee_e8s' : IDL.Nat,
    'backed_io_e8s' : IDL.Nat,
  });
  const JupiterReceiptProgress = IDL.Variant({
    'Stuck' : IDL.Text,
    'ReceiptProved' : IDL.Null,
    'Settling' : IDL.Null,
    'AwaitingReceipt' : IDL.Null,
    'Completed' : JupiterReceiptResult,
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
    'LiquidityShortfall' : IDL.Record({
      'net_icp_e8s' : IDL.Nat,
      'available_liquid_e8s' : IDL.Nat,
      'gross_icp_e8s' : IDL.Nat,
    }),
    'NonceAlreadyUsed' : IDL.Null,
    'Pending' : IDL.Text,
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
    'last_result' : IDL.Opt(RedemptionResult),
    'last_request_fingerprint' : IDL.Opt(IDL.Vec(IDL.Nat8)),
  });
  const RewardEntitlementEntry = IDL.Record({
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
    'MissedSkipped' : IDL.Null,
    'ProposalBearing' : IDL.Null,
  });
  const Status = IDL.Record({
    'accumulated_policy_credit' : IDL.Nat,
    'accumulated_entitlements' : IDL.Vec(RewardEntitlementEntry),
    'operation_kind' : IDL.Opt(IDL.Text),
    'processed_reward_event_count' : IDL.Nat64,
    'next_nns_receipt_sequence' : IDL.Nat64,
    'pending_entitlement_batch_policy_credit' : IDL.Opt(IDL.Nat),
    'reward_work_due' : IDL.Bool,
    'operation_phase' : IDL.Opt(IDL.Text),
    'latest_reconciliation_checkpoint' : IDL.Opt(ReconciliationCheckpoint),
    'governance_parameters_fresh' : IDL.Bool,
    'latest_processed_reward_event' : IDL.Opt(RewardEventId),
    'lifecycle' : Lifecycle,
    'accumulated_eligible_credit' : IDL.Nat,
    'pending_entitlement_batch_eligible_credit' : IDL.Opt(IDL.Nat),
    'latest_reward_event_classification' : IDL.Opt(RewardEventClassification),
    'missed_reward_event_count' : IDL.Nat64,
    'latest_entitlement_batch_generation' : IDL.Nat64,
    'reward_processing_paused' : IDL.Bool,
  });
  const BackingInflowKind = IDL.Variant({
    'PooledMaturity' : IDL.Null,
    'PermanentMaturity' : IDL.Null,
  });
  const PrepareBackingInflowArgs = IDL.Record({
    'nns_fingerprint' : IDL.Vec(IDL.Nat8),
    'actual_mint_e8s' : IDL.Nat,
    'staging_account' : Account,
    'claim_transfer_fee_e8s' : IDL.Nat,
    'kind' : BackingInflowKind,
    'source_operation_id' : IDL.Vec(IDL.Nat8),
    'maturity_generation' : IDL.Nat64,
    'mint_block' : IDL.Nat,
    'permanent_transfer_fee_e8s' : IDL.Nat,
  });
  const FrozenRewardRecipient = IDL.Record({
    'destination' : Account,
    'io_e8s' : IDL.Nat,
    'sns_neuron_id' : IDL.Vec(IDL.Nat8),
  });
  const RewardAllocation = IDL.Record({
    'io_e8s' : IDL.Nat,
    'sns_neuron_id' : IDL.Vec(IDL.Nat8),
  });
  const AllocationOutcome = IDL.Record({
    'rounding_dust_e8s' : IDL.Nat,
    'forfeited_io_e8s' : IDL.Nat,
    'allocations' : IDL.Vec(RewardAllocation),
  });
  const ClaimRoute = IDL.Variant({
    'AllPool' : IDL.Null,
    'AllLiquid' : IDL.Null,
    'Mixed' : IDL.Null,
  });
  const ClaimRoutePlan = IDL.Record({
    'over_target' : IDL.Nat,
    'under_target' : IDL.Nat,
    'fee_count' : IDL.Nat8,
    'claim_credit' : IDL.Nat,
    'pooled_credit' : IDL.Nat,
    'target' : IDL.Nat,
    'route' : ClaimRoute,
    'liquid_credit' : IDL.Nat,
  });
  const TwoWeekSettlementPlan = IDL.Record({
    'reward_target' : IDL.Nat,
    'post_active_backing' : IDL.Nat,
    'maximum_io_pool' : IDL.Nat,
    'distributed_io' : IDL.Nat,
    'post_active_reward' : IDL.Nat,
    'snapshot_fingerprint' : IDL.Vec(IDL.Nat8),
    'recipient_io_fees' : IDL.Nat,
    'rewards' : AllocationOutcome,
    'post_backing' : IDL.Nat,
    'post_claims' : IDL.Nat,
    'route' : ClaimRoutePlan,
    'permanent_credit' : IDL.Nat,
  });
  const FrozenInflowEconomics = IDL.Variant({
    'Pooled' : IDL.Record({
      'recipients' : IDL.Vec(FrozenRewardRecipient),
      'settlement' : TwoWeekSettlementPlan,
    }),
    'Permanent' : IDL.Record({ 'route' : ClaimRoutePlan }),
  });
  const BackingInflowPermit = IDL.Record({
    'nns_fingerprint' : IDL.Vec(IDL.Nat8),
    'liquid_destination' : Account,
    'pool_destination' : Account,
    'actual_mint_e8s' : IDL.Nat,
    'staging_account' : Account,
    'claim_transfer_fee_e8s' : IDL.Nat,
    'source_operation_id' : IDL.Vec(IDL.Nat8),
    'snapshot_fingerprint' : IDL.Vec(IDL.Nat8),
    'maturity_generation' : IDL.Nat64,
    'mint_block' : IDL.Nat,
    'economics' : FrozenInflowEconomics,
    'stream_operation_sequence' : IDL.Nat64,
    'expected_parent_before_e8s' : IDL.Nat,
    'permanent_destination' : Account,
    'permanent_transfer_fee_e8s' : IDL.Nat,
  });
  const PrepareJupiterReceiptArgs = IDL.Record({
    'liquid_amount_e8s' : IDL.Nat,
    'source_operation_id' : IDL.Vec(IDL.Nat8),
    'receipt_sequence' : IDL.Nat64,
  });
  const JupiterReceiptPermit = IDL.Record({
    'destination' : Account,
    'memo' : IDL.Vec(IDL.Nat8),
    'sequence' : IDL.Nat64,
  });
  const BackingEffect = IDL.Variant({
    'FirstClaimCredit' : IDL.Null,
    'PermanentCredit' : IDL.Null,
    'PooledCredit' : IDL.Null,
  });
  const ProveBackingEffectArgs = IDL.Record({
    'block_index' : IDL.Nat,
    'effect' : BackingEffect,
    'stream_operation_sequence' : IDL.Nat64,
  });
  const BackingInflowProgress = IDL.Variant({
    'Stuck' : IDL.Text,
    'AwaitingPooledTransfer' : IDL.Null,
    'SettlingRewards' : IDL.Null,
    'AwaitingPooledProof' : IDL.Record({ 'block_index' : IDL.Nat }),
    'Completed' : IDL.Record({
      'source_operation_id' : IDL.Vec(IDL.Nat8),
      'distributed_io_e8s' : IDL.Nat,
    }),
    'AwaitingNnsEffects' : BackingInflowPermit,
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
  const RedemptionProgress = IDL.Variant({
    'IoPullSubmitted' : IDL.Null,
    'IoInReserve' : IDL.Null,
    'Stuck' : IDL.Text,
    'PayoutSucceeded' : IDL.Null,
    'PayoutSubmitted' : IDL.Null,
    'Preparing' : IDL.Null,
    'Completed' : RedemptionResult,
    'Completing' : IDL.Null,
  });
  const StreamProgress = IDL.Variant({
    'BackingReconciliation' : IDL.Null,
    'Idle' : IDL.Null,
    'JupiterReceipt' : JupiterReceiptProgress,
    'BackingInflow' : BackingInflowProgress,
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
  const RewardEventCredit = IDL.Record({
    'destination' : Account,
    'event_credit' : IDL.Nat,
    'sns_neuron_id' : IDL.Vec(IDL.Nat8),
  });
  const RewardEventObservation = IDL.Record({
    'credits' : IDL.Vec(RewardEventCredit),
    'eligible_credit_total' : IDL.Nat,
    'observed_at_nanos' : IDL.Nat64,
    'event' : RewardEventId,
    'proposal_count' : IDL.Nat64,
    'policy_credit' : IDL.Nat,
    'classification' : RewardEventClassification,
  });
  return IDL.Service({
    'complete_jupiter_receipt' : IDL.Func(
        [CompleteJupiterReceiptArgs],
        [IDL.Variant({ 'Ok' : JupiterReceiptProgress, 'Err' : ApiError })],
        [],
      ),
    'get_caller_redemption_state' : IDL.Func(
        [],
        [IDL.Variant({ 'Ok' : CallerRedemptionState, 'Err' : ApiError })],
        ['query'],
      ),
    'get_status' : IDL.Func([], [Status], ['query']),
    'prepare_backing_inflow' : IDL.Func(
        [PrepareBackingInflowArgs],
        [IDL.Variant({ 'Ok' : BackingInflowPermit, 'Err' : ApiError })],
        [],
      ),
    'prepare_jupiter_receipt' : IDL.Func(
        [PrepareJupiterReceiptArgs],
        [IDL.Variant({ 'Ok' : JupiterReceiptPermit, 'Err' : ApiError })],
        [],
      ),
    'prove_active_transfer' : IDL.Func(
        [IDL.Nat],
        [IDL.Variant({ 'Ok' : IDL.Null, 'Err' : ApiError })],
        [],
      ),
    'prove_backing_effect' : IDL.Func(
        [ProveBackingEffectArgs],
        [IDL.Variant({ 'Ok' : BackingInflowProgress, 'Err' : ApiError })],
        [],
      ),
    'redeem' : IDL.Func(
        [RedeemArgs],
        [IDL.Variant({ 'Ok' : RedemptionProgress, 'Err' : ApiError })],
        [],
      ),
    'resume' : IDL.Func(
        [],
        [IDL.Variant({ 'Ok' : StreamProgress, 'Err' : ApiError })],
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
    'jupiter_receipt_source' : Account,
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
