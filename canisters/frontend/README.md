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

For the connected principal and selected subaccount, the client queries in
parallel:

- IO Ledger `icrc1_fee`;
- IO Ledger `icrc2_allowance` for the Stream Manager spender; and
- Stream Manager `get_caller_redemption_state` for the next nonce, last request
  fingerprint, and last completed result.

It constructs the exact allowance and redemption requests before consent. The
wallet receives the IO amount, Stream Manager spender, selected source
subaccount, exact allowance, current IO fee, observed existing allowance,
approval expiry/memo/timestamp, request nonce, minimum ICP output, maximum ICP
fee, redemption expiry, and exact network. Only affirmative consent permits
`icrc2_approve`; only a successful approval permits `redeem`. The ordering is
queries, exact construction, consent, approve, redeem. Denial performs no
monetary call.

The allowance is `io_amount + current transfer_from fee`, uses the exact
observed allowance as `expected_allowance`, includes a deterministic
nonce-bound memo/timestamp, and expires after five minutes. The approval itself
burns its own IO fee. The subsequent `redeem` request uses the same canonical
subaccount, a two-minute request expiry, minimum ICP output, and IO and ICP fee
maxima.

The public UI renders coarse `Pending`, `Completed`, and `Stuck` workflow
progress. Detailed durable phase names remain operator diagnostics exposed by
status rather than public workflow compatibility variants. Anyone may call the
canister's permissionless `resume`; the connected UI exposes that operation.
For a `Stuck` own transfer, the user may submit the exact canonical ledger block
to `prove_active_transfer`, then resume.
The canister exact-matches the active persisted intent. Direct IO transfer is
unsupported and cannot create a redemption intent.

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
