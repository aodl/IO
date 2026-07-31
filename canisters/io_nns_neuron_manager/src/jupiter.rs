pub fn checked_split(gross_e8s: u128) -> Result<(u128, u128), String> {
    let stake = gross_e8s.checked_mul(40).ok_or("Jupiter split overflow")? / 100;
    let liquid = gross_e8s
        .checked_sub(stake)
        .ok_or("Jupiter split underflow")?;
    Ok((stake, liquid))
}

pub fn sequence_memo(sequence: u64) -> u64 {
    const JUPITER_DOMAIN: u64 = 0x494f_4a55_0000_0000;
    JUPITER_DOMAIN ^ sequence
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_checked_40_60_split() {
        assert_eq!(checked_split(101).unwrap(), (40, 61));
        assert_eq!(
            checked_split(u128::MAX),
            Err("Jupiter split overflow".into())
        );
    }
}
