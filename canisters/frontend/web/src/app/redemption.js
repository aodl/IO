const APPROVAL_LIFETIME_NANOS = 300_000_000_000n;
const REDEEM_LIFETIME_NANOS = 120_000_000_000n;

export function canonicalSubaccount(value) {
  if (!(value instanceof Uint8Array) || value.length !== 32) {
    throw new Error("wallet must provide one canonical 32-byte subaccount");
  }
  return new Uint8Array(value);
}

export function progressLabel(progress) {
  const key = Object.keys(progress ?? {})[0];
  return ({
    Pending: "Pending external proof or retry",
    Completed: "Completed",
    Stuck: "Stuck — submit the exact transfer block proof",
  })[key] ?? key ?? "Unknown";
}

export async function prepareRedemption({
  ledger,
  stream,
  owner,
  streamCanister,
  selectedSubaccount,
  ioAmountE8s,
  minIcpOutE8s,
  maxIcpFeeE8s,
  nowNanos,
}) {
  const subaccount = canonicalSubaccount(selectedSubaccount);
  const source = { owner, subaccount: [subaccount] };
  const spender = { owner: streamCanister, subaccount: [] };
  const [fee, allowance, callerState] = await Promise.all([
    ledger.icrc1_fee(),
    ledger.icrc2_allowance({ account: source, spender }),
    stream.get_caller_redemption_state(),
  ]);
  if (!("Ok" in callerState)) throw new Error(`nonce query failed: ${JSON.stringify(callerState.Err)}`);
  const amount = BigInt(ioAmountE8s);
  const requiredAllowance = amount + BigInt(fee);
  const createdAt = BigInt(nowNanos);
  const approvalExpires = createdAt + APPROVAL_LIFETIME_NANOS;
  const approvalMemo = new TextEncoder().encode(`IO:redeem:${callerState.Ok.next_nonce}`);
  return {
    approval: {
      from_subaccount: [subaccount],
      spender,
      amount: requiredAllowance,
      expected_allowance: [BigInt(allowance.allowance)],
      expires_at: [approvalExpires],
      fee: [BigInt(fee)],
      memo: [approvalMemo],
      created_at_time: [createdAt],
    },
    redeem: {
      from_subaccount: [subaccount],
      io_amount_e8s: amount,
      min_icp_out_e8s: BigInt(minIcpOutE8s),
      max_io_fee_e8s: BigInt(fee),
      max_icp_fee_e8s: BigInt(maxIcpFeeE8s),
      expires_at_nanos: createdAt + REDEEM_LIFETIME_NANOS,
      nonce: BigInt(callerState.Ok.next_nonce),
    },
  };
}

export async function submitRedemption({ ledger, stream, request }) {
  const approval = await ledger.icrc2_approve(request.approval);
  if (!("Ok" in approval)) throw new Error("ICRC-2 approval failed");
  return stream.redeem(request.redeem);
}

export function redemptionConsentTerms(request, network) {
  const approvalSource = canonicalSubaccount(request.approval.from_subaccount[0]);
  const redeemSource = canonicalSubaccount(request.redeem.from_subaccount[0]);
  if (!approvalSource.every((value, index) => value === redeemSource[index])) {
    throw new Error("approval and redemption source subaccounts differ");
  }
  return Object.freeze({
    action: "icrc2_approve_for_io_redemption",
    network,
    ioAmountE8s: request.redeem.io_amount_e8s,
    spender: request.approval.spender,
    selectedSourceSubaccount: redeemSource,
    exactAllowanceE8s: request.approval.amount,
    currentIoFeeE8s: request.approval.fee[0],
    expectedExistingAllowanceE8s: request.approval.expected_allowance[0],
    approvalExpiresAtNanos: request.approval.expires_at[0],
    approvalMemo: new Uint8Array(request.approval.memo[0]),
    approvalCreatedAtNanos: request.approval.created_at_time[0],
    redemptionNonce: request.redeem.nonce,
    minimumIcpOutputE8s: request.redeem.min_icp_out_e8s,
    maximumIcpFeeE8s: request.redeem.max_icp_fee_e8s,
    redemptionExpiresAtNanos: request.redeem.expires_at_nanos,
  });
}

export async function consentAndSubmitRedemption({ ledger, stream, request, session }) {
  const consent = await session.requestApprovalConsent(
    redemptionConsentTerms(request, session.network),
  );
  if (consent !== true) throw new Error("Wallet approval consent was not granted");
  return submitRedemption({ ledger, stream, request });
}

export async function resumeRedemption(stream) {
  return stream.resume();
}
