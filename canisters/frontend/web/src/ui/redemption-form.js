import { consentPushAndSettleRedemption, prepareRedemption, progressLabel, resumeRedemption } from "../app/redemption.js";

function text(node, value) {
  if (node) node.textContent = value;
}

export function mountRedemptionForm(document, actors, session) {
  const form = document.querySelector("[data-redemption-form]");
  const status = document.querySelector("[data-redemption-status]");
  const resume = document.querySelector("[data-redemption-resume]");
  const proof = document.querySelector("[data-redemption-proof]");
  if (!form) return;
  if (!actors || !session?.identity || !(session.selectedSubaccount instanceof Uint8Array)
      || typeof session.requestTransferConsent !== "function") {
    text(status, "Connect a wallet that supplies one canonical subaccount and explicit ICRC-1 transfer consent.");
    form.querySelector("button").disabled = true;
    resume.disabled = true;
    proof.disabled = true;
    return;
  }
  form.addEventListener("submit", async (event) => {
    event.preventDefault();
    try {
      text(status, "Preparing an exact push-redemption quote");
      const prepared = await prepareRedemption({
        ...actors,
        owner: session.identity.getPrincipal(),
        selectedSubaccount: session.selectedSubaccount,
        ioAmountE8s: BigInt(form.elements.ioAmount.value),
        minIcpOutE8s: BigInt(form.elements.minIcpOut.value),
        maxIcpFeeE8s: BigInt(form.elements.maxIcpFee.value),
        nowNanos: BigInt(Date.now()) * 1_000_000n,
      });
      text(status, "Awaiting consent to push the exact IO amount to the reserve");
      const result = await consentPushAndSettleRedemption({
        ...actors,
        prepared,
        session,
      });
      if ("Err" in result) throw new Error(JSON.stringify(result.Err));
      text(status, progressLabel(result.Ok));
    } catch (error) {
      text(status, error?.message || String(error));
    }
  });
  resume.addEventListener("click", async () => {
    const result = await resumeRedemption(actors.stream, session.identity.getPrincipal());
    text(status, "Ok" in result ? progressLabel(result.Ok) : JSON.stringify(result.Err));
  });
  proof.addEventListener("click", async () => {
    const block = form.elements.proofBlock.value;
    if (!/^\d+$/.test(block)) return text(status, "Enter the exact ledger block index for a Stuck transfer.");
    const result = await actors.stream.prove_active_transfer(BigInt(block));
    text(status, "Ok" in result ? "Exact transfer proof accepted; resume the operation." : JSON.stringify(result.Err));
  });
}
