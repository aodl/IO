import { prepareRedemption, progressLabel, resumeRedemption, submitRedemption } from "../app/redemption.js";

function text(node, value) {
  if (node) node.textContent = value;
}

export function mountRedemptionForm(document, actors, session) {
  const form = document.querySelector("[data-redemption-form]");
  const status = document.querySelector("[data-redemption-status]");
  const resume = document.querySelector("[data-redemption-resume]");
  const proof = document.querySelector("[data-redemption-proof]");
  if (!form) return;
  if (!actors || !session?.identity || !(session.selectedSubaccount instanceof Uint8Array)) {
    text(status, "Connect a wallet that supplies one canonical subaccount. Direct IO transfers are unsupported and will not start redemption.");
    form.querySelector("button").disabled = true;
    resume.disabled = true;
    proof.disabled = true;
    return;
  }
  form.addEventListener("submit", async (event) => {
    event.preventDefault();
    try {
      text(status, "Querying exact fee, allowance and caller nonce");
      const request = await prepareRedemption({
        ...actors,
        owner: session.identity.getPrincipal(),
        selectedSubaccount: session.selectedSubaccount,
        ioAmountE8s: BigInt(form.elements.ioAmount.value),
        minIcpOutE8s: BigInt(form.elements.minIcpOut.value),
        maxIcpFeeE8s: BigInt(form.elements.maxIcpFee.value),
        nowNanos: BigInt(Date.now()) * 1_000_000n,
      });
      const result = await submitRedemption({ ...actors, request });
      if ("Err" in result) throw new Error(JSON.stringify(result.Err));
      text(status, progressLabel(result.Ok));
    } catch (error) {
      text(status, error?.message || String(error));
    }
  });
  resume.addEventListener("click", async () => {
    const result = await resumeRedemption(actors.stream);
    text(status, "Ok" in result && "Redemption" in result.Ok ? progressLabel(result.Ok.Redemption) : JSON.stringify(result));
  });
  proof.addEventListener("click", async () => {
    const block = form.elements.proofBlock.value;
    if (!/^\d+$/.test(block)) return text(status, "Enter the exact ledger block index for a Stuck transfer.");
    const result = await actors.stream.prove_active_transfer(BigInt(block));
    text(status, "Ok" in result ? "Exact transfer proof accepted; resume the operation." : JSON.stringify(result.Err));
  });
}
