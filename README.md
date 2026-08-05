# soroban-VaultFactory

[![CI](https://github.com/Ibawa12/soroban_vaultFactory_crates/actions/workflows/ci.yml/badge.svg)](https://github.com/Ibawa12/soroban_vaultFactory_crates/actions/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

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

## The problem this solves in the Soroban ecosystem

Custody is the first question any team, DAO, or protocol treasury has to
answer before it can hold funds on Stellar/Soroban at all, and today the
ecosystem has no canonical answer to point to. The practical alternatives
are:

- **A single EOA-equivalent signer.** Fast, but a single compromised key
  (or single point of human error) is a total loss. Not a serious option
  for anything beyond a hobby project.
- **Rolling a bespoke multisig contract per team.** This is what actually
  happens today, and it means the same security-critical logic — signature
  threshold checks, replay protection, upgrade paths — gets written and
  audited independently by every team that needs it, with wildly varying
  levels of rigor depending on who's building it and how much runway they
  have for a security review.
- **Importing a multisig pattern designed for a different chain.** EVM
  multisig patterns (Gnosis Safe and its many derivatives) don't map
  cleanly onto Soroban's native `auth` framework, storage-TTL model, or
  resource-metered execution — a naive port either fights the platform or
  quietly reintroduces assumptions that don't hold here.

What's specifically missing beyond plain threshold signatures is just as
important: **a mandatory delay between "enough signers approved" and "funds
actually move."** Without a timelock, a compromised or colluding majority
of signers can drain a treasury before anyone watching has a chance to
react. And without **per-asset spending limits**, every single payout — no
matter how small or routine — has to go through the exact same
full-weight governance process as a treasury-emptying transfer, which
either slows an organization down or (more likely, in practice) trains
signers to rubber-stamp proposals they haven't actually reviewed.

`soroban-VaultFactory` exists to make "multisig + timelock + spending
limits, correctly composed, on native Soroban `auth`" a reusable primitive
instead of something every serious Stellar treasury, DAO, or protocol
reimplements — and re-discovers the same edge cases in — on its own.

## Status

**Feature-complete, still looking for review.** All six entrypoints —
initialize, configure_spending_limit, propose, approve, execute, and
deploy_vault — are implemented and covered by 20 passing integration
tests, including the full multisig+timelock+spending-limit happy path,
`UpdateSigners` governance actions, and `deploy_vault` deploying (and
successfully initializing) a real child instance of this same contract.
Security review is especially welcome on `execute` (moves real funds) and
`approve` (the M-of-N auth loop) — see
[Known limitations](#known-limitations) below for what's been flagged so
far, and [CONTRIBUTING.md](CONTRIBUTING.md) for the workflow.

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
├── contract.rs   # #[contract] entrypoints — all six implemented and tested
└── test.rs       # Env-based contract-level integration test harness
```

`types.rs` has no storage or `Env` dependency — every data structure in it
can be constructed and asserted on in an ordinary unit test. All persistence
logic is isolated in `storage.rs`, behind typed, storage-class-explicit
accessors, so it's never ambiguous at a call site whether a piece of state
is long-lived (instance) or ephemeral (temporary).

## Quick start

```bash
rustup target add wasm32v1-none    # rust-toolchain.toml pins this
cargo build --target wasm32v1-none --release   # build the deployable Wasm first —
                                                # deploy_vault's integration test
                                                # deploys a real instance of it
cargo test                                     # full test suite
```

Note the target is `wasm32v1-none`, not `wasm32-unknown-unknown`: on
current stable Rust, `wasm32-unknown-unknown`'s default codegen emits a
reference-types-style encoding for indirect calls that Soroban's Wasm
host rejects at upload time. `wasm32v1-none` targets the Wasm MVP
explicitly and is what the host actually accepts — see the comment on
`targets` in [`rust-toolchain.toml`](rust-toolchain.toml).

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

// 4. Collect approvals, wait out the timelock, execute
client.approve(&bob, &proposal_id);
// ... after `timelock_blocks` ledgers have passed:
client.execute(&alice, &proposal_id);
```

## Known limitations

Found while implementing and testing `approve`/`execute`/`deploy_vault` —
documented rather than silently worked around, and good starting points
if you want to contribute:

- **`execute` is permissionless by design.** The `executor` address isn't
  auth-checked or required to be a current signer — the multisig+timelock
  gate on the *proposal* is what authorizes the action, and `execute`
  re-validates `Ready` status, the timelock, and the approval count itself
  before doing anything irreversible. This is a deliberate choice (see the
  doc-comment on `execute`), not an oversight, but it's worth a second set
  of eyes given how security-sensitive this entrypoint is.
- **`deploy_vault` is permissionless too**, for the reasons in its
  doc-comment. An admin-gated variant would be an additive, non-breaking
  follow-up if a deployment wants one.
- **`GenericInvokeAction::args` decoding failure maps to one generic
  error**, [`VaultError::InvalidActionPayload`] — it doesn't distinguish
  "not valid XDR at all" from "valid XDR but not a shape `invoke_contract`
  accepts."
- **No getter/view entrypoint for a `Proposal`'s current state.**
  Off-chain callers (and this crate's own tests) can only infer a
  proposal's status indirectly, through how a subsequent `approve`/
  `execute` call behaves. A read-only `get_proposal` entrypoint would be a
  small, valuable, and non-breaking addition.

See [CONTRIBUTING.md](CONTRIBUTING.md) for the full workflow, coding
conventions, and PR checklist — `execute` in particular moves real funds, so
its PR checklist is stricter than usual.

## Safety notes for reviewers

- `overflow-checks = true` is set explicitly in `[profile.release]` in
  `Cargo.toml` and must never be removed.
- `execute` is the last line of defense before funds move — any
  implementation must re-check the approval threshold and timelock
  expiry at execution time, not merely trust that `approve` already
  enforced them (see the doc-comment on `execute` for why this
  defense-in-depth check matters).
- `MAX_SIGNERS` (20) and `MAX_TIMELOCK_LEDGERS` bound the vault's
  configuration space specifically to keep the M-of-N approval loop's
  gas/CPU cost predictable and to prevent a misconfigured vault from
  locking funds for an effectively unbounded duration — don't raise
  either constant without re-evaluating why it was set where it is (see
  [`src/types.rs`](src/types.rs)).

## Security

Found a suspected vulnerability — especially anything touching
authorization, the timelock, or spending limits? Please don't open a
public issue — see [SECURITY.md](SECURITY.md) for the private reporting
process, response targets, and what's in/out of scope.

## License

Apache-2.0 — see [LICENSE](LICENSE).
