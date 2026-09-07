import {
  checkRedemptionReceipt,
  consentStageAndProcessRedemption,
  progressLabel,
  safeCandidText,
} from "../app/redemption.js";

function text(node, value) {
  if (node) node.textContent = value;
}

export function mountRedemptionForm(document, actors, session) {
  const form = document.querySelector("[data-redemption-form]");
  const status = document.querySelector("[data-redemption-status]");
  const process = document.querySelector("[data-redemption-process]");
  if (!form) return;
  if (!actors || !session?.identity || !(session.selectedSubaccount instanceof Uint8Array)
      || typeof session.requestTransferConsent !== "function") {
    text(status, "Connect a wallet that supplies one canonical subaccount and explicit ICRC-1 transfer consent.");
    form.querySelector("button").disabled = true;
    process.disabled = true;
    return;
  }
  const submit = form.querySelector("button");
  const storage = session.redemptionStorage;
  const renderReceipt = (result) => {
    if (!result.hasReceipt) {
      if (result.workerError) {
        text(status, `Global redemption worker unavailable: ${result.workerError.message || String(result.workerError)}`);
      } else if (result.workerResult?.Err) {
        text(status, `Global redemption worker: ${safeCandidText(result.workerResult.Err)}`);
      } else if (result.workerResult?.Ok) {
        text(status, `Global redemption worker: ${progressLabel(result.workerResult.Ok)}`);
      }
      return;
    }
    if (!result.processingPending) {
      text(status, "Completed");
      return;
    }
    let message = result.reviewRequired
      ? "IO staging effect uncertain; review wallet or ledger history"
      : result.submissionUncertain
        ? "IO staging response unavailable; safe retry will reuse the same transfer"
        : "IO staged; processing pending";
    if (result.workerError) {
      message += ` — global worker unavailable: ${result.workerError.message || String(result.workerError)}`;
    } else if (result.workerResult?.Err) {
      message += ` — global worker: ${safeCandidText(result.workerResult.Err)}`;
    } else if (result.workerResult?.Ok
      && !("Pending" in result.workerResult.Ok)) {
      message += ` — global worker: ${progressLabel(result.workerResult.Ok)}`;
    } else if (result.statusError) {
      message += ` — completion status unavailable: ${result.statusError.message || String(result.statusError)}`;
    }
    text(status, message);
  };
  const checkLocalReceipt = (promptWorker) => checkRedemptionReceipt({
    stream: actors.stream,
    selectedSubaccount: session.selectedSubaccount,
    session,
    storage,
    promptWorker,
  });
  let submitting = false;
  let viewOwner = 0;
  form.addEventListener("submit", async (event) => {
    event.preventDefault();
    if (submitting) return;
    submitting = true;
    const ownedView = ++viewOwner;
    submit.disabled = true;
    process.disabled = true;
    try {
      text(status, "Awaiting consent to transfer IO into redemption staging");
      const result = await consentStageAndProcessRedemption({
        ...actors,
        selectedSubaccount: session.selectedSubaccount,
        ioAmountE8s: BigInt(form.elements.ioAmount.value),
        session,
        storage,
      });
      if (ownedView === viewOwner) renderReceipt(result);
    } catch (error) {
      if (ownedView === viewOwner) {
        text(status, error?.transferAttemptPending
          ? "IO staging response unavailable; safe retry will reuse the same transfer"
          : error?.message || String(error));
      }
    } finally {
      submitting = false;
      submit.disabled = false;
      process.disabled = false;
    }
  });
  process.addEventListener("click", async () => {
    if (submitting) return;
    const ownedView = ++viewOwner;
    try {
      const result = await checkLocalReceipt(true);
      if (ownedView === viewOwner) renderReceipt(result);
    } catch (error) {
      if (ownedView === viewOwner) {
        text(status, `Unable to check staged redemption: ${error?.message || String(error)}`);
      }
    }
  });
  const restoreView = viewOwner;
  const ready = checkLocalReceipt(false)
    .then((result) => {
      if (restoreView === viewOwner) renderReceipt(result);
    })
    .catch((error) => {
      if (restoreView === viewOwner) {
        text(status, `Unable to restore staged receipt: ${error?.message || String(error)}`);
      }
    });
  return { ready };
}
