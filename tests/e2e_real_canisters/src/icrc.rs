use candid::{decode_one, encode_one, CandidType, Nat, Principal};
use io_ledger_types::{IcrcAccount, IcrcTransferArg, IcrcTransferError};
use pocket_ic::PocketIc;
use serde::Deserialize;

pub const TOKEN_NAME: &str = "IO Local Real SNS Ledger";
pub const TOKEN_SYMBOL: &str = "IO";
pub const DECIMALS: u8 = 8;
pub const FEE_E8S: u64 = 10_000;

#[derive(Clone, Debug, CandidType, Deserialize)]
#[allow(clippy::large_enum_variant)]
pub enum LedgerArgument {
    Init(LedgerInitArgs),
    Upgrade(Option<LedgerUpgradeArgs>),
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct LedgerInitArgs {
    pub token_symbol: String,
    pub token_name: String,
    pub minting_account: IcrcAccount,
    pub fee_collector_account: Option<IcrcAccount>,
    pub transfer_fee: Nat,
    pub decimals: Option<u8>,
    pub max_memo_length: Option<u16>,
    pub metadata: Vec<(String, MetadataValue)>,
    pub initial_balances: Vec<(IcrcAccount, Nat)>,
    pub archive_options: ArchiveOptions,
    pub feature_flags: Option<FeatureFlags>,
    pub index_principal: Option<Principal>,
    pub maximum_number_of_accounts: Option<u64>,
    pub accounts_overflow_trim_quantity: Option<u64>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct LedgerUpgradeArgs {}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub enum MetadataValue {
    Nat(Nat),
    Int(i128),
    Text(String),
    Blob(Vec<u8>),
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct ArchiveOptions {
    pub num_blocks_to_archive: u64,
    pub trigger_threshold: u64,
    pub controller_id: Principal,
    pub cycles_for_archive_creation: Option<u64>,
    pub max_transactions_per_response: Option<u64>,
    pub max_message_size_bytes: Option<u64>,
    pub node_max_memory_size_bytes: Option<u64>,
    pub more_controller_ids: Option<Vec<Principal>>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct FeatureFlags {
    pub icrc2: bool,
    pub icrc152: bool,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub enum IndexArg {
    Init(IndexInitArg),
    Upgrade(IndexUpgradeArg),
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct IndexInitArg {
    pub ledger_id: Principal,
    pub retrieve_blocks_from_ledger_interval_seconds: Option<u64>,
    pub min_retrieve_blocks_from_ledger_interval_seconds: Option<u64>,
    pub max_retrieve_blocks_from_ledger_interval_seconds: Option<u64>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct IndexUpgradeArg {
    pub ledger_id: Option<Principal>,
    pub retrieve_blocks_from_ledger_interval_seconds: Option<u64>,
    pub min_retrieve_blocks_from_ledger_interval_seconds: Option<u64>,
    pub max_retrieve_blocks_from_ledger_interval_seconds: Option<u64>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct GetAccountTransactionsArgs {
    pub account: IcrcAccount,
    pub start: Option<Nat>,
    pub max_results: Nat,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct GetTransactionsResult {
    pub balance: Nat,
    pub transactions: Vec<TransactionWithId>,
    pub oldest_tx_id: Option<Nat>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct TransactionWithId {
    pub id: Nat,
    pub transaction: IndexTransaction,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct IndexTransaction {
    pub burn: Option<Transfer>,
    pub mint: Option<Transfer>,
    pub approve: Option<Approve>,
    pub transfer: Option<Transfer>,
    pub timestamp: u64,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct Transfer {
    pub from: IcrcAccount,
    pub to: IcrcAccount,
    pub amount: Nat,
    pub fee: Option<Nat>,
    pub memo: Option<Vec<u8>>,
    pub created_at_time: Option<u64>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct Approve {
    pub from: IcrcAccount,
    pub spender: IcrcAccount,
    pub amount: Nat,
    pub expected_allowance: Option<Nat>,
    pub expires_at: Option<u64>,
    pub fee: Option<Nat>,
    pub memo: Option<Vec<u8>>,
    pub created_at_time: Option<u64>,
}

pub fn account(owner: Principal, subaccount: Option<[u8; 32]>) -> IcrcAccount {
    IcrcAccount {
        owner,
        subaccount: subaccount.map(|bytes| bytes.to_vec()),
    }
}

pub fn subaccount(label: &str) -> [u8; 32] {
    let bytes = label.as_bytes();
    let mut subaccount = [0; 32];
    let len = bytes.len().min(31);
    subaccount[0] = len as u8;
    subaccount[1..=len].copy_from_slice(&bytes[..len]);
    subaccount
}

pub fn ledger_init_arg(
    controller: Principal,
    minting_account: IcrcAccount,
    initial_balances: Vec<(IcrcAccount, u64)>,
) -> Vec<u8> {
    encode_one(LedgerArgument::Init(LedgerInitArgs {
        token_symbol: TOKEN_SYMBOL.to_string(),
        token_name: TOKEN_NAME.to_string(),
        minting_account,
        fee_collector_account: None,
        transfer_fee: Nat::from(FEE_E8S),
        decimals: Some(DECIMALS),
        max_memo_length: None,
        metadata: Vec::new(),
        initial_balances: initial_balances
            .into_iter()
            .map(|(account, amount)| (account, Nat::from(amount)))
            .collect(),
        archive_options: ArchiveOptions {
            num_blocks_to_archive: 1_000,
            trigger_threshold: 2_000,
            controller_id: controller,
            cycles_for_archive_creation: None,
            max_transactions_per_response: None,
            max_message_size_bytes: None,
            node_max_memory_size_bytes: None,
            more_controller_ids: None,
        },
        feature_flags: Some(FeatureFlags {
            icrc2: true,
            icrc152: false,
        }),
        index_principal: None,
        maximum_number_of_accounts: None,
        accounts_overflow_trim_quantity: None,
    }))
    .expect("ledger init args should encode")
}

pub fn ledger_upgrade_arg() -> Vec<u8> {
    encode_one(LedgerArgument::Upgrade(None::<LedgerUpgradeArgs>))
        .expect("ledger upgrade args should encode")
}

pub fn index_init_arg(ledger: Principal) -> Vec<u8> {
    encode_one(Some(IndexArg::Init(IndexInitArg {
        ledger_id: ledger,
        retrieve_blocks_from_ledger_interval_seconds: None,
        min_retrieve_blocks_from_ledger_interval_seconds: Some(1),
        max_retrieve_blocks_from_ledger_interval_seconds: Some(1),
    })))
    .expect("index init args should encode")
}

pub fn index_upgrade_arg() -> Vec<u8> {
    encode_one(Some(IndexArg::Upgrade(IndexUpgradeArg {
        ledger_id: None,
        retrieve_blocks_from_ledger_interval_seconds: None,
        min_retrieve_blocks_from_ledger_interval_seconds: Some(1),
        max_retrieve_blocks_from_ledger_interval_seconds: Some(1),
    })))
    .expect("index upgrade args should encode")
}

pub fn query_one<T: for<'de> Deserialize<'de> + CandidType>(
    pic: &PocketIc,
    canister: Principal,
    method: &str,
    arg: impl CandidType,
) -> T {
    let bytes = pic
        .query_call(
            canister,
            Principal::anonymous(),
            method,
            encode_one(arg).expect("query arg should encode"),
        )
        .unwrap_or_else(|err| panic!("query {method} failed: {err:?}"));
    decode_one(&bytes).unwrap_or_else(|err| panic!("query {method} decode failed: {err:?}"))
}

pub fn update_one<T: for<'de> Deserialize<'de> + CandidType>(
    pic: &PocketIc,
    canister: Principal,
    caller: Principal,
    method: &str,
    arg: impl CandidType,
) -> T {
    let bytes = pic
        .update_call(
            canister,
            caller,
            method,
            encode_one(arg).expect("update arg should encode"),
        )
        .unwrap_or_else(|err| panic!("update {method} failed: {err:?}"));
    decode_one(&bytes).unwrap_or_else(|err| panic!("update {method} decode failed: {err:?}"))
}

pub fn icrc1_name(pic: &PocketIc, ledger: Principal) -> String {
    query_one(pic, ledger, "icrc1_name", ())
}

pub fn icrc1_symbol(pic: &PocketIc, ledger: Principal) -> String {
    query_one(pic, ledger, "icrc1_symbol", ())
}

pub fn icrc1_decimals(pic: &PocketIc, ledger: Principal) -> u8 {
    query_one(pic, ledger, "icrc1_decimals", ())
}

pub fn icrc1_fee(pic: &PocketIc, ledger: Principal) -> Nat {
    query_one(pic, ledger, "icrc1_fee", ())
}

pub fn icrc1_total_supply(pic: &PocketIc, ledger: Principal) -> Nat {
    query_one(pic, ledger, "icrc1_total_supply", ())
}

pub fn icrc1_balance_of(pic: &PocketIc, ledger: Principal, account: IcrcAccount) -> Nat {
    query_one(pic, ledger, "icrc1_balance_of", account)
}

pub fn icrc1_transfer(
    pic: &PocketIc,
    ledger: Principal,
    caller: Principal,
    arg: IcrcTransferArg,
) -> Result<Nat, IcrcTransferError> {
    update_one(pic, ledger, caller, "icrc1_transfer", arg)
}

pub fn get_account_transactions(
    pic: &PocketIc,
    index: Principal,
    account: IcrcAccount,
    start: Option<Nat>,
    max_results: u64,
) -> Result<GetTransactionsResult, String> {
    query_one(
        pic,
        index,
        "get_account_transactions",
        GetAccountTransactionsArgs {
            account,
            start,
            max_results: Nat::from(max_results),
        },
    )
}

pub fn transfer_arg(
    from_subaccount: Option<[u8; 32]>,
    to: IcrcAccount,
    amount: u64,
    fee: Option<u64>,
    memo: Option<&[u8]>,
    created_at_time: Option<u64>,
) -> IcrcTransferArg {
    IcrcTransferArg {
        from_subaccount: from_subaccount.map(|bytes| bytes.to_vec()),
        to,
        amount: Nat::from(amount),
        fee: fee.map(Nat::from),
        memo: memo.map(|bytes| bytes.to_vec()),
        created_at_time,
    }
}
