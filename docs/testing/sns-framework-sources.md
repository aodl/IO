# SNS framework sources

`tools/scripts/test-sns-framework` is the single entry point for resolving SNS
framework Wasms and running IO variant tests. It supports three source modes:
the reviewed official lock, a local Governance overlay built from the sibling
IC checkout, and replay of an immutable resolved bundle. It never deploys or
contacts an IC mainnet endpoint.

## Supported interface

| Flag | Environment | Supported values |
| --- | --- | --- |
| `--source` | `IO_SNS_SOURCE` | `official`, `local`, `bundle`; default `official` |
| `--ic-repo` | `IO_IC_REPO` | local IC checkout; default `<IO>/../ic` |
| `--scope` | `IO_SNS_SCOPE` | `governance`, `governance-root` |
| `--profile` | `IO_SNS_PROFILE` | `contract`, `io`, `upgrade`, `lifecycle`; default `contract` |
| `--bundle` | `IO_SNS_BUNDLE` | absolute bundle path for bundle mode |
| `--cache-dir` | `IO_SNS_CACHE_DIR` | external content-addressed cache root |
| `--require-capability` | — | `latest_reward_event_participation` |

`lifecycle` is a thin adapter to the existing guarded official local rehearsal.
It requires the reviewed ignored loopback install inputs and one running,
uniquely owned fresh SNS testing topology. The adapter refuses a reused run
directory. It does not duplicate SNS-W lifecycle machinery. There is no `all`
profile or local full-SNS-suite overlay.
Before phase execution it attaches read-only to the declared PocketIC instance
and requires its clock to be within 300 seconds of host time. A previously
day-advanced topology therefore fails before any signed ingress.

Examples:

```bash
tools/scripts/test-sns-framework --source official --profile contract

tools/scripts/test-sns-framework \
  --source local \
  --scope governance-root \
  --require-capability latest_reward_event_participation \
  --profile io

tools/scripts/test-sns-framework \
  --source bundle \
  --bundle /absolute/path/to/resolved-bundle \
  --profile upgrade
```

The default cache is
`${XDG_CACHE_HOME:-$HOME/.cache}/io/sns-framework`. Wasms, bundles, and build
caches remain outside the IO worktree and must not be committed.

## Official source

Official mode reads `tests/e2e_real_canisters/wasms.example.toml`. That reviewed
lock pins component revisions, artifact URLs, compressed and raw hashes, and
test-orchestration capability metadata. A normal run never edits the lock and
never searches for a moving latest release. An official result is only as
current as the checked-in reviewed lock.

When artifacts are absent from the external cache, the runner uses the existing
allowlisted, hash-verifying fetch script. `--official-manifest` can test a
separately prepared proposed lock without editing the repository; the runner
does not create or bless that proposal.

## Local component overlay

Local mode treats `<IO>/../ic` as read/build input. The runner does not switch,
clean, reset, pull, rebase, commit, or otherwise modify that checkout. It builds
the production Governance target and, for `governance-root`, Root from the same
source commit:

```text
//rs/sns/governance:sns-governance-canister
//rs/sns/root:sns-root-canister
```

The candidate Governance Wasm and DID replace Governance in the official base.
The `governance-root` scope also replaces Root and hash-binds its DID; all other
artifacts remain byte-for-byte official. A requested local component build is
never silently replaced with its official baseline.

The runner records the IC commit, branch, merge base, clean/dirty state, tracked
diff SHA-256, Bazel version, exact target, candidate DID hash, artifact hashes,
official baseline, and component override. A dirty tree is local-only and its
diff hash enters the bundle identity. The runner does not export a source patch.
A local candidate Wasm is not an official release.

Local Bazel uses the same selected Bazel version for version reporting, cquery,
and build. It runs in batch mode through a generated external rc, defaults to
`IO_SNS_BAZEL_JOBS=2`, and accepts a lower override. On the 4-GiB/no-swap local
VM use `IO_SNS_BAZEL_JOBS=1`. The sibling checkout remains unchanged.

## Immutable bundle replay

