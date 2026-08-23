use std::collections::BTreeSet;

use crate::{
    api::ApiError,
    daily_stake::DailyStakeObservation,
    state::{
        BackingRewardRecord, BackingRewardStatus, FrozenEntitlement, RewardEventCredit,
        StreamConfig, StructuralStakeState,
    },
};

pub fn reconcile(
    existing: &[BackingRewardRecord],
    observation: &DailyStakeObservation,
    event_marker: u64,
    config: &StreamConfig,
) -> Result<Vec<BackingRewardRecord>, String> {
    let live = observation
        .assets
        .live_cohorts
        .iter()
        .map(|cohort| cohort.generation)
        .collect::<BTreeSet<_>>();
    let mut records = Vec::new();
    for stake in &observation.stakes {
        let prior = existing
            .binary_search_by(|record| record.sns_neuron_id.cmp(&stake.sns_neuron_id))
            .ok()
            .map(|index| &existing[index]);
        let status = match (stake.state, prior.map(|record| &record.status)) {
            (_, Some(BackingRewardStatus::ExitCommitted { generation }))
                if live.contains(generation) =>
            {
                BackingRewardStatus::ExitCommitted {
                    generation: *generation,
                }
            }
            (_, Some(BackingRewardStatus::ExitPrepared { generation })) => {
                BackingRewardStatus::ExitPrepared {
                    generation: *generation,
                }
            }
            (
                StructuralStakeState::Active,
                Some(BackingRewardStatus::ActiveEligible {
                    eligible_from_event,
                }),
            ) => BackingRewardStatus::ActiveEligible {
                eligible_from_event: *eligible_from_event,
            },
            (
                StructuralStakeState::Active,
                Some(BackingRewardStatus::ReentryPending {
                    eligible_from_event,
                }),
            ) => BackingRewardStatus::ReentryPending {
                eligible_from_event: *eligible_from_event,
            },
            (StructuralStakeState::Active, Some(BackingRewardStatus::ExitCommitted { .. })) => {
                BackingRewardStatus::ReentryPending {
                    eligible_from_event: event_marker.saturating_add(1),
                }
            }
            (StructuralStakeState::Active, _) => BackingRewardStatus::ReentryPending {
                eligible_from_event: event_marker.saturating_add(1),
            },
            (StructuralStakeState::IneligibleActive, _) => BackingRewardStatus::ActiveIneligible,
            (StructuralStakeState::Dissolving, _) => BackingRewardStatus::ExitObserved,
            (StructuralStakeState::LiquidOrDissolved, _) => BackingRewardStatus::Inactive,
        };
        let credit = prior.map_or(0, |record| record.accumulated_eligible_credit);
        if retain(stake.state, &status, credit) {
            records.push(BackingRewardRecord {
                sns_neuron_id: stake.sns_neuron_id.clone(),
                staking_account: stake.staking_account.clone(),
                accumulated_eligible_credit: credit,
                latest_structural_state: stake.state,
                status,
            });
        }
    }
    for prior in existing {
        if records
            .binary_search_by(|record| record.sns_neuron_id.cmp(&prior.sns_neuron_id))
            .is_err()
            && (prior.accumulated_eligible_credit > 0
                || prior
                    .status
                    .generation()
                    .is_some_and(|generation| live.contains(&generation)))
        {
            records.push(prior.clone());
        }
    }
    records.sort_by(|left, right| left.sns_neuron_id.cmp(&right.sns_neuron_id));
    crate::state::validate_backing_registry(&records, config)?;
    Ok(records)
}

fn retain(structural: StructuralStakeState, status: &BackingRewardStatus, credit: u128) -> bool {
    credit > 0
        || matches!(
            status,
            BackingRewardStatus::ExitPrepared { .. } | BackingRewardStatus::ExitCommitted { .. }
        )
        || matches!(
            structural,
            StructuralStakeState::Active | StructuralStakeState::Dissolving
        )
        || matches!(status, BackingRewardStatus::ReentryPending { .. })
}

impl BackingRewardStatus {
    fn generation(&self) -> Option<u64> {
        match self {
            Self::ExitPrepared { generation } | Self::ExitCommitted { generation } => {
                Some(*generation)
            }
            _ => None,
        }
    }
}

