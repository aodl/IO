# SNS framework sources

`tools/scripts/test-sns-framework` is the normative entry point for testing IO
against SNS framework variants. Every invocation resolves immutable Wasm bytes,
a Governance DID, capabilities, hashes, and provenance before it starts the
shared IO tests. The runner never contacts an IC boundary node and never
deploys to mainnet.

## Workspace and source selection

The normal workspace has sibling repositories:

```text
<workspace>/src/
├── IO/
└── ic/
```

The runner resolves the IO root with `git rev-parse --show-toplevel`. Local
mode defaults to the canonicalized `<IO root>/../ic` path. Flags override the
matching environment variables.

| Flag | CI environment alias | Values/default |
| --- | --- | --- |
| `--source` | `IO_SNS_SOURCE` | `official`, `local`, `bundle`; default `official` |
| `--ic-repo` | `IO_IC_REPO` | local IC checkout path |
| `--scope` | `IO_SNS_SCOPE` | `governance`, `sns-suite`; default `governance` |
| `--profile` | `IO_SNS_PROFILE` | `contract`, `io`, `upgrade`, `lifecycle`, `all`; default `contract` |
| `--bundle` | `IO_SNS_BUNDLE` | absolute immutable bundle path |
| `--cache-dir` | `IO_SNS_CACHE_DIR` | content-addressed cache root |

The default cache is
`${XDG_CACHE_HOME:-$HOME/.cache}/io/sns-framework`. It is deliberately outside
the IO worktree. Do not copy its Wasms into the repository.

Common commands are:

```bash
# The reviewed repository pin.
tools/scripts/test-sns-framework --source official

# The current sibling Governance candidate over the official base.
tools/scripts/test-sns-framework --source local

# A coordinated full-suite candidate.
tools/scripts/test-sns-framework \
  --source local --scope sns-suite --profile all

# Replay exactly the bytes prepared elsewhere.
tools/scripts/test-sns-framework \
  --source bundle --bundle /absolute/path/to/bundle --profile upgrade
```

`--help` lists all supported flags. Callers do not export
`IO_REAL_SNS_WASM_DIR` or `IO_REAL_SNS_WASM_MANIFEST`; the runner sets those
low-level compatibility variables only for the child test process.

## Official source

Official means the exact reviewed baseline in
`tests/e2e_real_canisters/wasms.example.toml`, which acts as IO's official lock.
It pins component revisions, HTTPS artifact URLs, compressed hashes, raw Wasm
hashes, and capabilities. It does not perform a mutable “latest” lookup.

An official run validates the lock and reuses
`tools/scripts/fetch-real-canister-artifacts`, the existing allowlisted and
hash-verifying fetch path. A normal run never edits the lock. An official run
is only as current as this checked-in pin.

To propose a newly blessed revision, use the explicit updater:

```bash
tools/scripts/update-official-sns-baseline \
  --revision <40-hex-blessed-IC-revision>
```

The command prepares a proposed lock, verifies artifacts through the same
fetcher, and prints the repository diff. It does not commit. Review the
authoritative blessed-version evidence, component revisions, hashes, DIDs, and
capabilities separately. Never infer a blessed release from `master`, the
newest Git tag, or the newest GitHub commit.

When DFINITY provides a complete proposed manifest, test it before adoption
without modifying the checked-in lock:

```bash
tools/scripts/test-sns-framework --source official \
  --official-manifest /absolute/path/to/proposed-official.toml
```

## Local source

Local mode treats the sibling checkout as a read/build input. The runner never
changes its branch, index, worktree, remotes, or configuration. It validates
the Git repository, Governance DID, and canonical Bazel files, then records:

- canonical repository path, branch/detached state, HEAD, remotes, and upstream
  merge base when available;
- clean/dirty state and SHA-256 of `git diff --binary HEAD`;
- Bazel version, exact targets, artifact hashes, and Governance DID hash.

The runner rejects changed credential-like paths. A dirty developer checkout
is allowed with a prominent warning: its diff hash enters the bundle ID,
`source_tree_clean = false`, and `source.patch` is saved inside the ignored
bundle. CI and `--reject-dirty` reject it. A local feature branch is not an
official SNS release, whether or not it is published.

The runner uses a generated local Bazel rc under the IO cache. It disables the
DFINITY-internal remote cache and relocates the hermetic Zig cache under the
user's IO cache. It does not write an rc or build output into tracked IC source.

### Governance overlay

`--scope governance` is the local default. It builds only the canonical
production target discovered with `bazel cquery --output=files`:

```text
//rs/sns/governance:sns-governance-canister
```

Both `sns_governance.wasm.gz` and its decompressed Wasm are retained. The exact
local Governance DID is copied and hashed. Governance is marked as a local
override; Root, Ledger, Index, Archive, and Swap remain byte-for-byte pinned to
the official baseline. A requested local build never substitutes official
Governance.

### Full SNS suite

