# IO

IO is a pre-launch Internet Computer protocol for turning an SNS-governed IO
reserve and liquid ICP reserve into a direct, auditable redemption and reward-
backing system. Its value-moving canisters make one authenticated intent
durable before performing ledger or governance effects; canonical ledgers and
governance canisters remain the sources of truth, while the Historian and
frontend provide a rebuildable public view.

IO exists to keep the monetary path small and reviewable: reserve transfers,
checked integer economics, explicit authority, proof-ordered external effects,
and exact ledger/governance proof when a callback is ambiguous. The governing
constraints are enforced by the implementation and by the
[simplicity check](docs/architecture/simplicity-constitution.md), not by this
README alone.

## Status: pre-launch and not live

IO issuance and redemption are not live. The canonical SNS IO ledger has not
been launched, `ProductionActive` wiring is rejected, production activation is
unavailable, and the recorded fiduciary canisters are reserved/inert. Local
SNS/PocketIC evidence is launch rehearsal evidence, not a mainnet deployment or
authorization.

The explicit [current-evidence selector](deploy/local-sns-rehearsal/evidence/current-canonical.toml)
must identify one immutable local package for the recorded release; validation
fails closed when its release or protected-target identity is stale. Important
launch work remains. The authoritative state is kept in the
machine-readable [launch-readiness register](tools/sns/launch-readiness.toml),
[release manifest](release-artifacts/manifest.json), and
[remaining-work register](docs/operations/remaining-work.md), rather than
duplicating transient commit or CI identities here.

| Area | Status |
| --- | --- |
| Protocol implementation | Anchored dynamic-backing replacement under source validation |
| Product and launch configuration | Partly open; local fixtures do not imply final production values |
| Official SNS reward-share capability | External dependency until an accepted official release supplies it |
| External audit | Required |
| Mainnet deployment and activation | Not performed; separately authorized |

## Protocol overview

The production model has four IO canisters around canonical SNS, ICP, Index,
Root, and Governance services:

1. An authenticated user asks the Stream Manager to prepare a short-lived,
   exact redemption quote and deterministic ICRC-1 push memo.
2. The Stream Manager reads canonical supply, reserve, excluded-account,
   backing, and fee values. The user pushes IO to the protocol reserve, and the
   manager exact-proves that ledger block before creating an immutable ICP
   payout obligation. If liquid ICP is unexpectedly late, the obligation waits
   durably and pays exactly once after recovery.
3. Once per exact SNS reward event, the Stream Manager converts eligible
   proposal-bearing reward shares into policy credit. A genuinely
   no-proposal event uses the defined eligible-stake fallback; an ambiguous
   skipped sequence earns no credit.
4. A separate 12-hour structural observation cadence promptly reconciles active
   and dissolving SNS positions with the NNS Manager. Daily reward-event
   processing remains independently fenced by the event deadline and margin.
5. The NNS Manager disburses maturity into one of two fixed semantic staging
   Accounts and freezes the complete post-finalization Account balance. Two-week maturity
   and Jupiter share the checked 40% permanent / 60% claim paired-inflow path;
   two-year maturity first replenishes any Dynamic-neuron anchor deficit, then
   applies the ordinary 40/60 gross allocation to the remaining fresh maturity;
   it issues no IO.
   Permissionless Jupiter notification additionally requires an exact routed
   ICP block at or above the immutable launch activation floor and not already
   present in permanent replay state.
6. A paired receipt lets the Stream Manager settle backed IO to either Jupiter
   or the immutable two-week batch. Later daily credit can continue accumulating
   while that one batch is pending.
7. The Historian periodically observes ledgers, indexes, SNS Root/Governance,
   both managers, and public NNS neuron information. The frontend renders that
   read model and provides an authenticated redemption client, but neither is
   monetary authority.

Unsupported direct transfers create no protocol claim and are not
automatically refunded.

## Components and boundaries

