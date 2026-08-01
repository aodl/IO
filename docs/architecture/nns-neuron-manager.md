# NNS neuron manager

The manager is self-bound to its running canister principal and uses distinct Jupiter, two-year maturity, two-week maturity, unwind and operational-fee Accounts. Each sending staging Account requires explicit fee float.

Jupiter stakes checked 40% into the permanent neuron and delivers the remainder as proved liquid backing. Maturity stakes 40% of ordinary maturity and disburses all remaining maturity; actual modulated ICP is backing. A two-week command can be prepared only by the stream manager for one exact closed cohort generation. Target capacity is only the canonical non-dissolving parent stake; a dissolving child is reported separately. Target growth reports UnderTarget and never consumes liquid backing. Material excess permits one direct unwind child, while fee-sized excess is recorded within conservative unwind tolerance.

Same-generation target replay re-queries the parent. When the immediate operation slot becomes idle, `resume` reconciles the latest target before returning. Merge-back and direct disbursement clear child evidence only after a canonical parent observation, and later reconciliation creates at most one fresh child if the latest target still requires it.

Production authority is intended to remain at existing controller `oae4c`; `tatch` is unused. No mainnet operation is authorized.
