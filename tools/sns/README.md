# IO Local SNS Harness Fixtures

This directory contains IO-owned local SNS testing fixtures. They are for harness iteration only and are not production launch configuration.

Files:

- `sns_init.io.local.yaml`: SNS-shaped local init skeleton for IO.
- `official-sns-testing-notes.md`: optional local-only notes for official SNS testing tools.

The required IO workflow remains `xtask`, `icp-cli`, Rust tests, and PocketIC. Required checks must not depend on `dfx`, must not use `--network ic`, and must not call mainnet.

The local fixture uses placeholder principals and TODOs because final SNS launch values are not locked. Mainnet principals, token economics, controller handoff, SNS-root lifecycle, and official launch validation remain future work.
