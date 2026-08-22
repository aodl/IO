use std::collections::BTreeSet;

use crate::{
    redemption::CanonicalRedemptionSnapshot,
    state::{BackingRewardRecord, BackingRewardStatus, StreamConfig, StructuralStakeState},
};

pub fn reconcile(
    existing: &[BackingRewardRecord],
    snapshot: &CanonicalRedemptionSnapshot,
    event_marker: u64,
    config: &StreamConfig,
) -> Result<Vec<BackingRewardRecord>, String> {
    let live_generations = snapshot
        .live_cohort_generations
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let excluded = snapshot
        .excluded_io_balances
        .iter()
        .try_fold(0u128, |total, (_, balance)| total.checked_add(*balance))
        .ok_or("nonredeemable balance overflow")?;
    let claims = io_core_model::claim_supply(
        snapshot.total_supply_e8s,
        snapshot.reserve_io_e8s,
        &[excluded],
    )
    .map_err(|error| format!("claim supply failed: {error:?}"))?;
    let covered = io_core_model::rewards_covered(io_core_model::EconomicState {
        backing: io_core_model::Backing {
            liquid: snapshot.liquid_icp_e8s,
            pooled: snapshot.pooled_principal_e8s,
            unwinding: snapshot.unwinding_principal_e8s,
            transit: snapshot.transit_backing_e8s,
        },
        claims,
        active_backing: snapshot.active_backing_io_e8s,
        active_reward: snapshot.active_reward_io_e8s,
    })
    .is_ok();
    let mut result = Vec::new();
    for stake in &snapshot.structural_stakes {
        let prior = existing
            .binary_search_by(|record| record.sns_neuron_id.cmp(&stake.sns_neuron_id))
            .ok()
            .map(|index| &existing[index]);
        let newly_committed = snapshot.active_unwind_generation.filter(|_| {
            prior.is_some_and(|record| matches!(record.status, BackingRewardStatus::ExitObserved))
        });
        let unresolved = newly_committed.or_else(|| {
            prior
                .and_then(|record| record.unresolved_cohort_generation)
                .filter(|generation| live_generations.contains(generation))
        });
        let status = match (stake.state, prior.map(|record| &record.status), unresolved) {
            (_, _, Some(generation)) => BackingRewardStatus::ExitCommitted { generation },
            (
                StructuralStakeState::Active,
                Some(BackingRewardStatus::ActiveEligible {
                    eligible_from_event,
                }),
                _,
            ) => BackingRewardStatus::ActiveEligible {
                eligible_from_event: *eligible_from_event,
            },
            (
                StructuralStakeState::Active,
                Some(BackingRewardStatus::ReentryPending {
                    eligible_from_event,
                }),
                _,
            ) if covered && *eligible_from_event <= event_marker => {
                BackingRewardStatus::ActiveEligible {
                    eligible_from_event: *eligible_from_event,
                }
            }
            (
                StructuralStakeState::Active,
                Some(BackingRewardStatus::ReentryPending {
                    eligible_from_event,
                }),
                _,
            ) => BackingRewardStatus::ReentryPending {
                eligible_from_event: *eligible_from_event,
            },
            (
                StructuralStakeState::Active,
                Some(BackingRewardStatus::ExitCommitted { .. }),
                None,
            ) => BackingRewardStatus::ReentryPending {
                eligible_from_event: event_marker.saturating_add(1),
            },
            (StructuralStakeState::Active, Some(BackingRewardStatus::ActiveIneligible), _)
                if covered =>
            {
                BackingRewardStatus::ActiveEligible {
                    eligible_from_event: event_marker.saturating_add(1),
                }
            }
            (StructuralStakeState::Active, _, _) => BackingRewardStatus::ActiveIneligible,
            (
                StructuralStakeState::Dissolving,
                Some(BackingRewardStatus::ExitCommitted { .. }),
                None,
            )
            | (StructuralStakeState::Dissolving, _, _) => BackingRewardStatus::ExitObserved,
            (
                StructuralStakeState::LiquidOrDissolved,
                Some(BackingRewardStatus::ExitCommitted { .. }),
                None,
            )
            | (StructuralStakeState::LiquidOrDissolved, _, _) => BackingRewardStatus::Inactive,
        };
        if status != BackingRewardStatus::Inactive || unresolved.is_some() {
            result.push(BackingRewardRecord {
                sns_neuron_id: stake.sns_neuron_id.clone(),
                staking_account: stake.staking_account.clone(),
                latest_structural_state: stake.state,
                status,
                unresolved_cohort_generation: unresolved,
            });
        }
    }
    for record in existing {
        if record
            .unresolved_cohort_generation
            .is_some_and(|generation| live_generations.contains(&generation))
            && !result
                .iter()
                .any(|candidate| candidate.sns_neuron_id == record.sns_neuron_id)
        {
            result.push(record.clone());
        }
    }
    result.sort_by(|left, right| left.sns_neuron_id.cmp(&right.sns_neuron_id));
    crate::state::validate_backing_registry(&result, config)?;
    Ok(result)
}

