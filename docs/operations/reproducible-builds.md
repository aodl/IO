# Reproducible Builds

IO artifacts are reproducibility-improved but not fully hermetic.

## Exact-source release model

Release provenance uses two commits:

1. A source-finalization commit contains every build input and no newly generated
   release artifacts.
2. `tools/scripts/build-release-from-source <source-finalization-sha>` creates a
   detached worktree at that exact commit, builds there, verifies the raw and
   deterministic `gzip -n` Wasms, and copies the result back.
3. A following artifact-recording commit contains only `release-artifacts/`
   changes and records the source-finalization SHA in every manifest entry.

The artifact-recording commit must have the exact same tree as the recorded
source commit outside `release-artifacts/`. Reachable ancestry is insufficient.
Verification also rejects dirty tracked or untracked source files. A direct
`build_canisters` invocation requires `HEAD` to equal `IO_RELEASE_SOURCE_COMMIT`;
normal release work should use the detached-worktree script.

## Commands

```bash
source_commit="$(git rev-parse HEAD)"
tools/scripts/build-release-from-source "${source_commit}"
cargo run -p xtask -- verify_artifacts
```

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
run the exact-source script twice using the manifest SHA. Compare the complete
manifest, all SHA sidecars, and both raw and gzip Wasm hashes. The CI workflow
performs this repeated build and fails if either result differs.

## Current limitations

- Builds are not executed inside a pinned Docker/Nix image.
- Rust/Cargo cache contents may differ between hosts.
- Wasm metadata policy is minimal.
- Independent builder attestations remain external release evidence.
