# Pinned NNS execution boundary

IO's NNS command DTOs and real PocketIC execution tests are bound to one exact
DFINITY source revision:

`021bf342f66296d5605b355a61b2430406a83783`

The Wasm metadata section `git_commit_id` in both artifacts contains that exact
revision.

## Artifacts

| Artifact | Official source artifact | Compressed SHA-256 | Raw Wasm SHA-256 | Candid DID SHA-256 |
| --- | --- | --- | --- | --- |
| NNS Governance | `canisters/governance-canister.wasm.gz` | `c66ff7d948ff79a826e61eab9e11714082d93a45e42f3b7deec1c2377341285f` | `0a341fd53eba8cdfdd2330f968758bab2858fe7a26bbe1bc6a55320c23ba0ec5` | `edbbc660d8a819ac4c400296d444f3caf01b21fed3680d0defc099bac3d02c84` |
| ICP ledger | `canisters/ledger-canister_notify-method.wasm.gz` | `5d69ec2e26e5546fe7e94bab721d6c4ed840106f9e2e69d11a8f3ee6e7721df0` | `9c1ff658635daabb7a3e9dcc5dca337eee5008bc2033d0e929c3fae53814f91c` | `45a6f13779ead0f7247b728f7a8953d649173863fea1f01fbf7c04f30589aad7` |

The Candid hash is SHA-256 over the exact public `candid:service` metadata text
extracted from the corresponding raw Wasm by `ic-wasm`.

## Source paths and behavior

The local NNS DTOs are checked against these exact source paths at the pinned
revision:

- `rs/nns/governance/proto/ic_nns_governance/pb/v1/governance.proto`
- `rs/nns/governance/src/governance/disburse_maturity.rs`
- `rs/nns/governance/src/governance/ledger_helper.rs`
- `rs/nervous_system/canisters/src/ledger.rs`
- `rs/ledger_suite/icp/ledger/src/main.rs`
- `rs/ledger_suite/icp/src/lib.rs`
- `rs/ledger_suite/icp/ledger.did`

The pinned behavior is:

- `StakeMaturityResponse` returns exact `maturity_e8s` and
  `staked_maturity_e8s` values.
- `DisburseMaturityResponse` returns optional `amount_disbursed_e8s`.
- the minimum maturity disbursement is `100_000_000` e8s;
- finalization is scheduled exactly `604_800` seconds after initiation;
- finalization applies the cached daily maturity modulation to the nominal
  maturity before minting ICP;
- the resulting Mint uses legacy ICP ledger `transfer` with fee zero, no source
  subaccount, native memo equal to the NNS Governance finalization
  `now_seconds`, no ICRC memo, and no caller-provided `created_at_time`;
- the ICP ledger therefore records its own processing time as the transaction
  `created_at_time`.

`cargo run -p xtask -- validate_nns_boundary_pin` rejects any difference among
the implementation source pin, NNS Governance artifact revision, NNS/ICP ledger
artifact revision, hashes, and this evidence record. Real NNS compatibility is
not established unless that validator and the pinned real PocketIC tests pass.
