# ADR: IO ledger fee and supply authority

Status: Accepted

The SNS ledger is the canonical IO supply authority. Launch configuration requires a positive explicit transfer fee and no fee collector; ordinary transfer fees therefore burn IO and reduce `icrc1_total_supply`.

The stream manager never reconstructs global supply from transactions. It reads canonical total supply, reserve balance and the bounded configured excluded Accounts when pricing an authenticated redemption or settling a receipt. A redemption pulls IO directly from the caller Account into reserve with ICRC-2. Reserve issuance and rewards use explicit ICRC-1 intents and account for the sender-paid fee burn.

Historian ledger/index/archive ingestion may explain fee burns and supply history, but it cannot provide a monetary input or complete an operation. Any intentional fee-policy change requires pause, drain, governance approval, configuration update, validation and an audited forward upgrade.

Earlier alternatives and scanner-based reconciliation analysis are non-normative pre-simplification research.
