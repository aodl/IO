# Stream Manager Architecture

The stream manager observes ledger/index flows and records durable operations before downstream value-moving work. Its production DID remains constructor-only; debug ticks and state inspection are debug/test APIs only.

The ledger/index boundary lives in `io-ledger-types`. The stream scheduler uses boundary-level cursor validation for future production-shaped index scans while current PocketIC scan sources continue to use mock ledger and index canisters.

Current mock mode:

- scans mock ICP index history for Jupiter Faucet and NNS maturity deposits through mock `debug_get_transactions`;
- scans mock IO index history for redemption transfers through mock `debug_get_transactions`;
- transfers IO from protocol reserve for issuance and rewards through `LedgerTransferClient` mock adapters;
- transfers ICP for redemption payouts through a `LedgerTransferClient` mock adapter;
- returns redeemed IO to protocol reserve through a `LedgerTransferClient` mock adapter.

Future production mode must preserve the same journal semantics through real ICP/IO ledger and index adapters:

- observed source block creates or reuses one operation;
- downstream transfer success records its block;
- duplicate transfer results are accepted only after matching the expected operation;
- transfer failures leave the operation retryable;
- completed phases are not repeated.

Fees are explicit at the boundary, but mock economics still ignore ledger fees. Production fee and dust handling must be finalized before mainnet.

Production scan/index adapters and archive traversal are still future work.
