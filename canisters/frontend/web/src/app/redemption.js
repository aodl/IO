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
    Pending: "Payout owed — waiting for exact recovery",
    Completed: "Completed",
    Stuck: "Stuck — submit the exact payout block proof",
  })[key] ?? key ?? "Unknown";
}

export async function prepareRedemption({
  ledger,
  stream,
  selectedSubaccount,
  ioAmountE8s,
  minIcpOutE8s,
  maxIcpFeeE8s,
  nowNanos,
}) {
  const subaccount = canonicalSubaccount(selectedSubaccount);
  const [fee, callerState] = await Promise.all([
    ledger.icrc1_fee(),
    stream.get_caller_redemption_state(),
  ]);
  if (!("Ok" in callerState)) {
    throw new Error(`nonce query failed: ${JSON.stringify(callerState.Err)}`);
  }
  const args = {
    from_subaccount: [subaccount],
    io_amount_e8s: BigInt(ioAmountE8s),
    min_icp_out_e8s: BigInt(minIcpOutE8s),
    max_io_fee_e8s: BigInt(fee),
    max_icp_fee_e8s: BigInt(maxIcpFeeE8s),
    expires_at_nanos: BigInt(nowNanos) + REDEEM_LIFETIME_NANOS,
    nonce: BigInt(callerState.Ok.next_nonce),
  };
  const result = await stream.prepare_redemption(args);
  if (!("Ok" in result)) throw new Error(`quote preparation failed: ${JSON.stringify(result.Err)}`);
  return result.Ok;
}

export function redemptionConsentTerms(prepared, network) {
  return Object.freeze({
    action: "icrc1_push_for_io_redemption",
    network,
    ioAmountE8s: prepared.request.io_amount_e8s,
    sourceSubaccount: canonicalSubaccount(prepared.account.subaccount[0]),
    reserveDestination: prepared.reserve,
    exactIoFeeE8s: prepared.snapshot.io_fee_e8s,
    exactMemo: new Uint8Array(prepared.push_memo),
    exactGrossIcpE8s: prepared.gross_icp_e8s,
    exactNetIcpE8s: prepared.net_icp_e8s,
    exactIcpFeeE8s: prepared.snapshot.icp_fee_e8s,
    redemptionNonce: prepared.request.nonce,
    transferExpiresAtNanos: prepared.request.expires_at_nanos,
  });
}

export async function consentPushAndSettleRedemption({ ledger, stream, prepared, session }) {
  const consent = await session.requestTransferConsent(
    redemptionConsentTerms(prepared, session.network),
  );
  if (consent !== true) throw new Error("Wallet transfer consent was not granted");
  const transfer = await ledger.icrc1_transfer({
    from_subaccount: prepared.account.subaccount,
    to: prepared.reserve,
    amount: prepared.request.io_amount_e8s,
    fee: [prepared.snapshot.io_fee_e8s],
    memo: [prepared.push_memo],
    created_at_time: [prepared.prepared_at_nanos],
  });
  if (!("Ok" in transfer)) throw new Error("ICRC-1 redemption push failed");
  return stream.settle_redemption(transfer.Ok);
}

export async function resumeRedemption(stream, caller) {
  return stream.resume_redemption(caller);
}
