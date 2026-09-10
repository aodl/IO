import { consentStageAndProcessRedemption } from "../app/redemption.js";

function text(node, value) {
  if (node) node.textContent = value;
}

export function mountRedemptionForm(document, actors, session) {
  const form = document.querySelector("[data-redemption-form]");
  const status = document.querySelector("[data-redemption-status]");
  if (!form) return;
  const submit = form.querySelector("button");
  if (!actors || !session?.identity || !(session.selectedSubaccount instanceof Uint8Array)
      || typeof session.requestTransferConsent !== "function") {
    text(status, "Connect a wallet that supplies one canonical subaccount and explicit ICRC-1 transfer consent.");
    submit.disabled = true;
    return;
  }

  let submitting = false;
  let outcomeUnknown = false;
  form.addEventListener("submit", async (event) => {
    event.preventDefault();
    if (submitting || outcomeUnknown) return;
    submitting = true;
    submit.disabled = true;
    try {
      text(status, "Awaiting consent to transfer IO into redemption staging");
      const result = await consentStageAndProcessRedemption({
        ...actors,
        selectedSubaccount: session.selectedSubaccount,
        ioAmountE8s: BigInt(form.elements.ioAmount.value),
        session,
      });
      text(status, result.wakeError
        ? "IO staged. Automatic processing will continue."
        : "IO staged. Processing is automatic and normally begins within about a minute.");
    } catch (error) {
      outcomeUnknown = error?.transferOutcomeUnknown === true;
      text(status, error?.message || String(error));
    } finally {
      submitting = false;
      submit.disabled = outcomeUnknown;
    }
  });
}
