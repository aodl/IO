use super::*;

#[test]
fn account_identifier_normalizes_zero_subaccount() {
    let owner = Principal::from_slice(&[1]);
    let implicit = Account {
        owner,
        subaccount: None,
    };
    let explicit = Account {
        owner,
        subaccount: Some(vec![0; 32]),
    };
    assert_eq!(
        icp_account_identifier(&implicit),
        icp_account_identifier(&explicit)
    );
}

#[test]
fn exact_match_rejects_changed_transfer_semantics() {
    let account = Account {
        owner: Principal::from_slice(&[1]),
        subaccount: None,
    };
    let transfer = ExactIcrcTransfer {
        from: account.clone(),
        to: Account {
            owner: Principal::from_slice(&[2]),
            subaccount: None,
        },
        amount_e8s: 10,
        fee_e8s: Some(1),
        memo: Some(vec![7]),
        created_at_time: Some(8),
        spender: None,
    };
    assert!(transfer
        .matches(&ExpectedIcrcTransfer {
            from: &account,
            to: &transfer.to,
            amount_e8s: 10,
            fee_e8s: Some(1),
            memo: Some(&[7]),
            created_at_time: Some(8),
            spender: None,
        })
        .unwrap());
    assert!(!transfer
        .matches(&ExpectedIcrcTransfer {
            from: &account,
            to: &transfer.to,
            amount_e8s: 11,
            fee_e8s: Some(1),
            memo: Some(&[7]),
            created_at_time: Some(8),
            spender: None,
        })
        .unwrap());
}

#[test]
fn exact_icp_match_distinguishes_native_and_icrc_memos() {
    let transfer = ExactIcpTransfer {
        from: vec![1; 32],
        to: vec![2; 32],
        amount_e8s: 10,
        fee_e8s: 1,
        native_memo_u64: 0,
        icrc1_memo: Some(vec![7]),
        created_at_time: 8,
        spender: None,
    };
    let expected = ExpectedQueryBlockTransfer {
        from: &transfer.from,
        to: &transfer.to,
        amount_e8s: 10,
        fee_e8s: 1,
        native_memo_u64: 0,
        icrc1_memo: Some(&[7]),
        created_at_time: 8,
        spender: None,
    };
    assert!(transfer.matches(&expected));

    let wrong_native = ExpectedQueryBlockTransfer {
        native_memo_u64: 7,
        ..expected
    };
    assert!(!transfer.matches(&wrong_native));
}

#[test]
fn exact_icp_mint_preserves_native_memo_without_icrc_memo() {
    let mint = ExactIcpMint {
        to: vec![3; 32],
        amount_e8s: 100_000_000,
        native_memo_u64: 1_234,
        icrc1_memo: None,
        created_at_time: 1_234_000_000_000,
    };
    assert!(mint.matches(&mint.to, 100_000_000, 1_234, None, 1_234_000_000_000,));
    assert!(!mint.matches(&mint.to, 100_000_000, 0, None, 1_234_000_000_000,));
}