| Component | Role | Canonical dependencies | Authority |
| --- | --- | --- | --- |
| [Stream Manager](canisters/io_stream_manager/README.md) | Prepared ICRC-1 push redemption, IO/liquid-ICP reserves, structural synchronization, daily entitlement accounting, proof-bound NNS receipts, and backed settlement | SNS IO Ledger, ICP Ledger, SNS Root/Governance, NNS Manager | SNS Governance controls lifecycle; users authorize only their exact source Account push |
| [NNS Neuron Manager](canisters/io_nns_neuron_manager/README.md) | Bootstrapped Dynamic-neuron partition, protected-neuron commands, Jupiter/TwoYear replenishment, semantic maturity Accounts, and generation-based unwind children | NNS Governance, ICP Ledger, Stream Manager, Jupiter source Account | Executes as the existing protected-neuron controller; SNS Governance controls reviewed entry points and Stream Manager controls reconciliation/two-week paths |
| [Historian](canisters/io_historian/README.md) | Bounded monitoring, module/controller topology, account histories, reconciliation, and public read models | Ledgers, Index, SNS Root/Governance, both managers, public NNS neuron info | Install/upgrade configuration only; production API is read-only and non-authoritative |
| [Frontend](canisters/frontend/README.md) | Certified dashboard assets and authenticated redemption UX | Historian for dashboard reads; IO Ledger and Stream Manager for redemption | Advisory client only; canisters recompute all monetary facts |

Canonical balances come from ledgers. Account history normally comes from
Index, with archives discovered and reported. SNS Root is the source for SNS
topology, controllers, and module hashes; SNS Governance is the source for SNS
parameters and reward events; public NNS Governance neuron information is
observation only. The Historian never fills a missing observation with zero.

## Frozen design, launch constraints, and open configuration

The replacement monetary design includes authenticated prepared ICRC-1 push redemption,
canonical-ledger supply and balance authority, `B=L+P+U+T` claim backing with
an excluded 10-ICP Dynamic-neuron anchor, Jupiter/two-week shared 40/60,
semantic Account balances as custody authority, exact proof of immutable
outgoing effects, one active monetary operation, daily canonical
SNS participation-based entitlement, 12-hour structural synchronization, and one live accumulator plus at most one
immutable pending batch. Ambiguity pauses for exact proof; it does not trigger
global absence reconstruction.

The following are deliberate launch constraints, not missing features:

- serialization can return `Busy`;
- unsolicited/direct monetary paths are unsupported and are not automatically
  recovered;
- passive unwind is generation-aggregated, services ready children first, and
  has no product capacity branch;
- missed reward events receive no fabricated credit; and
- Historian observation can never authorize monetary completion.

Still-open product/launch configuration includes the production token symbol,
SNS tokenomics and distribution, fallback controllers and recovery policy,
initial reserve quantity, final fee floats, production Account/subaccount
topology, cycles operations, public metadata/domain, and final official SNS
module hashes and canister IDs. Those choices do not make the monetary
algorithms unfinished.

## Simplicity architecture

IO deliberately chooses:

- explicit authenticated intent over scanner-inferred intent;
- canonical balances over replicated balance accounting;
- one active operation over reservation and concurrency algebra;
- typed operation state over optional-field state bags;
- exact Account roles over generic liability classification;
- proof of IO's own persisted effect over global absence proofs;
- Historian observation over Historian monetary authority; and
- safe pause for genuinely ambiguous rare cases over speculative completion.

When a monetary path is replaced, the superseded path is deleted rather than
retained in parallel. Unsupported activity does not become a launch feature by
default. Reintroducing scanners, generic liability systems, reconciliation or
entitlement queues, ballot reconstruction, cohort accounting, or general
operation queues requires demonstrated need and explicit architectural
justification under the
[simplicity constitution](docs/architecture/simplicity-constitution.md).

## Core economics

