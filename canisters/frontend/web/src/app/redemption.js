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

function principalText(principal) {
  return typeof principal?.toText === "function" ? principal.toText() : String(principal);
}

function subaccountHex(subaccount) {
  return Array.from(subaccount, (byte) => byte.toString(16).padStart(2, "0")).join("");
}

function attemptStorage(storage) {
  if (storage?.getItem && storage?.setItem && storage?.removeItem) return storage;
  try {
    const candidate = globalThis.sessionStorage;
    if (candidate?.getItem && candidate?.setItem && candidate?.removeItem) return candidate;
  } catch {
    // Fail closed below.
  }
  throw new Error("Redemption requires browser session storage so an uncertain transfer can be recovered safely");
}

function encodeAttempt(attempt) {
  return JSON.stringify(attempt, (_, value) => typeof value === "bigint" ? `${value}` : value);
}

function emptyReceiptState() {
  return {
    current: null,
    previousReceipt: null,
    previousCompleted: null,
    nextIntentSequence: 1,
    lastCreatedAtTime: 0n,
  };
}

function decodeReceiptState(value) {
  if (!value) return emptyReceiptState();
  try {
    const state = JSON.parse(value);
    const decode = (attempt) => attempt == null ? null : {
      ...attempt,
      amount: BigInt(attempt.amount),
      fee: BigInt(attempt.fee),
      createdAtTime: BigInt(attempt.createdAtTime),
      transferBlock: attempt.transferBlock == null ? null : BigInt(attempt.transferBlock),
    };
    return {
      current: decode(state.current),
      previousReceipt: decode(state.previousReceipt),
      previousCompleted: decode(state.previousCompleted),
      nextIntentSequence: Number(state.nextIntentSequence ?? 1),
      lastCreatedAtTime: BigInt(state.lastCreatedAtTime ?? 0),
    };
  } catch {
    return emptyReceiptState();
  }
}

function duplicateBlock(error) {
  const duplicate = error?.Duplicate ?? error?.duplicate;
  const block = duplicate?.duplicate_of ?? duplicate?.duplicateOf ?? duplicate;
  return block == null ? null : BigInt(block);
}

function effectiveSubaccount(value) {
  const bytes = Array.isArray(value)
    ? (value.length === 0 ? null : value[0])
    : value;
  if (bytes == null) return new Uint8Array(32);
  const canonical = new Uint8Array(bytes);
  return canonical.length === 32 ? canonical : null;
}

function sameAccount(left, right) {
  const leftSub = effectiveSubaccount(left?.subaccount);
  const rightSub = effectiveSubaccount(right?.subaccount);
  return principalText(left?.owner) === principalText(right?.owner)
    && leftSub !== null
    && rightSub !== null
    && leftSub.every((value, index) => value === rightSub[index]);
}

function accountIdentity(account) {
  const subaccount = effectiveSubaccount(account?.subaccount);
  if (subaccount === null) throw new Error("staging Account has an invalid subaccount");
  return `${principalText(account.owner)}:${subaccountHex(subaccount)}`;
}

function pendingProgress() {
  return { Ok: { Pending: null } };
}

function candidOptional(value) {
  return Array.isArray(value) ? (value[0] ?? null) : (value ?? null);
}

export function completionMatches(receipt, sourceAccount, completed) {
  return receipt?.transferBlock != null
    && completed != null
    && BigInt(completed.source_io_block) === receipt.transferBlock
    && sameAccount(completed.source_account, sourceAccount);
}

export function safeCandidText(value) {
  try {
    return JSON.stringify(value, (_, field) => typeof field === "bigint" ? `${field}` : field);
  } catch {
    return String(value);
  }
}

function receiptKey(session, owner, sourceSubaccount) {
  return `io-redemption:${session.network}:${principalText(owner)}:${subaccountHex(sourceSubaccount)}`;
}

