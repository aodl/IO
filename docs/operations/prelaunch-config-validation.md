# Prelaunch configuration validation

Install arguments are validated against the actual canister principal. Every install starts Paused. Governance unpause performs canonical fee, supported-standard, supply/exclusion and protected-identity preflight checks.

Checked-in mainnet install-argument files are deliberately non-runnable TODO templates. ProductionActive is unavailable. A later explicitly approved audit must verify that reserved canisters contain no value-bearing state before any clean install decision.
