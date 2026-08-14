# Local SNS evidence disposition

The immutable packages `deploy/local-sns-rehearsal/evidence/2026-08-11-4320fdf/` and `deploy/local-sns-rehearsal/evidence/2026-08-12-4320fdf-monitoring/` retain valid checksums and historical observations. Their bytes are preserved.

They remain valid evidence for SNS-W mechanics, controller handoff, lifecycle authority, transfer mechanics, upgrade mechanics, replay behavior, reward observation, and historian connectivity.

They are superseded as current redemption-rate and excluded-Account evidence. Both runs configured the SNS Governance default Account as excluded, while the pinned DFINITY SNS implementation holds treasury genesis tokens in the Governance-owned `token-distribution` subaccount with nonce `0`. A successful zero balance for the Governance default Account did not prove economic completeness.

`canonical_redemption_economics = true` identifies the complete canonical proof shape; it does not mean that every such historical package must match the repository's checked-in release. Current launch readiness instead requires the closed `deploy/local-sns-rehearsal/evidence/current-canonical.toml` selector to name exactly one later immutable package and bind its source, artifact commit, release manifest, package manifest and checksum inventory. The selected package must preserve the exact Stream, NNS-manager, historian, SNS-init and Account-map inputs; prove the derived treasury Account and nonzero observed balance; and pass checked cross-consistency for the Stream quote, both ledgers and the historian snapshot against the selected current release. Historical package integrity remains independently verifiable against its recorded release pair and is not described as corruption.