The SNS Ledger is the canonical IO supply and staking-balance authority. The
Stream Manager does not maintain a second supply or backing scalar. Reserve IO
and genuinely nonredeemable governance staking Accounts are distinct
denominator terms. Ordinary liquid, active-staked, and dissolving user IO is
claim-bearing; the Jupiter IO recipient is not implicitly excluded.

All token quantities are integer e8s. Checked multiplication is performed
before integer division, so ratios round down; overflow, underflow, a zero
denominator, or a payout that cannot cover the current fee rejects or pauses the
operation rather than approximating it.

Canonical claim accounting is:

```text
C = total_io_supply - protocol_reserve_io - nonredeemable_governance_io
B = liquid_icp + claim_bearing_dynamic_principal + net_live_child_backing + net_in_transit_backing
gross_icp = redeemed_io * B / C
net_icp = gross_icp - current_icp_payout_fee
pooled_target = floor(A_backing * B / C)
reward_target = floor(A_reward * B / C)
```

The Dynamic parent's excluded anchor and unexplained positive surplus,
permanent capital, uncaptured maturity, cycles, and operational balances are
outside `B`. Its physical principal is exactly partitioned into claim-bearing
principal, anchor available, excluded surplus, and any explicit physical
in-flight adjustment. Live-child and committed active-unwind
values are net of their exactly derived unavoidable future disbursement fees;
physical principal remains separate for Governance commands and transfer
proof. Each e8s of claim backing exists in exactly one of its four buckets.
Fees incurred while relocating existing claim backing consume anchor capacity
without lowering `B`. Fees incurred while delivering fresh Jupiter or maturity
value reduce the corresponding fresh net credit; paired IO issuance uses that
same net claim credit. Permanent capital is outside `B`, so its fresh delivery
fee creates no liability. IO issuance is an explicit reserve transfer, not
application minting.

Redemption freezes a `B/C` quote without reserving ICP. Claim-rate
monotonicity keeps an earlier valid quote economically conservative. The user
then performs one exact ICRC-1 push to reserve. Canonical proof of that block
removes the IO from claim-bearing circulation and creates a durable payout
obligation. Unexpectedly missing liquid ICP is an invariant-breach recovery
state after the push, not a normal admission failure or cancellation.

`A_backing` is structurally active ordinary SNS IO; `A_reward` is its currently
prospective reward-eligible subset. Rewards require pooled principal to cover
`reward_target`. The memo-0 Dynamic parent exists before Ready, has an exact
1,209,600-second delay, a fixed configured following policy, and an excluded
10-ICP anchor. Daily
proposal-bearing allocations normalize canonical current-event shares;
ineligible shares are forfeited, not redistributed. A true no-proposal event
uses eligible stake, and ambiguous skipped events receive no synthetic credit.

## Lifecycle and readiness

The value-moving Stream and NNS Manager canisters install and return from
upgrade in `Paused`. `Paused` blocks new preparation; it does not erase an
already immutable or in-flight monetary operation, which remains resumable.
Activation is an SNS-governed transition whose asynchronous preflight binds the
reviewed configuration to actual canister, ledger, Governance, fee, supply,
semantic staging-balance, and protected-neuron observations.

The Historian has a different lifecycle: `null` install configuration leaves
sources `PrelaunchNotConfigured`; a validated install/upgrade configuration
arms its one-shot refresh cycle; an upgrade with `null` preserves existing
configuration. The frontend initializes as a certified asset canister and has
no monetary `Paused` state.

Readiness is not launch authorization. The full distinction between proved
local behavior and incomplete official/product/audit/mainnet work is recorded
in [launch readiness](tools/sns/launch-readiness.toml),
[mainnet readiness](docs/operations/mainnet-readiness.md), and
[audit readiness](docs/security/audit-readiness.md).

## Failure, recovery, and exact proof

Before every potentially irreversible external effect, the canister persists
its exact immutable intent. It never submits a later dependent effect while an
earlier effect is ambiguous or lacks its required canonical postcondition.
Once success is definitive and canonically proved, a fixed-size flow may
continue to its next step in the same invocation. A definitive no-effect result
is retried or aborted under that operation's rules and is never treated as
success. Variable fan-out, including recipient settlement, remains explicitly
bounded across invocations.

