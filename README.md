# soroban-VaultFactory

A multi-signature & timelock vault contract for
[Soroban](https://developers.stellar.org/docs/build/smart-contracts/overview),
built on Soroban's native `auth` framework — customizable M-of-N
multi-sig, block-based (ledger-based) timelocks, and value-based dynamic
spending limits, all backed by explicit, storage-class-aware persistence.

Most "multisig wallet" examples in the smart-contract world stop at
threshold signatures. Real treasuries also want a mandatory delay between
approval and execution (so a compromised majority can be noticed and
reacted to before funds move), and a way to bound how much any single
approved action can move without a full governance round-trip. This
contract treats all three — multisig, timelock, and spending limits — as
first-class, independently configurable primitives rather than bolting the
last two on as an afterthought.

## Status

**Early-stage / actively looking for contributors.** Vault initialization,
signer/threshold validation, spending-limit configuration, and proposal
creation are complete and tested. The security-critical approval and
execution flow, and the factory's child-vault deployment mechanism, are
fully specified — signatures, required steps, and every error condition are
documented directly on the functions in
[`src/contract.rs`](src/contract.rs) — but their bodies are open `todo!()`s.
See [Open Issues](#open-issues) below if you want to pick one up.

## Why this design

- **Native Soroban `auth`, not a custom signature scheme.** Every signer's
  approval goes through `Address::require_auth()`, which delegates to
  Soroban's built-in authorization framework (including support for
  contract-based custodial signers, not just plain keypairs). We don't
  re-implement signature verification.
- **Proposals live in temporary storage, not instance storage.** An
  executed or cancelled proposal has no further relevance to the vault's
  logic. Leaving it in instance storage would grow the vault's instance
  footprint — and therefore the base read cost of *every* unrelated future
  invocation — without bound. Temporary storage lets the host garbage
  collect resolved proposals once their TTL lapses, at essentially no
  cost to the contract. See [`src/storage.rs`](src/storage.rs) for the full
  rationale and the instance/temporary split.
- **Timelock duration is snapshotted per-proposal.** A proposal's
  `executable_after_ledger` is computed from the vault's `timelock_blocks`
  *at proposal-creation time* and stored on the proposal itself, not
  re-read from live config at execution time. This means a later governance
  change to the vault's default timelock can never retroactively speed up
  or slow down a proposal that's already in flight.
- **`Val` never crosses a storage boundary.** A `soroban_sdk::Val` is a
  handle into the *current* host invocation frame — it isn't meaningful
  once that invocation returns. A naive design might store `GenericInvoke`
  call arguments as `Vec<Val>` directly on a `Proposal`; since proposals are
  created in one transaction and executed in a later one, that would hold
  dangling handles. Instead, `GenericInvokeAction::args` stores each
  argument as pre-serialized XDR bytes (`Vec<Bytes>`), decoded back into a
  `Val` only at execution time. See the doc-comment on
  `GenericInvokeAction` in [`src/types.rs`](src/types.rs).
- **Every enum variant that needs multiple fields wraps a struct.**
  Soroban's `#[contracttype]` derive only supports enum variants that are
  unit variants or a single tuple field, not multi-field struct variants —
  so `ProposalAction::Transfer`, `::GenericInvoke`, and `::UpdateSigners`
  each wrap a dedicated payload struct rather than inlining fields directly
  into the enum.

## Module layout

```
src/
├── lib.rs        # module tree + re-exports
├── errors.rs     # VaultError: the contract's complete #[contracterror] taxonomy
├── types.rs      # VaultConfig, Proposal, ProposalAction, SpendingLimit, ...
├── storage.rs    # Instance vs. Temporary storage keys + TTL bumping
├── contract.rs   # #[contract] entrypoints (init/config/propose implemented;
│                 #  approve/execute/deploy_vault are documented todo!() skeletons)
└── test.rs       # Env-based contract-level integration test harness
```

`types.rs` has no storage or `Env` dependency — every data structure in it
can be constructed and asserted on in an ordinary unit test. All persistence
logic is isolated in `storage.rs`, behind typed, storage-class-explicit
accessors, so it's never ambiguous at a call site whether a piece of state
is long-lived (instance) or ephemeral (temporary).

## Quick start

```bash
cargo test                                                # full test suite
cargo build --target wasm32-unknown-unknown --release     # deployable contract Wasm
```

## Usage sketch

```rust
// 1. Stand up a 2-of-3 vault with a ~5.5-day timelock (100,000 ledgers @ ~5s each)
let signers = vec![&env, alice.clone(), bob.clone(), carol.clone()];
client.initialize(&signers, &2u32, &100_000u32);

// 2. Cap USDC outflow to 10,000 per rolling ~24h window
client.configure_spending_limit(&alice, &usdc_token, &10_000_0000000i128, &17_280u32);

// 3. Propose a transfer
let action = ProposalAction::Transfer(TransferAction {
    asset: usdc_token,
    to: recipient,
    amount: 5_000_0000000,
});
let proposal_id = client.propose(&alice, &action);

// 4. (once implemented) collect approvals, wait out the timelock, execute
// client.approve(&bob, &proposal_id);
// client.execute(&alice, &proposal_id);
```

