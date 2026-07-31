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
