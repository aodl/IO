# Sale-free acceptance runbook

This is an offline construction and local-rehearsal workflow. It authorizes no
mainnet query, identity access, funding, installation or controller change.
The nine acceptance IDs in acceptance-manifest.json are frozen. Resolve external
inputs explicitly; stop on any mismatch or unresolved required stage input.

1. Check available CPU, RAM, free disk and active parent aggregates as well as
   their current test children. Run IO value-moving suites serially on PocketIC
   14 with independent servers/state/output paths. Independent projects may run
   concurrently when measured capacity and path isolation support it; defer
   resource-intensive work on demonstrated contention. Never stop another
   workspace's job. General agent-policy amendments require separate review.
2. Verify the approved official lock, each component's raw/gzip bytes and DID,
   clean pinned constructor source and adapter/build binding. Build optimized
   current-source IO Wasms and isolated acceptance browser/Wasm artifacts.
   Preserve historical releases and all original tracked/ignored browser assets.
3. Establish checkpoint A: exact nine identities, compatible framework subnet,
   empty modules, sole observed operator authority and at least 20T cycles each;
   complete permanent NNS baseline and source-bound IO artifacts; canonical
   Accounts and typed five-component constructors. Root init registers exactly
   the four empty IO dapps. No late funding constants or C constructors are
   required at A.
4. Install Ledger, Index, Root, parked Swap and Governance from reviewed bytes.
   Governance is Normal at genesis, with a truthful standard six-component
   deployed_version, capacity 1000, zero native rewards and automatic target
   advancement false. Ledger must report IO/IO and exactly the three canonical
   staking Accounts totaling 1 billion IO. Jupiter is sole Vote-only at genesis;
   no permission-removal step or default/topic/ordinary following is permitted.
5. Upload the four IO raw Wasms through the supported chunk path to each target
   itself before handoff. Establish sole-controller topology: Governance owns
   Root; Root owns Governance/Ledger/Index/Swap and the four IO dapps. Verify
   actual status/hash observations; a Root acknowledgement alone does not prove
   asynchronous installation completion.
6. Verify parked Swap before and after periodic tasks: Adopted, MAX-second
   opening/due, auto-finalization false, zero genesis allocation, buyers, recipes
   and derived participation, active timer, no terminal state or proceeds. Frozen nonzero
   limits and proposal sentinel zero are constructor data only. Never subscribe,
   open, finalize or simulate a successful sale. Record default-Account ICP/IO
   balances without claiming all Swap subaccounts are empty. Unsolicited ICP is
   quarantined, never backing, permanent capital or funding, and does not block
   acceptance. It may be irrecoverable through normal Adopted Swap interfaces.
   Later IO remains claim-bearing, including actual fee burns in canonical supply;
   never add Swap to exclusions. Retain independent genesis/reserve/accounting
   validation. Review future Swap
   upgrades for clock/lifecycle compatibility and zero-price treasury restrictions.
7. Disburse the reserve-transport neuron directly to Stream's exact reserve
   Account. Checkpoint B requires the actual fee burn, total supply, reserve,
   both exclusions, operator stake, framework version and graph. Canonical
   `C0=supply-reserve-reserve_neuron-Jupiter_neuron=30 IO`.
8. Capitalize from those observations: Stream backing `B0=C0` (30 ICP), permanent
   increment `ceil(2*C0/3)` (20 ICP), and independent Dynamic anchor (10 ICP).
   Transfer fees and cycles are additional. Do not issue IO. Top up the existing
   permanent staking Account; ClaimOrRefresh the existing ID; verify exact
   before/after cached and Ledger principal with unchanged controller, Account,
   dissolve state, auto-stake and hotkeys/followees, with absent pending maturity
   disbursements. Retain ordinary/staked maturity separately as informational
   evidence; ordinary NNS reward accrual is never counted as principal top-up. Do not add hotkeys or impersonate NNS
   Manager. Reject any identity/configuration change.
9. Generate final C constructors using production Rust types. Checkpoint C
   binds audited principal, activation block floor, exact backing/anchor,
   no-issuance, installed module expectations and Root install authority. Keep
   source approval separate from byte-for-byte artifact verification.
