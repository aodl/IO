use crate::{receipt::TwoWeekSettlement, state::StreamConfig, transfer::OwnTransferIntent};

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
            .checked_add(settlement.dust_io_e8s)
            .ok_or("two-week settlement total overflow")?
        || settlement.recipient_index as usize > settlement.recipients.len()
        || settlement.forfeited_io_e8s > settlement.dust_io_e8s
    {
        return Err("two-week reward settlement totals are inconsistent".into());
    }
    let reserve = config.io_reserve.canonical()?.subaccount;
    for recipient in &settlement.recipients {
        if recipient.sns_neuron_id.len() != 32
            || recipient.io_e8s == 0
            || recipient.destination.owner != config.sns_governance
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
