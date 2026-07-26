# Reproducible Builds

IO artifacts are reproducibility-improved but not fully hermetic.

## Commands

```bash
cargo run -p xtask -- build_canisters
cargo run -p xtask -- verify_artifacts
```

Artifacts:

```text
release-artifacts/io_stream_manager.wasm
release-artifacts/io_stream_manager.wasm.gz
release-artifacts/io_nns_neuron_manager.wasm
release-artifacts/io_nns_neuron_manager.wasm.gz
release-artifacts/io_historian.wasm
release-artifacts/io_historian.wasm.gz
release-artifacts/io_frontend.wasm
release-artifacts/io_frontend.wasm.gz
release-artifacts/manifest.json
```

Each raw/gz artifact has a `.sha256` sidecar. Gzip output is produced with `gzip -n -c` so filename and timestamp metadata are omitted.

## Manifest

`release-artifacts/manifest.json` records:

- canister name;
- raw and gz path;
- raw and gz SHA-256;
- raw and gz byte size;
- build profile;
- target;
- source git commit.

It intentionally omits build timestamps.

The manifest source commit is the commit whose tree contains the build inputs
for the release artifacts. Verification requires that commit to be available
locally and reachable from `HEAD`. Branches carrying checked-in release
artifacts whose manifest records an implementation source commit must be merged
with GitHub's **Create a merge commit** option so the recorded source SHA
remains an ancestor of the destination branch.

Do not use **Squash and merge** or **Rebase and merge** for these branches.
Squash merging discards the recorded source commit, and GitHub rebase merging
creates new commit SHAs. Both invalidate the manifest's exact source-commit
ancestry check.

After merging this release-artifact branch to `master`, run:

```bash
git merge-base --is-ancestor \
  e1f1e1e69c19fe08161706c4fc6345e7e63bf88c \
  master

cargo run -p xtask -- verify_artifacts
```

Both commands must pass on `master`.

## Multi-Builder Comparison

On two builders:

```bash
cargo run -p xtask -- build_canisters
cargo run -p xtask -- verify_artifacts
sha256sum release-artifacts/*.wasm release-artifacts/*.wasm.gz
```

Compare `manifest.json` and all SHA sidecars. If the source git commit differs,
compare only artifact hashes and byte sizes.

## Current Limitations

- Builds are not executed inside a pinned Docker/Nix image.
- Rust/cargo cache contents may differ between hosts.
- Wasm metadata policy is minimal.
- Real production client dependencies are not yet integrated.

Future work should add a pinned container or Nix build and independent builder attestation.