`--scope sns-suite` builds the canonical local Governance, Root, ICRC Ledger,
Index-ng, Archive, and Swap targets from one checkout. SNS-W is included only
for `lifecycle` or `all`. Target outputs are always located with Bazel cquery,
not a hard-coded `bazel-bin` path. Use this scope for coordinated framework or
release-branch changes, Root/Governance upgrades, and local SNS-W publication.

The same resolved manifest feeds direct PocketIC installation, baseline to
candidate upgrades, and SNS-W publication. Tests must compare the published
compressed hashes with the resolved bundle hashes.

## Bundle format and validation

Bundles are immutable, content-addressed directories:

```text
<cache>/<bundle-id>/
├── manifest.toml
├── provenance.toml
├── SHA256SUMS
├── governance.did          # capability-bearing local candidates
├── source.patch            # dirty local candidates only
└── wasms/
    ├── sns_governance.wasm
    ├── sns_governance.wasm.gz
    └── ...
```

The manifest extends the existing real-canister format additively. Each
artifact retains its filename, raw/compressed SHA-256, source kind, source
revision, and source URL or local target provenance. `[variant]` records the
official baseline, IC state, build targets, and component overrides.
`[capabilities]` records additive API capabilities.

Bundle mode requires an absolute path. Before tests it rejects missing or
unexpected files, unlisted files, path traversal, symlinks, non-regular Wasm
files, credential-like names, malformed hashes, hash mismatches, and a
`SHA256SUMS` file-set mismatch. Share the entire directory as an immutable CI
artifact; CI invokes the same runner with `--source bundle`.

## Capability and compatibility behavior

`latest_reward_event_participation` is not a caller assertion. A local bundle
sets it only after the candidate DID proves the additive optional neuron field,
the `Uint128` share shape, `get_latest_reward_event`, and paginated
`list_neurons`. The `contract` profile then proves its runtime semantics with
the production Governance Wasm.

The current official lock declares the capability false. Its old neuron value
decodes with `None`; compatibility tests run, candidate allocation tests report
an explicit skip, and IO readiness fails closed with “SNS latest reward-event
participation feature unavailable.” There is no ballot/proposal fallback. Once
a reviewed official lock enables the capability, the same contract and IO tests
become mandatory without another IO binary or code-path switch.

## Profiles

- `contract` validates the bundle, additive DID/DTO compatibility, exact
  reward shares, zero native reward, event replacement, deterministic
  pagination, and E1/pages/E2 consistency.
- `io` includes `contract`, the installed stream manager, the backed reward
  pool, immutable reward-share snapshot, sequential transfers, exact dust,
  same-Wasm upgrade between recipients, and zero native SNS maturity.
- `upgrade` installs the pinned official Governance baseline, preserves old
  state through candidate upgrade, proves the post-upgrade field, exercises IO,
  and upgrades the populated candidate again.
- `lifecycle` uses the existing local NNS/SNS-W real-framework infrastructure
  for publication/deployment or Root-mediated upgrade. It is intentionally not
  an edit-loop default.
- `all` runs every available profile serially.

The integration is latest-event-only. If Governance overwrites an unprocessed
event, IO reports `RewardEventMissed`, retains the backed pool in reserve, and
does not reconstruct ballots or maturity. Supporting several unprocessed daily
events requires a separately approved upstream history or checkpoint
accumulator and is out of scope.

Candidate IO fixtures configure the native Governance reward-event duration to
the IO two-week backed-reward cadence and set both native reward rates to zero.
The participation field is therefore populated without native ordinary or
staked maturity, and one retained latest event corresponds to one IO capture
window.

## Failure guide and safety

- “dirty IC checkout rejected” means CI or `--reject-dirty` observed a local
  patch; the runner does not alter it.
- “canonical target … did not resolve” means the IC Bazel graph has changed;
  inspect and review the new canonical target rather than hard-coding output.
- “capability is true but governance.did is missing” means provenance is
  incomplete; never bypass this check.
- “SHA-256 mismatch” or “file set mismatch” invalidates the entire bundle.
- “feature unavailable” is the expected fail-closed state for an old official
  Governance. It must never trigger a ballot fallback.
- A PocketIC profile requires the pinned local `POCKET_IC_BIN`; preparation-only
  artifact resolution does not.

This workflow permits only local PocketIC execution and no-network artifact
preparation after bytes are cached. It must never use `--network ic`, `-n ic`,
public mainnet boundary URLs, production identities, or mainnet canister
install/upgrade/settings operations.

## Reusable CI path

`.github/workflows/sns-framework-variant.yml` accepts an IO ref, public IC
repository URL, exact IC ref, scope, and profile. It checks out sibling trees,
rejects a dirty candidate, calls this runner once, and uploads the manifest,
provenance, hashes, DID, and log. Candidate code is restricted to an ephemeral
GitHub-hosted runner with read-only repository permissions.

The repository does not yet contain a reviewed download URL and SHA-256 for the
pinned PocketIC 14 server. The workflow therefore fails explicitly at its
PocketIC provisioning step instead of downloading an unpinned executable or
silently skipping candidate tests. Enabling the reusable job requires one
infrastructure follow-up: provision that exact server on the hosted runner from
a reviewed hash-pinned source, then retain the runner invocation unchanged.