A resolved bundle contains the exact manifest, provenance, hashes, Governance
DID when present, and Wasms used by a run:

```text
<cache>/<bundle-id>/
├── manifest.toml
├── provenance.toml
├── SHA256SUMS
├── governance.did
└── wasms/
```

Bundle mode requires an absolute path. Validation rejects missing or unexpected
files, path traversal, symlinks, non-regular artifacts, credential-like names,
malformed hashes, artifact hash mismatches, and a mismatched `SHA256SUMS` file
set. Replay uses those exact bytes; it does not rebuild or substitute sources.

Every result must report source mode, resolved manifest path, profile, official
baseline, IC commit and dirty state when applicable, diff hash, component
overrides, Governance DID hash, and artifact hashes.

## Capability and runtime policy

`latest_reward_event_participation` in a bundle is test-orchestration metadata,
not a runtime authorization or caller-supplied capability Boolean. Local mode
sets it only after the candidate DID proves the additive participation/share
contract. A capability-bearing bundle makes the exact candidate tests
mandatory; exact-test discovery must find one test before execution begins.

Runtime readiness independently verifies exact SNS Root, exact Governance
principal, exact Governance module hash, the exact 86,400-second native
reward-event duration, zero dissolve-delay and age voting-power bonuses, and
that both native Governance reward rates are zero. Canonical SNS
Governance reward shares are the complete weight for a proposal-bearing event,
including the SNS's native voting-power policy. IO filters the exact eligible
two-week non-dissolving neurons and excluded protocol/Jupiter accounts; it does
not reconstruct age, dissolve-delay, or voting-power multiplier arithmetic.

If no proposals settled, the event uses exact currently eligible stake. If
proposals settled but eligible canonical shares total zero, IO issues no reward
and does not fall back to full participation.

IO accepts only the exact next reward event: round delta one and
`rounds_since_last_distribution` one. A missed event or multi-round span is
reported as one bounded typed skipped-event observation; no entitlement credit
is added and undistributed backing remains in reserve. The field is
latest-event-only; IO advances its observation checkpoint without reconstructing
skipped events.

## Profiles and execution safety

- `contract` validates the resolved bundle, runs the complete
  `io-sns-reward-boundary` unit suite, enforces the exact additive DTO test, and
  requires the exact candidate Governance contract test when the capability is
  present.
- `io` runs contract coverage plus the installed IO reward path.
- `upgrade` runs the exact official-to-candidate Governance upgrade test.
- `lifecycle` invokes the existing rehearsal phases 10–17 in a unique external
  run directory, reruns the ledger and index phases after Governance activation,
  and preserves their restart markers. Bootstrap, build, deployment, every
  `dfx`/`sns`/Quill/agent proposal and observation, redemption, and reward-proposal
  submission all precede the exact 86,400-second PocketIC advance. After that
  boundary the adapter uses only the existing direct PocketIC reward observer,
  offline immutable evidence packaging, and scoped cleanup; it performs no
  host-clock-signed ingress or rediscovery. The monitoring package is created
  only after the direct observer has proved canonical historian convergence.

The variant runner does not copy SNS-W lifecycle code. Its `lifecycle` profile
passes the resolved bundle directory to `deploy/local-sns-rehearsal/runbook.sh`
and invokes the existing phases as a thin provenance-preserving adapter.
Official lifecycle mode fails closed until the reviewed official bundle carries
the required reward-share capability.

Test enumeration is the only captured child output. Actual `--nocapture` test
output is inherited and streamed. Each profile run creates one
`IO_POCKETIC_RUN_ID`; cleanup targets only descendant processes carrying that
run ID. `CARGO_BUILD_JOBS` defaults to 2 and `RUST_TEST_THREADS` to 1 unless the
caller explicitly overrides them. Staging directory names include the bundle
or run identity so an interrupted resolver can be reviewed safely.

Never run Bazel, a PocketIC profile, `test_all`, or `verify_release`
concurrently. Candidate testing is local only: no `--network ic`, mainnet
install, upgrade, settings change, push, or deployment is part of this workflow.
