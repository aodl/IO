use candid::{CandidType, Nat};
use io_ledger_types::{
    Account, IcrcAccount, IcrcTransferArg, IcrcTransferError, LedgerBlock, LedgerOperationKind,
    Memo, Subaccount,
};
use serde::Deserialize;
use std::cell::RefCell;

const DEFAULT_FEE_E8S: u128 = 10_000;

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct AccountBalanceArgs {
    pub account: String,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct TransferArgs {
    pub from: String,
    pub to: String,
    pub amount_e8s: u128,
    pub memo: String,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct LedgerTransaction {
    pub from: String,
    pub to: String,
    pub from_account: Option<Account>,
    pub to_account: Option<Account>,
    pub amount_e8s: u128,
    pub memo: String,
    pub memo_bytes: Option<Vec<u8>>,
    pub block_index: u64,
    pub timestamp: u64,
    pub native_memo_u64: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct DebugMintArgs {
    pub to: String,
    pub amount_e8s: u128,
    pub memo: String,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct DebugMintAccountArgs {
    pub to: IcrcAccount,
    pub amount_e8s: u128,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, CandidType, Deserialize)]
pub struct LedgerCallCounters {
    pub fee: u64,
    pub total_supply: u64,
    pub balance: u64,
    pub allowance: u64,
    pub transfer: u64,
    pub transfer_from: u64,
    pub query_blocks: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct DebugRejectAccountArgs {
    pub account: String,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct DebugTransferFailureArgs {
    pub account: String,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct DebugDuplicateResponseArgs {
    pub duplicate_of: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct DebugFeeArgs {
    pub fee_e8s: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct DebugNnsDisbursementArgs {
    pub from: IcrcAccount,
    pub to: IcrcAccount,
    pub amount_e8s: u128,
    pub native_memo_u64: u64,
}

#[derive(Default)]
struct LedgerState {
    balances: Vec<(String, u128)>,
    transactions: Vec<LedgerTransaction>,
    rejected_to_accounts: Vec<String>,
    duplicate_response: Option<u64>,
    fee_e8s: Option<u128>,
    call_counters: LedgerCallCounters,
}

thread_local! {
    static STATE: RefCell<LedgerState> = RefCell::new(LedgerState::default());
}

fn now() -> u64 {
    #[cfg(target_family = "wasm")]
    {
        ic_cdk::api::time()
    }
    #[cfg(not(target_family = "wasm"))]
    {
        0
    }
}

fn balance_of(state: &LedgerState, account: &str) -> u128 {
    state
        .balances
        .iter()
        .find(|(name, _)| name == account)
        .map(|(_, balance)| *balance)
        .unwrap_or(0)
}

fn fee_e8s(state: &LedgerState) -> u128 {
    state.fee_e8s.unwrap_or(DEFAULT_FEE_E8S)
}

fn set_balance(state: &mut LedgerState, account: &str, balance: u128) {
    match state.balances.iter_mut().find(|(name, _)| name == account) {
        Some((_, current)) => *current = balance,
        None => state.balances.push((account.to_string(), balance)),
    }
}

struct RecordTransfer {
    from: String,
    to: String,
    from_account: Option<Account>,
    to_account: Option<Account>,
    amount_e8s: u128,
    memo: String,
    memo_bytes: Option<Vec<u8>>,
    native_memo_u64: u64,
}

fn record(state: &mut LedgerState, transfer: RecordTransfer) -> u64 {
    let block_index = state.transactions.len() as u64;
    state.transactions.push(LedgerTransaction {
        from: transfer.from,
        to: transfer.to,
        from_account: transfer.from_account,
        to_account: transfer.to_account,
        amount_e8s: transfer.amount_e8s,
        memo: transfer.memo,
        memo_bytes: transfer.memo_bytes,
        block_index,
        timestamp: now(),
        native_memo_u64: transfer.native_memo_u64,
    });
    block_index
}

fn caller() -> candid::Principal {
    #[cfg(target_family = "wasm")]
    {
        ic_cdk::api::msg_caller()
    }
    #[cfg(not(target_family = "wasm"))]
    {
        candid::Principal::anonymous()
    }
}

fn mock_subaccount(label: &str) -> Subaccount {
    let bytes = label.as_bytes();
    let mut subaccount = [0; 32];
    let len = bytes.len().min(31);
    subaccount[0] = len as u8;
    subaccount[1..=len].copy_from_slice(&bytes[..len]);
    Subaccount(subaccount)
}

fn mock_label_from_subaccount(subaccount: &Subaccount) -> Option<String> {
    let len = subaccount.0[0] as usize;
    if len == 0 || len > 31 {
        return None;
    }
    std::str::from_utf8(&subaccount.0[1..=len])
        .ok()
        .map(ToString::to_string)
}

fn mock_label_from_account(account: &Account) -> String {
    if let Some(subaccount) = account.subaccount.as_ref() {
        if subaccount.0[..24].iter().all(|byte| *byte == 0) {
            let mut id = [0_u8; 8];
            id.copy_from_slice(&subaccount.0[24..]);
            let id = u64::from_be_bytes(id);
            if id != 0 {
                return format!("sns_neuron_{id}");
            }
        }
    }
    account
        .subaccount
        .as_ref()
        .and_then(mock_label_from_subaccount)
        .unwrap_or_else(|| account.owner.to_text())
}

fn account_from_icrc(account: IcrcAccount) -> Result<Account, IcrcTransferError> {
    let subaccount = match account.subaccount {
        Some(bytes) => Some(Subaccount(bytes.try_into().map_err(|bytes: Vec<u8>| {
            IcrcTransferError::GenericError {
                error_code: Nat::from(1_u64),
                message: format!("subaccount must be 32 bytes, got {}", bytes.len()),
            }
        })?)),
        None => None,
    };
    Ok(Account::new(account.owner, subaccount))
}

fn label_from_icrc(account: IcrcAccount) -> Result<String, IcrcTransferError> {
    Ok(mock_label_from_account(&account_from_icrc(account)?))
}

fn label_from_from_subaccount(subaccount: Option<Vec<u8>>) -> Result<String, IcrcTransferError> {
    let subaccount = subaccount
        .map(|bytes| {
            bytes.try_into().map(Subaccount).map_err(|bytes: Vec<u8>| {
                IcrcTransferError::GenericError {
                    error_code: Nat::from(1_u64),
                    message: format!("subaccount must be 32 bytes, got {}", bytes.len()),
                }
            })
        })
        .transpose()?;
    Ok(mock_label_from_account(&Account::new(caller(), subaccount)))
}

fn nat_to_u128(value: &Nat, field: &str) -> Result<u128, IcrcTransferError> {
    value
        .0
        .to_str_radix(10)
        .parse::<u128>()
        .map_err(|err| IcrcTransferError::GenericError {
            error_code: Nat::from(1_u64),
            message: format!("{field} does not fit in u128: {err}"),
        })
}

fn memo_to_string(memo: Option<Vec<u8>>) -> String {
    memo.map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .unwrap_or_default()
}

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn icrc1_fee() -> Nat {
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        state.call_counters.fee += 1;
        Nat::from(fee_e8s(&state))
    })
}

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn icrc1_total_supply() -> Nat {
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        state.call_counters.total_supply += 1;
        Nat::from(
            state
                .balances
                .iter()
                .fold(0_u128, |sum, (_, value)| sum.saturating_add(*value)),
        )
    })
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct SupportedStandard {
    pub name: String,
    pub url: String,
}

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn icrc1_supported_standards() -> Vec<SupportedStandard> {
    ["ICRC-1", "ICRC-2", "ICRC-3"]
        .into_iter()
        .map(|name| SupportedStandard {
            name: name.into(),
            url: format!("https://example.invalid/{name}"),
        })
        .collect()
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn icrc1_balance_of(account: IcrcAccount) -> Nat {
    let label = label_from_icrc(account).unwrap_or_else(|_| String::new());
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        state.call_counters.balance += 1;
        Nat::from(balance_of(&state, &label))
    })
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct AllowanceArgs {
    pub account: IcrcAccount,
    pub spender: IcrcAccount,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct Allowance {
    pub allowance: Nat,
    pub expires_at: Option<u64>,
}

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn icrc2_allowance(_args: AllowanceArgs) -> Allowance {
    STATE.with(|cell| cell.borrow_mut().call_counters.allowance += 1);
    Allowance {
        allowance: Nat::from(u128::MAX),
        expires_at: None,
    }
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn icrc1_transfer(args: IcrcTransferArg) -> Result<Nat, IcrcTransferError> {
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        state.call_counters.transfer += 1;
        if let Some(duplicate_of) = state.duplicate_response {
            state.duplicate_response = None;
            return Err(IcrcTransferError::Duplicate {
                duplicate_of: Nat::from(duplicate_of),
            });
        }
        if let Some(fee) = args.fee.as_ref() {
            let fee = nat_to_u128(fee, "fee")?;
            let current_fee = fee_e8s(&state);
            if fee != current_fee {
                return Err(IcrcTransferError::BadFee {
                    expected_fee: Nat::from(current_fee),
                });
            }
        }
        let from_subaccount = args
            .from_subaccount
            .clone()
            .map(|bytes| Subaccount(bytes.try_into().unwrap_or([0; 32])));
        let from_account = Account::new(caller(), from_subaccount);
        let to_account = account_from_icrc(args.to.clone())?;
        let from = label_from_from_subaccount(args.from_subaccount)?;
        let to = mock_label_from_account(&to_account);
        let amount_e8s = nat_to_u128(&args.amount, "amount")?;
        let memo_bytes = args.memo;
        let memo = memo_to_string(memo_bytes.clone());
        if state
            .rejected_to_accounts
            .iter()
            .any(|account| account == &to)
        {
            return Err(IcrcTransferError::TemporarilyUnavailable);
        }
        let from_balance = balance_of(&state, &from);
        if from_balance < amount_e8s {
            return Err(IcrcTransferError::InsufficientFunds {
                balance: Nat::from(from_balance),
            });
        }
        let to_balance = balance_of(&state, &to);
        set_balance(&mut state, &from, from_balance - amount_e8s);
        set_balance(&mut state, &to, to_balance.saturating_add(amount_e8s));
        Ok(Nat::from(record(
            &mut state,
            RecordTransfer {
                from,
                to,
                from_account: Some(from_account),
                to_account: Some(to_account),
                amount_e8s,
                memo,
                memo_bytes,
                native_memo_u64: 0,
            },
        )))
    })
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct TransferFromArg {
    pub spender_subaccount: Option<Vec<u8>>,
    pub from: IcrcAccount,
    pub to: IcrcAccount,
    pub amount: Nat,
    pub fee: Option<Nat>,
    pub memo: Option<Vec<u8>>,
    pub created_at_time: Option<u64>,
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn icrc2_transfer_from(args: TransferFromArg) -> Result<Nat, IcrcTransferError> {
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        state.call_counters.transfer_from += 1;
        if let Some(fee) = args.fee.as_ref() {
            let fee = nat_to_u128(fee, "fee")?;
            let current_fee = fee_e8s(&state);
            if fee != current_fee {
                return Err(IcrcTransferError::BadFee {
                    expected_fee: Nat::from(current_fee),
                });
            }
        }
        let from_account = account_from_icrc(args.from.clone())?;
        let to_account = account_from_icrc(args.to.clone())?;
        let from = mock_label_from_account(&from_account);
        let to = mock_label_from_account(&to_account);
        let amount_e8s = nat_to_u128(&args.amount, "amount")?;
        let debit_e8s = amount_e8s.checked_add(fee_e8s(&state)).ok_or_else(|| {
            IcrcTransferError::GenericError {
                error_code: Nat::from(1_u64),
                message: "transfer debit overflow".into(),
            }
        })?;
        let from_balance = balance_of(&state, &from);
        if from_balance < debit_e8s {
            return Err(IcrcTransferError::InsufficientFunds {
                balance: Nat::from(from_balance),
            });
        }
        set_balance(&mut state, &from, from_balance - debit_e8s);
        let to_balance = balance_of(&state, &to);
        set_balance(&mut state, &to, to_balance.saturating_add(amount_e8s));
        Ok(Nat::from(record(
            &mut state,
            RecordTransfer {
                from,
                to,
                from_account: Some(from_account),
                to_account: Some(to_account),
                amount_e8s,
                memo: memo_to_string(args.memo.clone()),
                memo_bytes: args.memo,
                native_memo_u64: 0,
            },
        )))
    })
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_reject_to(args: DebugRejectAccountArgs) {
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        if !state
            .rejected_to_accounts
            .iter()
            .any(|account| account == &args.account)
        {
            state.rejected_to_accounts.push(args.account);
        }
    });
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_set_transfer_failure(args: DebugTransferFailureArgs) {
    debug_reject_to(DebugRejectAccountArgs {
        account: args.account,
    });
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_set_duplicate_response(args: DebugDuplicateResponseArgs) {
    STATE.with(|cell| cell.borrow_mut().duplicate_response = Some(args.duplicate_of));
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_set_fee(args: DebugFeeArgs) {
    STATE.with(|cell| cell.borrow_mut().fee_e8s = Some(args.fee_e8s));
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_clear_rejections() {
    STATE.with(|cell| cell.borrow_mut().rejected_to_accounts.clear());
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_clear() {
    STATE.with(|cell| *cell.borrow_mut() = LedgerState::default());
}

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn debug_get_call_counters() -> LedgerCallCounters {
    STATE.with(|cell| cell.borrow().call_counters)
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_mint_account(args: DebugMintAccountArgs) -> u64 {
    let account = account_from_icrc(args.to).expect("debug mint Account must be canonical");
    let label = mock_label_from_account(&account);
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        let balance = balance_of(&state, &label);
        set_balance(&mut state, &label, balance.saturating_add(args.amount_e8s));
        record(
            &mut state,
            RecordTransfer {
                from: "mint".into(),
                to: label,
                from_account: None,
                to_account: Some(account),
                amount_e8s: args.amount_e8s,
                memo: "debug mint".into(),
                memo_bytes: None,
                native_memo_u64: 0,
            },
        )
    })
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_record_nns_disbursement(args: DebugNnsDisbursementArgs) -> u64 {
    let from = account_from_icrc(args.from).expect("debug NNS source Account must be canonical");
    let to = account_from_icrc(args.to).expect("debug NNS destination Account must be canonical");
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        record(
            &mut state,
            RecordTransfer {
                from: mock_label_from_account(&from),
                to: mock_label_from_account(&to),
                from_account: Some(from),
                to_account: Some(to),
                amount_e8s: args.amount_e8s,
                memo: args.native_memo_u64.to_string(),
                memo_bytes: None,
                native_memo_u64: args.native_memo_u64,
            },
        )
    })
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct QueryBlocksArgs {
    pub start: u64,
    pub length: u64,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct Tokens {
    pub e8s: u64,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct Timestamp {
    pub timestamp_nanos: u64,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub enum Operation {
    Transfer {
        from: Vec<u8>,
        to: Vec<u8>,
        amount: Tokens,
        fee: Tokens,
        spender: Option<Vec<u8>>,
    },
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct Transaction {
    pub memo: u64,
    pub icrc1_memo: Option<Vec<u8>>,
    pub operation: Option<Operation>,
    pub created_at_time: Timestamp,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct Block {
    pub transaction: Transaction,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct QueryBlocksResponse {
    pub blocks: Vec<Block>,
    pub first_block_index: u64,
    pub archived_blocks: Vec<ArchivedRange>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct ArchivedRange {
    pub start: u64,
    pub length: u64,
    callback: ArchiveCallback,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct BlockRange {
    pub blocks: Vec<Block>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub enum ArchiveResult {
    Ok(BlockRange),
    Err(candid::Reserved),
}

candid::define_function!(ArchiveCallback : (QueryBlocksArgs) -> (ArchiveResult) query);

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn query_blocks(args: QueryBlocksArgs) -> QueryBlocksResponse {
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        state.call_counters.query_blocks += 1;
        let transaction = state
            .transactions
            .get(args.start as usize)
            .filter(|_| args.length > 0)
            .and_then(|tx| {
                let from = tx.from_account.as_ref()?;
                let to = tx.to_account.as_ref()?;
                Some(Block {
                    transaction: Transaction {
                        memo: tx.native_memo_u64,
                        icrc1_memo: tx.memo_bytes.clone(),
                        operation: Some(Operation::Transfer {
                            from: io_ledger_boundary::icp_account_identifier(
                                &io_accounts::Account {
                                    owner: from.owner,
                                    subaccount: from
                                        .subaccount
                                        .as_ref()
                                        .map(|value| value.0.to_vec()),
                                },
                            )
                            .ok()?,
                            to: io_ledger_boundary::icp_account_identifier(&io_accounts::Account {
                                owner: to.owner,
                                subaccount: to.subaccount.as_ref().map(|value| value.0.to_vec()),
                            })
                            .ok()?,
                            amount: Tokens {
                                e8s: tx.amount_e8s.try_into().ok()?,
                            },
                            fee: Tokens {
                                e8s: fee_e8s(&state).try_into().ok()?,
                            },
                            spender: None,
                        }),
                        created_at_time: Timestamp {
                            timestamp_nanos: tx.timestamp.max(1),
                        },
                    },
                })
            });
        QueryBlocksResponse {
            first_block_index: args.start,
            blocks: transaction.into_iter().collect(),
            archived_blocks: Vec::new(),
        }
    })
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_mint(args: DebugMintArgs) -> u64 {
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        let balance = balance_of(&state, &args.to);
        set_balance(
            &mut state,
            &args.to,
            balance.saturating_add(args.amount_e8s),
        );
        record(
            &mut state,
            RecordTransfer {
                from: "mint".to_string(),
                to: args.to,
                from_account: None,
                to_account: None,
                amount_e8s: args.amount_e8s,
                memo: args.memo,
                memo_bytes: None,
                native_memo_u64: 0,
            },
        )
    })
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_get_transactions() -> Vec<LedgerTransaction> {
    STATE.with(|cell| cell.borrow().transactions.clone())
}

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn debug_get_boundary_transactions() -> Vec<LedgerBlock> {
    STATE.with(|cell| {
        let state = cell.borrow();
        let fee = fee_e8s(&state);
        state
            .transactions
            .iter()
            .map(|tx| LedgerBlock {
                block_index: io_ledger_types::BlockIndex(tx.block_index),
                timestamp_nanos: tx.timestamp,
                created_at_time: None,
                from: Some(Account::new(
                    candid::Principal::anonymous(),
                    Some(mock_subaccount(&tx.from)),
                )),
                to: Some(Account::new(
                    candid::Principal::anonymous(),
                    Some(mock_subaccount(&tx.to)),
                )),
                amount_e8s: tx.amount_e8s,
                fee_e8s: Some(fee),
                memo: Some(Memo::from(tx.memo.clone())),
                operation_kind: LedgerOperationKind::Transfer,
            })
            .collect()
    })
}
