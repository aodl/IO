# Local SNS evidence disposition

The immutable packages `deploy/local-sns-rehearsal/evidence/2026-08-11-4320fdf/` and `deploy/local-sns-rehearsal/evidence/2026-08-12-4320fdf-monitoring/` retain valid checksums and historical observations. Their bytes are preserved.

They remain valid evidence for SNS-W mechanics, controller handoff, lifecycle authority, transfer mechanics, upgrade mechanics, replay behavior, reward observation, and historian connectivity.

They are superseded as current redemption-rate and excluded-Account evidence. Both runs configured the SNS Governance default Account as excluded, while the pinned DFINITY SNS implementation holds treasury genesis tokens in the Governance-owned `token-distribution` subaccount with nonce `0`. A successful zero balance for the Governance default Account did not prove economic completeness.

`canonical_redemption_economics = true` identifies the complete canonical proof shape; it does not mean that every such historical package must match the repository's checked-in release. Current launch readiness instead requires the closed `deploy/local-sns-rehearsal/evidence/current-canonical.toml` selector to name exactly one later immutable package and bind its source, artifact commit, release manifest, package manifest and checksum inventory. The selected package must preserve the exact Stream, NNS-manager, historian, SNS-init and Account-map inputs; prove the derived treasury Account and nonzero observed balance; and pass checked cross-consistency for the Stream quote, both ledgers and the historian snapshot against the selected current release. Historical package integrity remains independently verifiable against its recorded release pair and is not described as corruption.

The established current selection is:

- package: `2026-08-14-4320fdf-canonical-economics`
- source: `55b2099a555799c4a032308eb8a39049c7946193`
- artifact: `09b115f708ec784766327539f9cf4e5e21668d84`
- selector SHA-256: `de87d5a875ff72582152f7493f350565ef39f81aa7fb182614f7d101bd09ec86`
- release-manifest SHA-256: `9c477bf415621762a42454ee89864da034a996ef36c5637dac111c7f4f2adae1`
- package-manifest SHA-256: `e8de7445e79838b555168807f93fc8a3b87e1bb1746bd6eb4af9083ae8eed5c9`
- package `SHA256SUMS` SHA-256: `868356c51676388b580f2222f00129cadb30c73677d9ca578489dd3c02d3700f`

The immutable `2026-08-12-4320fdf-canonical-economics` package remains
historical corrected evidence for its own recorded release and was not rebound.
