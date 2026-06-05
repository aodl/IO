use candid::CandidType;
use serde::Deserialize;

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct SchedulerTickOutcome {
    pub checked_two_year_maturity: u64,
    pub checked_two_week_maturity: u64,
    pub planned_pool_rebalances: u64,
    pub checked_ready_unwind_neurons: u64,
    pub planned_steps: Vec<String>,
}

impl SchedulerTickOutcome {
    fn no_work_configured() -> Self {
        Self {
            checked_two_year_maturity: 0,
            checked_two_week_maturity: 0,
            planned_pool_rebalances: 0,
            checked_ready_unwind_neurons: 0,
            planned_steps: vec![
                "check and disburse 2-year maturity".to_string(),
                "check and disburse 2-week maturity".to_string(),
                "rebalance pooled 2-week neuron".to_string(),
                "disburse ready unwind child neurons".to_string(),
            ],
        }
    }
}

pub fn scheduler_tick_once() -> SchedulerTickOutcome {
    SchedulerTickOutcome::no_work_configured()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_is_idempotent_without_configured_work() {
        assert_eq!(scheduler_tick_once(), scheduler_tick_once());
    }

    #[test]
    fn outcome_is_debuggable_and_candid_serializable() {
        let outcome = scheduler_tick_once();
        assert!(format!("{outcome:?}").contains("planned_steps"));
        candid::encode_one(outcome).unwrap();
    }
}
