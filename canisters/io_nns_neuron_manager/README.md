# IO NNS Neuron Manager

## Role in IO

`io_nns_neuron_manager` exclusively owns IO's proof and commands for the
two-year protected NNS neuron (`two_year_neuron_id`), the two-week reward-
backing NNS neuron for the 14-day SNS product (`two_week_neuron_id`), Jupiter
and maturity staging Accounts, direct 40/60 maturity policy, and one pending
unwind child. It does not own a general fee Account or a queue of target
positions.

The reward-backing parent is non-dissolving at the approved 252,460,800-second
(eight-year) NNS delay with `auto_stake_maturity = false`. “14-day” describes
ordinary IO SNS-neuron eligibility; it is not the NNS parent's dissolve delay.
IO is pre-launch and the production authority has not been activated by this
repository.

## Dependencies and authority

The manager directly calls the configured NNS Governance canister as the
executing canister. The two-year protected NNS neuron
`10292412127977304661` has
controller authority at `oae4c-3iaaa-aaaar-qb5qq-cai`, so the accepted
production model places this implementation at that existing controller. The
architecture has no authority adapter. Any mainnet inspection or change
requires a separate audit and explicit authorization.

The launch inventory is explicit; TODO values are blockers, not fixture
defaults:

| NNS role | Neuron ID | Controller/executor | Expected launch principal | Maturity baseline | Dissolve configuration | Staging/destination |
| --- | --- | --- | --- | --- | --- | --- |
| Two-year protected NNS neuron | `10292412127977304661` | `oae4c-3iaaa-aaaar-qb5qq-cai` | audited `seeded_two_year_principal_e8s` (production template TODO) | ordinary `0`, staked `0`, no pending disbursement | non-dissolving, approved `252460800` seconds, auto-stake off | actual Mint proof to Stream Manager liquid ICP Account |
| Two-week reward-backing NNS neuron | final production ID is open | same NNS Manager execution canister | audited `seeded_two_week_principal_e8s` (production template TODO) | ordinary `0`, staked `0`, no pending disbursement/child ambiguity | non-dissolving, approved `252460800` seconds, auto-stake off | self-owned two-week maturity staging Account, then proof-bound Stream receipt |

SNS Governance controls `set_paused` and the reviewed two-year maturity generic
function. Only the configured Stream Manager can reconcile and prepare the
two-week backing path. Jupiter observation is permissionless but authoritative
only when one exact ICP Ledger block matches the configured Jupiter source and
manager staging Accounts and is at or above the immutable launch activation
floor.

The correctness-critical NNS command DTOs and behavior are pinned to
`dfinity/ic` commit `021bf342f66296d5605b355a61b2430406a83783`; see the
[NNS boundary pin](../../docs/testing/nns-boundary-pin.md). The manager does not
depend on a floating generic governance-types crate.

## Accounts and value flow

Every sending staging Account is owned by the executing NNS Manager. Jupiter's
source Account is owned by the configured Jupiter canister. The Stream liquid
Account is owned by the Stream Manager. Jupiter source and Jupiter staging must
both be canonical default Accounts, while the two staging Accounts must be
distinct from each other. Neither staging Account, nor the Jupiter source
Account, may equal the Stream liquid Account.

An exact Jupiter deposit is split with checked integer arithmetic: 40% (rounded
down) is staked in the two-year protected NNS neuron and the remainder goes through
Jupiter staging to the Stream Manager's proof-bound liquid receipt. Processed
Jupiter block indexes have narrow permanent replay protection; there is no
Jupiter callback. Blocks below `jupiter_activation_block_floor` are rejected
without a Ledger/archive call. New unprocessed lookups share one persisted
one-second cooldown; invalid probes create no per-block stable state.

For direct maturity, the manager observes canonical ordinary maturity `M`,
calls `StakeMaturity(40%)`, verifies returned remaining and staked maturity,
then calls `DisburseMaturity(100% of remaining)`. The nominal disbursed amount
must be at least 100,000,000 e8s (1 ICP). NNS schedules maturity disbursement
exactly 604,800 seconds (seven days) after initiation. Only the actual modulated
ICP proved by one exact ICP Mint block becomes liquid backing.

Protected-neuron Mint proof completes against the Stream liquid Account and
issues no IO. Reward-backing-parent Mint proof enters a typed delivery phase,
transfers the actual ICP through its staging Account, and completes the Stream
Manager's proof-bound receipt.

## Production API

The checked-in [production Candid](io_nns_neuron_manager.did) exposes:

- `notify_jupiter_deposit`
- `reconcile_two_week_backing_readiness`
- `prepare_two_week_maturity`
- `resume`
- `prove_active_transfer`
- `start_maturity`
- `validate_start_maturity` (query)
- `prove_maturity_mint`
- `set_paused`
- `validate_set_paused` (query)
- `get_status` (query)

