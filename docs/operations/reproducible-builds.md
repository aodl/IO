# Reproducible Builds

IO artifacts are reproducibility-improved but not fully hermetic.

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
source commit outside `release-artifacts/`. Reachable ancestry is insufficient.
Verification also rejects dirty tracked or untracked source files. A direct
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
sidecars, manifest hashes and byte sizes against the exact current source tree:

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

Run it with:

```bash
cargo run -p xtask -- verify_recorded_source
```

Neither `verify_recorded_source` nor `verify_release` replaces the caller's
`release-artifacts/`. A bad checked-in Wasm, deterministic gzip, SHA sidecar, or
manifest therefore fails verification even when the recorded source rebuilds
correctly.

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