pub fn reward_eligible_ids(
    records: &[BackingRewardRecord],
    event_marker: u64,
) -> BTreeSet<Vec<u8>> {
    records
        .iter()
        .filter(|record| {
            matches!(
                record.status,
                BackingRewardStatus::ActiveEligible {
                    eligible_from_event
                } if eligible_from_event <= event_marker
            )
        })
        .map(|record| record.sns_neuron_id.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{redemption::StructuralStakeObservation, state::Account};
    use candid::Principal;

    fn config(governance: Principal) -> StreamConfig {
        let principal = |byte| Principal::from_slice(&[byte; 29]);
        let account = |owner, byte| Account {
            owner,
            subaccount: Some(vec![byte; 32]),
        };
        StreamConfig {
            io_ledger: principal(1),
            icp_ledger: principal(3),
            nns_manager: principal(4),
            jupiter_receipt_source: account(principal(4), 2),
            jupiter_io_account: account(principal(7), 3),
            sns_governance: governance,
            sns_root: principal(5),
            expected_sns_governance_module_hash: vec![1; 32],
            approved_reward_event_duration_seconds: 86_400,
            io_reserve: account(principal(6), 4),
            liquid_icp: account(principal(6), 5),
            nonredeemable_governance_io_accounts: Vec::new(),
            minimum_redemption_io_e8s: 20_000,
            expected_io_fee_e8s: 10_000,
            expected_icp_fee_e8s: 10_000,
            maximum_request_lifetime_nanos: 1_000_000,
            retry_delay_nanos: 1,
            ledger_deduplication_window_nanos: 2_000_000,
        }
    }

    fn snapshot(
        observation: StructuralStakeObservation,
        live: Vec<u64>,
        active: Option<u64>,
        pooled: u128,
        active_reward: u128,
    ) -> CanonicalRedemptionSnapshot {
        CanonicalRedemptionSnapshot {
            total_supply_e8s: 1_000,
            liquid_icp_e8s: 900,
            pooled_principal_e8s: pooled,
            total_claim_backing_e8s: 1_000,
            active_backing_io_e8s: 100,
            active_reward_io_e8s: active_reward,
            structural_stakes: vec![observation],
            active_unwind_generation: active,
            live_cohort_generations: live,
            ..Default::default()
        }
    }

    fn record(governance: Principal, state: StructuralStakeState) -> BackingRewardRecord {
        BackingRewardRecord {
            sns_neuron_id: vec![1; 32],
            staking_account: Account {
                owner: governance,
                subaccount: Some(vec![1; 32]),
            },
            latest_structural_state: state,
            status: BackingRewardStatus::ExitCommitted { generation: 7 },
            unresolved_cohort_generation: Some(7),
        }
    }

    fn observation(
        record: &BackingRewardRecord,
        state: StructuralStakeState,
    ) -> StructuralStakeObservation {
        StructuralStakeObservation {
            sns_neuron_id: record.sns_neuron_id.clone(),
            staking_account: record.staking_account.clone(),
            state,
            ledger_balance_e8s: 100,
            prior_record: Some(record.clone()),
        }
    }

    #[test]
    fn committed_exit_survives_every_structural_observation_until_cleanup() {
        let governance = Principal::from_slice(&[2; 29]);
        let record = record(governance, StructuralStakeState::Active);
        for state in [
            StructuralStakeState::Active,
            StructuralStakeState::Dissolving,
            StructuralStakeState::LiquidOrDissolved,
        ] {
            let updated = reconcile(
                std::slice::from_ref(&record),
                &snapshot(observation(&record, state), vec![7], None, 100, 0),
                20,
                &config(governance),
            )
            .unwrap();
            assert_eq!(
                updated[0].status,
                BackingRewardStatus::ExitCommitted { generation: 7 }
            );
            assert_eq!(updated[0].unresolved_cohort_generation, Some(7));
            assert_eq!(updated[0].latest_structural_state, state);
        }
    }

    #[test]
    fn cleanup_retires_generation_before_reward_reentry() {
        let governance = Principal::from_slice(&[2; 29]);
        let record = record(governance, StructuralStakeState::Active);
        let active = reconcile(
            std::slice::from_ref(&record),
            &snapshot(
                observation(&record, StructuralStakeState::Active),
                vec![],
                None,
                100,
                0,
            ),
            20,
            &config(governance),
        )
        .unwrap();
        assert_eq!(
            active[0].status,
            BackingRewardStatus::ReentryPending {
                eligible_from_event: 21
            }
        );
        assert_eq!(active[0].unresolved_cohort_generation, None);

        let dissolving = reconcile(
            std::slice::from_ref(&record),
            &snapshot(
                observation(&record, StructuralStakeState::Dissolving),
                vec![],
                None,
                100,
                0,
            ),
            20,
            &config(governance),
        )
        .unwrap();
        assert_eq!(dissolving[0].status, BackingRewardStatus::ExitObserved);
        assert_eq!(dissolving[0].unresolved_cohort_generation, None);

        let liquid = reconcile(
            std::slice::from_ref(&record),
            &snapshot(
                observation(&record, StructuralStakeState::LiquidOrDissolved),
                vec![],
                None,
                100,
                0,
            ),
            20,
            &config(governance),
        )
        .unwrap();
        assert!(liquid.is_empty());
    }

    #[test]
    fn reward_reentry_waits_for_global_coverage_and_next_event() {
        let governance = Principal::from_slice(&[2; 29]);
        let mut record = record(governance, StructuralStakeState::Active);
        record.status = BackingRewardStatus::ReentryPending {
            eligible_from_event: 21,
        };
        record.unresolved_cohort_generation = None;
        let observed = observation(&record, StructuralStakeState::Active);
        let uncovered = reconcile(
            std::slice::from_ref(&record),
            &snapshot(observed.clone(), vec![], None, 0, 100),
            21,
            &config(governance),
        )
        .unwrap();
        assert!(matches!(
            uncovered[0].status,
            BackingRewardStatus::ReentryPending { .. }
        ));
        let covered = reconcile(
            std::slice::from_ref(&record),
            &snapshot(observed, vec![], None, 100, 100),
            21,
            &config(governance),
        )
        .unwrap();
        assert!(matches!(
            covered[0].status,
            BackingRewardStatus::ActiveEligible { .. }
        ));
    }
}
