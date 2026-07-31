# NNS neuron manager

The manager is self-bound to its running canister principal and uses distinct Jupiter, two-year maturity, two-week maturity, unwind and operational-fee Accounts. Each sending staging Account requires explicit fee float.

Jupiter stakes checked 40% into the permanent neuron and delivers the remainder as proved liquid backing. Maturity stakes 40% of ordinary maturity and disburses all remaining maturity; actual modulated ICP is backing. Target growth above canonical principal reports UnderTarget and never consumes liquid backing. Target excess permits one unwind child.

Production authority is intended to remain at existing controller `oae4c`; `tatch` is unused. No mainnet operation is authorized.
