export function shouldUseFallbackOnly(win = window) {
  const reduced = win.matchMedia?.("(prefers-reduced-motion: reduce)")?.matches;
  const small = win.matchMedia?.("(max-width: 820px)")?.matches;
  const coarse = win.matchMedia?.("(pointer: coarse)")?.matches;
  const limitedCpu = (win.navigator?.hardwareConcurrency || 8) <= 4;
  return Boolean(reduced || small || (coarse && limitedCpu));
}

export function initHeroSphere(root = document, win = window) {
  const hero = root.getElementById("hero");
  const canvas = root.getElementById("scene");
  const loading = root.getElementById("loading");
  if (!hero || !canvas) return { mode: "missing" };

  const focus = () => hero.classList.add("is-focused");
  if (root.readyState === "complete") focus();
  else win.addEventListener?.("load", focus, { once: true });

  if (shouldUseFallbackOnly(win)) {
    hero.classList.add("is-fallback-only");
    loading?.classList.add("is-hidden");
    return { mode: "fallback" };
  }

  const gl = canvas.getContext?.("webgl", { alpha: true, antialias: true, depth: false });
  if (!gl) {
    hero.classList.add("is-fallback-only");
    loading?.classList.add("is-hidden");
    return { mode: "fallback" };
  }

  let frame = 0;
  let running = true;
  function draw() {
    if (!running) return;
    frame += 1;
    gl.viewport(0, 0, canvas.width || 1, canvas.height || 1);
    gl.clearColor(0, 0, 0, 0);
    gl.clear(gl.COLOR_BUFFER_BIT);
    if (frame === 1) {
      hero.classList.add("is-ready");
      loading?.classList.add("is-hidden");
    }
    win.requestAnimationFrame?.(draw);
  }
  win.requestAnimationFrame?.(draw);
  root.addEventListener?.("visibilitychange", () => {
    running = root.visibilityState !== "hidden";
    if (running) win.requestAnimationFrame?.(draw);
  });
  return { mode: "webgl", stop: () => { running = false; } };
}
