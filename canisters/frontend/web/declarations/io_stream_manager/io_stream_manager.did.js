export const idlFactory = ({ IDL }) => {
  const Account = IDL.Record({ owner: IDL.Principal, subaccount: IDL.Opt(IDL.Vec(IDL.Nat8)) });
  const RedemptionResult = IDL.Record({
    request_fingerprint: IDL.Vec(IDL.Nat8),
    nonce: IDL.Nat64,
    io_block: IDL.Nat,
    icp_block: IDL.Nat,
    gross_icp_e8s: IDL.Nat,
    net_icp_e8s: IDL.Nat,
    io_fee_e8s: IDL.Nat,
    icp_fee_e8s: IDL.Nat,
    completed_at_nanos: IDL.Nat64,
  });
  const ApiError = IDL.Variant({
    Anonymous: IDL.Null,
    Unauthorized: IDL.Null,
    Paused: IDL.Null,
    Busy: IDL.Null,
    WrongNonce: IDL.Record({ expected: IDL.Nat64 }),
    NonceAlreadyUsed: IDL.Null,
    Invalid: IDL.Text,
    Ledger: IDL.Text,
    Pending: IDL.Text,
    Stuck: IDL.Text,
  });
  const RedemptionProgress = IDL.Variant({
    Preparing: IDL.Null,
    IoPullSubmitted: IDL.Null,
    IoInReserve: IDL.Null,
    PayoutSubmitted: IDL.Null,
    PayoutSucceeded: IDL.Null,
    Completing: IDL.Null,
    Completed: RedemptionResult,
    Stuck: IDL.Text,
  });
  const StreamProgress = IDL.Variant({
    Redemption: RedemptionProgress,
    LiquidReceipt: IDL.Reserved,
    Idle: IDL.Null,
  });
  const RedeemArgs = IDL.Record({
    from_subaccount: IDL.Opt(IDL.Vec(IDL.Nat8)),
    io_amount_e8s: IDL.Nat,
    min_icp_out_e8s: IDL.Nat,
    max_io_fee_e8s: IDL.Nat,
    max_icp_fee_e8s: IDL.Nat,
    expires_at_nanos: IDL.Nat64,
    nonce: IDL.Nat64,
  });
  const CallerState = IDL.Record({
    next_nonce: IDL.Nat64,
    last_request_fingerprint: IDL.Opt(IDL.Vec(IDL.Nat8)),
    last_result: IDL.Opt(RedemptionResult),
  });
  return IDL.Service({
    redeem: IDL.Func([RedeemArgs], [IDL.Variant({ Ok: RedemptionProgress, Err: ApiError })], []),
    resume: IDL.Func([], [IDL.Variant({ Ok: StreamProgress, Err: ApiError })], []),
    prove_active_transfer: IDL.Func([IDL.Nat], [IDL.Variant({ Ok: IDL.Null, Err: ApiError })], []),
    get_caller_redemption_state: IDL.Func([], [IDL.Variant({ Ok: CallerState, Err: ApiError })], ["query"]),
  });
};