`validate_start_maturity` renders only the `TwoYear` SNS generic-function
payload. `start_maturity` independently checks SNS Governance, while the Stream-
bound two-week path cannot use that generic function. `validate_set_paused`
similarly renders a Boolean payload without changing state.

## Lifecycle and complete readiness model

Install and upgrade leave the manager `Paused`. New Jupiter, maturity, and
target preparation require `Ready`; immutable active effects and passive
delivery/unwind work remain resumable while paused.

Configuration validation requires:

- distinct, nonzero two-year and two-week NNS neuron IDs and valid non-system
  principals, with canister self, SNS Governance, Stream Manager, Jupiter, ICP
  Ledger, and NNS Governance pairwise distinct;
- self-owned, distinct staging Accounts;
- canonical default Accounts for Jupiter source and Jupiter staging;
- the configured Jupiter owner and Stream liquid owner to match their roles;
- nonzero reviewed IO and ICP fees;
- a Jupiter fee float covering at least two current ICP fees;
- a two-week staging fee float covering at least one current ICP fee;
- each configured staging fee float capped at 100,000,000 e8s (1 ICP);
- a nonzero immutable Jupiter activation block floor and nonzero exact seeded
  principals for both NNS roles; and
- a positive retry delay strictly inside the configured ledger deduplication
  window.

Activation then queries the current canonical ICP fee and requires it to equal
configuration. It reads both staging balances and requires each actual balance
to contain its full configured fee float. The captured control epoch,
configuration, lifecycle, and active/passive operation state must remain
unchanged through the asynchronous preflight.

On first readiness only, the manager proves those conditions independently for
both the two-year protected NNS neuron and the two-week reward-backing NNS
neuron: exact ID and seeded principal, zero pre-launch ordinary and staked
maturity, `auto_stake_maturity = false`, the exact non-dissolving approved
delay, and no pending maturity disbursement. The two-week role additionally
requires no child ambiguity. Separate baseline flags survive upgrade, and the
two-year maturity command is unavailable until its own flag is reconciled. A
nonzero pre-launch maturity observation blocks readiness for reviewed operator
resolution; it is never silently treated as protocol earnings.

## Target reconciliation and direct unwind

Authenticated reconciliation persists one exact desired target and returns
whether the liquid 60% maturity leg can start immediately. The target itself is
idempotent authority; only entitlement batches have generations. There is no
target queue.

An over-target canonical parent creates one immediate typed unwind operation.
It splits exactly the excess, starts the one child dissolving, and then makes
the child passive so the eight-year wait does not block maturity on the reduced
parent. A newer target may promote that exact child for merge-back; a ready
child may be promoted for direct disbursement. Completion requires the exact
ICP Transfer block returned by NNS Governance or an explicitly supplied block
for an ambiguous callback. There is no staging detour, IO issuance, ladder, or
second child.

## Stable state and upgrades

Launch state is one `StableCell<NnsStateV1>` with one typed immediate
operation, one optional passive unwind child, fixed passive slots for two-year
and reward-backing maturity, target state, baseline proof, and replay markers.
Only V1 is supported; no pre-launch migration chain is compiled. Upgrade
restores that snapshot and forces `Paused` without discarding immutable work.

## Failure, ambiguity, resume, and proof

Each invocation performs at most one external governance or ledger effect.
Before a transfer, the manager persists its exact ledger, source subaccount,
destination, amount, fee, memo, timestamp, and operation identity. Retry stays
inside the ledger deduplication window; uncertain expired outcomes become
`Stuck`.

`resume` advances the exact active or passive state. With no work present, its
optional target observation uses one persisted 60-second canister-wide
cooldown; continuing real monetary work is never delayed by that cooldown.
`prove_active_transfer`
accepts only a canonical block matching a Stuck Jupiter/two-week staging
transfer or an unwind disbursement effect. `prove_maturity_mint` accepts only
the exact delayed NNS maturity Mint to the configured destination, including
the expected amount/timing evidence. Neither method is a manual completion or
value rewrite.

## Commands and verification

```bash
cargo test -p io-nns-neuron-manager --lib
POCKET_IC_BIN=/home/codexdev/.local/bin/pocket-ic-server \
  cargo test -p io-nns-neuron-manager --test io_nns_neuron_manager_pocketic
cargo run -p xtask -- validate_nns_boundary_pin
cargo run -p xtask -- validate_install_args
cargo check -p io-nns-neuron-manager --target wasm32-unknown-unknown
```

Run PocketIC targets serially. The [Jupiter integration contract](../../docs/architecture/jupiter-integration-contract.md)
contains deeper boundary detail; this README states the value and authority
rules needed to review this component independently.

## Non-goals

The manager is not an NNS hotkey adapter, general NNS wallet, fee treasury,
arbitrary neuron manager, multi-child staking ladder, historical indexer, or IO
mint authority. It exposes no debug completion method in production Candid.
