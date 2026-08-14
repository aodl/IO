# Real-framework artifact and SNS setup

Real-source tests consume SHA-256-pinned local Wasms through `tests/e2e_real_canisters/wasms.local.toml` and `IO_REAL_SNS_WASM_DIR`. Tests never fetch artifacts or contact mainnet.

The maintained official rehearsal uses source-built `sns-testing-init`, `sns-testing`, `sns`, PocketIC and Quill. It does not use `dfx sns`. Evidence remains incomplete when Bazel or pinned source outputs are unavailable.