function saveReceiptState(store, key, state) {
  store.setItem(key, encodeAttempt(state));
}

function loadReceiptState(store, key) {
  return decodeReceiptState(store.getItem(key));
}

function locateOutstanding(state, intentId) {
  for (const slot of ["current", "previousReceipt"]) {
    if (state[slot]?.intentId === intentId) return { slot, attempt: state[slot] };
  }
  return null;
}

function latestReceipt(receipts) {
  return receipts.reduce((latest, receipt) => (
    latest == null || receipt.createdAtTime > latest.createdAtTime ? receipt : latest
  ), null);
}

function mergeCompletionEvidence(store, key, sourceAccount, candidates) {
  const state = loadReceiptState(store, key);
  const resolved = [];
  for (const candidate of candidates.filter(Boolean)) {
    for (const slot of ["current", "previousReceipt"]) {
      const receipt = state[slot];
      if (completionMatches(receipt, sourceAccount, candidate)) {
        resolved.push({ ...receipt, status: "completed" });
        state[slot] = null;
      }
    }
  }
  if (resolved.length > 0) {
    state.previousCompleted = latestReceipt([state.previousCompleted, ...resolved].filter(Boolean));
    saveReceiptState(store, key, state);
  }
  return state;
}

function receiptView(state, sourceAccount, workerResult, workerError, statusError) {
  const outstanding = state.current ?? state.previousReceipt;
  const receipt = outstanding ?? state.previousCompleted;
  const completed = !outstanding && state.previousCompleted != null;
  return {
    hasReceipt: receipt != null,
    transferBlock: receipt?.transferBlock ?? null,
    sourceAccount,
    progress: completed ? { Ok: { Completed: null } } : pendingProgress(),
    workerResult,
    workerError,
    statusError,
    processingPending: outstanding != null,
    reviewRequired: outstanding?.status === "reviewRequired",
    submissionUncertain: ["submitted", "ambiguous", "unresolved"].includes(outstanding?.status),
  };
}

function beginDispatch(store, key, intentId) {
  const state = loadReceiptState(store, key);
  const located = locateOutstanding(state, intentId);
  if (!located) return { state, attempt: null, dispatchEpoch: null };
  const dispatchEpoch = Number(located.attempt.dispatchEpoch ?? 0) + 1;
  const attempt = {
    ...located.attempt,
    status: "submitted",
    hadAmbiguousSubmission: true,
    dispatchEpoch,
  };
  state[located.slot] = attempt;
  saveReceiptState(store, key, state);
  return { state, attempt, dispatchEpoch };
}

function acknowledgeDispatch(store, key, intentId, transferBlock) {
  const state = loadReceiptState(store, key);
  const located = locateOutstanding(state, intentId);
  if (located) {
    state[located.slot] = { ...located.attempt, status: "staged", transferBlock };
    saveReceiptState(store, key, state);
  }
  return state;
}

function settleDispatchFailure(store, key, intentId, dispatchEpoch, error, definitiveFreshFailure) {
  const state = loadReceiptState(store, key);
  const located = locateOutstanding(state, intentId);
  if (!located
      || located.attempt.dispatchEpoch !== dispatchEpoch
      || located.attempt.status !== "submitted") {
    return { state, outcome: "stale" };
  }
  if (definitiveFreshFailure) {
    state[located.slot] = null;
    saveReceiptState(store, key, state);
    return { state, outcome: "cleared" };
  }
  state[located.slot] = {
    ...located.attempt,
    status: error == null ? "ambiguous" : "reviewRequired",
    ...(error == null ? {} : { lastLedgerError: safeCandidText(error) }),
  };
  saveReceiptState(store, key, state);
  return { state, outcome: error == null ? "ambiguous" : "reviewRequired" };
}

