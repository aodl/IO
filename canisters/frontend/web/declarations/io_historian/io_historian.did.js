import { IDL } from "@dfinity/candid";

export const idlFactory = ({ IDL }) => {
  const AccountHistoryPageOrder = IDL.Variant({ Ascending: IDL.Null, Descending: IDL.Null });
  const ArtifactMatchStatus = IDL.Variant({ Matching: IDL.Null, Mismatch: IDL.Null, Unknown: IDL.Null, Unobserved: IDL.Null });
  const DataAvailability = IDL.Variant({ Missing: IDL.Null, NotApplicable: IDL.Null, Observed: IDL.Null });
  const DataCompleteness = IDL.Record({
    liquid_icp_reserve: DataAvailability,
    non_redeemable_governance_io: DataAvailability,
    protocol_reserve_io: DataAvailability,
    redeemable_io_supply: DataAvailability,
    redemption_rate: DataAvailability,
    total_io_supply: DataAvailability,
    two_year_nns_principal: DataAvailability,
  });
  const GovernanceExcludedCount = IDL.Record({ count: IDL.Nat64, reason: IDL.Text });
  const GovernanceNeuronParticipation = IDL.Record({
    eligible_closed_proposals: IDL.Nat64,
    eligible_seconds: IDL.Nat64,
    eligible_stake_e8s: IDL.Nat,
    neuron_id: IDL.Nat64,
    participation_denominator: IDL.Nat,
    participation_numerator: IDL.Nat,
    voted_closed_proposals: IDL.Nat64,
  });
  const PublicOperationPhase = IDL.Variant({
    AwaitingIcpPayout: IDL.Null,
    AwaitingIoIssuance: IDL.Null,
    AwaitingIoReturn: IDL.Null,
    Completed: IDL.Null,
    FailedRetryable: IDL.Null,
    FailedTerminal: IDL.Null,
    Observed: IDL.Null,
    PartiallyDistributed: IDL.Null,
    Previewed: IDL.Null,
    Unknown: IDL.Null,
  });
  const GovernanceParticipationSnapshot = IDL.Record({
    counted_proposals: IDL.Nat64,
    last_governance_snapshot_timestamp_nanos: IDL.Opt(IDL.Nat64),
    neuron_participation: IDL.Vec(GovernanceNeuronParticipation),
    nns_lifecycle_status_summary: IDL.Opt(IDL.Text),
    pending_nns_operation_count: IDL.Opt(IDL.Nat64),
    proposal_epoch_end: IDL.Opt(IDL.Nat64),
    proposal_epoch_start: IDL.Opt(IDL.Nat64),
    sns_eligible_neuron_count: IDL.Nat64,
    sns_excluded_neuron_count_by_reason: IDL.Vec(GovernanceExcludedCount),
    total_eligible_stake_e8s: IDL.Nat,
  });
  const RetentionLimits = IDL.Record({
    artifact_status: IDL.Nat64,
    canister_status: IDL.Nat64,
    governance_neuron_summaries: IDL.Nat64,
    index_health: IDL.Nat64,
    max_page_limit: IDL.Nat64,
    nns_lifecycle_history: IDL.Nat64,
    redemption_history: IDL.Nat64,
    reward_history: IDL.Nat64,
    stream_history: IDL.Nat64,
  });
  const HistorianIngestionStatus = IDL.Record({
    artifact_status_count: IDL.Nat64,
    canister_status_count: IDL.Nat64,
    index_health_record_count: IDL.Nat64,
    last_ingested_timestamp_nanos: IDL.Opt(IDL.Nat64),
    nns_lifecycle_record_count: IDL.Nat64,
    redemption_record_count: IDL.Nat64,
    retained_record_limits: RetentionLimits,
    reward_record_count: IDL.Nat64,
    schema_version: IDL.Nat32,
    stream_record_count: IDL.Nat64,
  });
  const LedgerKind = IDL.Variant({ IcpLedger: IDL.Null, IoLedger: IDL.Null });
  const IndexHealthSummary = IDL.Record({
    account_label: IDL.Text,
    backfill_complete: IDL.Bool,
    invariant_broken_count: IDL.Nat64,
    lag_suspected: IDL.Bool,
    last_error: IDL.Opt(IDL.Text),
    last_observed_balance_e8s: IDL.Opt(IDL.Nat),
    last_observed_newest_tx_id: IDL.Opt(IDL.Nat64),
    last_success_timestamp_nanos: IDL.Opt(IDL.Nat64),
    latest_cursor: IDL.Opt(IDL.Nat64),
    ledger_kind: LedgerKind,
    num_blocks_synced: IDL.Opt(IDL.Nat64),
    oldest_cursor: IDL.Opt(IDL.Nat64),
    page_cap_reached: IDL.Bool,
    page_order: IDL.Opt(AccountHistoryPageOrder),
    record_id: IDL.Text,
    scan_incomplete: IDL.Bool,
    unreadable_count: IDL.Nat64,
  });
  const IngestionSourceKind = IDL.Variant({
    CanisterStatusModuleHash: IDL.Null,
    FrontendDashboardFreshness: IDL.Null,
    FutureIoSnsIndexHealth: IDL.Null,
    IcpIndexHealth: IDL.Null,
    NnsGovernanceFreshness: IDL.Null,
    ProtocolSnapshot: IDL.Null,
    ReleaseArtifacts: IDL.Null,
    ReserveSnapshot: IDL.Null,
    SnsGovernanceFreshness: IDL.Null,
  });
  const IngestionWatermark = IDL.Record({
    governance_neuron_snapshot_timestamp_nanos: IDL.Opt(IDL.Nat64),
    governance_proposal_timestamp_nanos: IDL.Opt(IDL.Nat64),
    observed_module_hash: IDL.Opt(IDL.Text),
    oldest_source_cursor: IDL.Opt(IDL.Nat64),
    release_manifest_hash: IDL.Opt(IDL.Text),
    source_block_height: IDL.Opt(IDL.Nat64),
    source_index_height: IDL.Opt(IDL.Nat64),
  });
  const ListGovernanceParticipationRequest = IDL.Record({
    limit: IDL.Opt(IDL.Nat64),
    start_after_neuron_id: IDL.Opt(IDL.Nat64),
  });
  const ListGovernanceParticipationResponse = IDL.Record({
    next_start_after_neuron_id: IDL.Opt(IDL.Nat64),
    records: IDL.Vec(GovernanceNeuronParticipation),
  });
  const ListNnsLifecycleEventsRequest = IDL.Record({ limit: IDL.Opt(IDL.Nat64), start_after: IDL.Opt(IDL.Text) });
  const NnsLifecycleKind = IDL.Variant({
    TwoWeekPoolMergeBack: IDL.Null,
    TwoWeekPoolRestake: IDL.Null,
    TwoWeekPoolSplit: IDL.Null,
    TwoWeekPoolStartDissolving: IDL.Null,
    TwoWeekPoolStopDissolving: IDL.Null,
    TwoWeekMaturityDisbursement: IDL.Null,
    TwoWeekUnwindPrincipalDisbursement: IDL.Null,
    TwoYearMaturityDisbursement: IDL.Null,
    Unknown: IDL.Null,
  });
  const NnsLifecycleSummary = IDL.Record({
    amount_e8s: IDL.Opt(IDL.Nat),
    kind: NnsLifecycleKind,
    neuron_id: IDL.Opt(IDL.Nat64),
    phase: PublicOperationPhase,
    record_id: IDL.Text,
    retry_count: IDL.Nat32,
    safe_error: IDL.Opt(IDL.Text),
    timestamp_nanos: IDL.Opt(IDL.Nat64),
  });
  const ListNnsLifecycleEventsResponse = IDL.Record({
    next_start_after: IDL.Opt(IDL.Text),
    records: IDL.Vec(NnsLifecycleSummary),
  });
  const ListRedemptionsRequest = IDL.Record({ limit: IDL.Opt(IDL.Nat64), start_after: IDL.Opt(IDL.Text) });
  const ListRewardsRequest = IDL.Record({ limit: IDL.Opt(IDL.Nat64), start_after: IDL.Opt(IDL.Text) });
  const ListStreamsRequest = IDL.Record({ limit: IDL.Opt(IDL.Nat64), start_after: IDL.Opt(IDL.Text) });
  const RedemptionRateSnapshot = IDL.Record({
    last_updated_timestamp_nanos: IDL.Opt(IDL.Nat64),
    liquid_icp_per_io_e8s_denominator: IDL.Nat,
    liquid_icp_per_io_e8s_numerator: IDL.Nat,
    liquid_icp_reserve_e8s: IDL.Nat,
    redeemable_io_supply_e8s: IDL.Nat,
  });
  const ProtocolSnapshot = IDL.Record({
    completeness: DataCompleteness,
    last_updated_timestamp_nanos: IDL.Opt(IDL.Nat64),
    liquid_icp_reserve_e8s: IDL.Opt(IDL.Nat),
    non_redeemable_governance_io_e8s: IDL.Opt(IDL.Nat),
    protocol_reserve_io_e8s: IDL.Opt(IDL.Nat),
    redeemable_io_supply_e8s: IDL.Opt(IDL.Nat),
    redemption_rate: IDL.Opt(RedemptionRateSnapshot),
    total_io_supply_e8s: IDL.Opt(IDL.Nat),
    two_year_nns_principal_e8s: IDL.Opt(IDL.Nat),
  });
  const PublicStatus = IDL.Record({
    ingestion: HistorianIngestionStatus,
    model: IDL.Text,
    schema_version: IDL.Nat32,
    version: IDL.Text,
  });
  const PublicRecipientPolicy = IDL.Variant({ EligibleIoSnsNeurons: IDL.Null, JupiterFaucet: IDL.Null, None: IDL.Null, Unknown: IDL.Null });
  const PublicStreamKind = IDL.Variant({ JupiterFaucet: IDL.Null, TwoWeekMaturity: IDL.Null, TwoYearMaturity: IDL.Null, UnknownIcpDeposit: IDL.Null });
  const RedemptionHistoryRecord = IDL.Record({
    gross_icp_payout_e8s: IDL.Opt(IDL.Nat),
    icp_payout_amount_e8s: IDL.Opt(IDL.Nat),
    icp_payout_block: IDL.Opt(IDL.Nat64),
    icp_payout_fee_e8s: IDL.Opt(IDL.Nat),
    io_amount_e8s: IDL.Nat,
    io_burn_or_transfer_block: IDL.Opt(IDL.Nat64),
    io_return_fee_e8s: IDL.Opt(IDL.Nat),
    io_return_block: IDL.Opt(IDL.Nat64),
    net_user_icp_payout_e8s: IDL.Opt(IDL.Nat),
    phase: PublicOperationPhase,
    record_id: IDL.Text,
    retry_count: IDL.Nat32,
    retry_status: IDL.Opt(IDL.Text),
    timestamp_nanos: IDL.Opt(IDL.Nat64),
    user_account: IDL.Opt(IDL.Text),
  });
  const ListRedemptionsResponse = IDL.Record({
    next_start_after: IDL.Opt(IDL.Text),
    records: IDL.Vec(RedemptionHistoryRecord),
  });
  const ReserveSnapshot = IDL.Record({
    completeness: DataCompleteness,
    last_updated_timestamp_nanos: IDL.Opt(IDL.Nat64),
    liquid_icp_reserve_e8s: IDL.Opt(IDL.Nat),
    two_year_nns_principal_e8s: IDL.Opt(IDL.Nat),
  });
  const RewardDistributionRecord = IDL.Record({
    dust_unissued_e8s: IDL.Opt(IDL.Nat),
    eligible_stake_e8s: IDL.Opt(IDL.Nat),
    epoch_end_timestamp_nanos: IDL.Opt(IDL.Nat64),
    epoch_start_timestamp_nanos: IDL.Opt(IDL.Nat64),
    participation_summary_id: IDL.Opt(IDL.Text),
    payout_block: IDL.Opt(IDL.Nat64),
    recipient_account: IDL.Opt(IDL.Text),
    recipient_neuron_id: IDL.Opt(IDL.Nat64),
    record_id: IDL.Text,
    reward_amount_e8s: IDL.Nat,
    status: PublicOperationPhase,
  });
  const ListRewardsResponse = IDL.Record({
    next_start_after: IDL.Opt(IDL.Text),
    records: IDL.Vec(RewardDistributionRecord),
  });
  const StreamHistoryRecord = IDL.Record({
    amount_e8s: IDL.Nat,
    io_issued_e8s: IDL.Opt(IDL.Nat),
    memo_label: IDL.Opt(IDL.Text),
    phase: PublicOperationPhase,
    recipient_policy: PublicRecipientPolicy,
    record_id: IDL.Text,
    safe_subaccount_label: IDL.Opt(IDL.Text),
    source_block_index: IDL.Opt(IDL.Nat64),
    source_ledger: IDL.Text,
    stream_kind: PublicStreamKind,
    terminal_rejection_reason: IDL.Opt(IDL.Text),
    timestamp_nanos: IDL.Opt(IDL.Nat64),
  });
  const ListStreamsResponse = IDL.Record({
    next_start_after: IDL.Opt(IDL.Text),
    records: IDL.Vec(StreamHistoryRecord),
  });
  const SupplySnapshot = IDL.Record({
    completeness: DataCompleteness,
    last_updated_timestamp_nanos: IDL.Opt(IDL.Nat64),
    non_redeemable_governance_io_e8s: IDL.Opt(IDL.Nat),
    protocol_reserve_io_e8s: IDL.Opt(IDL.Nat),
    redeemable_io_supply_e8s: IDL.Opt(IDL.Nat),
    total_io_supply_e8s: IDL.Opt(IDL.Nat),
  });
  const CanisterArtifactStatus = IDL.Record({
    artifact_byte_size: IDL.Opt(IDL.Nat64),
    build_profile: IDL.Opt(IDL.Text),
    canister_name: IDL.Text,
    expected_canister_principal_text: IDL.Opt(IDL.Text),
    git_commit: IDL.Opt(IDL.Text),
    gz_artifact_byte_size: IDL.Opt(IDL.Nat64),
    gz_wasm_sha256: IDL.Opt(IDL.Text),
    last_checked_timestamp_nanos: IDL.Opt(IDL.Nat64),
    observed_module_hash: IDL.Opt(IDL.Text),
    raw_wasm_sha256: IDL.Opt(IDL.Text),
    status: ArtifactMatchStatus,
    target: IDL.Opt(IDL.Text),
  });
  const ObservationFreshness = IDL.Variant({
    ErrorRetryable: IDL.Null,
    Fresh: IDL.Null,
    Incomplete: IDL.Null,
    Missing: IDL.Null,
    ObservedOnly: IDL.Null,
    PrelaunchNotApplicable: IDL.Null,
    Stale: IDL.Null,
    Unknown: IDL.Null,
  });
  const StalenessPolicy = IDL.Record({
    max_age_nanos: IDL.Opt(IDL.Nat64),
    prelaunch_expected_absent: IDL.Bool,
    required: IDL.Bool,
  });
  const SourceHealth = IDL.Record({
    error_summary: IDL.Opt(IDL.Text),
    freshness: ObservationFreshness,
    kind: IngestionSourceKind,
    last_attempt_timestamp_nanos: IDL.Opt(IDL.Nat64),
    last_success_timestamp_nanos: IDL.Opt(IDL.Nat64),
    policy: StalenessPolicy,
    retryable: IDL.Bool,
    source_id: IDL.Text,
    summary: IDL.Text,
    watermark: IngestionWatermark,
  });
  const PublicDashboardState = IDL.Record({
    canister_status: IDL.Vec(CanisterArtifactStatus),
    governance: GovernanceParticipationSnapshot,
    index_health: IDL.Vec(IndexHealthSummary),
    protocol: ProtocolSnapshot,
    redemption_rate: IDL.Opt(RedemptionRateSnapshot),
    release_artifacts: IDL.Vec(CanisterArtifactStatus),
    reserve: ReserveSnapshot,
    source_health: IDL.Vec(SourceHealth),
    status: PublicStatus,
    supply: SupplySnapshot,
  });
  return IDL.Service({
    get_canister_status_summary: IDL.Func([], [IDL.Vec(CanisterArtifactStatus)], ["query"]),
    get_dashboard_state: IDL.Func([], [PublicDashboardState], ["query"]),
    get_governance_summary: IDL.Func([], [GovernanceParticipationSnapshot], ["query"]),
    get_index_health: IDL.Func([], [IDL.Vec(IndexHealthSummary)], ["query"]),
    get_protocol_snapshot: IDL.Func([], [ProtocolSnapshot], ["query"]),
    get_public_status: IDL.Func([], [PublicStatus], ["query"]),
    get_redemption_rate: IDL.Func([], [IDL.Opt(RedemptionRateSnapshot)], ["query"]),
    get_release_artifacts: IDL.Func([], [IDL.Vec(CanisterArtifactStatus)], ["query"]),
    get_reserve_snapshot: IDL.Func([], [ReserveSnapshot], ["query"]),
    list_governance_participation: IDL.Func([ListGovernanceParticipationRequest], [ListGovernanceParticipationResponse], ["query"]),
    list_nns_lifecycle_events: IDL.Func([ListNnsLifecycleEventsRequest], [ListNnsLifecycleEventsResponse], ["query"]),
    list_redemptions: IDL.Func([ListRedemptionsRequest], [ListRedemptionsResponse], ["query"]),
    list_rewards: IDL.Func([ListRewardsRequest], [ListRewardsResponse], ["query"]),
    list_streams: IDL.Func([ListStreamsRequest], [ListStreamsResponse], ["query"]),
    version: IDL.Func([], [IDL.Text], ["query"]),
  });
};

export const init = () => [];
