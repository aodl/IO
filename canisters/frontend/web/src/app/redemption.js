const MAX_U128 = (1n << 128n) - 1n;

export function canonicalSubaccount(value) {
  if (!(value instanceof Uint8Array) || value.length !== 32) {
    throw new Error("wallet must provide one canonical 32-byte subaccount");
  }
  return new Uint8Array(value);
}

function e8s(value, field) {
  let parsed;
  try {
    parsed = typeof value === "bigint" ? value : BigInt(value);
  } catch {
    throw new Error(`${field} must be a positive integer`);
  }
  if (parsed <= 0n || parsed > MAX_U128) {
    throw new Error(`${field} must be a positive integer within the protocol bound`);
  }
  return parsed;
}

function duplicateBlock(error) {
  const duplicate = error?.Duplicate ?? error?.duplicate;
  const block = duplicate?.duplicate_of ?? duplicate?.duplicateOf ?? duplicate;
  return block == null ? null : BigInt(block);
}

function unknownTransfer(cause) {
  const error = new Error(
    "The transfer outcome is unknown. It may have succeeded. Check wallet or ledger history before submitting another redemption.",
  );
  error.cause = cause;
  error.transferOutcomeUnknown = true;
  return error;
}

export function redemptionConsentTerms({ amount, sourceSubaccount, staging, ioFee, minimum }, network) {
  return Object.freeze({
    action: "icrc1_transfer_to_io_redemption_staging",
    network,
    ioAmountE8s: BigInt(amount),
    minimumRedemptionIoE8s: BigInt(minimum),
    sourceSubaccount: canonicalSubaccount(sourceSubaccount),
    stagingDestination: staging,
    ioTransferFeeE8s: BigInt(ioFee),
    quoteStatus: "indicative_until_stream_accepts_staged_transfer",
    finalPayoutFeePolicy: "canonical_icp_fee_is_subtracted_from_frozen_gross",
    delayedProcessingPolicy: "staged_io_remains_claim_bearing_until_icp_payout_succeeds",
  });
}

export async function consentStageAndProcessRedemption({
  ledger,
  stream,
  selectedSubaccount,
  ioAmountE8s,
  session,
  nowNanos = () => BigInt(Date.now()) * 1_000_000n,
}) {
  const sourceSubaccount = canonicalSubaccount(selectedSubaccount);
  const amount = e8s(ioAmountE8s, "redemption amount");
  const owner = session?.identity?.getPrincipal?.();
  if (!owner) throw new Error("wallet identity is unavailable");
  const sourceAccount = { owner, subaccount: [sourceSubaccount] };
  const [ioFeeValue, staging, minimumValue, balanceValue] = await Promise.all([
    ledger.icrc1_fee(),
    stream.get_redemption_staging_account(),
    stream.get_minimum_redemption_io_e8s(),
    ledger.icrc1_balance_of(sourceAccount),
  ]);
  const ioFee = e8s(ioFeeValue, "IO fee");
  const minimum = e8s(minimumValue, "minimum redemption");
  const balance = BigInt(balanceValue);
  if (amount < minimum) {
    throw new Error(`redemption amount is below the configured minimum of ${minimum} e8s`);
  }
  if (amount + ioFee > balance) {
    throw new Error("wallet balance is insufficient for the redemption amount and IO fee");
  }
  const consent = await session.requestTransferConsent(
    redemptionConsentTerms({ amount, sourceSubaccount, staging, ioFee, minimum }, session.network),
  );
  if (consent !== true) throw new Error("Wallet transfer consent was not granted");

  let transfer;
  try {
    transfer = await ledger.icrc1_transfer({
      from_subaccount: [sourceSubaccount],
      to: staging,
      amount,
      fee: [ioFee],
      memo: [],
      created_at_time: [e8s(nowNanos(), "created_at_time")],
    });
  } catch (cause) {
    throw unknownTransfer(cause);
  }
  if (!transfer || typeof transfer !== "object" || !("Ok" in transfer || "Err" in transfer)) {
    throw unknownTransfer(new Error("ledger returned an unrecognized transfer response"));
  }
  let transferBlock;
  try {
    transferBlock = "Ok" in transfer ? BigInt(transfer.Ok) : duplicateBlock(transfer.Err);
  } catch (cause) {
    throw unknownTransfer(cause);
  }
  if (transferBlock === null) {
    throw new Error("IO was not staged: the ICRC-1 ledger rejected the transfer");
  }

  let wakeError = null;
  try {
    const result = await stream.process_redemptions();
    if (result?.Err) wakeError = result.Err;
  } catch (error) {
    wakeError = error;
  }
  return { transferBlock, wakeError };
}
