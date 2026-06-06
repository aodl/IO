import { byData, replaceList, setText, setWarnings } from "../dom-helpers.js";
import { artifactSummary, redemptionSummary, rewardSummary, streamSummary } from "../data/dashboard-transforms.js";
import { installRangeButtons, renderChart } from "./charts.js";

export function renderDashboard(document, view) {
  setText(byData(document, "statusLine"), view.statusLine);
  setText(byData(document, "lastUpdated"), view.lastUpdated);
  setWarnings(byData(document, "warnings"), view.warnings);

  for (const [key, value] of Object.entries(view.metrics)) {
    setText(byData(document, key), value);
  }

  for (const card of document.querySelectorAll("[data-chart]")) {
    installRangeButtons(card);
    renderChart(card.querySelector("[data-role='chart']"), view.charts[card.dataset.chart] ?? []);
    setText(card.querySelector("[data-role='primary-value']"), view.metrics[card.dataset.chart === "rate" ? "redemptionRate" : "redeemableSupply"]);
    setText(card.querySelector("[data-role='delta-value']"), "Live history required");
  }

  replaceList(document.querySelector("[data-list='streams']"), view.lists.streams, streamSummary);
  replaceList(document.querySelector("[data-list='redemptions']"), view.lists.redemptions, redemptionSummary);
  replaceList(document.querySelector("[data-list='rewards']"), view.lists.rewards, rewardSummary);
  replaceList(document.querySelector("[data-list='artifacts']"), view.lists.artifacts, artifactSummary);
}
