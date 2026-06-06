# frontend

Certified asset canister for the IO browser dashboard.

Phase 1 public shell is live on mainnet as canister `6h2pa-qiaaa-aaaao-qp4fa-cai`.

- Gateway URL: `https://6h2pa-qiaaa-aaaao-qp4fa-cai.icp0.io/`
- Raw URL: `https://6h2pa-qiaaa-aaaao-qp4fa-cai.raw.icp0.io/`
- Historian canister consumed by the Phase 1 build: `yo47z-piaaa-aaaac-qg3xa-cai`

IO remains pre-launch. The canonical SNS IO ledger is not launched, no value-moving protocol canister is live, IO issuance is not live, and IO redemption is not live.

## Role

- Serves the IO landing/dashboard shell as certified static assets.
- Consumes `io_historian` production read APIs from the browser.
- Does not call `io_stream_manager` or `io_nns_neuron_manager`.
- Does not expose custom metrics or dashboard JSON routes.
- Is not protocol truth. Canonical value-moving facts remain in ledgers, indexes, governance canisters, release artifacts, and reviewed canister state transitions.

## Layout

- Rust canister: `src/lib.rs`
- Public certified assets: `public/`
- Browser source: `web/src/`
- Historian declarations: `web/declarations/io_historian/`
- Build script: `web/build-frontend.mjs`
- Frontend tests: `web/test/`

The build writes a content-hashed browser bundle to `public/generated/app.<hash>.js`, stamps `public/index.html` from `web/index.template.html`, and writes a private `public/generated/frontend-bundle.json` build manifest. The Rust router embeds `public/` at compile time and excludes that private manifest from routing.

## Data Path

The browser creates an actor from the production `io_historian.did` declarations and queries:

- `get_dashboard_state`
- `get_public_status`
- bounded list methods for recent streams, redemptions, and rewards

The loader preserves partial success. If one optional query fails, the dashboard renders the successful sections and shows a scoped warning. Missing values render as `-`; no production path fills gaps with mock metrics.

## Security And Cache Policy

The canister serves certified GET and HEAD responses. `/` aliases to `index.html`; unknown paths return certified `404.html`.

- `index.html`, `404.html`, and `.well-known/ic-domains`: `public, no-cache, no-store`
- generated bundles and assets: `public, max-age=31536000, immutable`
- CSP disallows inline scripts and inline styles
- no Google Fonts or third-party runtime network dependencies are loaded by the page

Standard response headers include HSTS, `X-Content-Type-Options`, `Referrer-Policy`, `Permissions-Policy`, COEP, COOP, CORP, and a restrictive CSP.

## Commands

```bash
npm run setup:frontend
npm run build:frontend
npm run test:frontend-unit
cargo test -p io-frontend
cargo run -p xtask -- frontend_required
```

`tools/scripts/build-canister io-frontend release` runs the browser build before compiling the frontend Wasm so release artifacts embed the stamped bundle.

For the Phase 1 public shell, the browser bundle was built with `CANISTER_ID_IO_HISTORIAN=yo47z-piaaa-aaaac-qg3xa-cai` so it reads from the deployed historian public read model. The release artifact manifest reference is `release-artifacts/manifest.json`.

## Visual Provenance

The visual direction comes from `io-frontend-mock.zip`: dark Io sphere hero, corner links, primary nav, IO/REAL LIQUID STAKING copy, coming-soon tagline, and glassy metric cards. The production implementation self-hosts the image assets and omits the mock's base64 `texture-data.js` payload.

## Limitations

- Custom-domain certification setup is not implemented.
- Production historian canister IDs are injected by build/runtime config and may be empty in local builds.
- Historian production ingestion remains separate from this frontend.
- The frontend is a dashboard over historian observations, not a protocol authority.
- The existing IO neuron-owner canister `oae4c-3iaaa-aaaar-qb5qq-cai` and IO neuron `6345890886899317159` are not touched by the Phase 1 frontend.