Transport ambiguity retains the exact submitted phase and stops dependent
effects until canonical proof or safe recovery is available. Other real stop
boundaries include a postcondition not yet visible after one immediate reread,
ledger retry delay, maturity finalization time, external exact-block proof,
an owed payout waiting for invariant recovery, and durable `Stuck` state.

`resume` advances an existing operation idempotently. `prove_active_transfer`
accepts only an exact canonical ledger block for the active proof slot; it is
not a manual balance rewrite or debug completion path. Stream proof slots cover
redemption, the paired liquid claim receipt, pooled top-up, and reward
transfers. The NNS Manager similarly proves exact outgoing maturity, parent,
and cohort effects. It does not prove the provenance of fungible ICP already
held in a semantic Account. Upgrades preserve durable operation state and force
reviewed reactivation while allowing immutable work to resume.

## NNS terminology and production authority

The NNS Manager directly calls NNS Governance, and its configuration requires
its staging Accounts to be owned by the executing canister. The two-year
protected NNS neuron `10292412127977304661` has controller authority at
`oae4c-3iaaa-aaaar-qb5qq-cai`; therefore the accepted production model places
the manager at that existing controller. Static wiring permits that principal
only as the NNS Manager authority target. Any inspection, installation,
upgrade, controller change, or neuron action requires a separate audit and
explicit mainnet authorization.

The two-year protected neuron is distinct from the Dynamic 14-day IO neuron.
The Dynamic parent is claimed from its externally seeded deterministic memo-0
Account before Ready, accepts canonical balance at or above 10 ICP, classifies
excess as excluded surplus, uses an exact `1209600`-second non-dissolving delay,
and follows the protected neuron. Ordinary eligible IO SNS neurons use the
separate `1296060`-second (15 days + 1 minute) delay. Structural synchronization
does not wait for the daily reward event.

| Role | Identifier | Status |
| --- | --- | --- |
| Stream Manager | `thset-pqaaa-aaaar-qb7wa-cai` | Fiduciary reservation, `ReservedNotLive` |
| NNS Manager execution authority | `oae4c-3iaaa-aaaar-qb5qq-cai` | Existing protected-neuron controller; not inspected or modified by repository validation |
| Historian | `tjqj3-uaaaa-aaaar-qb7xa-cai` | Fiduciary reservation, `ReservedNotLive` |
| Frontend | `torpp-zyaaa-aaaar-qb7xq-cai` | Fiduciary reservation, `ReservedNotLive` |
| SNS IO Ledger/Index/Governance/Root/Swap | not assigned | Final production SNS configuration is incomplete |

## Source, build, and release verification

Use `rust-toolchain.toml`, `Cargo.lock`, `package-lock.json`, and the checked-in
SNS/NNS pins as build inputs. A truthful release starts with a finalized
committed source identity, deterministically builds exactly that source, and
records artifacts in the immediate artifact-only descendant required by the
repository release model. Independent rebuilds must equal the checked-in raw
and gzip bytes. An artifact cannot truthfully claim a commit that did not exist
when the build ran, so generated bytes are never relabelled after history
changes. The machine-readable manifest records the current source identity,
sizes, and SHA-256 hashes.

Reproducibility checks also cover pinned compiler/toolchain paths, absolute
source and Cargo-home path remapping, clean frontend generation, deterministic
compression, manifest contents, and complete artifact inventories. Later
evidence/documentation tails do not become artifact-recording commits.
Hosted test, security, and reproducible-build results count only when all three
ran against the exact reviewed release-tail SHA; a green ancestor is not
current-head evidence.

```bash
cargo run -p xtask -- verify_artifacts
CARGO_INCREMENTAL=0 cargo run -p xtask -- verify_recorded_source
cargo run -p xtask -- verify_release
```