pub fn prepare_observed_exits(
    records: &mut [BackingRewardRecord],
    generation: u64,
) -> Result<usize, String> {
    if generation == 0 {
        return Err("exit preparation generation must be non-zero".into());
    }
    let mut prepared = 0usize;
    for record in records.iter_mut() {
        if matches!(record.status, BackingRewardStatus::ExitObserved) {
            record.status = BackingRewardStatus::ExitPrepared { generation };
            prepared = prepared
                .checked_add(1)
                .ok_or("prepared exit count overflow")?;
        }
    }
    Ok(prepared)
}

pub fn commit_prepared_exits(records: &mut [BackingRewardRecord], generation: u64) -> usize {
    let mut committed = 0;
    for record in records.iter_mut() {
        if record.status == (BackingRewardStatus::ExitPrepared { generation }) {
            record.status = BackingRewardStatus::ExitCommitted { generation };
            committed += 1;
        }
    }
    committed
}

pub fn rollback_prepared_exits(records: &mut [BackingRewardRecord], generation: u64) -> usize {
    let mut rolled_back = 0;
    for record in records.iter_mut() {
        if record.status == (BackingRewardStatus::ExitPrepared { generation }) {
            record.status = BackingRewardStatus::ExitObserved;
            rolled_back += 1;
        }
    }
    rolled_back
}

pub fn reward_eligible_ids(
    records: &[BackingRewardRecord],
    event_marker: u64,
) -> BTreeSet<Vec<u8>> {
    records
        .iter()
        .filter(|record| {
            record.latest_structural_state == StructuralStakeState::Active
                && matches!(
                    record.status,
                    BackingRewardStatus::ActiveEligible { eligible_from_event }
                        if eligible_from_event <= event_marker
                )
        })
        .map(|record| record.sns_neuron_id.clone())
        .collect()
}

pub fn apply_credits(
    records: &mut [BackingRewardRecord],
    credits: &[RewardEventCredit],
) -> Result<(), ApiError> {
    let mut seen = BTreeSet::new();
    for credit in credits {
        if !seen.insert(&credit.sns_neuron_id) {
            return Err(ApiError::Invalid("reward event duplicates a neuron".into()));
        }
        let index = records
            .binary_search_by(|record| record.sns_neuron_id.cmp(&credit.sns_neuron_id))
            .map_err(|_| {
                ApiError::Invalid("reward credit has no canonical neuron record".into())
            })?;
        let record = &mut records[index];
        if !record
            .staking_account
            .effective_eq(&credit.destination)
            .map_err(ApiError::Invalid)?
        {
            return Err(ApiError::Invalid("reward destination changed".into()));
        }
        record.accumulated_eligible_credit = record
            .accumulated_eligible_credit
            .checked_add(credit.event_credit)
            .ok_or_else(|| ApiError::Invalid("reward credit overflow".into()))?;
    }
    Ok(())
}

pub fn freeze(records: &mut [BackingRewardRecord]) -> Vec<FrozenEntitlement> {
    records
        .iter_mut()
        .filter_map(|record| {
            let credit = std::mem::take(&mut record.accumulated_eligible_credit);
            (credit > 0).then(|| FrozenEntitlement {
                sns_neuron_id: record.sns_neuron_id.clone(),
                destination: record.staking_account.clone(),
                accumulated_eligible_credit: credit,
            })
        })
        .collect()
}

