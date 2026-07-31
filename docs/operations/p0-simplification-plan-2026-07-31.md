# P0 simplification execution plan — 2026-07-31

## Safety boundary

This work is local-only. It performs no mainnet call, deployment, installation,
upgrade, funding, lifecycle action, or controller/settings change. Production
activation remains unavailable.

## Replacement order

1. Preserve the truth-pass and both frozen research branches in a verified
   bundle; branch from the exact truth-pass commit.
2. Record the simplicity constitution, feature preservation, exact official
   interfaces, and deliberate launch constraints.
3. Delete inference-driven production monetary execution and prelaunch migration
   chains.
4. Introduce clean V1 stable state and typed serialized operations.
5. Implement immutable own-transfer attempts and exact external block proofs.
6. Complete direct-reserve ICRC-2 redemption before proceeding to other flows.
7. Implement direct NNS maturity, Jupiter receipts, serialized rewards, and one
   unwind child.
8. Move all scanning and reconciliation authority to historian-only code.
9. Enforce deletion and complexity constraints with `xtask simplicity_check`.
10. Run focused tests first, then the required broad release gates without
    committing generated artifacts.

Each numbered replacement is complete only after its old production path has
been deleted. No old/new parallel monetary implementation is allowed.

## Launch reset gate

Before any later production first install or reinstall, operators must verify
under separately approved mainnet procedures that reserved canisters contain no
value-bearing state and no live protocol Wasm, then explicitly choose clean first
install or reinstall. This plan does not authorize or perform that operation.
