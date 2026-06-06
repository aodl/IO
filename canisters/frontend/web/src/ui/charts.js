const RANGES = ["week", "month", "year", "all"];

export function installRangeButtons(card, onRange = () => {}) {
  const container = card.querySelector(".metric-card__range");
  if (!container || container.children.length) return;
  for (const range of RANGES) {
    const button = document.createElement("button");
    button.className = `metric-card__range-button${range === "week" ? " is-active" : ""}`;
    button.type = "button";
    button.dataset.range = range;
    button.textContent = range[0].toUpperCase() + range.slice(1);
    button.addEventListener("click", () => {
      for (const peer of container.querySelectorAll("button")) peer.classList.remove("is-active");
      button.classList.add("is-active");
      onRange(range);
    });
    container.append(button);
  }
}

export function renderChart(container, series) {
  container.replaceChildren();
  if (!Array.isArray(series) || series.length < 2) {
    const empty = document.createElement("div");
    empty.className = "chart-empty";
    empty.textContent = "Insufficient live history";
    container.append(empty);
    return;
  }

  const width = 600;
  const height = 190;
  const values = series.map((point) => Number(point.value)).filter(Number.isFinite);
  const min = Math.min(...values);
  const max = Math.max(...values);
  const span = max - min || 1;
  const points = series.map((point, index) => {
    const x = (index / (series.length - 1)) * width;
    const y = height - ((Number(point.value) - min) / span) * (height - 28) - 14;
    return `${x.toFixed(1)},${y.toFixed(1)}`;
  });
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("class", "chart-svg");
  svg.setAttribute("viewBox", `0 0 ${width} ${height}`);
  const polyline = document.createElementNS("http://www.w3.org/2000/svg", "polyline");
  polyline.setAttribute("fill", "none");
  polyline.setAttribute("stroke", "rgba(247,238,168,.96)");
  polyline.setAttribute("stroke-width", "3");
  polyline.setAttribute("points", points.join(" "));
  svg.append(polyline);
  container.append(svg);
}
