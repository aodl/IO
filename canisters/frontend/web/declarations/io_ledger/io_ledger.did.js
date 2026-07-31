export const idlFactory = ({ IDL }) => {
  const Account = IDL.Record({ owner: IDL.Principal, subaccount: IDL.Opt(IDL.Vec(IDL.Nat8)) });
  const ApproveArgs = IDL.Record({
    from_subaccount: IDL.Opt(IDL.Vec(IDL.Nat8)),
    spender: Account,
    amount: IDL.Nat,
    expected_allowance: IDL.Opt(IDL.Nat),
    expires_at: IDL.Opt(IDL.Nat64),
    fee: IDL.Opt(IDL.Nat),
    memo: IDL.Opt(IDL.Vec(IDL.Nat8)),
    created_at_time: IDL.Opt(IDL.Nat64),
  });
  const AllowanceArgs = IDL.Record({ account: Account, spender: Account });
  const Allowance = IDL.Record({ allowance: IDL.Nat, expires_at: IDL.Opt(IDL.Nat64) });
  return IDL.Service({
    icrc1_fee: IDL.Func([], [IDL.Nat], ["query"]),
    icrc2_allowance: IDL.Func([AllowanceArgs], [Allowance], ["query"]),
    icrc2_approve: IDL.Func([ApproveArgs], [IDL.Variant({ Ok: IDL.Nat, Err: IDL.Reserved })], []),
  });
};
