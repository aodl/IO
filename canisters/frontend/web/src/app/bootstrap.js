import { runtimeConfig } from "./config.js";
import { createHistorianActor, createRedemptionActors } from "./agent.js";
import { loadHistorianDashboard } from "../data/historian-loaders.js";
import { transformDashboard } from "../data/dashboard-transforms.js";
import { renderDashboard } from "../ui/dashboard-renderer.js";
import { initHeroSphere } from "../ui/hero-sphere.js";
import { mountRedemptionForm } from "../ui/redemption-form.js";

export async function bootstrap(document, config = runtimeConfig) {
  initHeroSphere(document, window);
  const redemptionSession = window.ioRedemptionSession;
  mountRedemptionForm(
    document,
    createRedemptionActors(config, redemptionSession?.identity),
    redemptionSession,
  );
  const actor = createHistorianActor(config);
  const result = await loadHistorianDashboard(actor, config);
  renderDashboard(document, transformDashboard(result));
}
