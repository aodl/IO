# io_historian

`io_historian` is IO's public read model. It is rebuildable from canonical observations, is not canonical protocol truth, and is not a value-moving authority. It cannot authorize issuance, redemption, reserve movement, NNS commands, SNS lifecycle, or launch state.

IO protocol is not live. The SNS IO ledger remains not launched on mainnet. Production reservations remain empty/inert. The missing/stale/error observations are never treated as zero.

## Configuration authority

The production service has no public configuration or ingestion update method. Its typed `ObservationConfig` is accepted only as an install/upgrade argument, so configuration authority is controller/SNS Root upgrade authority:

- `null` on a prelaunch install leaves every source `PrelaunchNotConfigured` and performs no calls;
- `opt config` on the SNS-governed historian upgrade validates and activates canonical observation;
- `null` on a later same-Wasm upgrade preserves the existing configuration;
- `opt replacement` validates the complete replacement and clears observations from the previous topology.

Configuration is bounded and rejects anonymous/duplicate source topology, duplicate Accounts, malformed expected SHA-256 values, missing required module identities, and unbounded refresh intervals. The optional reward-share capability hash must equal the expected Governance module hash: it is present for the reviewed local candidate and absent for an official bundle that lacks the field, so generic module freshness cannot fabricate capability availability. Mainnet values remain absent pending separately authorized launch configuration.

## Canonical observations

One non-overlapping, one-shot timer generation observes:

- SNS ledger total supply, protocol-reserve balance, and configured excluded IO balances;
- ICP ledger liquid-reserve balance;
- a checked coherent redeemable-supply/redemption-rate snapshot, committed only when every monetary query succeeds;
- Stream manager `get_status`;
- NNS manager `get_status`, including the exact latest target/status and passive-unwind principal;
- public NNS Governance build metadata and bounded neuron-info queries for the distinct configured reward-backing and two-year neuron IDs;
- SNS Root `get_sns_canisters_summary` module hashes, controllers, topology, and archives;
- SNS Governance parameters and latest reward event;
- SNS Index status plus bounded recent histories for configured Accounts; index canisters remain the normal history abstraction.

Root-mediated canister summaries distinguish `Matching`, `Mismatch`, `Unavailable`, and `Unknown`. Index canisters are the normal account-history abstraction; ledgers remain canonical for current balances. Archive canisters are discovered and represented without unbounded archive traversal or a monetary scanner.
Public NNS neuron info supplies stake, staked maturity, dissolve delay and state.
It does not expose ordinary maturity, so the historian does not invent it or
impersonate a neuron controller.

The public production surface is read-only:

- `version`
- `get_public_status`
- `get_dashboard_state`
- `get_protocol_snapshot`
- `get_redemption_rate`

Debug ingestion and the frozen-cohort/proposal-ratio/scanner-era presentation surface were deleted. A debug build retains only a permissionless local refresh trigger; completed monitoring evidence must use autonomous canonical refresh, not debug ingestion.

## Stable state and freshness

Configuration, last-known successful observations, timestamps, per-source errors, and the bounded read model survive upgrades. A transient refresh-active flag does not survive, so an interrupted refresh cannot wedge the canister. Upgrade marks previously fresh sources stale until the re-armed timer completes. Retryable errors preserve the original successful observation timestamp and values.

The historical v1/v2 stable record is decoded through a narrow legacy compatibility shape. Historical scanner/cohort records do not re-enter the current public model.

The protected canister `oae4c-3iaaa-aaaar-qb5qq-cai` and neuron `6345890886899317159` are not observation sources or deployment targets.