async function observeReceipt({ stream, store, key, sourceAccount, promptWorker }) {
  let workerResult = null;
  let workerError = null;
  if (promptWorker) {
    try {
      workerResult = await stream.process_redemptions();
    } catch (error) {
      workerError = error;
    }
  }
  let statusResult = null;
  let statusError = null;
  if (typeof stream.get_status === "function") {
    try {
      statusResult = await stream.get_status();
    } catch (error) {
      statusError = error;
    }
  }
  const workerCompletion = workerResult?.Ok?.Completed ?? null;
  const latestCompletion = candidOptional(statusResult?.last_completed_redemption);
  const state = mergeCompletionEvidence(
    store,
    key,
    sourceAccount,
    [workerCompletion, latestCompletion],
  );
  return receiptView(state, sourceAccount, workerResult, workerError, statusError);
}

export async function checkRedemptionReceipt({
  stream,
  selectedSubaccount,
  session,
  storage,
  promptWorker = true,
}) {
  const sourceSubaccount = canonicalSubaccount(selectedSubaccount);
  const owner = session?.identity?.getPrincipal?.();
  if (!owner) throw new Error("wallet identity is unavailable");
  const sourceAccount = { owner, subaccount: [sourceSubaccount] };
  const store = attemptStorage(storage);
  const key = receiptKey(session, owner, sourceSubaccount);
  return observeReceipt({ stream, store, key, sourceAccount, promptWorker });
}

