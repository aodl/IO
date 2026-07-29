# ADR: IO Ledger Fee Disposition and Supply Authority

Status: Proposed, unresolved

Date: 2026-07-28

## Context

This ADR records an unresolved monetary decision. It does not change IO monetary code.

Observed official DFINITY source:

- Repository: `dfinity/ic`
- Commit: `2d7f90fb23672cc3b81c216a33d04c75672dd308`
- Source paths:
  - `rs/sns/init/src/lib.rs`
  - `rs/ledger_suite/icrc1/ledger/src/lib.rs`
  - `rs/ledger_suite/common/ledger_core/src/balances.rs`
  - `rs/sns/testing/README.md`

At that commit, the standard SNS init path builds ledger init args with minting account, transfer fee, archive options, index principal and initial balances, but does not set a fee collector. The ICRC ledger stores `fee_collector_account` as an optional fee collector. Ledger balance transfer debits `amount + fee`, credits the recipient with `amount`, and when no fee collector is present, adds the fee to the token pool. In this ledger implementation, that is fee burn from circulating supply.

Working invariant:

Ordinary protocol issuance must not mint IO. Canonical ledger fee effects, whether burned or collected, must be explicitly observed, modelled and reconciled. Total supply must not be assumed constant unless the final ledger configuration proves that property.

## Option A: Standard SNS Fee Burn

The SNS ledger launches without an explicit fee collector. Transfer fees are burned.

- Total supply behavior: decreases by every non-mint transfer fee.
- Protocol reserve behavior: outgoing reserve transfers reduce reserve by amount plus fee; incoming redemption returns increase reserve by the transferred amount only.
- Redeemable supply formula: must use canonical ledger total supply minus protocol reserve minus non-redeemable governance supply, with burned fees observed.
- Fee account: none.
- Unrelated user transfers: burn fees and reduce canonical total supply without an IO protocol journal entry.
- Staking transfers: burn fees and reduce total supply while changing governance-visible stake.
- Incoming redemption fees: user-to-redemption fee is burned before IO return handling.
- Outgoing reserve/reward/redemption-return fees: burn from the sender account and total supply.
- Canonical data source: SNS ledger total supply plus account balances and block/index history.
- Global block/archive traversal: required to observe unrelated burns unless total supply query is accepted as canonical supply authority.
- Bootstrap/upgrade implications: model bootstrap must reconcile against ledger total supply and reserve balances before monetary execution.
- Governance authority required: none beyond accepting default SNS ledger configuration; future changes require SNS governance.
- Historian presentation: must present fee-burn supply changes explicitly.
- Monitoring: total supply, reserve balance, fee burns, archive gaps, and index lag.
- Migration feasibility after SNS launch: difficult to reverse without governance-approved ledger upgrade/config change.

## Option B: Explicit Fee Collector

The SNS ledger launches or upgrades with a fee collector account.

- Total supply behavior: ordinary transfers keep total supply constant.
- Protocol reserve behavior: reserve pays outgoing fees to the collector; incoming reserve receives only transferred amounts.
- Redeemable supply formula: must decide whether collector balance is redeemable, protocol-owned, excluded, or separately governed.
- Fee account: exact collector account must be configured, observed, and monitored.
- Unrelated user transfers: move fees to collector, not burn them.
- Staking transfers: move fees to collector and require governance/stake accounting to treat collector balance consistently.
- Incoming redemption fees: user-to-redemption fee accrues to collector.
- Outgoing reserve/reward/redemption-return fees: accrue to collector.
- Canonical data source: ledger total supply, collector balance, reserve balance, block/index history.
- Global block/archive traversal: required for collector-flow presentation and duplicate proof.
- Bootstrap/upgrade implications: must reconcile collector account and policy before monetary execution.
- Governance authority required: SNS governance approval for launch config or upgrade/change.
- Historian presentation: must identify collector balance and exclude/include it according to the final policy.
- Monitoring: collector balance deltas, unexplained collector inflows, total supply, archive gaps, index lag.
- Migration feasibility after SNS launch: possible only through supported governance-controlled ledger upgrade/config path.

## Option C: Zero Fee or Separately Approved Configuration

The SNS ledger uses zero fee or another explicitly approved tokenomics configuration.

- Total supply behavior: depends on the approved configuration; zero fee preserves total supply for transfers.
- Protocol reserve behavior: no fee debit under zero fee; other configurations must be specified.
- Redeemable supply formula: must be derived from the final configuration and observed ledger behavior.
- Fee account: none for zero fee; otherwise explicit.
- Unrelated user transfers: no fee effect under zero fee.
- Staking transfers: no fee effect under zero fee.
- Incoming redemption fees: none under zero fee.
- Outgoing reserve/reward/redemption-return fees: none under zero fee.
- Canonical data source: ledger metadata, total supply, account balances, and governance-approved config.
- Global block/archive traversal: still required for duplicate proof and account-history completeness.
- Bootstrap/upgrade implications: must reject stale nonzero-fee assumptions.
- Governance authority required: explicit approval before SNS launch or later governance action.
- Historian presentation: must label the nonstandard policy plainly.
- Monitoring: config drift, total supply, account balances, archive gaps, index lag.
- Migration feasibility after SNS launch: governance-dependent and should not be assumed.

## Decision

Unresolved. No option is selected in this tranche.
