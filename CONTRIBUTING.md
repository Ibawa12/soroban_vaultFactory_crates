# Contributing to soroban-VaultFactory

Thanks for considering a contribution. This contract moves (or will move,
once `execute` is implemented) real funds under real multisig/timelock
guarantees — the bar for review here is correspondingly higher than for a
typical open-source PR. This document explains how to contribute
effectively and safely.

## Before you start

1. **Check for an existing issue or claim.** If you're picking up
   `approve`, `execute`, or `deploy_vault` (see the
   [README's Open Issues table](README.md#open-issues)), comment on (or
   open) the corresponding GitHub issue first so two people don't
   duplicate the same work.
2. **Read the doc-comment before writing code.** Every `todo!()` entrypoint
   in [`src/contract.rs`](src/contract.rs) has a doc-comment enumerating
   the exact required steps, in order, and every error variant it must be
   able to return. That doc-comment is the spec. If you believe it's wrong,
   incomplete, or unsafe as written, open a discussion issue before
   implementing something different — for `execute` especially, a
   "better" undiscussed deviation is a bigger risk than a slower discussed
   one.
3. **Small PRs over big ones.** One entrypoint, one bug fix, or one test
   improvement per PR. Don't bundle `approve` and `execute` into a single
   PR even though they're related — they should be reviewable (and
   revertable) independently.

## Development setup

```bash
git clone <your-fork-url>
cd soroban-VaultFactory
rustup target add wasm32-unknown-unknown   # rust-toolchain.toml pins this
cargo test                                  # Env-based integration tests
cargo build --target wasm32-unknown-unknown --release   # the deployable artifact
```

No other tooling is required to get started. If you also want to exercise
the contract via the Soroban CLI (`stellar contract deploy`, invoking it
against a local/testnet network), see the
[Soroban CLI docs](https://developers.stellar.org/docs/tools/developer-tools#cli)
— optional for contract-logic contributions.

## Coding conventions

CI (`.github/workflows/ci.yml`) enforces `cargo fmt --check`, `cargo clippy
-- -D warnings`, `cargo test`, and the `wasm32-unknown-unknown` release
build on every push and PR. The rest of these are enforced in review, so
please self-check before requesting it:

- **Every state-mutating entrypoint calls `require_auth()` on the
  address it claims to act as, before doing anything else.** This is the
  entire basis of the contract's security model — see `initialize`,
  `configure_spending_limit`, and `propose` in `src/contract.rs` for the
  established pattern (auth check → validate → load/mutate state via
  `crate::storage` → persist).
- **Never introduce a bare `panic!`.** Every fallible entrypoint returns
  `Result<T, VaultError>`. If your change hits a new failure mode not
  already covered by [`VaultError`](src/errors.rs), add a variant in the
  appropriate section (rather than overloading an existing one) with a
  discriminant one higher than the current maximum in that section, and
  document what triggers it.
- **Respect the instance vs. temporary storage split.** Long-lived
  configuration (`VaultConfig`, `SpendingLimit`) belongs in instance
  storage; ephemeral, naturally-expiring state (`Proposal`,
  `SpendingUsage`) belongs in temporary storage. See the module doc-comment
  in [`src/storage.rs`](src/storage.rs) for the full rationale — if you're
  tempted to put a new piece of state in instance storage "to be safe,"
  read that rationale first.
- **Never store a `soroban_sdk::Val` in a persisted type.** It's a
  handle into the current host invocation frame and doesn't survive across
  transactions. See the doc-comment on `GenericInvokeAction` in
  [`src/types.rs`](src/types.rs) for the XDR-bytes pattern used instead.
- **Enum variants needing more than one field wrap a dedicated struct**,
  because Soroban's `#[contracttype]` derive doesn't support multi-field
  struct variants. Follow the `TransferAction`/`GenericInvokeAction`/
  `UpdateSignersAction` pattern in `src/types.rs` for any new
  `ProposalAction` variant.
- **Checked arithmetic only** on any `i128` amount — never bare `+`/`*`/`-`.
- **No unrelated formatting churn.** Run `cargo fmt` on files you actually
  touched; don't reformat files you didn't otherwise change.

## Testing requirements

- Add unit/integration tests in [`src/test.rs`](src/test.rs) following the
  existing `setup()`/`setup_uninitialized()` harness pattern — every test
  should register a fresh contract instance rather than sharing state
  across tests.
- Cover the golden path **and** every documented error condition for the
  entrypoint you're implementing (e.g. for `execute`: successful dispatch
  of each `ProposalAction` variant, `TimelockNotExpired`,
  `InsufficientApprovals`, `SpendingLimitExceeded`).
- Remove the `#[ignore = "..."]` attribute from the corresponding
  placeholder test in `src/test.rs` and complete its assertions once your
  implementation is in — those tests currently exist specifically as your
  target.
- Run `cargo test` (not just `cargo check`) before opening a PR. All
  existing tests must still pass.

## PR checklist

- [ ] Linked to (or created) the tracking issue
- [ ] Implementation matches the doc-comment's specified steps and error
      conditions, or the doc-comment was updated to match a deliberate,
      discussed deviation
- [ ] Every new state-mutating path calls `require_auth()` before mutating
      anything
- [ ] New/updated tests in `src/test.rs`, covering the golden path and
      every documented error condition
- [ ] Corresponding `#[ignore]` removed with real assertions
- [ ] `cargo test` passes locally
- [ ] `cargo build --target wasm32-unknown-unknown --release` succeeds
- [ ] No new `unwrap()`/`expect()`/`panic!()` — every failure path returns
      `Result<T, VaultError>`
- [ ] No `soroban_sdk::Val` introduced into a persisted (`#[contracttype]`)
      struct

### Additional checklist for `execute` specifically

- [ ] Re-validates approval threshold and timelock expiry at execution
      time, independent of whatever `approve` already checked
      (defense-in-depth — see the `execute` doc-comment)
- [ ] Spending-limit rollover logic correctly handles a period boundary
      falling exactly on the current ledger, not just "sometime after"
- [ ] Includes a test where `execute` is attempted twice on the same
      proposal (must fail the second time)

## Commit / PR style

- Commit messages: imperative mood, explain *why* over *what* where it's
  not obvious from the diff.
- Keep the PR description focused on: what entrypoint/bug this addresses,
  what testing you did, and any deliberate deviation from the doc-comment
  spec.

## Getting help

If you're unsure about an approach before investing the time to implement
it — especially for `execute`, where a wrong assumption is expensive to
unwind in review — open a draft PR or a discussion issue describing your
plan first.