export function progressLabel(progress) {
  const key = Object.keys(progress ?? {})[0];
  if (key === "RateLimited") {
    return "Redemption staged — automatic processing runs approximately once per minute";
  }
  return ({
    Idle: "No staged redemption is ready",
    Pending: "IO staged; processing pending",
    Completed: "Completed",
    Stuck: "Stuck — exact transfer recovery requires operator review",
  })[key] ?? key ?? "Unknown";
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
  storage,
  nowNanos = () => BigInt(Date.now()) * 1_000_000n,
}) {
  const sourceSubaccount = canonicalSubaccount(selectedSubaccount);
  const amount = e8s(ioAmountE8s, "redemption amount");
  const owner = session?.identity?.getPrincipal?.();
  if (!owner) throw new Error("wallet identity is unavailable");
  const sourceAccount = { owner, subaccount: [sourceSubaccount] };
  const store = attemptStorage(storage);
  const attemptKey = receiptKey(session, owner, sourceSubaccount);
  const initialState = loadReceiptState(store, attemptKey);
  let attempt = initialState.current ?? initialState.previousReceipt;
  let staging;
  let dispatchEpoch;
  let definitiveFreshFailure = false;

  if (attempt && attempt.amount !== amount) {
    if (attempt.status !== "staged") {
      throw new Error("an unresolved staging attempt must be reviewed before a different redemption starts");
    }
    if (initialState.current && initialState.previousReceipt) {
      throw new Error("check the two retained staged redemptions before starting another redemption");
    }
    attempt = null;
  }

  if (!attempt) {
    const [ioFeeValue, stagingValue, minimumValue, balanceValue] = await Promise.all([
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
    staging = stagingValue;
    const consent = await session.requestTransferConsent(
      redemptionConsentTerms({ amount, sourceSubaccount, staging, ioFee, minimum }, session.network),
    );
    if (consent !== true) throw new Error("Wallet transfer consent was not granted");
    const receiptState = loadReceiptState(store, attemptKey);
    const initialCurrentId = initialState.current?.intentId ?? null;
    if (initialCurrentId) {
      const current = locateOutstanding(receiptState, initialCurrentId);
      if (current?.attempt.status === "staged" && current.slot === "current") {
        if (receiptState.previousReceipt) {
          throw new Error("check the two retained staged redemptions before starting another redemption");
        }
        receiptState.previousReceipt = current.attempt;
        receiptState.current = null;
      } else if (current && current.attempt.status !== "completed") {
        throw new Error("the retained staging attempt changed while consent was pending");
      }
    }
    if (receiptState.current) {
      throw new Error("another staging attempt became current while consent was pending");
    }
    const observedNow = e8s(nowNanos(), "created_at_time");
    const createdAtTime = observedNow > receiptState.lastCreatedAtTime
      ? observedNow
      : receiptState.lastCreatedAtTime + 1n;
    attempt = {
      intentId: `${createdAtTime}:${receiptState.nextIntentSequence}`,
      amount,
      fee: ioFee,
      stagingIdentity: accountIdentity(staging),
      createdAtTime,
      transferBlock: null,
      status: "submitted",
      hadAmbiguousSubmission: true,
      dispatchEpoch: 1,
    };
    receiptState.current = attempt;
    receiptState.nextIntentSequence += 1;
    receiptState.lastCreatedAtTime = createdAtTime;
    saveReceiptState(store, attemptKey, receiptState);
    dispatchEpoch = 1;
    definitiveFreshFailure = true;
  } else {
    staging = await stream.get_redemption_staging_account();
    const current = locateOutstanding(loadReceiptState(store, attemptKey), attempt.intentId);
    if (!current) {
      return receiptView(loadReceiptState(store, attemptKey), sourceAccount, null, null, null);
    }
    attempt = current.attempt;
    if (accountIdentity(staging) !== attempt.stagingIdentity) {
      throw new Error("stored transfer destination differs from canonical redemption staging");
    }
    if (attempt.status === "staged") {
      return observeReceipt({
        stream,
        store,
        key: attemptKey,
        sourceAccount,
        promptWorker: true,
      });
    }
    if (attempt.status === "reviewRequired") {
      const review = new Error("IO staging effect remains uncertain; review wallet or ledger history before any new redemption");
      review.transferAttemptReviewRequired = true;
      throw review;
    }
    const dispatch = beginDispatch(store, attemptKey, attempt.intentId);
    if (!dispatch.attempt) {
      return receiptView(dispatch.state, sourceAccount, null, null, null);
    }
    attempt = dispatch.attempt;
    dispatchEpoch = dispatch.dispatchEpoch;
  }

  if (attempt.status !== "staged") {
    let transfer;
    try {
      transfer = await ledger.icrc1_transfer({
        from_subaccount: [sourceSubaccount],
        to: staging,
        amount: attempt.amount,
        fee: [attempt.fee],
        memo: [],
        created_at_time: [attempt.createdAtTime],
      });
    } catch (error) {
      const settled = settleDispatchFailure(
        store,
        attemptKey,
        attempt.intentId,
        dispatchEpoch,
        null,
        false,
      );
      if (settled.outcome === "stale") {
        return receiptView(settled.state, sourceAccount, null, null, null);
      }
      const ambiguous = new Error("IO staging response unavailable; retry will preserve the same transfer identity");
      ambiguous.cause = error;
      ambiguous.transferAttemptPending = true;
      throw ambiguous;
    }
    const transferBlock = "Ok" in transfer ? BigInt(transfer.Ok) : duplicateBlock(transfer.Err);
    if (transferBlock === null) {
      const settled = settleDispatchFailure(
        store,
        attemptKey,
        attempt.intentId,
        dispatchEpoch,
        transfer.Err,
        definitiveFreshFailure,
      );
      if (settled.outcome === "reviewRequired") {
        const review = new Error("IO staging effect remains uncertain after retry; review wallet or ledger history before any new redemption");
        review.transferAttemptReviewRequired = true;
        throw review;
      }
      if (settled.outcome === "cleared") {
        throw new Error("ICRC-1 rejected the initial staging transfer without an effect");
      }
      return receiptView(settled.state, sourceAccount, null, null, null);
    }
    acknowledgeDispatch(store, attemptKey, attempt.intentId, transferBlock);
  }
  return observeReceipt({
    stream,
    store,
    key: attemptKey,
    sourceAccount,
    promptWorker: true,
  });
}

export async function processRedemptions(stream) {
  return stream.process_redemptions();
}
