# Local SNS evidence disposition

The sole current local authority is the schema-v2 package
`deploy/local-sns-rehearsal/evidence/2026-08-26-9462a1a-account-semantic`.
`deploy/local-sns-rehearsal/evidence/current-canonical.toml` is the sole
selector and binds:

- source-finalization commit
  `9462a1a0df602f06fa845bd31f9fcd0adf80067a`;
- immediate artifact-recording commit
  `46713f8499cf9a63a6cd4879b1fff9c1f9ef0be5`;
- canonical evidence commit
  `1ed1399130358ed5788cae99f3d65d82cbbc70a9`;
- release-manifest SHA-256
  `011113d83510e66976f5d3eabefc57ef30ba44fc49a8973fad29be043b374431`;
- package-manifest SHA-256
  `254e8866394d62ba9fbe2b0290709d2c9dbaeb8ed84c8b3446b9a1912af0eb78`;
- package `SHA256SUMS` SHA-256
  `a1915cef319b569955c5d75ad88c4a3715af8483573d2cf9b3a2af108d6bea29`.

That package is current evidence for `B=L+P+U+T`, the exact 14-day pooled
parent, bounded cohorts, fixed TwoWeek and TwoYear semantic Accounts, paired
Jupiter/TwoWeek settlement, TwoYear no issuance, donation carry-forward,
liquidity-shortfall recovery and same-release upgrade/restart. No earlier
package has been deleted, rewritten, rebound or reinterpreted as proof of this
release.

Its evidence layers are intentionally distinct:

- Layer A uses source-built official SNS tooling for local launch, wiring,
  Governance, Root, Ledger, Index, Swap, controllers and reward observation.
- Layer B uses exact proposal-143660 PocketIC evidence for the active NNS
  Governance boundary.
- Layer C uses current-release controlled IO fixtures for account-semantic
  orchestration and induced failure/recovery cases.

The immutable packages `deploy/local-sns-rehearsal/evidence/2026-08-11-4320fdf/` and `deploy/local-sns-rehearsal/evidence/2026-08-12-4320fdf-monitoring/` retain valid checksums and historical observations. Their bytes are preserved.

They remain valid evidence for SNS-W mechanics, controller handoff, lifecycle authority, transfer mechanics, upgrade mechanics, replay behavior, reward observation, and historian connectivity.

They are superseded as current redemption-rate and excluded-Account evidence. Both runs configured the SNS Governance default Account as excluded, while the pinned DFINITY SNS implementation holds treasury genesis tokens in the Governance-owned `token-distribution` subaccount with nonce `0`. A successful zero balance for the Governance default Account did not prove economic completeness.

`canonical_redemption_economics = true` identifies the complete canonical proof shape; it does not mean that every historical package matches the current release. The selected package preserves the exact Stream, NNS Manager, historian, SNS-init and Account-map inputs; proves the derived treasury Account and nonzero observed balance; and passes checked cross-consistency for the Stream quote, both ledgers and the historian snapshot. Historical package integrity remains independently verifiable against its recorded release pair and is not described as corruption.

The preceding selection, which remains an immutable historical record, is:

- package: `2026-08-14-4320fdf-canonical-economics`
- source: `55b2099a555799c4a032308eb8a39049c7946193`
- artifact: `09b115f708ec784766327539f9cf4e5e21668d84`
- selector SHA-256: `de87d5a875ff72582152f7493f350565ef39f81aa7fb182614f7d101bd09ec86`
- release-manifest SHA-256: `9c477bf415621762a42454ee89864da034a996ef36c5637dac111c7f4f2adae1`
- package-manifest SHA-256: `e8de7445e79838b555168807f93fc8a3b87e1bb1746bd6eb4af9083ae8eed5c9`
- package `SHA256SUMS` SHA-256: `868356c51676388b580f2222f00129cadb30c73677d9ca578489dd3c02d3700f`

The immutable `2026-08-12-4320fdf-canonical-economics` package remains
historical corrected evidence for its own recorded release and was not rebound.
The preceding 2026-08-14 package likewise remains immutable. Both recorded the
obsolete protected-neuron guard and are historical rather than current.

The following corrected-target package is immutable historical evidence from
the diverged `misc` lineage; it is not current evidence for a newly finalized
master-descended release:

- package: `2026-08-14-final-validator-4320fdf-canonical-economics`
- source: `45b87d6459a1fc7652cc1f63b75dfddcc4c6b98f`
- artifact: `22c6cfb06c354061b75d40ec45b668ff0b3ef12c`
- selector SHA-256: `5434ad90719c1c0621537c56969264297280bb9f700cbf1302e61993eeae131f`
- release-manifest SHA-256: `f77238236520e0c42c0c8d2d1cd834ce8d49510697388a607e12d476fb7e2328`
- package-manifest SHA-256: `4e86bdaae6e2b6af39e8d16c44607e82881697dea64a69a94d18b01be70699ef`
- package `SHA256SUMS` SHA-256: `eee2cf8708c61ddf5e93d2b11a749c248c8d8502c38c02d344b7b1f617bac5de`

The selector is the machine-readable source of currentness. The three
authority, final-readme, and final-validator packages from `misc` remain bound
to their recorded commits and must not be rebound. They are retained on the
untouched `misc` branch, not copied into the master-descended history.

The preceding master-descended selection is immutable historical evidence for
its own release:

- package: `2026-08-14-master-descended-4320fdf-canonical-economics`
- source: `23fbc4f62863f421803cc49c34aa8c5c576b4d89`
- artifact: `2d28f84a4d36e8e9fd17d019478a0ae432d55838`
- selector SHA-256: `85cf63364014e8dbb3ef1da9a0efc4d87fe46744abf5e450741ad3f6656df992`
- release-manifest SHA-256: `144bd505bab85b05ac5f3e89c11276d21c55260b39b7076cd227a9143f1c1fc3`
- package-manifest SHA-256: `8e99919a6ed0c93c5db328b07182c07182faba947709192406ccee6d172ae7bb`
- package `SHA256SUMS` SHA-256: `11f92f48bcbfab56be360f67da74f975917e242c2ad29c404b0c841ec925c292`

The immutable
`2026-08-26-716d51e-account-semantic` package is a non-current intermediate
package from the final rehearsal/tooling closure. It remains historical
evidence for its recorded source/artifact pair and is not selected or rebound.

Package names and local fixture values are not production configuration,
candidate upstream is not an official release, and no local package is mainnet
evidence.