`verify_recorded_source` preserves the checked-in artifact directory and proves
it equals two independent detached builds of the exact recorded source.
Generation is a separate, intentional operation documented in
[reproducible builds](docs/operations/reproducible-builds.md). See the
[release checklist](docs/operations/release-checklist.md) before any release
work.

## Development and validation

Fast static/unit checks:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo run -p xtask -- simplicity_check
```

Repository orchestration:

```bash
cargo run -p xtask -- test_all
POCKET_IC_BIN=/home/codexdev/.local/bin/pocket-ic-server \
  cargo run -p xtask -- test_ci
cargo run -p xtask -- verify_release
```

`test_all`, `test_ci`, and `verify_release` are serial entry points; do not run
them concurrently because they share generated frontend/debug artifacts.
PocketIC-required commands need `POCKET_IC_BIN`. Frontend setup and release-like
build paths may run `npm ci`; review network/dependency policy before invoking
them. The complete, grouped command surface is in the
[xtask README](tools/xtask/README.md).

No normal validation command authorizes a mainnet operation. Local rehearsal
commands require explicit local-only acknowledgement and loopback endpoints.

Terminology used by testing and release documentation is strict:

- **local fixture**: ephemeral local principals, Accounts, balances, proposal
  and neuron IDs, and token symbol; never production configuration;
- **candidate upstream**: source-built, unpublished SNS components used for
  compatibility proof; not an official release;
- **official pinned upstream**: an exact official DFINITY revision with
  reviewed Wasm/DID hashes; and
- **production configuration**: final IO/SNS launch values selected through a
  separately authorized process.

## Repository layout

| Path | Contents |
| --- | --- |
| `canisters/` | Stream Manager, NNS Manager, Historian, certified frontend, production DIDs, and install templates |
| `crates/` | Shared account, ledger, governance, economics, reward, receipt, stable-schema, build, and wiring types |
| `tests/` | Mock canisters, PocketIC integration tests, and opt-in real-canister fixtures |
| `tools/xtask/` | Stable Rust orchestration and validation interface |
| `tools/sns/` | SNS compatibility config, readiness register, and testflight planning |
| `deploy/local-sns-rehearsal/` | Maintained local-only SNS/SNS-W rehearsal scripts and immutable evidence |
| `deploy/production-wiring/` | Non-runnable production-planned static wiring validation |
| `release-artifacts/` | Recorded raw/gzip Wasm, checksums, and source-bound manifest |
| `docs/` | Architecture decisions, operations, testing, security, and release evidence |

## Documentation map

Start with the component READMEs for implementation-facing contracts:

- [Stream Manager](canisters/io_stream_manager/README.md)
- [NNS Neuron Manager](canisters/io_nns_neuron_manager/README.md)
- [Historian](canisters/io_historian/README.md)
- [Frontend](canisters/frontend/README.md)
- [xtask command guide](tools/xtask/README.md)

Maintained integration and operations material:

- [Production wiring](deploy/production-wiring/README.md)
- [Official local SNS rehearsal](deploy/local-sns-rehearsal/README.md)
- [SNS compatibility package](tools/sns/README.md)
- [Optional local SNS helpers](tools/sns-testing/README.md)
- [SNS framework source profiles](docs/testing/sns-framework-sources.md)
- [Stable-state fixtures](tests/fixtures/stable-state/README.md)
- [Testing guide](docs/development/testing.md)
- [Release checklist](docs/operations/release-checklist.md)
- [Cycles operations](docs/operations/cycles-management.md)
- [Threat model](docs/security/threat-model.md)
- [Audit-readiness index](docs/security/audit-readiness.md)

Historical material is explicitly labelled and is not current deployment
instruction:

- [`docs/research/pre-simplification/`](docs/research/pre-simplification/) — superseded, non-normative research
- [Historical research-branch disposition](docs/operations/p0-research-branch-disposition-2026-07-29.md)
