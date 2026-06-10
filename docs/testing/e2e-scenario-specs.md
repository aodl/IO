# E2E Scenario Specs

Each scenario states the intended real-canister proof target and the current coverage status.

## 1. ICP deposit -> IO reserve issuance

Starting state: user has ICP; IO reserve has funded SNS IO; stream manager is local/test only. Canisters: ICP ledger/index, IO stream manager, real SNS IO ledger/index. Actions: user deposits authorized ICP. Expected ledger blocks: ICP deposit block, SNS IO reserve-to-user block. Expected index history: both account histories expose the blocks in supported order. Expected governance state: unchanged. Expected IO state: journal terminal success, no minting. Historian/frontend: local/prelaunch only. Failure modes: duplicate deposit, BadFee, insufficient reserve, index lag. Current coverage: mock/model and SNS-shaped PocketIC for stream-manager flow; opt-in real-framework PocketIC proves the SNS reserve-to-user ledger/index slice when pinned artifacts are supplied.

## 2. Tiny ICP dust deposit -> terminal rejection, scanner advances

Starting state: scanner cursor before tiny deposit. Canisters: ICP ledger/index, stream manager. Actions: authorized tiny ICP transfer. Expected ledger blocks: ICP deposit only. Expected IO state: terminal rejection and cursor advance. Current coverage: SNS-shaped PocketIC; real ledger/index not covered.

## 3. Duplicate ICP deposit -> no double issuance

Starting state: processed deposit recorded. Actions: same ledger event observed again. Expected IO state: idempotent no-op/no second reserve transfer. Current coverage: unit/model and mock/PocketIC; real index duplicate replay not covered.

## 4. User stakes IO into SNS neuron -> governance state observed

Starting state: user has liquid SNS IO. Canisters: real SNS ledger/index/governance/root. Actions: user follows normal SNS staking path. Expected ledger blocks: stake transfer/subaccount funding per SNS model. Expected governance state: neuron exists with controller/permissions/stake/dissolve state. Expected IO state: read-only snapshot can see neuron, no value-moving mutation. Current coverage: production-shaped DTO unit tests only; normal real SNS staking not covered.

## 5. User increases IO neuron stake -> IO APY increases according to policy

Starting state: eligible staked SNS neuron exists. Actions: normal SNS top-up or equivalent governance state change. Expected governance state: cached stake increases for same neuron. Expected IO protocol state: reward/APY entitlement increases by stake-time formula without duplicate counting. Current coverage: unit `increasing_staked_io_increases_reward_weight_without_double_counting`; real SNS top-up not covered.

## 6. User votes/follows -> maturity/participation reflected if policy requires it

Starting state: eligible neuron and reward-eligible proposal. Actions: direct vote or followed vote. Expected governance state: ballot records direct/followed vote and rewards/maturity fields are represented honestly. Expected IO state: participation ratio changes reward allocation. Current coverage: unit production-shaped DTO and mock governance tests; real voting/rewards not covered.

## 7. User redeems IO -> net ICP payout and IO return

Starting state: user has liquid IO, stream manager has liquid ICP. Actions: user sends IO redemption transfer. Expected ledger blocks: SNS IO user-to-reserve transfer, ICP payout block. Expected IO state: gross/net/fee intent preserved, terminal success. Current coverage: model and mock/PocketIC; real SNS ledger not covered.

## 8. Redemption retry after ICP payout failure

Expected: IO return proof remains pending or retryable, ICP not double-paid. Current coverage: mock/PocketIC retry tests; real ledgers not covered.

## 9. Duplicate redemption retry after upgrade

Expected: stable journal preserves payout/return intent and duplicate proof prevents double-pay. Current coverage: stable fixtures and mock/PocketIC upgrade retry tests; real ledgers not covered.

## 10. SNS governance unavailable -> APY fails safe

Expected: no fake APY/reward increase, retry/error state auditable. Current coverage: governance snapshot error unit tests; no real transient governance failure.

## 11. SNS index lag -> scanner does not corrupt state

Expected: cursor does not advance past unreadable history; later catch-up succeeds. Current coverage: unit and mock/PocketIC for induced lag; opt-in real-framework PocketIC observes index catch-up after ticks but does not deterministically induce lag.

## 12. Archive-required account history -> documented/future handling

Expected: archive-required is detected and scanner fails closed until archive traversal is implemented. Current coverage: unit and mock/PocketIC flag handling; real archive not covered because the real-framework smoke does not generate enough transactions or archive config pressure.

## 13. Mid-flight upgrade at each critical boundary

Boundaries: before deposit processing, after deposit before IO transfer, after IO transfer before journal terminal state, during redemption before ICP payout, after ICP payout before IO return, after IO return before terminal state, after SNS neuron stake observed before APY update. Current coverage: several mock/PocketIC retry upgrade paths and stable fixtures; SNS stake-observed boundary and all-real canisters not covered.

## 14. Historian displays local/real/prelaunch status honestly

Expected historian state: source freshness/staleness explicit; local evidence local-only; no protocol-truth claim. Frontend claim: IO protocol not live and SNS IO ledger not launched on mainnet. Current coverage: static/host historian freshness and prelaunch shell gates.

## 15. Frontend never calls value-moving canisters

Expected: frontend imports only historian production declarations; no stream-manager/NNS-manager calls. Current coverage: static `did_surface`, `validate_prelaunch_public_shell`, and `validate_historian_freshness` gates.
