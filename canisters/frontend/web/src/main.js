import { bootstrap } from "./app/bootstrap.js";

bootstrap(document).catch((error) => {
  const node = document.querySelector("[data-field='warnings']");
  if (node) {
    node.textContent = `Frontend loader failed: ${error?.message || String(error)}`;
  }
});