10. Operator proposes each IO installation; actual Jupiter principal votes;
    Governance/Root installs Stream, NNS Manager, Historian and Frontend. Observe
    final physical module hashes/controllers, served browser bytes and autonomous
    readiness. Actual Jupiter mirror implementation remains a separate acceptance
    requirement even after a local mock caller succeeds.
11. Rehearse timer-only Dynamic reconciliation, structural TopUp, legitimate
    operator-neuron disbursement, exact redemption and subsequent eligible reward
    shares with Jupiter excluded. After readiness use only simulated time, ticks
    and read-only observations, apart from explicit user/proposal/ledger actions.
    Do not use keeper calls to claim timer liveness. Preserve exact fees and real
    NNS child-return behavior; do not infer a universal liquidity-before-unlock
    bound from one schedule. Drain bounded cross-subnet calls before day-sized
    simulated clock advances. After relocking, observe full backing restoration
    before submitting the reward motion: re-entry is eligible from the following
    event.
12. Freeze source/patch hashes, bundle/DIDs, payloads, raw observations, funding
    blocks, journal outcomes, controller graph, resource heartbeats and test exit
    counts. Complete serial full NNS/Stream targets and canonical test_ci. Keep
    the independent fresh owned-topology maintained lifecycle requirement BLOCKED
    / NOT RUN until genuinely completed. No deployment follows automatically.

Public permanent-neuron observation uses an unrelated caller's authentic typed
list_neurons request/reply with the exact ID and
include_public_neurons_in_full_neurons=Some(true). Require exactly one matching
Public full record, complete economic checks, and the same caller's actual
get_full_neuron NotAuthorized result. Authorized full-neuron replies are accepted
only from an existing genuine authority. Record the route and caller in reviewed
inputs and context. Actual acceptance visibility remains unverified. The fresh
local fixture records real DisburseToNeuron and public get_neuron_info before
installation; NNS Manager subsequently verifies its own full state. This fixture
is not live-neuron evidence.

Local artifact inputs:

```sh
# Reuse only an exact completed pinned upstream dependency build binding.
IO_ACCEPTANCE_UPSTREAM_BUILD_BINDING=ABSOLUTE_COMPLETED_BUILD_INPUTS_JSON \
  tools/acceptance/build-sns-payload-tool ABSOLUTE_CLEAN_PINNED_IC_SOURCE ABSOLUTE_NEW_TOOL_OUTPUT
bash tools/acceptance/build-frontend ABSOLUTE_NEW_FRONTEND_OUTPUT
cargo build --offline --locked --release --target wasm32-unknown-unknown \
  -p io-stream-manager -p io-nns-neuron-manager -p io-historian
```

Run the single local complete launch with explicit artifacts:

```sh
POCKET_IC_BIN=/home/codexdev/.local/bin/pocket-ic-server \
POCKET_IC_MUTE_SERVER=1 RUST_TEST_THREADS=1 \
IO_REAL_SNS_WASM_DIR=ABSOLUTE_REVIEWED_BUNDLE/wasms \
IO_REAL_SNS_WASM_MANIFEST=ABSOLUTE_REVIEWED_BUNDLE/manifest.toml \
IO_ACCEPTANCE_SNS_PAYLOAD_TOOL=ABSOLUTE_TYPED_TOOL/sns-payloads \
IO_ACCEPTANCE_IO_WASM_DIR=ABSOLUTE_CURRENT_IO_WASMS \
IO_ACCEPTANCE_FRONTEND_BUILD_DIR=ABSOLUTE_FRONTEND_OUTPUT \
  cargo run --offline --locked -p xtask -- acceptance_genesis_real
```

The exact primary test is `acceptance_sale::real_launch_checkpoints_a_b_c`;
`acceptance_sale` is a historical Rust module name, not a sale path. Discover with
`-- --ignored --exact --list`, then execute with
`-- --ignored --exact --test-threads=1 --nocapture`. The smaller timer-only,
checkpoint mutation, proposer, Vote-only, public NNS and standard-upgrade cases
remain independently executable. Retain their full logs. Historical sale-only
records under history/ are immutable evidence, never active authority.
