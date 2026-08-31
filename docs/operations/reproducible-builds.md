# Reproducible Builds

IO artifacts are reproducibility-improved but not fully hermetic.

Release compilation uses the exact Rust 1.96.0 toolchain pinned by
`rust-toolchain.toml`. A floating Rust channel is not a valid release build
input because compiler drift changes the raw Wasm bytes.

Release Cargo invocations remap the builder's absolute Cargo home to the stable
virtual prefix `/io/cargo-home`. Rust dependencies otherwise embed absolute
registry source paths in runtime diagnostics and Wasm name metadata, changing
raw Wasm bytes between builders such as `/home/runner/.cargo` and a developer's
Cargo home. Ambient `RUSTFLAGS` and `CARGO_ENCODED_RUSTFLAGS` are not release
inputs; the release canister builder owns the exact remapping flag.

## Exact-source release model

Release provenance uses two commits:

1. A source-finalization commit contains every build input and no newly generated
   release artifacts.
2. `tools/scripts/build-release-from-source <source-finalization-sha>` creates a
   detached worktree at that exact commit, builds in that worktree's isolated
   Cargo target directory, verifies the raw and deterministic `gzip -n` Wasms,
   and copies the result back.
3. A following artifact-recording commit contains only `release-artifacts/`
   changes and records the source-finalization SHA in every manifest entry.

The artifact-recording commit must have the exact same tree as the recorded
source commit outside `release-artifacts/`. Artifact generation rejects dirty
tracked or untracked source files. A direct
`build_canisters` invocation requires `HEAD` to equal `IO_RELEASE_SOURCE_COMMIT`;
normal release work should use the detached-worktree script. The builder checks
the exact tree again after frontend setup and after compilation, before copying
artifacts, so a generated tracked-asset rewrite cannot be attributed to the
recorded commit.

## Generation, verification, and reproducibility

Artifact generation is the only operation that replaces the caller's
`release-artifacts/` directory:

```bash
source_commit="$(git rev-parse HEAD)"
tools/scripts/build-release-from-source "${source_commit}"
```

Artifact verification is non-generating and checks the checked-in file set,
sidecars, manifest hashes and byte sizes, and requires the exact recorded source
commit to be an ancestor of the checkout:

```bash
cargo run -p xtask -- verify_artifacts
```

Reproducibility verification preserves the complete checked-in directory before
building. It builds the exact manifest source twice into temporary output
directories and requires byte-for-byte equality, equal sizes, and the exact
expected file set for both comparisons:

```text
checked-in artifacts == exact-source build A
exact-source build A == exact-source build B
```

Build A and build B intentionally use different absolute source-root lengths and
different logical Cargo-home lengths. Build B links only the already-populated
public Cargo registry cache into its isolated Cargo home. This makes absolute
path leakage a required reproducibility failure rather than a host coincidence.

Frontend setup removes the complete generated-asset directory before `npm ci`,
and the browser bundler repeats that cleanup before writing exactly one
content-addressed `app.<hash>.js` plus its private manifest. Obsolete bundles are
therefore never embedded by `include_dir!`.

Run it with:

```bash
cargo run -p xtask -- verify_recorded_source
```

Neither `verify_recorded_source` nor `verify_release` replaces the caller's
`release-artifacts/`. A bad checked-in Wasm, deterministic gzip, SHA sidecar, or
manifest therefore fails verification even when the recorded source rebuilds
correctly.

Later immutable evidence-only commits may descend from the exact source/artifact
pair. They do not change the build attribution: `verify_recorded_source` still
checks the checked-in bytes against a detached rebuild of the manifest's exact
source commit. Generation remains stricter and is only permitted while the
working tree differs from that source under `release-artifacts/` alone.

For every release lineage, obtain the source-finalization identity from the
machine-readable manifest and prove the artifact-recording commit is its
immediate artifact-only child. Integration must preserve both commit identities.
Later evidence/documentation-tail commits select current evidence and record
readiness; they are not artifact-recording commits.
Pull-request jobs check out the PR head directly and require the PR base commit
to be an ancestor of that head. After every required workflow passes on the
exact final release-tail head and explicit authorization is given, integration
may occur only by a direct fast-forward of `master`. Squash merge, rebase merge,
an ordinary added merge commit, and ordinary GitHub merge-button integration are
forbidden. The only pre-integration exception is the release-checklist's
machine-proved ancestry reconciliation whose reviewed base tree is byte-identical
to an existing release ancestor and whose merge tree equals its first-parent
tree. If branch protection prevents a direct fast-forward, do not merge or weaken
provenance; return for an explicit integration decision and, if build inputs must
change, create and validate a new source-finalization/artifact-recording pair.

Detached worktrees live outside the repository under one portable temporary
root selected in this order: `IO_RELEASE_BUILD_TMPDIR`, `RUNNER_TEMP`, `TMPDIR`,
`XDG_CACHE_HOME/io/release-builds`, then `HOME/.cache/io/release-builds`. The
selected root must be absolute. `mktemp` gives each build a unique directory,
and the scripts remove their detached worktrees on every exit path.

`release-artifacts/manifest.json` records each canister's raw and gzip path,
SHA-256, byte size, build profile, target, and the one exact source commit. It
intentionally omits timestamps. Each Wasm also has a `.sha256` sidecar.

The checked-in artifacts are:

```text
release-artifacts/io_stream_manager.wasm{,.gz}
release-artifacts/io_nns_neuron_manager.wasm{,.gz}
release-artifacts/io_historian.wasm{,.gz}
release-artifacts/io_frontend.wasm{,.gz}
release-artifacts/manifest.json
```

## Independent comparison

Two builders must check out the artifact-recording commit with full history and
run `verify_recorded_source`. The CI workflow performs both exact-source builds,
first compares the preserved checked-in set with build A, then compares build A
with build B. Every raw Wasm, deterministic gzip, SHA sidecar, manifest byte,
file size, and file-set entry participates in both comparisons.

## Current limitations

- Builds are not executed inside a pinned Docker/Nix image.
- Rust/Cargo cache contents may differ between hosts.
- Wasm metadata policy is minimal.
- Independent builder attestations remain external release evidence.
