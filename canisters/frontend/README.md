# IO Frontend

## Role in IO

`io-frontend` is a certified asset canister for IO's browser dashboard and
authenticated redemption client. It is advisory and non-authoritative:
canonical monetary facts remain in ledgers, indexes, Governance/Root, release
artifacts, and reviewed manager state transitions, and the value-moving
canisters recompute every amount and fee.

The index canisters remain the normal source for bounded account-history
observation; the frontend does not scan raw ledgers or archives.

IO remains pre-launch. IO protocol is not live.
The SNS IO ledger remains not launched. The production frontend reservation
`torpp-zyaaa-aaaar-qb7xq-cai` is `ReservedNotLive`, empty/inert, and not live.

## Dependencies and data flow

The browser has two deliberately separate paths:

- The unauthenticated dashboard/read-model path creates only an Historian actor
  and calls `get_dashboard_state` and `get_public_status`.
- The authenticated redemption path creates an IO Ledger actor and an
  `io_stream_manager` actor using the wallet-supplied identity. It does not
  directly call `io_nns_neuron_manager`.

The Historian production Candid has no recent-stream, redemption, or reward
list methods, and the loader does not call any. It preserves partial success:
if one of its two queries fails, successful sections still render with a scoped
warning. Missing values render as `-`; no production path fills gaps with mock
metrics or treats missing/stale/error data as zero.

## Wallet integration contract

Production code prefers `window.ioWalletAdapter`. Its asynchronous `connect()`
result must provide:

- an authenticated `identity` with `getPrincipal()`;
- exactly one canonical 32-byte `Uint8Array` `selectedSubaccount`;
- a `network` string exactly equal to the configured frontend network; and
- a `requestApprovalConsent` function.

The frontend does not derive an Account from user-entered text and does not
silently select another subaccount. The production bundle resolves only
`window.ioWalletAdapter`; tests inject a fake implementation of that same
interface and ship no alternate authentication/session hook.

## Production API

The checked-in [production Candid](frontend.did) exposes only `http_request`
and the `version` query. Browser actors for Historian, the IO Ledger, and the
Stream Manager are outbound client dependencies; they are not frontend
canister methods. There is no frontend monetary, configuration, or ingestion
API.

## Authenticated redemption path

For the connected principal and selected subaccount, the client queries the IO
Ledger fee and balance plus Stream Manager's configured minimum and fixed
semantic redemption staging Account. Invalid, below-minimum, or unfunded input
is rejected before consent. The
wallet sees the IO amount, source subaccount, staging destination, ordinary IO
fee, exact network, the canonical ICP-fee policy, and the fact that the final
quote is not frozen until Stream accepts the staged transfer. Only affirmative
consent permits one ordinary `icrc1_transfer` to staging. It has no special
memo, canister nonce, expiry, allowance, or spender authority. The client keeps
one immutable `created_at_time` across an ambiguous transport retry and treats
ledger `Duplicate` as the original receipt. A later age, fee, or availability
error cannot erase an earlier ambiguous effect; the client retains a
review-required receipt and stops blind retransmission. A definitive rejection
of a live first dispatch may still be cleared. The possibly-effective dispatch
state is persisted before calling the ledger, and callbacks merge by immutable
intent after rereading storage so stale observations cannot regress newer work.

After recording the transfer block the client optionally calls permissionless, no-argument
`process_redemptions()` once. That call cannot select the block, amount, payout
Account, quote, or fee; the canonical ledger block supplies them. The UI renders
`Idle`, `Pending`, `RateLimited`, `Completed`, and `Stuck`, explains the roughly
one-minute automatic cadence, and never asks the user to send the IO again.
Worker failure is reported as staged/pending, and a returned completion is
shown only when its source block and Account match the locally retained receipt.
The same match applies to the manual worker button and the bounded
`get_status().last_completed_redemption` observation used after reload or timer
completion. Confirmed completion is cached locally; a later deliberate
same-amount submission receives a distinct client identity and consent.
Staged IO remains claim-bearing until canonical payout success. Fee drift or
insufficient liquid ICP can delay acceptance without creating payout debt.

## Certified assets, initialization, and cache policy

The build writes one content-hashed browser bundle to
`public/generated/app.<hash>.js`, stamps `public/index.html` from
`web/index.template.html`, and writes a private
`public/generated/frontend-bundle.json` build manifest. The Rust canister
recursively embeds `public/`, excludes the private manifest from routing, and
rebuilds/certifies the asset router on install and post-upgrade. It has no
monetary stable state.

The canister serves certified GET and HEAD responses. `/` aliases to
`index.html`; unknown paths return certified `404.html`.

- `index.html`, `404.html`, and `.well-known/ic-domains` use
  `public, no-cache, no-store`.
- Content-addressed generated bundles and assets use
  `public, max-age=31536000, immutable`.
- CSP forbids inline scripts and styles.
- The page loads no Google Fonts or third-party runtime dependencies.
- Standard headers include HSTS, `X-Content-Type-Options`, `Referrer-Policy`,
  `Permissions-Policy`, COEP, COOP, CORP, and a restrictive CSP.

## Layout

- Rust asset canister: `src/lib.rs`
- Embedded public assets: `public/`
- Browser source: `web/src/`
- Production declarations: `web/declarations/`
- Browser build: `web/build-frontend.mjs`
- Browser tests: `web/test/`

## Commands and verification

The command names below are defined in the repository `package.json`:

```bash
npm run setup:frontend
npm run build:frontend
npm run test:frontend-unit
npm run test:frontend-all
cargo test -p io-frontend
cargo run -p xtask -- frontend_required
```

`setup:frontend` runs `npm ci` and therefore uses the locked dependency graph
but may require network access. `tools/scripts/build-canister io-frontend
release` builds the browser bundle before compiling Wasm so the recorded asset
canister embeds the stamped files. See the [xtask guide](../../tools/xtask/README.md)
for aggregate frontend/release gates.

## Deployment status

The production frontend reservation remains pre-launch and does not activate
IO issuance or redemption. Local fixture deployments are test evidence only.

## Non-goals and limitations

- The frontend never directly calls the NNS Manager.
- Historian data is rebuildable, not canonical protocol truth.
- The public read model is not protocol truth and is not a value-moving authority.
- missing/stale/incomplete fields must not be interpreted as zero.
- Custom-domain certification setup and final SNS/testflight wallet integration
  remain incomplete.
- Production canister IDs are build/runtime inputs and may be empty in local
  builds.
- The frontend has no custom metrics/dashboard JSON endpoint and cannot
  authorize monetary or Governance effects.
- Local/frontend validation does not inspect or mutate protected canister
  `oae4c-3iaaa-aaaar-qb5qq-cai` or the two-year protected NNS neuron
  `10292412127977304661`.
