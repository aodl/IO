use io_core_model::{ProtocolState, E8S_PER_TOKEN};
use std::collections::BTreeSet;

pub const JUPITER_FAUCET_SOURCE: &str = "jupiter_faucet";
pub const IO_NNS_NEURON_MANAGER_SOURCE: &str = "io_nns_neuron_manager";
pub const TWO_YEAR_MATURITY_MEMO: &str = "two_year_maturity";
pub const TWO_WEEK_MATURITY_MEMO: &str = "two_week_maturity";

#[derive(Clone, Debug)]
pub struct StreamManager {
    pub state: ProtocolState,
    pub processed_transactions: BTreeSet<String>,
    pub active_staked_io_e8s: u128,
    pub two_week_pool_backing_bps: u128,
}

impl StreamManager {
    pub fn default_for_tests() -> Self {
        Self {
            state: ProtocolState::new(
                1_000_000 * E8S_PER_TOKEN,
                900_000 * E8S_PER_TOKEN,
                100_000 * E8S_PER_TOKEN,
            ),
            processed_transactions: BTreeSet::new(),
            active_staked_io_e8s: 0,
            two_week_pool_backing_bps: 10_000,
        }
    }
}
