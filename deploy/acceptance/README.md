# IO sale-free acceptance package

This draft has one launch path: manually install the reviewed standard SNS,
initialize Governance in Normal, migrate the reserve, capitalize IO, and let
Governance/Root install the four IO roles. It performs no network operation.
Deployment remains blocked on the evidence and external prerequisites in
[blockers.json](blockers.json). Executed results belong in
[implementation-status.md](implementation-status.md).

The frozen [genesis contract](genesis-contract.json) allocates exactly
1,000,000,000 IO: 30 IO operator, 100,000,000 IO Jupiter governance-only, and
899,999,970 IO reserve-transport. Swap, native SNS treasury and future Swap
allocations are zero. Ledger name and symbol are `IO`; display name is
`IO Protocol`. The temporary PNG and Frontend URL are explicit construction
inputs. Operator authority is
`zkkkd-i34qc-367ln-e2u7o-ezznu-dkfqh-gtfvz-cviph-6qa4v-evtfs-wqe`.
Acceptance Jupiter SNS Rewards is an independent required input.

Canonical memos `IOGOV001`, `IOJUP001`, `IORES001` and source-defined staking
subaccount derivations are unchanged. Jupiter has exactly one genesis principal
with `{Vote}`, 100% voting multiplier, and no ordinary/topic/default followees.
No subsequent permission-removal proposal is required. The operator cannot vote
or grant permissions on Jupiter's neuron. Real Jupiter proposal mirroring,
authenticated decisions, upgrade-safe retries and deadlines remain external
acceptance requirements; a local caller mock proves authorization only.

`tests/e2e_real_canisters/wasms.example.toml` is the approved six-component
SNS-W tuple after proposal 144202. Each publication, source revision, raw/gzip
hash and DID is bound separately. The typed constructor adapter links unmodified
upstream production types from the independently pinned source in
[sns-source-baseline.json](sns-source-baseline.json). This bypasses the friendly
sale parser, which rejects two-character `IO`. No SNS binary is modified.

Actual gzip payload hashes populate standard Governance `Version` and must match
Root's physical module hashes. Zero running Archives normalize to the blessed
Archive template. Governance starts with capacity 1000 and automatic target
advancement false. Root controls Governance, Ledger, Index, Swap and the four
IO dapps; Governance controls Root. There is no operator fallback after handoff.
IO chunks are stored on each target before handoff, preserving subnet locality.

The installed official Swap remains Adopted with an active periodic timer,
zero genesis allocation, buyers, recipes and accepted participation, with
auto-finalization false.
Both opening/due fields are `u64::MAX` seconds, unreachable by the pinned
u64-nanosecond clock. The mandatory nonzero participation limits and
`nns_proposal_id=0` are constructor compatibility data, not an offer or NNS
proposal. There is no named inactive mode and no completed sale. Record observed
Swap default-Account balances; these do not inventory all Swap-owned subaccounts.
Unsolicited ICP is quarantined, never IO backing, permanent capital or funding,
and does not block acceptance. Later IO transfers remain claim-bearing in the
canonical supply formula, with actual transfer fees burned; Swap is not an
exclusion. Genesis allocations, reserve migration and capitalization must still
reconcile independently. The standard ICP error-refund endpoint requires
Aborted/Committed, so unsolicited ICP can remain stranded in Adopted and needs
separate recovery review. Review
every future Swap upgrade against this invariant. Zero derived price restricts
native SNS treasury valuation. See [authority review](authority-review.md).

A verifies approved source and actual artifacts, all nine identities, empty
modules/controller/subnet/cycles authority, permanent-neuron baseline, Accounts
and exact typed SNS constructors. B verifies direct genesis, physical framework
version/controllers, parked Swap and the direct reserve disbursement. C verifies
actual backing, permanent principal before/after existing-ID refresh, independent
anchor, no issuance and final typed IO constructors. There is no monolithic
sealed launch or alternative installation route.

With a 10,000-e8s reserve-disbursement burn, observed supply is
99,999,999,999,990,000 e8s and reserve is 89,999,996,999,990,000 e8s. Excluding
the 100M IO Jupiter Account and empty reserve-neuron Account gives `C0=30 IO`.
`B0=C0=30 ICP`; permanent increment is `ceil(2*C0/3)=20 ICP`; Dynamic anchor is
an independent 10 ICP. Fees and cycles are additional. These are derived from
Ledger evidence, not editable funding targets. The 1 ICP/claim-bearing IO
initial backing invariant is independent of any sale price.

For a Public permanent neuron, A/C accept the authentic typed
`ListNeuronsResponse` requested with the exact ID and
`include_public_neurons_in_full_neurons=Some(true)`. Exactly one matching Public
full record is required, from an unrelated observer still unauthorized for
`get_full_neuron`. Every identity, stake, dissolve, auto-stake, maturity,
hotkey/followee and pending-disbursement check is retained. The existing
actually authorized full-neuron route remains available without adding rights.
Actual acceptance-neuron visibility and observer authority are unverified.
A fresh local fixture uses genuine NNS creation/public-info evidence and later
NNS Manager readiness; it does not prove live visibility.

Offline commands:

```sh
cargo run --offline --locked -p xtask -- acceptance_render acceptance
cargo run --offline --locked -p xtask -- acceptance_render production
cargo run --offline --locked -p xtask -- acceptance_render accounts
cargo run --offline --locked -p xtask -- acceptance_preflight draft
cargo run --offline --locked -p xtask -- acceptance_preframework
cargo run --offline --locked -p xtask -- acceptance_checkpoint A EVIDENCE_DIRECTORY
cargo run --offline --locked -p xtask -- acceptance_checkpoint B EVIDENCE_DIRECTORY
cargo run --offline --locked -p xtask -- acceptance_constructors EVIDENCE_DIRECTORY
cargo run --offline --locked -p xtask -- acceptance_checkpoint C EVIDENCE_DIRECTORY
cargo run --offline --locked -p xtask -- acceptance_genesis_real
```

The local executable needs the explicit artifact inputs in [runbook.md](runbook.md).
Preserved [historical sale evidence](history/sale-architecture-2026-10-18/README.md)
is not deployment authority or an active fallback. Feasibility probes and their
external evidence remain intact. The separate fresh owned-topology maintained
lifecycle requirement remains BLOCKED / NOT RUN until actually completed.