pub fn promote_pending(
    records: &mut [BackingRewardRecord],
    event_marker: u64,
    pooled: u128,
    backing: u128,
    claims: u128,
    active_backing: u128,
) -> Result<bool, String> {
    let has_pending = records.iter().any(|record| {
        record.latest_structural_state == StructuralStakeState::Active
            && matches!(record.status, BackingRewardStatus::ReentryPending { .. })
    });
    if !has_pending {
        return Ok(false);
    }
    let full_target = io_core_model::target(active_backing, backing, claims)
        .map_err(|error| format!("re-entry target failed: {error:?}"))?;
    if pooled < full_target {
        return Ok(false);
    }
    for record in records.iter_mut().filter(|record| {
        record.latest_structural_state == StructuralStakeState::Active
            && matches!(record.status, BackingRewardStatus::ReentryPending { .. })
    }) {
        record.status = BackingRewardStatus::ActiveEligible {
            eligible_from_event: event_marker.saturating_add(1),
        };
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use candid::Principal;
    use io_nns_types::backing::ClaimAssetObservation;

    fn principal(value: u8) -> Principal {
        Principal::from_slice(&[value; 29])
    }

    fn config() -> StreamConfig {
        let stream = principal(1);
        let nns = principal(4);
        let account = |owner, value| crate::state::Account {
            owner,
            subaccount: Some(vec![value; 32]),
        };
        StreamConfig {
            io_ledger: principal(2),
            icp_ledger: principal(3),
            nns_manager: nns,
            jupiter_receipt_source: account(nns, 1),
            jupiter_io_account: account(principal(7), 2),
            sns_governance: principal(5),
            sns_root: principal(6),
            expected_sns_governance_module_hash: vec![8; 32],
            approved_reward_event_duration_seconds: 86_400,
            io_reserve: account(stream, 3),
            liquid_icp: account(stream, 4),
            nonredeemable_governance_io_accounts: Vec::new(),
            minimum_redemption_io_e8s: 100,
            expected_io_fee_e8s: 10,
            expected_icp_fee_e8s: 10,
            maximum_request_lifetime_nanos: 1_000,
            retry_delay_nanos: 10,
            ledger_deduplication_window_nanos: 2_000,
        }
    }

    fn record(
        id: u8,
        state: StructuralStakeState,
        status: BackingRewardStatus,
    ) -> BackingRewardRecord {
        BackingRewardRecord {
            sns_neuron_id: vec![id; 32],
            staking_account: crate::state::Account {
                owner: principal(5),
                subaccount: Some(vec![id; 32]),
            },
            accumulated_eligible_credit: 0,
            latest_structural_state: state,
            status,
        }
    }

    fn daily(
        stakes: Vec<crate::redemption::StructuralStakeObservation>,
        live: Vec<u64>,
    ) -> DailyStakeObservation {
        let mut assets = ClaimAssetObservation {
            parent: None,
            pool_staking_account: crate::state::Account {
                owner: principal(9),
                subaccount: Some(vec![10; 32]),
            },
            minimum_parent_stake_e8s: 100,
            pooled_parent_principal_e8s: 0,
            live_cohorts: live
                .into_iter()
                .map(|generation| io_nns_types::backing::CohortObservation {
                    generation,
                    child_neuron_id: generation,
                    physical_principal_e8s: 2,
                    net_backing_e8s: 1,
                    ready_at_seconds: 1,
                    proof: io_nns_types::backing::CohortProofState::Dissolving,
                })
                .collect(),
            live_child_physical_principal_e8s: 0,
            live_child_net_backing_e8s: 0,
            live_child_committed_fee_liability_e8s: 0,
            transit_backing_e8s: 0,
            active_operation_sequence: 0,
            last_completed_pool_operation_sequence: None,
            control_epoch: 0,
            fingerprint: vec![1; 32],
            oldest_ready_at_seconds: None,
        };
        assets.live_child_physical_principal_e8s = assets.live_cohorts.len() as u128 * 2;
        assets.live_child_net_backing_e8s = assets.live_cohorts.len() as u128;
        assets.live_child_committed_fee_liability_e8s = assets.live_cohorts.len() as u128;
        DailyStakeObservation {
            claim: crate::redemption::ClaimSnapshot::default(),
            reward_event: io_sns_reward_boundary::RewardEvent::default(),
            neurons: Vec::new(),
            stakes,
            active_backing_io_e8s: 0,
            assets,
        }
    }

    fn stake(
        record: &BackingRewardRecord,
        state: StructuralStakeState,
    ) -> crate::redemption::StructuralStakeObservation {
        crate::redemption::StructuralStakeObservation {
            sns_neuron_id: record.sns_neuron_id.clone(),
            staking_account: record.staking_account.clone(),
            state,
            ledger_balance_e8s: 100,
        }
    }

    #[test]
    fn committed_generation_survives_every_structural_state_until_cleanup() {
        let prior = record(
            1,
            StructuralStakeState::Active,
            BackingRewardStatus::ExitCommitted { generation: 7 },
        );
        for state in [
            StructuralStakeState::Active,
            StructuralStakeState::Dissolving,
            StructuralStakeState::LiquidOrDissolved,
        ] {
            let updated = reconcile(
                std::slice::from_ref(&prior),
                &daily(vec![stake(&prior, state)], vec![7]),
                10,
                &config(),
            )
            .unwrap();
            assert_eq!(
                updated[0].status,
                BackingRewardStatus::ExitCommitted { generation: 7 }
            );
        }
    }

    #[test]
    fn resolved_child_frees_capacity_before_reward_reentry() {
        let prior = record(
            1,
            StructuralStakeState::Active,
            BackingRewardStatus::ExitCommitted { generation: 7 },
        );
        let updated = reconcile(
            std::slice::from_ref(&prior),
            &daily(
                vec![stake(&prior, StructuralStakeState::Active)],
                Vec::new(),
            ),
            10,
            &config(),
        )
        .unwrap();
        assert_eq!(
            updated[0].status,
            BackingRewardStatus::ReentryPending {
                eligible_from_event: 11
            }
        );
    }

    #[test]
    fn observed_exits_bind_only_to_the_exact_prepared_generation() {
        let mut records = vec![
            record(
                1,
                StructuralStakeState::Dissolving,
                BackingRewardStatus::ExitObserved,
            ),
            record(
                2,
                StructuralStakeState::Dissolving,
                BackingRewardStatus::ExitCommitted { generation: 1 },
            ),
        ];
        assert_eq!(prepare_observed_exits(&mut records, 2), Ok(1));
        assert_eq!(
            records[0].status,
            BackingRewardStatus::ExitPrepared { generation: 2 }
        );
        assert_eq!(commit_prepared_exits(&mut records, 1), 0);
        assert_eq!(commit_prepared_exits(&mut records, 2), 1);
        assert_eq!(
            records[0].status,
            BackingRewardStatus::ExitCommitted { generation: 2 }
        );
        assert_eq!(
            records[1].status,
            BackingRewardStatus::ExitCommitted { generation: 1 }
        );
    }

    #[test]
    fn definitive_no_effect_rolls_prepared_exit_back_without_cross_binding() {
        let mut records = vec![record(
            1,
            StructuralStakeState::Dissolving,
            BackingRewardStatus::ExitObserved,
        )];
        prepare_observed_exits(&mut records, 7).unwrap();
        assert_eq!(rollback_prepared_exits(&mut records, 6), 0);
        assert_eq!(rollback_prepared_exits(&mut records, 7), 1);
        assert_eq!(records[0].status, BackingRewardStatus::ExitObserved);
    }

    #[test]
    fn prepared_exit_stays_sticky_across_rapid_structural_cancellation() {
        let prior = record(
            1,
            StructuralStakeState::Dissolving,
            BackingRewardStatus::ExitPrepared { generation: 9 },
        );
        let updated = reconcile(
            std::slice::from_ref(&prior),
            &daily(
                vec![stake(&prior, StructuralStakeState::Active)],
                Vec::new(),
            ),
            10,
            &config(),
        )
        .unwrap();
        assert_eq!(
            updated[0].status,
            BackingRewardStatus::ExitPrepared { generation: 9 }
        );
    }

    #[test]
    fn precommit_cancellation_has_no_generation_and_reenters_next_event() {
        let prior = record(
            1,
            StructuralStakeState::Dissolving,
            BackingRewardStatus::ExitObserved,
        );
        let updated = reconcile(
            std::slice::from_ref(&prior),
            &daily(
                vec![stake(&prior, StructuralStakeState::Active)],
                Vec::new(),
            ),
            10,
            &config(),
        )
        .unwrap();
        assert_eq!(
            updated[0].status,
            BackingRewardStatus::ReentryPending {
                eligible_from_event: 11
            }
        );
    }

    #[test]
    fn non_fourteen_day_neuron_is_ignored_without_unresolved_credit() {
        let prior = record(
            1,
            StructuralStakeState::IneligibleActive,
            BackingRewardStatus::ActiveIneligible,
        );
        assert!(reconcile(
            &[],
            &daily(
                vec![stake(&prior, StructuralStakeState::IneligibleActive)],
                Vec::new()
            ),
            10,
            &config(),
        )
        .unwrap()
        .is_empty());
    }

    #[test]
    fn pending_reentries_promote_all_or_none_for_the_next_event() {
        let mut records = vec![
            record(
                1,
                StructuralStakeState::Active,
                BackingRewardStatus::ReentryPending {
                    eligible_from_event: 10,
                },
            ),
            record(
                2,
                StructuralStakeState::Active,
                BackingRewardStatus::ReentryPending {
                    eligible_from_event: 10,
                },
            ),
        ];
        assert!(!promote_pending(&mut records, 10, 599, 1_000, 1_000, 600).unwrap());
        assert!(reward_eligible_ids(&records, 11).is_empty());
        assert!(promote_pending(&mut records, 10, 600, 1_000, 1_000, 600).unwrap());
        assert!(reward_eligible_ids(&records, 10).is_empty());
        assert_eq!(reward_eligible_ids(&records, 11).len(), 2);
    }
}
