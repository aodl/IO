# Pinned NNS execution boundary

IO pins NNS Governance and the ICP Ledger independently. Sharing the
`dfinity/ic` repository does not make an unrelated component revision an
acceptable substitute for its own reproduced source and artifact identity.

## Active component pins

| Component | Source revision | Official compressed SHA-256 | Raw Wasm SHA-256 | Candid DID SHA-256 |
| --- | --- | --- | --- | --- |
| NNS Governance | `c748b8e76b90ceef329c055e6f7b38a00aae8745` | `e4e9e99730dbee3a6fb9a95b40b10b512ad4831c9d2f6efb51d3f0a5d243b503` | `573af1cde5bf55a5e4dbf2d47f8dd340f7a73a107eebbc645fe1202b97f61e85` | `6e9a397f4bf0adc913980ef6c176e765534617d0ce59d52e7bcc66add2b0cd71` |
| ICP Ledger | `021bf342f66296d5605b355a61b2430406a83783` | `5d69ec2e26e5546fe7e94bab721d6c4ed840106f9e2e69d11a8f3ee6e7721df0` | `9c1ff658635daabb7a3e9dcc5dca337eee5008bc2033d0e929c3fae53814f91c` | `45a6f13779ead0f7247b728f7a8953d649173863fea1f01fbf7c04f30589aad7` |

NNS proposal 143660 is the active Governance source. Public proposal metadata
was checked through proposal 143685 on 2026-08-25; no later executed Governance
upgrade superseded 143660.

## Bound behavior

The Governance boundary includes the production `ClaimOrRefresh`,
`IncreaseDissolveDelay`, `SetFollowing`, `RefreshVotingPower`, `Split`,
`StartDissolving`, `Disburse`, maturity, and merge response shapes. The pooled
parent is non-dissolving at exactly 1,209,600 seconds, has auto-stake disabled,
and follows one configured neuron for topics 0, 4, and 14. The exact candidate
tests prove the 14-day voting threshold, follow-based voting and maturity,
direct top-up, concurrent children, separate split/start, principal return,
and zero-principal maturity cleanup.

The ICP Ledger pin independently fixes exact transfer, fee, and block recovery.
Ambiguous outgoing-transfer recovery binds the destination, amount, fee,
native memo, and creation timestamp; there is no ICRC memo on this ICP path.
The minimum Governance stake is `100_000_000` e8s and maturity finalization is
scheduled after `604_800` seconds. IO treats the role staging Account balance as
controlled-value authority and supplies no maturity Mint block to its protocol.

`cargo run -p xtask -- validate_nns_boundary_pin` rejects drift in either
component revision, its artifact hashes, or this record. The exact candidate
lock and PocketIC tests are required in addition to the static validator.
