# Pinned NNS execution boundary

IO pins NNS Governance and the ICP Ledger independently. Sharing the
`dfinity/ic` repository does not make an unrelated component revision an
acceptable substitute for its own reproduced source and artifact identity.

## Active component pins

| Component | Source revision | Official compressed SHA-256 | Raw Wasm SHA-256 | Candid DID SHA-256 |
| --- | --- | --- | --- | --- |
| NNS Governance | `8aa4680e378f3248e7e7b9b8237915aded999bd9` | `b41a5add38d54751d53fb4f0c826b09aaee38e0c5bea632400f1dbaaa11cfd4b` | `eaa2da45722d980b25405525873571ab7dad426a93e1d4971f6b555d80906d85` | `6e9a397f4bf0adc913980ef6c176e765534617d0ce59d52e7bcc66add2b0cd71` |
| ICP Ledger | `021bf342f66296d5605b355a61b2430406a83783` | `5d69ec2e26e5546fe7e94bab721d6c4ed840106f9e2e69d11a8f3ee6e7721df0` | `9c1ff658635daabb7a3e9dcc5dca337eee5008bc2033d0e929c3fae53814f91c` | `45a6f13779ead0f7247b728f7a8953d649173863fea1f01fbf7c04f30589aad7` |

NNS proposal 143577 is the active Governance source. Public proposal metadata
was checked through proposal 143660 on 2026-08-22; 143660 was open rather than
executed, so no later executed Governance upgrade superseded 143577.

## Bound behavior

The Governance boundary includes the production `ClaimOrRefresh`,
`IncreaseDissolveDelay`, `SetFollowing`, `RefreshVotingPower`, `Split`,
`StartDissolving`, `Disburse`, maturity, and merge response shapes. The pooled
parent is non-dissolving at exactly 1,209,600 seconds, has auto-stake disabled,
and follows one configured neuron for topics 0, 4, and 14. The exact candidate
tests prove the 14-day voting threshold, follow-based voting and maturity,
direct top-up, concurrent children, separate split/start, principal return,
and zero-principal maturity cleanup.

The ICP Ledger pin independently fixes the exact transfer, fee, block, and Mint
proof shape. The minimum Governance stake is `100_000_000` e8s and maturity
finalization is scheduled after `604_800` seconds. A maturity Mint has a
native memo, no ICRC memo, and no caller-provided creation time.

`cargo run -p xtask -- validate_nns_boundary_pin` rejects drift in either
component revision, its artifact hashes, or this record. The exact candidate
lock and PocketIC tests are required in addition to the static validator.
