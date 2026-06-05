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
- git commit if available.

It intentionally omits build timestamps.

## Multi-Builder Comparison

On two builders:

```bash
cargo run -p xtask -- build_canisters
cargo run -p xtask -- verify_artifacts
sha256sum release-artifacts/*.wasm release-artifacts/*.wasm.gz
```

Compare `manifest.json` and all SHA sidecars. If the git commit differs, compare only artifact hashes and byte sizes.

## Current Limitations

- Builds are not executed inside a pinned Docker/Nix image.
- Rust/cargo cache contents may differ between hosts.
- Wasm metadata policy is minimal.
- Real production client dependencies are not yet integrated.

Future work should add a pinned container or Nix build and independent builder attestation.
