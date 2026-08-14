# Source-open package

The workspace's accepted package metadata declares `Apache-2.0`. The repository therefore includes the canonical Apache License 2.0 text in `LICENSE`, keeps source and exact build instructions public, and verifies dependency licenses with the pinned `cargo-deny` gate.

The source package includes Rust and frontend lockfiles, production DIDs, generated browser Candid bindings, deterministic release scripts, and exact artifact/source provenance. Generated frontend bundles and release Wasm are reproducible outputs; their preferred editable sources remain in this repository.

No vendored third-party source or inherited third-party `NOTICE` attribution was found in the distributable source tree. Apache 2.0 does not require inventing a `NOTICE` file when no applicable attribution notice exists. A project copyright-holder notice was not added because the repository does not record an approved legal owner/name. Confirming that optional copyright attribution is one explicit product/legal decision; it does not change the existing Apache-2.0 licensing decision.

Release diligence:

```bash
tools/scripts/provision-security-tools
cargo run -p xtask -- security_scan_required
cargo run -p xtask -- verify_release
```

This packaging review is not legal advice or an external legal audit.
