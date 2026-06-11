use crate::state::{
    IO_NNS_NEURON_MANAGER_SOURCE, JUPITER_FAUCET_SOURCE, TWO_WEEK_MATURITY_MEMO,
    TWO_YEAR_MATURITY_MEMO,
};
use crate::StreamManager;
use io_core_model::{
    preview_redeem_io, preview_stream, redeem_io, target_two_week_pool_e8s, ModelError,
    PreviewedRedemption, PreviewedStream, ProtocolState, RedemptionOutcome, StreamKind,
    StreamOutcome,
};
use io_governance_types::SnsNeuronEligibility;
use io_reward_policy::{active_staked_io_e8s, allocate_rewards, AllocationOutcome, NeuronSnapshot};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StreamManagerError {
    DuplicateTransaction,
    InvalidTransactionId,
    UnknownOrUnauthorizedStream { source: String, memo: String },
    Model(ModelError),
}

impl From<ModelError> for StreamManagerError {
    fn from(value: ModelError) -> Self {
        Self::Model(value)
    }
}

impl StreamManager {
    pub fn classify_stream(source: &str, memo: &str) -> Result<StreamKind, StreamManagerError> {
        match (source, memo) {
            (JUPITER_FAUCET_SOURCE, "") | (JUPITER_FAUCET_SOURCE, "faucet") => {
                Ok(StreamKind::JupiterFaucet)
            }
            (IO_NNS_NEURON_MANAGER_SOURCE, TWO_YEAR_MATURITY_MEMO) => {
                Ok(StreamKind::TwoYearMaturity)
            }
            (IO_NNS_NEURON_MANAGER_SOURCE, TWO_WEEK_MATURITY_MEMO) => {
                Ok(StreamKind::TwoWeekMaturity)
            }
            _ => Err(StreamManagerError::UnknownOrUnauthorizedStream {
                source: source.to_string(),
                memo: memo.to_string(),
            }),
        }
    }

    pub fn process_scanned_icp(
        &mut self,
        source: &str,
        memo: &str,
        amount_e8s: u128,
        transaction_id: impl Into<String>,
    ) -> Result<StreamOutcome, StreamManagerError> {
        let kind = Self::classify_stream(source, memo)?;
        self.process_authorized_stream(kind, amount_e8s, transaction_id)
    }

    pub fn process_authorized_stream(
        &mut self,
        kind: StreamKind,
        amount_e8s: u128,
        transaction_id: impl Into<String>,
    ) -> Result<StreamOutcome, StreamManagerError> {
        let tx = transaction_id.into();
        let preview = self.preview_authorized_stream(kind, amount_e8s, tx.clone())?;
        self.commit_previewed_stream(tx, preview.post_state)?;
        Ok(preview.outcome)
    }

    pub fn preview_authorized_stream(
        &self,
        kind: StreamKind,
        amount_e8s: u128,
        transaction_id: impl Into<String>,
    ) -> Result<PreviewedStream, StreamManagerError> {
        let tx = transaction_id.into();
        if tx.trim().is_empty() {
            return Err(StreamManagerError::InvalidTransactionId);
        }
        if self.processed_transactions.contains(&tx) {
            return Err(StreamManagerError::DuplicateTransaction);
        }
        preview_stream(&self.state, kind, amount_e8s).map_err(StreamManagerError::from)
    }

    pub fn commit_previewed_stream(
        &mut self,
        transaction_id: impl Into<String>,
        post_state: ProtocolState,
    ) -> Result<(), StreamManagerError> {
        let tx = transaction_id.into();
        if tx.trim().is_empty() {
            return Err(StreamManagerError::InvalidTransactionId);
        }
        if self.processed_transactions.contains(&tx) {
            return Err(StreamManagerError::DuplicateTransaction);
        }
        self.state = post_state;
        self.processed_transactions.insert(tx);
        Ok(())
    }

    pub fn redeem(&mut self, io_e8s: u128) -> Result<RedemptionOutcome, StreamManagerError> {
        redeem_io(&mut self.state, io_e8s).map_err(StreamManagerError::from)
    }

    pub fn preview_redemption(
        &self,
        io_e8s: u128,
        transaction_id: impl Into<String>,
    ) -> Result<PreviewedRedemption, StreamManagerError> {
        let tx = transaction_id.into();
        if tx.trim().is_empty() {
            return Err(StreamManagerError::InvalidTransactionId);
        }
        if self.processed_transactions.contains(&tx) {
            return Err(StreamManagerError::DuplicateTransaction);
        }
        preview_redeem_io(&self.state, io_e8s).map_err(StreamManagerError::from)
    }

    pub fn commit_previewed_redemption(
        &mut self,
        transaction_id: impl Into<String>,
        post_state: ProtocolState,
    ) -> Result<(), StreamManagerError> {
        self.commit_previewed_stream(transaction_id, post_state)
    }

    pub fn target_two_week_pool_e8s(&self) -> Result<u128, StreamManagerError> {
        let rate = self.state.redemption_rate()?;
        target_two_week_pool_e8s(
            self.active_staked_io_e8s,
            rate,
            self.two_week_pool_backing_bps,
        )
        .map_err(StreamManagerError::from)
    }

    pub fn refresh_active_staked_io_from_neurons(&mut self, neurons: &[NeuronSnapshot]) {
        self.active_staked_io_e8s = active_staked_io_e8s(neurons);
    }

    pub fn refresh_active_staked_io_from_sns_eligibility(
        &mut self,
        eligibilities: &[SnsNeuronEligibility],
    ) {
        self.active_staked_io_e8s = eligibilities
            .iter()
            .filter(|eligibility| eligibility.excluded_reason.is_none())
            .map(|eligibility| eligibility.eligible_stake_e8s)
            .sum();
    }

    pub fn allocate_two_week_maturity_io(
        &self,
        reward_pool_io_e8s: u128,
        neurons: &[NeuronSnapshot],
    ) -> AllocationOutcome {
        allocate_rewards(reward_pool_io_e8s, neurons)
    }
}
