use crate::{
    api::LiquidReceiptProgress,
    receipt::{LiquidReceiptOperation, ReceiptPhase, TwoWeekSettlement},
    state::StreamConfig,
    transfer::OwnTransferIntent,
};

pub(crate) fn receipt_progress(operation: &LiquidReceiptOperation) -> LiquidReceiptProgress {
    match operation.phase() {
        ReceiptPhase::AwaitingReceipt => LiquidReceiptProgress::AwaitingReceipt,
        ReceiptPhase::ReceiptProved => LiquidReceiptProgress::ReceiptProved,
        ReceiptPhase::Settling => LiquidReceiptProgress::Settling,
        ReceiptPhase::Completed => LiquidReceiptProgress::Stuck(
            "completed receipt must have been cleared into typed replay".into(),
        ),
        ReceiptPhase::Stuck => LiquidReceiptProgress::Stuck(
            "exact receipt settlement proof or governance upgrade required".into(),
        ),
    }
}

pub(crate) fn validate(
    settlement: &TwoWeekSettlement,
    config: &StreamConfig,
) -> Result<(), String> {
    let recipients = settlement
        .recipients
        .iter()
        .try_fold(0u128, |sum, recipient| sum.checked_add(recipient.io_e8s))
        .ok_or("two-week recipient total overflow")?;
    if settlement.backed_io_pool_e8s
        != recipients
            .checked_add(settlement.forfeited_io_e8s)
            .and_then(|value| value.checked_add(settlement.rounding_dust_io_e8s))
            .ok_or("two-week settlement total overflow")?
        || settlement.recipient_index as usize > settlement.recipients.len()
    {
        return Err("two-week reward settlement totals are inconsistent".into());
    }
    let reserve = config.io_reserve.canonical()?.subaccount;
    let mut ids = std::collections::BTreeSet::new();
    let mut accounts = std::collections::BTreeSet::new();
    for (index, recipient) in settlement.recipients.iter().enumerate() {
        let account = recipient.destination.canonical()?;
        if recipient.sns_neuron_id.len() != 32
            || !ids.insert(recipient.sns_neuron_id.clone())
            || !accounts.insert(account)
            || recipient.io_e8s == 0
            || account.owner != config.sns_governance
            || account.subaccount.as_slice() != recipient.sns_neuron_id
            || config
                .nonredeemable_governance_io_accounts
                .iter()
                .try_fold(false, |matched, excluded| {
                    recipient
                        .destination
                        .effective_eq(excluded)
                        .map(|same| matched || same)
                })?
            || recipient.refresh_attempted
                && !matches!(
                    recipient.transfer.as_ref().map(|attempt| &attempt.state),
                    Some(crate::transfer::TransferState::Succeeded { .. })
                )
            || index < settlement.recipient_index as usize && !recipient.refresh_attempted
        {
            return Err("two-week reward recipient is inconsistent".into());
        }
        let Some(attempt) = &recipient.transfer else {
            continue;
        };
        attempt.validate()?;
        if !matches!(
            &attempt.intent,
            OwnTransferIntent::Icrc1 {
                ledger,
                from_subaccount,
                to,
                amount,
                fee,
                ..
            } if *ledger == config.io_ledger
                && *from_subaccount == reserve
                && to.effective_eq(&recipient.destination)?
                && *amount == recipient.io_e8s
                && *fee == config.expected_io_fee_e8s
        ) {
            return Err("two-week reward transfer intent is inconsistent".into());
        }
    }
    Ok(())
}
