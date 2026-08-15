# Jupiter proof-carrying integration contract

Status: normative for the simplified IO launch design.

Pinned source: [`aodl/JUPITER_FAUCET_SUITE`](https://github.com/aodl/JUPITER_FAUCET_SUITE)
commit `51ba8a46f66fae70d9b2ec9c5ff23df91bc856ad`.

The pin was inspected from source without contacting an Internet Computer
boundary endpoint. The relevant implementation is Jupiter Faucet's raw-ICP
memo route, not an invented callback API.

## Canonical route

Jupiter Disburser sends its base flow from its default Account
`{ owner = uccpi-cqaaa-aaaar-qby3q-cai; subaccount = null }` to Jupiter Faucet's
default Account `{ owner = acjuz-liaaa-aaaar-qb4qq-cai; subaccount = null }`.
Jupiter Faucet is therefore the exact source of the later raw-ICP payout into
IO.

An eligible faucet commitment uses the ASCII `canister_id.memo` directive.
For IO, `canister_id` is the NNS manager canister and the faucet sends raw ICP
to that canister's default Account. The suffix after the first dot is copied to
the outgoing ICRC-1 memo. The complete directive is limited to 32 bytes. An
empty suffix is valid. A bounded sequence suffix can be used when it fits, but
Jupiter neither requires nor interprets an IO sequence.

The faucet does not call `notify_jupiter_deposit` or any other IO method.
Notification is permissionless and proof-carrying: the exact ICP block is the
authority. The NNS manager accepts only a canonical ICP `Transfer` whose source
is the configured Jupiter Faucet default Account and whose destination is the
NNS manager default Account. Amount and operation kind come from that block,
not the notifier.

Launch authority is the conjunction of the canonical exact ICP block, the
exact configured route, the immutable `jupiter_activation_block_floor`, and
permanent completed-block replay state. Blocks below the floor are rejected
locally before any Ledger or archive call; the first block at the floor and
later blocks remain eligible for normal exact proof. The production value is
an audited launch input and remains an unresolved TODO in the non-runnable
mainnet template.

The NNS manager keeps a narrowly scoped stable set of completed Jupiter ICP
block indexes for permanent replay protection. This set is not a source-event
journal, account-history cursor, scanner, or proof-of-absence mechanism.
Processed-block replay and activation-floor rejection are always local. New
at-or-above-floor block indexes perform one bounded canonical Ledger/archive
lookup and create no negative-cache or throttle state. Because the Faucet does
not call IO, there is no configured-caller priority path. Repeated invalid
permissionless lookups remain a monitored cycles/liveness risk; they cannot
fabricate a deposit or bypass exact proof.

## Route that does not issue IO

Jupiter's decimal neuron-ID directive resolves a public NNS neuron's staking
subaccount and sends ICP directly to NNS Governance. That is a separate Jupiter
feature. It bypasses IO's NNS-manager staging Account and therefore cannot be
used as the 40/60 input for IO issuance.

No IO leaves reserve until the NNS manager has proved the inbound raw-ICP
transfer, the exact protected-neuron stake increase, and the exact liquid
receipt delivered to the stream manager.
